use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Size;
use tui_textarea::{CursorMove, Input};

use crate::app::{ActivePane, App, Divider, VimInputMode};
use crate::ui::diff_viewer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingKey {
    None,
    G,            // Waiting for second 'g' for gg
    CloseBracket, // Waiting for second ']' for ]]
    OpenBracket,  // Waiting for second '[' for [[
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VimPending {
    None,
    D, // Waiting for motion after 'd'
    C, // Waiting for motion after 'c'
}

pub struct InputHandler {
    pending: PendingKey,
    vim_pending: VimPending,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            pending: PendingKey::None,
            vim_pending: VimPending::None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, app: &mut App, viewport_height: usize) {
        // Handle Ctrl+C for quit (requires confirmation)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if app.quit_pending {
                app.quit();
            } else {
                app.quit_pending = true;
            }
            return;
        }

        // Any other key clears quit pending
        app.quit_pending = false;

        // When Chat pane is active, route to editor handling
        if app.active_pane == ActivePane::Chat {
            self.handle_chat_input(key, app);
            return;
        }

        // Otherwise handle app-level navigation
        self.handle_normal_mode(key, app, viewport_height);
    }

    fn handle_chat_input(&mut self, key: KeyEvent, app: &mut App) {
        // Tab/Shift+Tab always switches panes
        if key.code == KeyCode::Tab {
            app.set_active_pane(ActivePane::Diff);
            return;
        }
        if key.code == KeyCode::BackTab {
            app.set_active_pane(ActivePane::Minimap);
            return;
        }

        if app.vim_enabled {
            match app.vim_mode {
                VimInputMode::Normal => self.handle_vim_normal(key, app),
                VimInputMode::Insert => self.handle_vim_insert(key, app),
            }
        } else {
            // Non-vim: always in insert mode for the textarea
            self.handle_vim_insert(key, app);
        }
    }

    fn handle_vim_insert(&mut self, key: KeyEvent, app: &mut App) {
        // Escape goes to vim normal mode (or exits Chat focus if vim disabled)
        if key.code == KeyCode::Esc {
            if app.vim_enabled {
                app.vim_mode = VimInputMode::Normal;
            } else {
                app.set_active_pane(ActivePane::Diff);
            }
            return;
        }

        // Enter submits, Shift+Enter for newline
        if key.code == KeyCode::Enter {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.textarea.insert_newline();
            } else {
                app.send_message();
            }
            return;
        }

        // Forward to textarea (handles readline-style bindings by default)
        let input = Input::from(key);
        app.textarea.input(input);
    }

    fn handle_vim_normal(&mut self, key: KeyEvent, app: &mut App) {
        // Handle pending operations (d, c)
        match self.vim_pending {
            VimPending::D => {
                self.vim_pending = VimPending::None;
                match key.code {
                    KeyCode::Char('d') => {
                        // dd - delete line
                        app.textarea.move_cursor(CursorMove::Head);
                        app.textarea.delete_line_by_end();
                    }
                    KeyCode::Char('w') => {
                        // dw - delete word
                        app.textarea.delete_next_word();
                    }
                    KeyCode::Char('b') => {
                        // db - delete back word
                        app.textarea.delete_word();
                    }
                    KeyCode::Char('$') => {
                        // d$ - delete to end of line
                        app.textarea.delete_line_by_end();
                    }
                    KeyCode::Char('0') => {
                        // d0 - delete to start of line
                        app.textarea.delete_line_by_head();
                    }
                    _ => {}
                }
                return;
            }
            VimPending::C => {
                self.vim_pending = VimPending::None;
                match key.code {
                    KeyCode::Char('c') => {
                        // cc - change line
                        app.textarea.move_cursor(CursorMove::Head);
                        app.textarea.delete_line_by_end();
                        app.vim_mode = VimInputMode::Insert;
                    }
                    KeyCode::Char('w') => {
                        // cw - change word
                        app.textarea.delete_next_word();
                        app.vim_mode = VimInputMode::Insert;
                    }
                    KeyCode::Char('$') => {
                        // c$ - change to end of line
                        app.textarea.delete_line_by_end();
                        app.vim_mode = VimInputMode::Insert;
                    }
                    _ => {}
                }
                return;
            }
            VimPending::None => {}
        }

        match key.code {
            // Escape does nothing in vim normal (already in normal mode)
            KeyCode::Esc => {}

            // Enter insert mode
            KeyCode::Char('i') => {
                app.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('a') => {
                app.textarea.move_cursor(CursorMove::Forward);
                app.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('I') => {
                app.textarea.move_cursor(CursorMove::Head);
                app.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('A') => {
                app.textarea.move_cursor(CursorMove::End);
                app.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('o') => {
                app.textarea.move_cursor(CursorMove::End);
                app.textarea.insert_newline();
                app.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('O') => {
                app.textarea.move_cursor(CursorMove::Head);
                app.textarea.insert_newline();
                app.textarea.move_cursor(CursorMove::Up);
                app.vim_mode = VimInputMode::Insert;
            }

            // Motion
            KeyCode::Char('h') | KeyCode::Left => {
                app.textarea.move_cursor(CursorMove::Back);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                app.textarea.move_cursor(CursorMove::Forward);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.textarea.move_cursor(CursorMove::Down);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.textarea.move_cursor(CursorMove::Up);
            }
            KeyCode::Char('w') => {
                app.textarea.move_cursor(CursorMove::WordForward);
            }
            KeyCode::Char('b') => {
                app.textarea.move_cursor(CursorMove::WordBack);
            }
            KeyCode::Char('0') | KeyCode::Home => {
                app.textarea.move_cursor(CursorMove::Head);
            }
            KeyCode::Char('$') | KeyCode::End => {
                app.textarea.move_cursor(CursorMove::End);
            }
            KeyCode::Char('g') => {
                // gg - go to top (simple version, just goes to top)
                app.textarea.move_cursor(CursorMove::Top);
            }
            KeyCode::Char('G') => {
                app.textarea.move_cursor(CursorMove::Bottom);
            }

            // Delete/change
            KeyCode::Char('x') => {
                app.textarea.delete_char();
            }
            KeyCode::Char('d') => {
                self.vim_pending = VimPending::D;
            }
            KeyCode::Char('c') => {
                self.vim_pending = VimPending::C;
            }
            KeyCode::Char('D') => {
                app.textarea.delete_line_by_end();
            }
            KeyCode::Char('C') => {
                app.textarea.delete_line_by_end();
                app.vim_mode = VimInputMode::Insert;
            }

            // Undo/redo
            KeyCode::Char('u') => {
                app.textarea.undo();
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.textarea.redo();
            }

            // Submit with Enter in vim normal mode
            KeyCode::Enter => {
                app.send_message();
            }

            // Pane navigation
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
            // 'i' focuses Chat and enters insert mode
            KeyCode::Char('i') => {
                app.set_active_pane(ActivePane::Chat);
                app.vim_mode = VimInputMode::Insert;
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
        // Calculate divider positions based on current pane percentages
        // Account for help bar at bottom (height - 1)
        let content_height = size.height.saturating_sub(1);
        let left_pane_width = (size.width as u32 * app.left_pane_percent as u32 / 100) as u16;
        let minimap_height = (content_height as u32 * app.minimap_percent as u32 / 100) as u16;

        // Divider hit zones (2 pixels on each side)
        let near_vertical_divider = mouse.column.abs_diff(left_pane_width) <= 2;
        let near_horizontal_divider =
            mouse.column < left_pane_width && mouse.row.abs_diff(minimap_height) <= 1;

        match mouse.kind {
            MouseEventKind::Down(_) => {
                if near_vertical_divider {
                    app.start_drag(Divider::Vertical);
                } else if near_horizontal_divider {
                    app.start_drag(Divider::Horizontal);
                } else if mouse.column < left_pane_width {
                    if mouse.row < minimap_height {
                        // Click in minimap - select step
                        app.set_active_pane(ActivePane::Minimap);
                        // Account for border (1) and padding, each item is 1 row
                        let clicked_row = mouse.row.saturating_sub(1) as usize;
                        if clicked_row < app.walkthrough.step_count() {
                            app.go_to_step(clicked_row);
                        }
                    } else {
                        // Click in chat area - focus and enter vim insert
                        app.set_active_pane(ActivePane::Chat);
                        app.vim_mode = VimInputMode::Insert;
                    }
                } else {
                    // Click in diff viewer
                    app.set_active_pane(ActivePane::Diff);
                }
            }
            MouseEventKind::Drag(_) => {
                if let Some(divider) = app.dragging {
                    match divider {
                        Divider::Vertical => {
                            let new_percent = (mouse.column as u32 * 100 / size.width as u32) as u16;
                            app.set_left_pane_percent(new_percent);
                        }
                        Divider::Horizontal => {
                            let new_percent =
                                (mouse.row as u32 * 100 / content_height as u32) as u16;
                            app.set_minimap_percent(new_percent);
                        }
                    }
                }
            }
            MouseEventKind::Up(_) => {
                app.stop_drag();
            }
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
            _ => {}
        }
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}
