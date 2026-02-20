use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Padding},
};

use crate::app::{ActivePane, App};
use crate::colors;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .walkthrough
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let is_current = i == app.current_step;

            // Left indicator shows completion status
            let (indicator, indicator_color) = if app.visited_steps.get(i).copied().unwrap_or(false) {
                ("✓", colors::STEP_COMPLETED)
            } else {
                ("○", colors::STEP_PENDING)
            };

            // Text style: bold white if current, else based on visited status
            let text_style = if is_current {
                Style::default().fg(colors::STEP_CURRENT).add_modifier(Modifier::BOLD)
            } else if app.visited_steps.get(i).copied().unwrap_or(false) {
                Style::default().fg(colors::STEP_COMPLETED)
            } else {
                Style::default().fg(colors::STEP_PENDING)
            };

            // Current step gets a left arrow on the right
            let current_indicator = if is_current { " ←" } else { "" };

            let line = Line::from(vec![
                Span::styled(format!("{} ", indicator), Style::default().fg(indicator_color)),
                Span::styled(&step.title, text_style),
                Span::styled(current_indicator, Style::default().fg(colors::STEP_CURRENT)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let title = format!(
        " Steps ({}/{}) · {}/{} lines ",
        app.current_step + 1,
        app.walkthrough.step_count(),
        app.reviewed_diff_lines(),
        app.total_diff_lines(),
    );

    let border_color = if app.active_pane == ActivePane::Minimap {
        colors::BORDER_ACTIVE
    } else {
        colors::BORDER_INACTIVE
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(border_color))
            .padding(Padding::horizontal(1)),
    );

    frame.render_widget(list, area);
}
