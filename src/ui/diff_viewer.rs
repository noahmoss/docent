use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::app::{ActivePane, App};
use crate::colors;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let inner_height = area.height.saturating_sub(2) as usize;

    let lines: Vec<Line> = if let Some(step) = app.current_step_data() {
        let mut all_lines: Vec<Line> = Vec::new();

        for hunk in &step.hunks {
            // File header
            all_lines.push(Line::from(vec![Span::styled(
                format!("─── {} ───", hunk.file_path),
                Style::default()
                    .fg(colors::DIFF_FILE_HEADER)
                    .add_modifier(Modifier::BOLD),
            )]));
            all_lines.push(Line::from(""));

            // Diff content with syntax highlighting
            for line in hunk.content.lines() {
                let styled_line = style_diff_line(line);
                all_lines.push(styled_line);
            }

            all_lines.push(Line::from(""));
        }

        all_lines
    } else {
        vec![Line::from("No diff content")]
    };

    let total_lines = lines.len();

    // Apply scroll offset
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(app.diff_scroll)
        .take(inner_height)
        .collect();

    let scroll_indicator = if total_lines > inner_height {
        let percent = if total_lines - inner_height > 0 {
            (app.diff_scroll * 100) / (total_lines - inner_height).max(1)
        } else {
            0
        };
        format!(" Diff [{}%] ", percent.min(100))
    } else {
        " Diff ".to_string()
    };

    let border_color = if app.active_pane == ActivePane::Diff {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER_INACTIVE
    };

    let paragraph = Paragraph::new(visible_lines).block(
        Block::default()
            .title(scroll_indicator)
            .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
            .border_style(Style::default().fg(border_color))
            .padding(Padding::horizontal(1)),
    );

    frame.render_widget(paragraph, area);
}

fn style_diff_line(line: &str) -> Line<'static> {
    let owned_line = line.to_string();

    if owned_line.starts_with("@@") {
        Line::from(Span::styled(
            owned_line,
            Style::default().fg(colors::DIFF_HUNK_HEADER),
        ))
    } else if owned_line.starts_with('+') && !owned_line.starts_with("+++") {
        Line::from(Span::styled(
            owned_line,
            Style::default().fg(colors::DIFF_ADDED),
        ))
    } else if owned_line.starts_with('-') && !owned_line.starts_with("---") {
        Line::from(Span::styled(
            owned_line,
            Style::default().fg(colors::DIFF_REMOVED),
        ))
    } else {
        Line::from(Span::raw(owned_line))
    }
}

pub fn content_height(app: &App) -> usize {
    if let Some(step) = app.current_step_data() {
        let mut count = 0;
        for hunk in &step.hunks {
            count += 2; // header + blank line
            count += hunk.content.lines().count();
            count += 1; // trailing blank
        }
        count
    } else {
        1
    }
}
