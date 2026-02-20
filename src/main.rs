mod app;
mod colors;
mod input;
mod model;
mod settings;
mod ui;

use std::io::{self, stdout};
use std::time::Duration;

use crossterm::{
    ExecutableCommand,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

use app::App;
use input::InputHandler;
use model::mock_walkthrough;
use settings::Settings;

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Run app
    let result = run_app(&mut terminal);

    // Restore terminal
    stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let settings = Settings::load();
    let walkthrough = mock_walkthrough();
    let mut app = App::new(walkthrough, &settings);
    let mut input_handler = InputHandler::new();

    loop {
        // Get viewport height for scroll calculations
        let viewport_height = terminal.size()?.height.saturating_sub(5) as usize;

        // Render
        terminal.draw(|frame| {
            ui::render(frame, &app);
        })?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    input_handler.handle_key(key, &mut app, viewport_height);
                }
                Event::Mouse(mouse) => {
                    input_handler.handle_mouse(mouse, &mut app, terminal.size()?);
                }
                _ => {}
            }
        }

        // Check for quit
        if app.should_quit {
            break;
        }
    }

    Ok(())
}
