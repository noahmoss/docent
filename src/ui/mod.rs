pub mod diff_viewer;
pub mod explanation;
pub mod minimap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

use crate::app::{App, AppState};
use crate::editor::VimInputMode;
use crate::layout::Pane;
use crate::colors;
use crate::constants::{
    ERROR_DIALOG_HEIGHT, ERROR_DIALOG_WIDTH, LOADING_DIALOG_HEIGHT, LOADING_DIALOG_WIDTH,
};

/// Creates a styled block for a pane with consistent styling.
pub fn pane_block(title: &str, borders: Borders, is_active: bool) -> Block<'_> {
    let border_color = if is_active {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER_INACTIVE
    };
    Block::default()
        .title(title)
        .borders(borders)
        .border_style(Style::default().fg(border_color))
        .padding(Padding::horizontal(1))
}

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

fn render_loading(frame: &mut Frame, area: Rect, status: &str, _steps_received: usize) {
    let dialog_area = centered_rect(LOADING_DIALOG_WIDTH, LOADING_DIALOG_HEIGHT, area);
    let block = Block::default()
        .title(" Generating Walkthrough ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

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
        Line::from(Span::styled(
            "Press Ctrl+C to cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, dialog_area);
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

    frame.render_widget(paragraph, centered_rect(ERROR_DIALOG_WIDTH, ERROR_DIALOG_HEIGHT, area));
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
    // If a pane is zoomed, render only that pane fullscreen
    if let Some(zoomed_pane) = app.layout.zoomed {
        match zoomed_pane {
            Pane::Minimap => minimap::render(frame, area, app),
            Pane::Chat => explanation::render(frame, area, app),
            Pane::Diff => diff_viewer::render(frame, area, app),
        }
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(app.layout.left_pane_percent),
            Constraint::Percentage(100 - app.layout.left_pane_percent),
        ])
        .split(area);

    render_left_pane(frame, chunks[0], app);
    diff_viewer::render(frame, chunks[1], app);
}

fn render_left_pane(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(app.layout.minimap_percent),
            Constraint::Percentage(100 - app.layout.minimap_percent),
        ])
        .split(area);

    minimap::render(frame, chunks[0], app);
    explanation::render(frame, chunks[1], app);
}

fn help(key: &str, action: &str) -> [Span<'static>; 2] {
    [
        Span::styled(format!(" {key} "), Style::default().fg(Color::Yellow)),
        Span::raw(format!("{action} ")),
    ]
}

fn render_help_bar(frame: &mut Frame, area: Rect, app: &App) {
    let is_zoomed = app.layout.is_zoomed();

    let help_text = if app.quit_pending {
        Line::from(Span::styled(
            "Press Ctrl+C again to quit",
            Style::default().fg(Color::White),
        ))
    } else {
        match app.layout.active_pane {
            Pane::Chat => {
                if app.chat_scroll.in_scrollback() {
                    // Scrollback mode (works in both vim and non-vim)
                    let mut spans = vec![Span::styled(
                        if is_zoomed { "-- ZOOMED | SCROLLBACK -- " } else { "-- SCROLLBACK -- " },
                        Style::default().fg(if is_zoomed { Color::Cyan } else { Color::DarkGray }),
                    )];
                    spans.extend(help("Ctrl+n/p", "scroll"));
                    if is_zoomed {
                        spans.extend(help("z", "unzoom"));
                    }
                    spans.extend(help("Esc", "exit"));
                    Line::from(spans)
                } else if app.editor.vim_enabled && app.editor.vim_mode == VimInputMode::Insert {
                    // Vim insert mode
                    Line::from(Span::styled(
                        if is_zoomed { "-- ZOOMED | INSERT --" } else { "-- INSERT --" },
                        Style::default().fg(if is_zoomed { Color::Cyan } else { Color::DarkGray }),
                    ))
                } else if app.editor.vim_enabled {
                    // Vim normal mode
                    let mut spans = vec![];
                    if is_zoomed {
                        spans.push(Span::styled("-- ZOOMED -- ", Style::default().fg(Color::Cyan)));
                    }
                    spans.extend(help("Ctrl+n/p", "scroll"));
                    spans.extend(help("z", if is_zoomed { "unzoom" } else { "zoom" }));
                    spans.extend(help("Ctrl+C", "quit"));
                    Line::from(spans)
                } else {
                    // Non-vim mode
                    let mut spans = vec![];
                    if is_zoomed {
                        spans.push(Span::styled("-- ZOOMED -- ", Style::default().fg(Color::Cyan)));
                    }
                    spans.extend(help("Ctrl+n/p", "scroll"));
                    spans.extend(help("Tab", "switch pane"));
                    spans.extend(help("Ctrl+C", "quit"));
                    Line::from(spans)
                }
            }
            Pane::Minimap => {
                let mut spans = vec![];
                if is_zoomed {
                    spans.push(Span::styled("-- ZOOMED -- ", Style::default().fg(Color::Cyan)));
                }
                spans.extend(help("n/p", "switch step"));
                spans.extend(help("Enter", "mark reviewed"));
                spans.extend(help("z", if is_zoomed { "unzoom" } else { "zoom" }));
                spans.extend(help("Ctrl+C", "quit"));
                Line::from(spans)
            }
            Pane::Diff => {
                let mut spans = vec![];
                if is_zoomed {
                    spans.push(Span::styled("-- ZOOMED -- ", Style::default().fg(Color::Cyan)));
                }

                // Show search status if there's an active search
                if app.search.query.is_some() {
                    let match_display = app.search.match_count_display();
                    spans.push(Span::styled(
                        format!("[{match_display}] "),
                        Style::default().fg(Color::Yellow),
                    ));
                    spans.extend(help("n/p", "next/prev"));
                    spans.extend(help("Esc", "clear"));
                } else {
                    spans.extend(help("/", "search"));
                }

                spans.extend(help("j/k", "scroll"));
                spans.extend(help("z", if is_zoomed { "unzoom" } else { "zoom" }));
                spans.extend(help("Ctrl+C", "quit"));
                Line::from(spans)
            }
        }
    };

    let paragraph = Paragraph::new(help_text);

    frame.render_widget(paragraph, area);
}
