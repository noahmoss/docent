use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Borders, Clear, Paragraph},
};

use super::pane_block;
use crate::app::App;
use crate::layout::Pane;
use crate::colors;
use crate::search::SearchState;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Reserve space for search prompt when active
    let search_active = app.search.active;
    let inner_height = area.height.saturating_sub(2) as usize;
    let content_height = if search_active {
        inner_height.saturating_sub(1)
    } else {
        inner_height
    };

    let lines: Vec<Line> = if let Some(step) = app.current_step_data() {
        let mut all_lines: Vec<Line> = Vec::new();
        let mut line_index = 0usize;

        for hunk in &step.hunks {
            // File header
            all_lines.push(style_line_with_search(
                &format!("─── {} ───", hunk.file_path),
                line_index,
                &app.search,
                Some(Style::default().fg(colors::DIFF_FILE_HEADER).add_modifier(Modifier::BOLD)),
            ));
            line_index += 1;

            all_lines.push(style_line_with_search("", line_index, &app.search, None));
            line_index += 1;

            // Diff content with syntax highlighting
            for line in hunk.content.lines() {
                let styled_line = style_diff_line_with_search(line, line_index, &app.search);
                all_lines.push(styled_line);
                line_index += 1;
            }

            all_lines.push(style_line_with_search("", line_index, &app.search, None));
            line_index += 1;
        }

        all_lines
    } else {
        vec![Line::from("No diff content")]
    };

    let total_lines = lines.len();

    // Apply scroll offset (clamped to valid range)
    let max_scroll = total_lines.saturating_sub(content_height);
    let scroll = app.diff_scroll.clamped(max_scroll);
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll)
        .take(content_height)
        .collect();

    let scroll_indicator = if total_lines > content_height && max_scroll > 0 {
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

    // Render search prompt at bottom of diff area (inside the border)
    if search_active {
        let prompt_area = Rect {
            x: area.x + 1, // inside left border/padding
            y: area.y + area.height - 2, // above bottom border
            width: area.width.saturating_sub(2), // inside borders
            height: 1,
        };

        frame.render_widget(Clear, prompt_area);

        let prompt = Line::from(vec![
            Span::styled("/", Style::default().fg(Color::White)),
            Span::raw(&app.search.input),
            Span::styled("_", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(prompt), prompt_area);
    }
}

fn get_base_style(line: &str) -> Style {
    if line.starts_with("@@") {
        Style::default().fg(colors::DIFF_HUNK_HEADER)
    } else if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(colors::DIFF_ADDED)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(colors::DIFF_REMOVED)
    } else {
        Style::default()
    }
}

fn style_diff_line_with_search(line: &str, line_index: usize, search: &SearchState) -> Line<'static> {
    let base_style = get_base_style(line);
    style_line_with_search(line, line_index, search, Some(base_style))
}

fn style_line_with_search(
    line: &str,
    line_index: usize,
    search: &SearchState,
    base_style: Option<Style>,
) -> Line<'static> {
    let base_style = base_style.unwrap_or_default();
    let owned_line = line.to_string();

    // If no search query, return simple styled line
    if search.query.is_none() {
        return Line::from(Span::styled(owned_line, base_style));
    }

    // Find all matches on this line
    let matches_on_line: Vec<_> = search
        .matches
        .iter()
        .enumerate()
        .filter(|(_, m)| m.line == line_index)
        .collect();

    if matches_on_line.is_empty() {
        return Line::from(Span::styled(owned_line, base_style));
    }

    // Build spans with highlighting
    let mut spans = Vec::new();
    let mut pos = 0;
    let line_len = owned_line.len();

    for (match_idx, m) in matches_on_line {
        // Validate match bounds
        if m.start > line_len || m.end > line_len || m.start > m.end {
            continue;
        }

        // Add text before match
        if m.start > pos
            && let Some(text) = owned_line.get(pos..m.start)
        {
            spans.push(Span::styled(text.to_string(), base_style));
        }

        // Add highlighted match
        let is_current = match_idx == search.current;
        let highlight_style = if is_current {
            Style::default()
                .bg(colors::SEARCH_MATCH_CURRENT)
                .fg(colors::SEARCH_MATCH_TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(colors::SEARCH_MATCH)
                .fg(colors::SEARCH_MATCH_TEXT)
        };

        if let Some(text) = owned_line.get(m.start..m.end) {
            spans.push(Span::styled(text.to_string(), highlight_style));
        }

        pos = m.end;
    }

    // Add remaining text after last match
    if pos < line_len
        && let Some(text) = owned_line.get(pos..)
    {
        spans.push(Span::styled(text.to_string(), base_style));
    }

    // Fallback if no spans were created
    if spans.is_empty() {
        return Line::from(Span::styled(owned_line, base_style));
    }

    Line::from(spans)
}

pub fn content_height(app: &App) -> usize {
    app.current_step_data()
        .map(|step| step.diff_line_count())
        .unwrap_or(0)
}
