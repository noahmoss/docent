use tui_textarea::TextArea;

/// Vim mode state for the text editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimInputMode {
    Normal,
    Insert,
}

/// Text editor state (vim mode + textarea)
pub struct Editor<'a> {
    pub textarea: TextArea<'a>,
    pub vim_enabled: bool,
    pub vim_mode: VimInputMode,
}

impl<'a> Editor<'a> {
    pub fn new(vim_enabled: bool) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        Self {
            textarea,
            vim_enabled,
            vim_mode: VimInputMode::Normal,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Reset to normal mode (called when leaving chat pane)
    pub fn reset_mode(&mut self) {
        self.vim_mode = VimInputMode::Normal;
    }
}
