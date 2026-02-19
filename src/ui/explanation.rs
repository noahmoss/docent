use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

use crate::app::{ActivePane, App, InputMode};
use crate::colors;
use crate::model::MessageRole;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Min(1),    // Chat history
            Constraint::Length(3), // Input box
        ])
        .split(area);

    render_chat_history(frame, chunks[0], app);
    render_input_box(frame, chunks[1], app);
}

fn render_chat_history(frame: &mut Frame, area: Rect, app: &App) {
    let inner_height = area.height.saturating_sub(2) as usize;

    let lines: Vec<Line> = if app.is_walkthrough_complete() {
        vec![
            Line::from(Span::styled(
                "✓ Walkthrough complete!",
                Style::default().fg(colors::STEP_COMPLETED),
            )),
            Line::from(""),
            Line::from("All steps have been reviewed."),
            Line::from("Press 'q' to exit or navigate back to review steps."),
        ]
    } else if let Some(step) = app.current_step_data() {
        let mut all_lines: Vec<Line> = Vec::new();

        for message in &step.messages {
            let mut first_line = true;
            match message.role {
                MessageRole::Assistant => {
                    for line in message.content.lines() {
                        if first_line {
                            all_lines.push(Line::from(vec![
                                Span::styled("⏺ ", Style::default().fg(colors::CHAT_ASSISTANT_BULLET)),
                                Span::styled(line, Style::default().fg(colors::CHAT_ASSISTANT_TEXT)),
                            ]));
                            first_line = false;
                        } else {
                            all_lines.push(Line::from(Span::styled(
                                line,
                                Style::default().fg(colors::CHAT_ASSISTANT_TEXT),
                            )));
                        }
                    }
                }
                MessageRole::User => {
                    for line in message.content.lines() {
                        if first_line {
                            all_lines.push(Line::from(Span::styled(
                                format!("> {} ", line),
                                Style::default().bg(colors::CHAT_USER_BG).fg(colors::CHAT_USER_TEXT),
                            )));
                            first_line = false;
                        } else {
                            all_lines.push(Line::from(Span::styled(
                                format!("  {} ", line),
                                Style::default().bg(colors::CHAT_USER_BG).fg(colors::CHAT_USER_TEXT),
                            )));
                        }
                    }
                }
            }
            all_lines.push(Line::from("")); // Spacing between messages
        }

        all_lines
    } else {
        vec![Line::from("No step selected")]
    };

    // Apply scroll
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(app.chat_scroll)
        .take(inner_height)
        .collect();

    let title = match app.input_mode {
        InputMode::Insert => " Chat (INSERT) ",
        InputMode::Normal => " Chat ",
    };

    let border_color = if app.active_pane == ActivePane::Chat {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER_INACTIVE
    };

    let paragraph = Paragraph::new(visible_lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(border_color))
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn render_input_box(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.active_pane == ActivePane::Chat {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER_INACTIVE
    };

    let prompt_style = Style::default().fg(colors::INPUT_PROMPT);
    let placeholder_style = Style::default().fg(colors::INPUT_PLACEHOLDER);

    let input_line = if app.input_buffer.is_empty() {
        let placeholder = match app.input_mode {
            InputMode::Insert => "Type your question...",
            InputMode::Normal => "Press 'i' to ask a question",
        };
        Line::from(vec![
            Span::styled("> ", prompt_style),
            Span::styled(placeholder, placeholder_style),
        ])
    } else {
        let before_cursor = &app.input_buffer[..app.cursor_position];
        let at_cursor = app
            .input_buffer
            .chars()
            .nth(app.cursor_position)
            .map(|c| c.to_string())
            .unwrap_or_else(|| " ".to_string());
        let after_cursor = if app.cursor_position < app.input_buffer.len() {
            &app.input_buffer[app.cursor_position + at_cursor.len()..]
        } else {
            ""
        };

        Line::from(vec![
            Span::styled("> ", prompt_style),
            Span::raw(before_cursor),
            Span::styled(
                at_cursor,
                Style::default().bg(colors::INPUT_CURSOR_BG).fg(colors::INPUT_CURSOR_FG),
            ),
            Span::raw(after_cursor),
        ])
    };

    let input = Paragraph::new(input_line).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(Style::default().fg(border_color))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(input, area);
}
