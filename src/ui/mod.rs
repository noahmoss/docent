pub mod diff_viewer;
pub mod explanation;
pub mod minimap;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{ActivePane, App, VimInputMode};

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Main content
            Constraint::Length(1), // Help bar
        ])
        .split(frame.area());

    render_main(frame, chunks[0], app);
    render_help_bar(frame, chunks[1], app);
}

fn render_main(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.left_pane_percent),
            Constraint::Percentage(100 - app.left_pane_percent),
        ])
        .split(area);

    render_left_pane(frame, chunks[0], app);
    diff_viewer::render(frame, chunks[1], app);
}

fn render_left_pane(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.minimap_percent),
            Constraint::Percentage(100 - app.minimap_percent),
        ])
        .split(area);

    minimap::render(frame, chunks[0], app);
    explanation::render(frame, chunks[1], app);
}

fn render_help_bar(frame: &mut Frame, area: Rect, app: &App) {
    let help_text = if app.quit_pending {
        Line::from(Span::styled(
            "Press Ctrl+C again to quit",
            Style::default().fg(Color::White),
        ))
    } else {
        match app.active_pane {
            ActivePane::Chat => {
                let in_insert = !app.vim_enabled || app.vim_mode == VimInputMode::Insert;
                if in_insert {
                    Line::from(Span::styled(
                        "-- INSERT --",
                        Style::default().fg(Color::DarkGray),
                    ))
                } else {
                    // Vim normal mode
                    Line::from(vec![
                        Span::styled(" Ctrl+C ", Style::default().fg(Color::Yellow)),
                        Span::raw("quit"),
                    ])
                }
            }
            ActivePane::Minimap => Line::from(vec![
                Span::styled(" n/p ", Style::default().fg(Color::Yellow)),
                Span::raw("step "),
                Span::styled(" i ", Style::default().fg(Color::Yellow)),
                Span::raw("chat "),
                Span::styled(" Ctrl+C ", Style::default().fg(Color::Yellow)),
                Span::raw("quit"),
            ]),
            ActivePane::Diff => Line::from(vec![
                Span::styled(" j/k ", Style::default().fg(Color::Yellow)),
                Span::raw("scroll "),
                Span::styled(" i ", Style::default().fg(Color::Yellow)),
                Span::raw("chat "),
                Span::styled(" Ctrl+C ", Style::default().fg(Color::Yellow)),
                Span::raw("quit"),
            ]),
        }
    };

    let paragraph = Paragraph::new(help_text);

    frame.render_widget(paragraph, area);
}
