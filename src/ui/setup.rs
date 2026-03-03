use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::app::{App, SetupFocus};
use crate::constants::{SETUP_DIALOG_HEIGHT, SETUP_DIALOG_WIDTH};
use crate::model::ReviewMode;
use crate::settings::ApiKeySource;

use super::centered_rect;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let dialog_area = centered_rect(SETUP_DIALOG_WIDTH, SETUP_DIALOG_HEIGHT, area);
    let block = Block::default()
        .title(" docent ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .padding(Padding::new(2, 2, 1, 1));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Subtitle
            Constraint::Length(1), // Mode label
            Constraint::Length(2), // Mode options
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // API key label
            Constraint::Length(1), // API key value
            Constraint::Min(1),    // Spacer
            Constraint::Length(1), // Help bar
        ])
        .split(inner);

    // Subtitle
    let subtitle = Paragraph::new(Line::from(Span::styled(
        "AI-guided code review walkthrough",
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(subtitle, sections[0]);

    render_mode_section(frame, &sections, app);
    render_api_key_section(frame, &sections, app);

    // Help bar
    let mut help_spans = vec![
        Span::styled(" Enter ", Style::default().fg(Color::Yellow)),
        Span::raw("start "),
    ];
    if app.session.api_key_source != ApiKeySource::EnvVar {
        help_spans.extend([
            Span::styled(" Tab ", Style::default().fg(Color::Yellow)),
            Span::raw("switch "),
        ]);
    }
    help_spans.extend([
        Span::styled(" q ", Style::default().fg(Color::Yellow)),
        Span::raw("quit"),
    ]);
    let help_line = Paragraph::new(Line::from(help_spans)).alignment(Alignment::Center);
    frame.render_widget(help_line, sections[7]);
}

fn render_mode_section(frame: &mut Frame, sections: &[Rect], app: &App) {
    let mode_focused = matches!(
        app.setup_focus,
        SetupFocus::Review | SetupFocus::Walkthrough
    );
    let mode_label_color = if mode_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let mode_label = Paragraph::new(Line::from(Span::styled(
        "Mode",
        Style::default()
            .fg(mode_label_color)
            .add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(mode_label, sections[1]);

    let review_selected = app.session.review_mode == ReviewMode::Review;
    let (review_bullet, walk_bullet) = if review_selected {
        ("●", "○")
    } else {
        ("○", "●")
    };

    let review_style = if review_selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let walk_style = if !review_selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let review_cursor = if app.setup_focus == SetupFocus::Review {
        "> "
    } else {
        "  "
    };
    let walk_cursor = if app.setup_focus == SetupFocus::Walkthrough {
        "> "
    } else {
        "  "
    };

    let mode_lines = vec![
        Line::from(vec![
            Span::styled(review_cursor, Style::default().fg(Color::Cyan)),
            Span::styled(format!("{review_bullet} Review"), review_style),
            Span::styled(
                "       Call out potential issues",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(walk_cursor, Style::default().fg(Color::Cyan)),
            Span::styled(format!("{walk_bullet} Walkthrough"), walk_style),
            Span::styled(
                "  Describe the changes",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];
    let mode_widget = Paragraph::new(mode_lines);
    frame.render_widget(mode_widget, sections[2]);
}

fn render_api_key_section(frame: &mut Frame, sections: &[Rect], app: &App) {
    let key_focused = app.setup_focus == SetupFocus::ApiKey;
    let key_label_color = if key_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let key_label = Paragraph::new(Line::from(Span::styled(
        "API Key",
        Style::default()
            .fg(key_label_color)
            .add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(key_label, sections[4]);

    let key_line = match app.session.api_key_source {
        ApiKeySource::Missing if !key_focused => Line::from(Span::styled(
            "  No API key found",
            Style::default().fg(Color::Red),
        )),
        _ if key_focused
            && matches!(
                app.session.api_key_source,
                ApiKeySource::Missing | ApiKeySource::UserEntry
            ) =>
        {
            let display = if app.session.api_key_input.is_empty() {
                "sk-ant-...".to_string()
            } else {
                app.session.api_key_input.clone()
            };
            let style = if app.session.api_key_input.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Cyan)),
                Span::styled(display, style),
                Span::styled("█", Style::default().fg(Color::White)),
            ])
        }
        _ => {
            let masked = mask_api_key(&app.session.api_key_input);
            let source_label = match app.session.api_key_source {
                ApiKeySource::EnvVar => " ✓ from env",
                ApiKeySource::Settings => " ✓ saved",
                ApiKeySource::UserEntry => " ✓ entered",
                ApiKeySource::Missing => "",
            };
            Line::from(vec![
                Span::raw("  "),
                Span::styled(masked, Style::default().fg(Color::Green)),
                Span::styled(source_label, Style::default().fg(Color::DarkGray)),
            ])
        }
    };
    frame.render_widget(Paragraph::new(key_line), sections[5]);
}

fn mask_api_key(key: &str) -> String {
    if key.len() <= 12 {
        return "*".repeat(key.len());
    }
    let prefix = &key[..10];
    format!("{prefix}...****")
}
