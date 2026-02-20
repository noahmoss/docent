use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Borders, Paragraph},
};

use super::pane_block;
use crate::app::App;
use crate::layout::Pane;
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

    // Apply scroll offset (clamped to valid range, persisted to prevent phantom scrolling)
    let max_scroll = total_lines.saturating_sub(inner_height);
    let scroll = app.diff_scroll.clamped(max_scroll);
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll)
        .take(inner_height)
        .collect();

    let scroll_indicator = if total_lines > inner_height && max_scroll > 0 {
        let percent = (scroll * 100) / max_scroll;
        format!(" Diff [{}%] ", percent.min(100))
    } else {
        " Diff ".to_string()
    };

    let is_active = app.layout.active_pane == Pane::Diff;
    let borders = if app.layout.is_zoomed() {
        Borders::TOP | Borders::BOTTOM
    } else {
        Borders::TOP | Borders::RIGHT | Borders::BOTTOM
    };
    let block = pane_block(&scroll_indicator, borders, is_active);
    let paragraph = Paragraph::new(visible_lines).block(block);

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
    app.current_step_data()
        .map(|step| step.diff_line_count())
        .unwrap_or(0)
}
