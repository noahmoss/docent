mod api;
mod app;
mod colors;
mod diff;
mod generation;
mod input;
mod model;
mod settings;
mod ui;

use std::io::{self, stdout, IsTerminal, Read};
use std::time::Duration;

use crossterm::{
    ExecutableCommand,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use app::App;
use api::ClaudeClient;
use generation::WalkthroughGenerator;
use input::InputHandler;
use model::mock_walkthrough;
use settings::Settings;

/// Events that can occur in the app
enum AppEvent {
    /// Terminal input event
    Terminal(Event),
    /// Walkthrough generation completed
    GenerationComplete(model::Walkthrough),
    /// Walkthrough generation failed
    GenerationError(String),
    /// Chat response chunk received (step_index, text_chunk)
    ChatChunk(usize, String),
    /// Chat response completed (step_index)
    ChatComplete(usize),
    /// Chat request failed (step_index, error_message)
    ChatError(usize, String),
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

    // If no diff input, use mock data
    let mut app = if diff_input.is_none() {
        let walkthrough = mock_walkthrough();
        App::new(walkthrough, &settings)
    } else {
        App::loading(&settings)
    };

    let mut input_handler = InputHandler::new();

    // Create channel for async events
    let (tx, mut rx) = mpsc::channel::<AppEvent>(32);

    // Start generation if we have diff input
    if let Some(diff_text) = diff_input {
        let tx_gen = tx.clone();
        app.set_loading_status("Parsing diff...".to_string());

        tokio::spawn(async move {
            match WalkthroughGenerator::new(&diff_text) {
                Ok(generator) => match generator.generate().await {
                    Ok(walkthrough) => {
                        let _ = tx_gen.send(AppEvent::GenerationComplete(walkthrough)).await;
                    }
                    Err(e) => {
                        let _ = tx_gen.send(AppEvent::GenerationError(e.to_string())).await;
                    }
                },
                Err(e) => {
                    let _ = tx_gen.send(AppEvent::GenerationError(e.to_string())).await;
                }
            }
        });
    }

    // Spawn terminal event reader using blocking task
    let tx_term = tx.clone();
    std::thread::spawn(move || loop {
        // Use a short poll timeout to keep the loop responsive
        match event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                if let Ok(evt) = event::read() {
                    if tx_term.blocking_send(AppEvent::Terminal(evt)).is_err() {
                        break;
                    }
                }
            }
            Ok(false) => {
                // No event, continue polling
            }
            Err(_) => {
                // Error polling, sleep briefly and retry
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    });

    // Main event loop
    loop {
        // Get viewport height for scroll calculations
        let viewport_height = terminal.size()?.height.saturating_sub(5) as usize;

        // Render
        terminal.draw(|frame| {
            ui::render(frame, &app);
        })?;

        // Handle events with timeout for spinner animation
        match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(event)) => match event {
                AppEvent::Terminal(Event::Key(key)) => {
                    input_handler.handle_key(key, &mut app, viewport_height);
                }
                AppEvent::Terminal(Event::Mouse(mouse)) => {
                    input_handler.handle_mouse(mouse, &mut app, terminal.size()?);
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
            },
            Ok(None) => {
                // Channel closed
                break;
            }
            Err(_) => {
                // Timeout - continue to re-render (for spinner)
            }
        }

        // Check for pending chat requests
        if let Some((step_index, walkthrough, messages)) = app.chat_request.take() {
            let tx_chat = tx.clone();
            tokio::spawn(async move {
                match ClaudeClient::from_env() {
                    Ok(client) => {
                        // Create channel for streaming chunks
                        let (chunk_tx, mut chunk_rx) = mpsc::channel::<String>(32);

                        // Spawn task to forward chunks to main event loop
                        let tx_chunks = tx_chat.clone();
                        let forward_task = tokio::spawn(async move {
                            while let Some(chunk) = chunk_rx.recv().await {
                                if tx_chunks.send(AppEvent::ChatChunk(step_index, chunk)).await.is_err() {
                                    break;
                                }
                            }
                        });

                        // Run streaming chat
                        match client.chat_streaming(&walkthrough, step_index, &messages, chunk_tx).await {
                            Ok(()) => {
                                // Wait for forward task to complete
                                let _ = forward_task.await;
                                let _ = tx_chat.send(AppEvent::ChatComplete(step_index)).await;
                            }
                            Err(e) => {
                                let _ = tx_chat.send(AppEvent::ChatError(step_index, e.to_string())).await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx_chat.send(AppEvent::ChatError(step_index, e.to_string())).await;
                    }
                }
            });
        }

        // Check for quit
        if app.should_quit {
            break;
        }
    }

    Ok(())
}
