mod api;
mod app;
mod colors;
mod constants;
mod diff;
mod editor;
mod generation;
mod input;
mod layout;
mod model;
mod scroll;
mod settings;
mod ui;

use std::io::{self, stdout, IsTerminal, Read};

use crossterm::{
    ExecutableCommand,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use api::ClaudeClient;
use app::App;
use constants::{EVENT_POLL_INTERVAL, EVENT_RECV_TIMEOUT, VIEWPORT_HEIGHT_OFFSET};
use generation::WalkthroughGenerator;
use input::InputHandler;
use model::{Message, Walkthrough, mock_walkthrough};
use settings::Settings;

/// Events that can occur in the app
enum AppEvent {
    /// Terminal input event
    Terminal(Event),
    /// Walkthrough generation completed
    GenerationComplete(Walkthrough),
    /// Walkthrough generation failed
    GenerationError(String),
    /// Chat response chunk received (step_index, text_chunk)
    ChatChunk(usize, String),
    /// Chat response completed (step_index)
    ChatComplete(usize),
    /// Chat request failed (step_index, error_message)
    ChatError(usize, String),
}

/// Spawns a task to generate a walkthrough from diff text
fn spawn_walkthrough_generation(tx: mpsc::Sender<AppEvent>, diff_text: String) {
    tokio::spawn(async move {
        match WalkthroughGenerator::new(&diff_text) {
            Ok(generator) => match generator.generate().await {
                Ok(walkthrough) => {
                    let _ = tx.send(AppEvent::GenerationComplete(walkthrough)).await;
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::GenerationError(e.to_string())).await;
                }
            },
            Err(e) => {
                let _ = tx.send(AppEvent::GenerationError(e.to_string())).await;
            }
        }
    });
}

/// Spawns a thread to read terminal events and forward them to the event channel
fn spawn_terminal_reader(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || loop {
        match event::poll(EVENT_POLL_INTERVAL) {
            Ok(true) => {
                if let Ok(evt) = event::read()
                    && tx.blocking_send(AppEvent::Terminal(evt)).is_err()
                {
                    break;
                }
            }
            Ok(false) => {}
            Err(_) => {
                std::thread::sleep(EVENT_POLL_INTERVAL);
            }
        }
    });
}

/// Spawns a task to handle streaming chat with the Claude API
fn spawn_chat_handler(
    tx: mpsc::Sender<AppEvent>,
    step_index: usize,
    walkthrough: Walkthrough,
    messages: Vec<Message>,
) {
    tokio::spawn(async move {
        match ClaudeClient::from_env() {
            Ok(client) => {
                let (chunk_tx, mut chunk_rx) = mpsc::channel::<String>(32);

                let tx_chunks = tx.clone();
                let forward_task = tokio::spawn(async move {
                    while let Some(chunk) = chunk_rx.recv().await {
                        if tx_chunks
                            .send(AppEvent::ChatChunk(step_index, chunk))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                match client
                    .chat_streaming(&walkthrough, step_index, &messages, chunk_tx)
                    .await
                {
                    Ok(()) => {
                        let _ = forward_task.await;
                        let _ = tx.send(AppEvent::ChatComplete(step_index)).await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(AppEvent::ChatError(step_index, e.to_string()))
                            .await;
                    }
                }
            }
            Err(e) => {
                let _ = tx
                    .send(AppEvent::ChatError(step_index, e.to_string()))
                    .await;
            }
        }
    });
}

#[derive(Debug)]
struct Args {
    diff_file: Option<String>,
    use_mock: bool,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    Args {
        diff_file: args.get(1).filter(|s| !s.starts_with('-')).cloned(),
        use_mock: args.iter().any(|a| a == "--mock"),
    }
}

fn read_diff_input(args: &Args) -> io::Result<Option<String>> {
    if args.use_mock {
        return Ok(None);
    }

    // Try to read from file if specified
    if let Some(path) = &args.diff_file {
        return Ok(Some(std::fs::read_to_string(path)?));
    }

    // Check if stdin is piped - read from it before crossterm initializes
    // The "use-dev-tty" feature in crossterm will handle terminal events from /dev/tty
    if !std::io::stdin().is_terminal() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;

        if input.trim().is_empty() {
            return Ok(None);
        }
        return Ok(Some(input));
    }

    Ok(None)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = parse_args();
    let diff_input = read_diff_input(&args)?;

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Run app
    let result = run_app(&mut terminal, diff_input).await;

    // Restore terminal
    stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    diff_input: Option<String>,
) -> io::Result<()> {
    let settings = Settings::load();

    let mut app = if diff_input.is_none() {
        App::new(mock_walkthrough(), &settings)
    } else {
        let mut app = App::loading(&settings);
        app.diff_input = diff_input.clone();
        app
    };

    let mut input_handler = InputHandler::new();
    let (tx, mut rx) = mpsc::channel::<AppEvent>(32);

    if let Some(diff_text) = diff_input {
        app.set_loading_status("Generating walkthrough...".to_string());
        spawn_walkthrough_generation(tx.clone(), diff_text);
    }

    spawn_terminal_reader(tx.clone());

    loop {
        let viewport_height = terminal.size()?.height.saturating_sub(VIEWPORT_HEIGHT_OFFSET) as usize;

        terminal.draw(|frame| {
            ui::render(frame, &app);
        })?;

        if let Ok(Some(event)) = tokio::time::timeout(EVENT_RECV_TIMEOUT, rx.recv()).await {
            handle_app_event(event, &mut app, &mut input_handler, terminal, viewport_height)?;
        }

        if let Some((step_index, walkthrough, messages)) = app.chat_request.take() {
            spawn_chat_handler(tx.clone(), step_index, walkthrough, messages);
        }

        if app.should_quit {
            break;
        }

        if app.retry_requested {
            app.retry_requested = false;
            if let Some(diff_text) = app.diff_input.clone() {
                app.set_loading_status("Generating walkthrough...".to_string());
                spawn_walkthrough_generation(tx.clone(), diff_text);
            }
        }
    }

    Ok(())
}

fn handle_app_event<B: Backend>(
    event: AppEvent,
    app: &mut App,
    input_handler: &mut InputHandler,
    terminal: &Terminal<B>,
    viewport_height: usize,
) -> io::Result<()> {
    match event {
        AppEvent::Terminal(Event::Key(key)) => {
            input_handler.handle_key(key, app, viewport_height);
        }
        AppEvent::Terminal(Event::Mouse(mouse)) => {
            input_handler.handle_mouse(mouse, app, terminal.size()?);
        }
        AppEvent::Terminal(_) => {}
        AppEvent::GenerationComplete(walkthrough) => {
            app.set_ready(walkthrough);
        }
        AppEvent::GenerationError(message) => {
            app.set_error(message);
        }
        AppEvent::ChatChunk(step_index, chunk) => {
            app.receive_chat_chunk(step_index, chunk);
        }
        AppEvent::ChatComplete(step_index) => {
            app.receive_chat_complete(step_index);
        }
        AppEvent::ChatError(step_index, error) => {
            app.receive_chat_error(step_index, error);
        }
    }
    Ok(())
}
