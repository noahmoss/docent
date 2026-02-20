use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Borders, List, ListItem},
};

use super::pane_block;
use crate::app::App;
use crate::layout::Pane;
use crate::colors;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .walkthrough
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let is_current = i == app.current_step;
            let is_visited = app.is_step_visited(i);

            let (indicator, indicator_color) = if is_visited {
                ("✓", colors::STEP_COMPLETED)
            } else {
                ("○", colors::STEP_PENDING)
            };

            let text_style = if is_current {
                Style::default().fg(colors::STEP_CURRENT).add_modifier(Modifier::BOLD)
            } else if is_visited {
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

    let is_active = app.layout.active_pane == Pane::Minimap;
    let borders = if app.layout.is_zoomed() {
        Borders::TOP | Borders::BOTTOM
    } else {
        Borders::TOP | Borders::LEFT | Borders::RIGHT
    };
    let block = pane_block(&title, borders, is_active);
    let list = List::new(items).block(block);

    frame.render_widget(list, area);
}
