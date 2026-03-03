use std::io;
use std::path::PathBuf;
use std::process;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use crate::api::ClaudeClient;
use crate::diff::FileFilter;
use crate::generation::{WalkthroughGenerator, create_sub_steps, format_step_for_rechunk};
use crate::model::{Message, ReviewMode, Step, Walkthrough};
use crate::protocol::{
    NavigateAction, NavigateParams, Notification, Request, Response, SendMessageParams,
    StateSnapshot,
};
use crate::session::{Session, SessionState};
use crate::settings::Settings;

enum EngineEvent {
    GenerationComplete(Walkthrough),
    GenerationError(String),
    ChatChunk(usize, String),
    ChatComplete(usize),
    ChatError(usize, String),
    RechunkComplete(usize, Vec<Step>),
    RechunkError(String),
}

enum ServerEvent {
    Engine(EngineEvent),
    NewClient(UnixStream),
    ClientMessage(usize, String),
    ClientDisconnected(usize),
}

pub async fn run(
    diff_input: Option<String>,
    filter: FileFilter,
    mode: ReviewMode,
) -> io::Result<()> {
    let diff_text = diff_input.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "headless mode requires a diff input",
        )
    })?;

    let settings = Settings::load();
    let (api_key, source) = settings.resolve_api_key();
    if api_key.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no API key found (set ANTHROPIC_API_KEY or configure in ~/.docent/settings.json)",
        ));
    }

    if source == crate::settings::ApiKeySource::Settings
        && let Some(key) = &settings.api_key
    {
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", key) };
    }

    let mut session = Session::new(Walkthrough { steps: vec![] }, mode);
    session.diff_input = Some(diff_text.clone());
    session.diff_filter = filter.clone();
    session.api_key_input = api_key.unwrap_or_default();
    session.api_key_source = source;

    let socket_path = PathBuf::from(format!("/tmp/docent-{}.sock", process::id()));

    // Clean up any stale socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;

    // Print socket path to stdout for client to read, then close stdout
    {
        use std::io::Write;
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", socket_path.display())?;
        stdout.flush()?;
    }

    let (tx, mut rx) = mpsc::channel::<ServerEvent>(64);

    // Spawn connection acceptor
    let tx_accept = tx.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let _ = tx_accept.send(ServerEvent::NewClient(stream)).await;
        }
    });

    // Start walkthrough generation immediately
    session.state = SessionState::Loading {
        status: "Generating...".to_string(),
        steps_received: 0,
    };
    spawn_generation(tx.clone(), diff_text, filter, mode);

    let mut clients: Vec<(usize, mpsc::Sender<String>)> = Vec::new();
    let mut next_client_id: usize = 0;

    loop {
        // Poll for pending requests from session
        if let Some((step_index, walkthrough, messages)) = session.chat_request.take() {
            spawn_chat(tx.clone(), step_index, walkthrough, messages, mode);
        }

        if let Some((step_index, step, diff_text)) = session.rechunk_request.take() {
            spawn_rechunk_task(tx.clone(), step_index, step, diff_text, mode);
        }

        let event = tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(e) => e,
                    None => break,
                }
            }
            _ = tokio::signal::ctrl_c() => {
                broadcast(&clients, &Notification::shutdown()).await;
                break;
            }
        };

        match event {
            ServerEvent::NewClient(stream) => {
                let client_id = next_client_id;
                next_client_id += 1;

                let (read_half, write_half) = stream.into_split();

                // Send state snapshot to new client
                let (write_tx, mut write_rx) = mpsc::channel::<String>(64);

                // Writer task
                tokio::spawn(async move {
                    let mut writer = write_half;
                    while let Some(msg) = write_rx.recv().await {
                        if writer.write_all(msg.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });

                // Send initial state
                let snapshot = build_snapshot(&session);
                let init = Notification::new("state_snapshot", snapshot);
                let init_json = serde_json::to_string(&init).unwrap_or_default();
                let _ = write_tx.send(format!("{}\n", init_json)).await;

                // If walkthrough is ready, also send walkthrough_loaded
                if matches!(session.state, SessionState::Ready)
                    && !session.walkthrough.steps.is_empty()
                {
                    let notif = Notification::walkthrough_loaded(
                        &session.walkthrough,
                        &session.reviewed_steps,
                    );
                    let json = serde_json::to_string(&notif).unwrap_or_default();
                    let _ = write_tx.send(format!("{}\n", json)).await;
                }

                clients.push((client_id, write_tx));

                // Reader task
                let tx_read = tx.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(read_half);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line).await {
                            Ok(0) | Err(_) => {
                                let _ = tx_read
                                    .send(ServerEvent::ClientDisconnected(client_id))
                                    .await;
                                break;
                            }
                            Ok(_) => {
                                let trimmed = line.trim().to_string();
                                if !trimmed.is_empty() {
                                    let _ = tx_read
                                        .send(ServerEvent::ClientMessage(client_id, trimmed))
                                        .await;
                                }
                            }
                        }
                    }
                });
            }

            ServerEvent::ClientDisconnected(client_id) => {
                clients.retain(|(id, _)| *id != client_id);
            }

            ServerEvent::ClientMessage(client_id, msg) => {
                let request: Result<Request, _> = serde_json::from_str(&msg);
                match request {
                    Ok(req) => {
                        let (response, notifications) = handle_request(&mut session, &req);
                        // Send response to requesting client
                        if let Some((_, sender)) = clients.iter().find(|(id, _)| *id == client_id) {
                            let json = serde_json::to_string(&response).unwrap_or_default();
                            let _ = sender.send(format!("{}\n", json)).await;
                        }
                        // Broadcast notifications to all clients
                        for notif in notifications {
                            broadcast(&clients, &notif).await;
                        }
                    }
                    Err(e) => {
                        // Send parse error back
                        let resp = Response::error(0, format!("invalid request: {}", e));
                        if let Some((_, sender)) = clients.iter().find(|(id, _)| *id == client_id) {
                            let json = serde_json::to_string(&resp).unwrap_or_default();
                            let _ = sender.send(format!("{}\n", json)).await;
                        }
                    }
                }
            }

            ServerEvent::Engine(engine_event) => {
                let notifications = handle_engine_event(&mut session, engine_event);
                for notif in notifications {
                    broadcast(&clients, &notif).await;
                }
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

fn handle_request(session: &mut Session, req: &Request) -> (Response, Vec<Notification>) {
    let mut notifications = Vec::new();

    let response = match req.method.as_str() {
        "get_state" => {
            let snapshot = build_snapshot(session);
            Response::ok(req.id, snapshot)
        }

        "navigate" => match serde_json::from_value::<NavigateParams>(req.params.clone()) {
            Ok(params) => {
                let changed = match params.action {
                    NavigateAction::Next => session.next_step(),
                    NavigateAction::Prev => session.prev_step(),
                    NavigateAction::Goto => {
                        if let Some(step) = params.step {
                            session.go_to_step(step)
                        } else {
                            false
                        }
                    }
                };
                if changed && let Some(step) = session.current_step_data() {
                    notifications.push(Notification::step_changed(
                        session.current_step,
                        step,
                        &session.reviewed_steps,
                        session.walkthrough_complete,
                    ));
                }
                Response::ok(req.id, "ok")
            }
            Err(e) => Response::error(req.id, format!("invalid params: {}", e)),
        },

        "complete_step" => {
            let changed = session.complete_step_and_advance();
            if let Some(step) = session.current_step_data() {
                notifications.push(Notification::step_changed(
                    session.current_step,
                    step,
                    &session.reviewed_steps,
                    session.walkthrough_complete,
                ));
            }
            let _ = changed;
            Response::ok(req.id, "ok")
        }

        "toggle_reviewed" => {
            session.toggle_step_reviewed();
            if let Some(step) = session.current_step_data() {
                notifications.push(Notification::step_changed(
                    session.current_step,
                    step,
                    &session.reviewed_steps,
                    session.walkthrough_complete,
                ));
            }
            Response::ok(req.id, "ok")
        }

        "send_message" => match serde_json::from_value::<SendMessageParams>(req.params.clone()) {
            Ok(params) => {
                if session.chat_pending.is_some() {
                    Response::error(req.id, "chat already pending")
                } else {
                    session.send_message(params.content);
                    Response::ok(req.id, "ok")
                }
            }
            Err(e) => Response::error(req.id, format!("invalid params: {}", e)),
        },

        "rechunk" => {
            if session.rechunk_pending {
                Response::error(req.id, "rechunk already pending")
            } else {
                session.request_rechunk();
                Response::ok(req.id, "ok")
            }
        }

        "shutdown" => {
            notifications.push(Notification::shutdown());
            Response::ok(req.id, "ok")
        }

        _ => Response::error(req.id, format!("unknown method: {}", req.method)),
    };

    (response, notifications)
}

fn handle_engine_event(session: &mut Session, event: EngineEvent) -> Vec<Notification> {
    let mut notifications = Vec::new();

    match event {
        EngineEvent::GenerationComplete(walkthrough) => {
            session.set_ready(walkthrough);
            notifications.push(Notification::state_changed(&session.state));
            notifications.push(Notification::walkthrough_loaded(
                &session.walkthrough,
                &session.reviewed_steps,
            ));
        }
        EngineEvent::GenerationError(message) => {
            session.set_error(message.clone());
            notifications.push(Notification::state_changed(&session.state));
            notifications.push(Notification::error(&message));
        }
        EngineEvent::ChatChunk(step_index, chunk) => {
            session.receive_chat_chunk(step_index, chunk.clone());
            notifications.push(Notification::chat_chunk(step_index, &chunk));
        }
        EngineEvent::ChatComplete(step_index) => {
            session.receive_chat_complete(step_index);
            notifications.push(Notification::chat_complete(step_index));
        }
        EngineEvent::ChatError(step_index, error) => {
            session.receive_chat_error(step_index, error.clone());
            notifications.push(Notification::error(&error));
        }
        EngineEvent::RechunkComplete(step_index, sub_steps) => {
            session.receive_rechunk_complete(step_index, sub_steps);
            notifications.push(Notification::rechunk_complete(
                &session.walkthrough.steps,
                session.current_step,
                &session.reviewed_steps,
            ));
        }
        EngineEvent::RechunkError(error) => {
            session.receive_rechunk_error(error.clone());
            notifications.push(Notification::error(&error));
        }
    }

    notifications
}

fn build_snapshot(session: &Session) -> StateSnapshot {
    StateSnapshot {
        state: session.state.clone(),
        review_mode: session.review_mode,
        current_step: session.current_step,
        walkthrough: session.walkthrough.clone(),
        reviewed: session.reviewed_steps.clone(),
        walkthrough_complete: session.walkthrough_complete,
        chat_pending: session.chat_pending,
        rechunk_pending: session.rechunk_pending,
    }
}

async fn broadcast(clients: &[(usize, mpsc::Sender<String>)], notification: &Notification) {
    let json = serde_json::to_string(notification).unwrap_or_default();
    let msg = format!("{}\n", json);
    for (_, sender) in clients {
        let _ = sender.send(msg.clone()).await;
    }
}

fn spawn_generation(
    tx: mpsc::Sender<ServerEvent>,
    diff_text: String,
    filter: FileFilter,
    mode: ReviewMode,
) {
    tokio::spawn(async move {
        let event = match WalkthroughGenerator::with_filter(&diff_text, &filter, mode) {
            Ok(generator) => match generator.generate().await {
                Ok(walkthrough) => EngineEvent::GenerationComplete(walkthrough),
                Err(e) => EngineEvent::GenerationError(e.to_string()),
            },
            Err(e) => EngineEvent::GenerationError(e.to_string()),
        };
        let _ = tx.send(ServerEvent::Engine(event)).await;
    });
}

fn spawn_chat(
    tx: mpsc::Sender<ServerEvent>,
    step_index: usize,
    walkthrough: Walkthrough,
    messages: Vec<Message>,
    mode: ReviewMode,
) {
    tokio::spawn(async move {
        match ClaudeClient::from_env() {
            Ok(client) => {
                let (chunk_tx, mut chunk_rx) = mpsc::channel::<String>(32);

                let tx_chunks = tx.clone();
                let forward_task = tokio::spawn(async move {
                    while let Some(chunk) = chunk_rx.recv().await {
                        let _ = tx_chunks
                            .send(ServerEvent::Engine(EngineEvent::ChatChunk(
                                step_index, chunk,
                            )))
                            .await;
                    }
                });

                match client
                    .chat_streaming(&walkthrough, step_index, &messages, mode, chunk_tx)
                    .await
                {
                    Ok(()) => {
                        let _ = forward_task.await;
                        let _ = tx
                            .send(ServerEvent::Engine(EngineEvent::ChatComplete(step_index)))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(ServerEvent::Engine(EngineEvent::ChatError(
                                step_index,
                                e.to_string(),
                            )))
                            .await;
                    }
                }
            }
            Err(e) => {
                let _ = tx
                    .send(ServerEvent::Engine(EngineEvent::ChatError(
                        step_index,
                        e.to_string(),
                    )))
                    .await;
            }
        }
    });
}

fn spawn_rechunk_task(
    tx: mpsc::Sender<ServerEvent>,
    step_index: usize,
    step: Step,
    diff_text: Option<String>,
    mode: ReviewMode,
) {
    tokio::spawn(async move {
        match ClaudeClient::from_env() {
            Ok(client) => {
                let step_content = format_step_for_rechunk(&step);
                let mut prompt = format!(
                    "Please split this step into smaller sub-steps.\n\n\
                     ## Step: {}\n\n{}\n\n## Hunks\n\n{}",
                    step.title, step.summary, step_content
                );
                if let Some(diff) = diff_text {
                    prompt.push_str(&format!(
                        "\n\nHere is the full diff for context:\n\n{}",
                        diff
                    ));
                }

                match client.rechunk_step(&prompt, mode).await {
                    Ok(response) => {
                        let event = match create_sub_steps(&step, response, &step.id) {
                            Ok(sub_steps) => EngineEvent::RechunkComplete(step_index, sub_steps),
                            Err(e) => EngineEvent::RechunkError(e.to_string()),
                        };
                        let _ = tx.send(ServerEvent::Engine(event)).await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(ServerEvent::Engine(EngineEvent::RechunkError(
                                e.to_string(),
                            )))
                            .await;
                    }
                }
            }
            Err(e) => {
                let _ = tx
                    .send(ServerEvent::Engine(EngineEvent::RechunkError(
                        e.to_string(),
                    )))
                    .await;
            }
        }
    });
}
