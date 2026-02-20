use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

use crate::app::{ActivePane, App};
use crate::colors;
use crate::model::MessageRole;

/// Parse markdown text and return styled spans.
fn parse_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let parser = Parser::new(text);

    let normal_style = Style::default().fg(colors::CHAT_ASSISTANT_TEXT);
    let bold_style = Style::default()
        .fg(colors::CHAT_ASSISTANT_BOLD)
        .add_modifier(Modifier::BOLD);
    let code_style = Style::default().fg(colors::CHAT_ASSISTANT_CODE);

    let mut current_style = normal_style;
    let mut style_stack: Vec<Style> = vec![normal_style];

    for event in parser {
        match event {
            Event::Start(Tag::Strong) => {
                style_stack.push(bold_style);
                current_style = bold_style;
            }
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
                current_style = *style_stack.last().unwrap_or(&normal_style);
            }
            Event::Start(Tag::Emphasis) => {
                let italic_style = current_style.add_modifier(Modifier::ITALIC);
                style_stack.push(italic_style);
                current_style = italic_style;
            }
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
                current_style = *style_stack.last().unwrap_or(&normal_style);
            }
            Event::Code(code) => {
                spans.push(Span::styled(code.to_string(), code_style));
            }
            Event::Text(text) => {
                spans.push(Span::styled(text.to_string(), current_style));
            }
            Event::SoftBreak | Event::HardBreak => {
                spans.push(Span::raw(" "));
            }
            _ => {}
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), normal_style));
    }

    spans
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.active_pane == ActivePane::Chat {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER_INACTIVE
    };

    // Outer block for the entire chat pane
    let outer_block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .padding(Padding::horizontal(1));

    let inner_area = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Calculate input height based on content (min 1, max 10 lines)
    let input_lines = app.textarea.lines().len().max(1).min(10) as u16;
    let input_height = input_lines + 1; // +1 for the top border

    let chunks = Layout::default()
        .constraints([
            Constraint::Min(1),              // Chat history
            Constraint::Length(input_height), // Input box (dynamic)
        ])
        .split(inner_area);

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
            match message.role {
                MessageRole::Assistant => {
                    for (i, line) in message.content.lines().enumerate() {
                        let mut spans = if i == 0 {
                            vec![Span::styled(
                                "⏺ ",
                                Style::default().fg(colors::CHAT_ASSISTANT_BULLET),
                            )]
                        } else {
                            vec![Span::raw("  ")]
                        };
                        spans.extend(parse_markdown(line));
                        all_lines.push(Line::from(spans));
                    }
                }
                MessageRole::User => {
                    for (i, line) in message.content.lines().enumerate() {
                        let prefix = if i == 0 { "> " } else { "  " };
                        all_lines.push(Line::from(Span::styled(
                            format!("{}{} ", prefix, line),
                            Style::default()
                                .bg(colors::CHAT_USER_BG)
                                .fg(colors::CHAT_USER_TEXT),
                        )));
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

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_input_box(frame: &mut Frame, area: Rect, app: &App) {
    // First render the top border across the full width
    let border_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(colors::BORDER_INACTIVE));
    let inner_area = border_block.inner(area);
    frame.render_widget(border_block, area);

    // Split into prompt column and textarea
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2), // "> " prompt
            Constraint::Min(1),    // Textarea
        ])
        .split(inner_area);

    // Render the prompt
    let prompt_style = Style::default().fg(colors::BORDER_INACTIVE);
    let prompt = Paragraph::new(Line::from(Span::styled("> ", prompt_style)));
    frame.render_widget(prompt, chunks[0]);

    let chat_focused = app.active_pane == ActivePane::Chat;

    // Show placeholder or the textarea
    if app.textarea_is_empty() && !chat_focused {
        let placeholder_style = Style::default().fg(colors::INPUT_PLACEHOLDER);
        let placeholder = "Press 'i' to ask a question";
        let input = Paragraph::new(Line::from(Span::styled(placeholder, placeholder_style)));
        frame.render_widget(input, chunks[1]);
    } else {
        let mut textarea = app.textarea.clone();

        // Show cursor when Chat pane is focused
        if chat_focused {
            textarea.set_cursor_style(
                Style::default()
                    .bg(colors::INPUT_CURSOR_BG)
                    .fg(colors::INPUT_CURSOR_FG),
            );
        } else {
            textarea.set_cursor_style(Style::default());
        }

        frame.render_widget(&textarea, chunks[1]);
    }
}
