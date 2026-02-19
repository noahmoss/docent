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

use crate::app::{App, InputMode};

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
    let help_text = match app.input_mode {
        InputMode::Normal => Line::from(vec![
            Span::styled(" j/k ", Style::default().fg(Color::Yellow)),
            Span::raw("scroll "),
            Span::styled(" n/p ", Style::default().fg(Color::Yellow)),
            Span::raw("step "),
            Span::styled(" i ", Style::default().fg(Color::Yellow)),
            Span::raw("chat "),
            Span::styled(" q ", Style::default().fg(Color::Yellow)),
            Span::raw("quit"),
        ]),
        InputMode::Insert => Line::from(vec![
            Span::styled(" Enter ", Style::default().fg(Color::Yellow)),
            Span::raw("send "),
            Span::styled(" Esc ", Style::default().fg(Color::Yellow)),
            Span::raw("cancel "),
        ]),
    };

    let paragraph = Paragraph::new(help_text);

    frame.render_widget(paragraph, area);
}
