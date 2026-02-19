use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Size;

use crate::app::{ActivePane, App, InputMode};
use crate::ui::diff_viewer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingKey {
    None,
    G,            // Waiting for second 'g' for gg
    CloseBracket, // Waiting for second ']' for ]]
    OpenBracket,  // Waiting for second '[' for [[
}

pub struct InputHandler {
    pending: PendingKey,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            pending: PendingKey::None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, viewport_height: usize) {
        // Handle Ctrl+C for quit in any mode
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            app.quit();
            return;
        }

        match app.input_mode {
            InputMode::Insert => self.handle_insert_mode(key, app),
            InputMode::Normal => self.handle_normal_mode(key, app, viewport_height),
        }
    }

    fn handle_insert_mode(&mut self, key: KeyEvent, app: &mut App) {
        match key.code {
            // Exit insert mode
            KeyCode::Esc => {
                app.exit_insert_mode();
            }

            // Submit message
            KeyCode::Enter => {
                app.send_message();
            }

            // Text editing
            KeyCode::Char(c) => {
                app.insert_char(c);
            }
            KeyCode::Backspace => {
                app.delete_char();
            }

            // Cursor movement
            KeyCode::Left => {
                app.move_cursor_left();
            }
            KeyCode::Right => {
                app.move_cursor_right();
            }
            KeyCode::Home => {
                app.move_cursor_to_start();
            }
            KeyCode::End => {
                app.move_cursor_to_end();
            }

            _ => {}
        }
    }

    fn handle_normal_mode(&mut self, key: KeyEvent, app: &mut App, viewport_height: usize) {
        // Handle pending key sequences first
        match self.pending {
            PendingKey::G => {
                self.pending = PendingKey::None;
                if key.code == KeyCode::Char('g') {
                    app.scroll_to_top();
                }
                return;
            }
            PendingKey::CloseBracket => {
                self.pending = PendingKey::None;
                if key.code == KeyCode::Char(']') {
                    app.next_step();
                }
                return;
            }
            PendingKey::OpenBracket => {
                self.pending = PendingKey::None;
                if key.code == KeyCode::Char('[') {
                    app.prev_step();
                }
                return;
            }
            PendingKey::None => {}
        }

        match key.code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => app.quit(),

            // Enter insert mode
            KeyCode::Char('i') => {
                app.enter_insert_mode();
            }

            // Pane navigation (Ctrl+h/j/k/l) - must come before plain j/k
            // Note: Ctrl+h often comes as Backspace, Ctrl+j as Enter in terminals
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.active_pane == ActivePane::Diff {
                    app.set_active_pane(ActivePane::Chat);
                }
            }
            KeyCode::Backspace if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+h fallback (terminals send this as Ctrl+Backspace)
                if app.active_pane == ActivePane::Diff {
                    app.set_active_pane(ActivePane::Chat);
                }
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.active_pane != ActivePane::Diff {
                    app.set_active_pane(ActivePane::Diff);
                }
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.active_pane == ActivePane::Minimap {
                    app.set_active_pane(ActivePane::Chat);
                }
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.active_pane == ActivePane::Chat {
                    app.set_active_pane(ActivePane::Minimap);
                }
            }

            // Tab/Shift+Tab as alternative pane navigation
            KeyCode::Tab => {
                match app.active_pane {
                    ActivePane::Minimap => app.set_active_pane(ActivePane::Chat),
                    ActivePane::Chat => app.set_active_pane(ActivePane::Diff),
                    ActivePane::Diff => app.set_active_pane(ActivePane::Minimap),
                }
            }
            KeyCode::BackTab => {
                match app.active_pane {
                    ActivePane::Minimap => app.set_active_pane(ActivePane::Diff),
                    ActivePane::Chat => app.set_active_pane(ActivePane::Minimap),
                    ActivePane::Diff => app.set_active_pane(ActivePane::Chat),
                }
            }

            // Scrolling diff
            KeyCode::Char('j') | KeyCode::Down => app.scroll_down(1),
            KeyCode::Char('k') | KeyCode::Up => app.scroll_up(1),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.scroll_down(viewport_height / 2);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.scroll_up(viewport_height / 2);
            }

            // Go to top (gg)
            KeyCode::Char('g') => {
                self.pending = PendingKey::G;
            }

            // Go to bottom (G)
            KeyCode::Char('G') => {
                let content_height = diff_viewer::content_height(app);
                app.scroll_to_bottom(content_height, viewport_height);
            }

            // Step navigation
            KeyCode::Char('n') => app.next_step(),
            KeyCode::Char('p') => app.prev_step(),
            KeyCode::Char(']') => {
                self.pending = PendingKey::CloseBracket;
            }

            // Complete step and advance (or finish walkthrough)
            KeyCode::Enter => app.complete_step_and_advance(),

            // Undo step completion
            KeyCode::Char('u') => app.uncomplete_step(),
            KeyCode::Char('[') => {
                self.pending = PendingKey::OpenBracket;
            }

            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, app: &mut App, size: Size) {
        let left_pane_width = (size.width as f32 * 0.30) as u16;
        let minimap_height = ((size.height - 1) as f32 * 0.40) as u16;

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if mouse.column < left_pane_width {
                    // Scroll in left pane - could be chat
                    if mouse.row > minimap_height {
                        app.scroll_chat_up(3);
                    }
                } else {
                    // Scroll in diff viewer
                    app.scroll_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if mouse.column < left_pane_width {
                    // Scroll in left pane - could be chat
                    if mouse.row > minimap_height {
                        app.scroll_chat_down(3);
                    }
                } else {
                    // Scroll in diff viewer
                    app.scroll_down(3);
                }
            }
            MouseEventKind::Down(_) => {
                if mouse.column < left_pane_width {
                    if mouse.row < minimap_height {
                        // Click in minimap - select step
                        app.set_active_pane(ActivePane::Minimap);
                        // Account for border (1) and padding, each item is 1 row
                        let clicked_row = mouse.row.saturating_sub(1) as usize;
                        if clicked_row < app.walkthrough.step_count() {
                            app.go_to_step(clicked_row);
                        }
                    } else {
                        // Click in chat area - enter insert mode
                        app.set_active_pane(ActivePane::Chat);
                        app.enter_insert_mode();
                    }
                } else {
                    // Click in diff viewer
                    app.set_active_pane(ActivePane::Diff);
                }
            }
            _ => {}
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}
