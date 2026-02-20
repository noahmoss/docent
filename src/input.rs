use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Size;
use tui_textarea::{CursorMove, Input};

use crate::app::App;
use crate::editor::VimInputMode;
use crate::layout::{Divider, Pane};
use crate::constants::{DIVIDER_HIT_ZONE, HELP_BAR_HEIGHT};
use crate::ui::diff_viewer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingKey {
    None,
    G, // Waiting for second 'g' for gg
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

        // Handle error state
        if app.is_error() {
            match key.code {
                KeyCode::Char('r') => app.request_retry(),
                KeyCode::Char('q') => app.quit(),
                _ => {}
            }
            return;
        }

        // When Chat pane is active, route to editor handling
        if app.layout.active_pane == Pane::Chat {
            self.handle_chat_input(key, app);
            return;
        }

        // Otherwise handle app-level navigation
        self.handle_normal_mode(key, app, viewport_height);
    }

    fn handle_chat_input(&mut self, key: KeyEvent, app: &mut App) {
        // Tab/Shift+Tab always switches panes
        if key.code == KeyCode::Tab {
            app.set_active_pane(Pane::Diff);
            return;
        }
        if key.code == KeyCode::BackTab {
            app.set_active_pane(Pane::Minimap);
            return;
        }

        if app.editor.vim_enabled {
            match app.editor.vim_mode {
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
            if app.editor.vim_enabled {
                app.editor.vim_mode = VimInputMode::Normal;
            } else {
                // In non-vim mode, Esc exits scrollback or switches pane
                if app.chat_scroll.in_scrollback() {
                    app.exit_chat_scrollback();
                } else {
                    app.set_active_pane(Pane::Diff);
                }
            }
            return;
        }

        // Ctrl+n/p for scrollback (works in both vim and non-vim mode)
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('n') => {
                    app.scroll_chat_down(1);
                    return;
                }
                KeyCode::Char('p') => {
                    app.scroll_chat_up(1);
                    return;
                }
                _ => {}
            }
        }

        // Enter submits, Shift+Enter for newline
        if key.code == KeyCode::Enter {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.editor.textarea.insert_newline();
            } else {
                app.send_message();
            }
            return;
        }

        // Any other input exits scrollback mode and goes to textarea
        app.exit_chat_scrollback();
        let input = Input::from(key);
        app.editor.textarea.input(input);
    }

    fn handle_vim_normal(&mut self, key: KeyEvent, app: &mut App) {
        if self.handle_vim_pending_operation(key, app) {
            return;
        }

        match key.code {
            KeyCode::Esc => app.exit_chat_scrollback(),
            KeyCode::Enter => app.send_message(),
            KeyCode::Tab => app.set_active_pane(app.layout.active_pane.next()),
            KeyCode::BackTab => app.set_active_pane(app.layout.active_pane.prev()),

            // Chat scroll (Ctrl+n/p)
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.scroll_chat_down(1);
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.scroll_chat_up(1);
            }

            // Insert mode commands
            KeyCode::Char('i') => app.editor.vim_mode = VimInputMode::Insert,
            KeyCode::Char('a') => {
                app.editor.textarea.move_cursor(CursorMove::Forward);
                app.editor.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('I') => {
                app.editor.textarea.move_cursor(CursorMove::Head);
                app.editor.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('A') => {
                app.editor.textarea.move_cursor(CursorMove::End);
                app.editor.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('o') => {
                app.editor.textarea.move_cursor(CursorMove::End);
                app.editor.textarea.insert_newline();
                app.editor.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('O') => {
                app.editor.textarea.move_cursor(CursorMove::Head);
                app.editor.textarea.insert_newline();
                app.editor.textarea.move_cursor(CursorMove::Up);
                app.editor.vim_mode = VimInputMode::Insert;
            }

            // Motion commands
            KeyCode::Char('h') | KeyCode::Left => app.editor.textarea.move_cursor(CursorMove::Back),
            KeyCode::Char('l') | KeyCode::Right => app.editor.textarea.move_cursor(CursorMove::Forward),
            KeyCode::Char('j') | KeyCode::Down => app.editor.textarea.move_cursor(CursorMove::Down),
            KeyCode::Char('k') | KeyCode::Up => app.editor.textarea.move_cursor(CursorMove::Up),
            KeyCode::Char('w') => app.editor.textarea.move_cursor(CursorMove::WordForward),
            KeyCode::Char('b') => app.editor.textarea.move_cursor(CursorMove::WordBack),
            KeyCode::Char('0') | KeyCode::Home => app.editor.textarea.move_cursor(CursorMove::Head),
            KeyCode::Char('$') | KeyCode::End => app.editor.textarea.move_cursor(CursorMove::End),
            KeyCode::Char('g') => app.editor.textarea.move_cursor(CursorMove::Top),
            KeyCode::Char('G') => app.editor.textarea.move_cursor(CursorMove::Bottom),

            // Edit commands
            KeyCode::Char('x') => { app.editor.textarea.delete_char(); }
            KeyCode::Char('d') => self.vim_pending = VimPending::D,
            KeyCode::Char('c') => self.vim_pending = VimPending::C,
            KeyCode::Char('D') => { app.editor.textarea.delete_line_by_end(); }
            KeyCode::Char('C') => {
                app.editor.textarea.delete_line_by_end();
                app.editor.vim_mode = VimInputMode::Insert;
            }
            KeyCode::Char('u') => { app.editor.textarea.undo(); }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.editor.textarea.redo();
            }

            // Toggle zoom mode
            KeyCode::Char('z') => app.layout.toggle_zoom(),

            _ => {}
        }
    }

    /// Handles pending vim operator (d/c) + motion combinations.
    /// Returns true if a pending operation was processed.
    fn handle_vim_pending_operation(&mut self, key: KeyEvent, app: &mut App) -> bool {
        match self.vim_pending {
            VimPending::None => false,
            VimPending::D => {
                self.vim_pending = VimPending::None;
                match key.code {
                    KeyCode::Char('d') => {
                        app.editor.textarea.move_cursor(CursorMove::Head);
                        app.editor.textarea.delete_line_by_end();
                    }
                    KeyCode::Char('w') => { app.editor.textarea.delete_next_word(); }
                    KeyCode::Char('b') => { app.editor.textarea.delete_word(); }
                    KeyCode::Char('$') => { app.editor.textarea.delete_line_by_end(); }
                    KeyCode::Char('0') => { app.editor.textarea.delete_line_by_head(); }
                    _ => {}
                }
                true
            }
            VimPending::C => {
                self.vim_pending = VimPending::None;
                let enter_insert = match key.code {
                    KeyCode::Char('c') => {
                        app.editor.textarea.move_cursor(CursorMove::Head);
                        app.editor.textarea.delete_line_by_end();
                        true
                    }
                    KeyCode::Char('w') => {
                        app.editor.textarea.delete_next_word();
                        true
                    }
                    KeyCode::Char('$') => {
                        app.editor.textarea.delete_line_by_end();
                        true
                    }
                    _ => false,
                };
                if enter_insert {
                    app.editor.vim_mode = VimInputMode::Insert;
                }
                true
            }
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
            PendingKey::None => {}
        }

        match key.code {
            // 'i' focuses Chat and enters insert mode
            KeyCode::Char('i') => {
                app.set_active_pane(Pane::Chat);
                app.editor.vim_mode = VimInputMode::Insert;
            }

            // Pane navigation (Ctrl+h/j/k/l) - must come before plain j/k
            // Note: Ctrl+h often comes as Backspace, Ctrl+j as Enter in terminals
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.layout.active_pane == Pane::Diff {
                    app.set_active_pane(Pane::Chat);
                }
            }
            KeyCode::Backspace if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+h fallback (terminals send this as Ctrl+Backspace)
                if app.layout.active_pane == Pane::Diff {
                    app.set_active_pane(Pane::Chat);
                }
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.layout.active_pane != Pane::Diff {
                    app.set_active_pane(Pane::Diff);
                }
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.layout.active_pane == Pane::Minimap {
                    app.set_active_pane(Pane::Chat);
                }
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if app.layout.active_pane == Pane::Chat {
                    app.set_active_pane(Pane::Minimap);
                }
            }

            // Tab/Shift+Tab as alternative pane navigation
            KeyCode::Tab => app.set_active_pane(app.layout.active_pane.next()),
            KeyCode::BackTab => app.set_active_pane(app.layout.active_pane.prev()),

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

            // Complete step and advance (or finish walkthrough)
            KeyCode::Enter => app.complete_step_and_advance(),

            // Toggle step reviewed
            KeyCode::Char('x') => app.toggle_step_reviewed(),

            // Toggle zoom mode
            KeyCode::Char('z') => app.layout.toggle_zoom(),

            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, app: &mut App, size: Size) {
        let content_height = size.height.saturating_sub(HELP_BAR_HEIGHT);
        let left_pane_width = (size.width as u32 * app.layout.left_pane_percent as u32 / 100) as u16;
        let minimap_height = (content_height as u32 * app.layout.minimap_percent as u32 / 100) as u16;

        let near_vertical_divider = mouse.column.abs_diff(left_pane_width) <= DIVIDER_HIT_ZONE;
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
                        app.set_active_pane(Pane::Minimap);
                        // Account for border (1) and padding, each item is 1 row
                        let clicked_row = mouse.row.saturating_sub(1) as usize;
                        if clicked_row < app.walkthrough.step_count() {
                            app.go_to_step(clicked_row);
                        }
                    } else {
                        // Click in chat area - focus and enter vim insert
                        app.set_active_pane(Pane::Chat);
                        app.editor.vim_mode = VimInputMode::Insert;
                    }
                } else {
                    // Click in diff viewer
                    app.set_active_pane(Pane::Diff);
                }
            }
            MouseEventKind::Drag(_) => {
                if let Some(divider) = app.layout.dragging {
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
                    if mouse.row >= minimap_height {
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
                    if mouse.row >= minimap_height {
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
