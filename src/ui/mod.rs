pub mod diff_viewer;
pub mod explanation;
pub mod minimap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{ActivePane, App, AppState, VimInputMode};

pub fn render(frame: &mut Frame, app: &App) {
    match &app.state {
        AppState::Loading { status, steps_received } => {
            render_loading(frame, frame.area(), status, *steps_received);
        }
        AppState::Error { message } => {
            render_error(frame, frame.area(), message);
        }
        AppState::Ready => {
            render_ready(frame, app);
        }
    }
}

fn render_ready(frame: &mut Frame, app: &App) {
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

fn render_loading(frame: &mut Frame, area: Rect, status: &str, steps_received: usize) {
    let block = Block::default()
        .title(" Generating Walkthrough ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let _inner = block.inner(centered_rect(60, 30, area));

    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() / 100)
        .unwrap_or(0) as usize)
        % spinner_frames.len();

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                spinner_frames[spinner_idx],
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(status),
        ]),
        Line::from(""),
        Line::from(format!("Steps received: {}", steps_received)),
        Line::from(""),
        Line::from(Span::styled(
            "Press Ctrl+C to cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, centered_rect(60, 30, area));
}

fn render_error(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default()
        .title(" Error ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().fg(Color::Red),
        )),
        Line::from(""),
        Line::from(""),
        Line::from(Span::styled(
            "Press 'r' to retry or 'q' to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, centered_rect(70, 40, area));
}

/// Helper to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
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
                        Span::styled(" Ctrl+n/p ", Style::default().fg(Color::Yellow)),
                        Span::raw("scroll "),
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
