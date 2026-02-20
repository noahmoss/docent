use tui_textarea::TextArea;

use crate::model::{Message, Walkthrough};
use crate::settings::Settings;

/// Vim mode state for the text editor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimInputMode {
    Normal,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Minimap,
    Chat,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    Vertical,   // Between left pane and diff viewer
    Horizontal, // Between minimap and chat
}

/// Application state machine
#[derive(Debug, Clone)]
pub enum AppState {
    /// Generating walkthrough from diff
    Loading {
        status: String,
        steps_received: usize,
    },
    /// Walkthrough is ready for viewing
    Ready,
    /// An error occurred
    Error { message: String },
}

impl Default for AppState {
    fn default() -> Self {
        Self::Ready
    }
}

pub struct App<'a> {
    pub state: AppState,
    pub walkthrough: Walkthrough,
    pub current_step: usize,
    pub visited_steps: Vec<bool>,
    pub diff_scroll: usize,
    pub chat_scroll: usize,
    pub should_quit: bool,
    pub quit_pending: bool,
    pub active_pane: ActivePane,
    pub walkthrough_complete: bool,
    // Pane layout (percentages)
    pub left_pane_percent: u16,
    pub minimap_percent: u16,
    // Drag state
    pub dragging: Option<Divider>,
    // Text editor
    pub textarea: TextArea<'a>,
    pub vim_enabled: bool,
    pub vim_mode: VimInputMode,
}

impl<'a> App<'a> {
    pub fn new(walkthrough: Walkthrough, settings: &Settings) -> Self {
        let step_count = walkthrough.step_count();
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(ratatui::style::Style::default());

        Self {
            state: AppState::Ready,
            walkthrough,
            current_step: 0,
            visited_steps: vec![false; step_count],
            diff_scroll: 0,
            chat_scroll: 0,
            should_quit: false,
            quit_pending: false,
            active_pane: ActivePane::Diff,
            walkthrough_complete: false,
            left_pane_percent: 50,
            minimap_percent: 40,
            dragging: None,
            textarea,
            vim_enabled: settings.vim_enabled(),
            vim_mode: VimInputMode::Normal,
        }
    }

    /// Create an app in loading state (empty walkthrough)
    pub fn loading(settings: &Settings) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(ratatui::style::Style::default());

        Self {
            state: AppState::Loading {
                status: "Initializing...".to_string(),
                steps_received: 0,
            },
            walkthrough: Walkthrough { steps: vec![] },
            current_step: 0,
            visited_steps: vec![],
            diff_scroll: 0,
            chat_scroll: 0,
            should_quit: false,
            quit_pending: false,
            active_pane: ActivePane::Diff,
            walkthrough_complete: false,
            left_pane_percent: 50,
            minimap_percent: 40,
            dragging: None,
            textarea,
            vim_enabled: settings.vim_enabled(),
            vim_mode: VimInputMode::Normal,
        }
    }

    /// Transition to ready state with the given walkthrough
    pub fn set_ready(&mut self, walkthrough: Walkthrough) {
        let step_count = walkthrough.step_count();
        self.walkthrough = walkthrough;
        self.visited_steps = vec![false; step_count];
        self.current_step = 0;
        self.state = AppState::Ready;
    }

    /// Transition to error state
    pub fn set_error(&mut self, message: String) {
        self.state = AppState::Error { message };
    }

    /// Update loading status
    pub fn set_loading_status(&mut self, status: String) {
        if let AppState::Loading { steps_received, .. } = &self.state {
            self.state = AppState::Loading {
                status,
                steps_received: *steps_received,
            };
        }
    }

    /// Add a step during loading (for streaming)
    #[allow(dead_code)]
    pub fn add_step(&mut self, step: crate::model::Step) {
        self.walkthrough.steps.push(step);
        self.visited_steps.push(false);
        if let AppState::Loading { status, .. } = &self.state {
            self.state = AppState::Loading {
                status: status.clone(),
                steps_received: self.walkthrough.steps.len(),
            };
        }
    }

    pub fn next_step(&mut self) {
        if self.current_step < self.walkthrough.step_count().saturating_sub(1) {
            self.current_step += 1;
            self.diff_scroll = 0;
            self.chat_scroll = 0;
        }
    }

    pub fn prev_step(&mut self) {
        if self.current_step > 0 {
            self.current_step -= 1;
            self.diff_scroll = 0;
            self.chat_scroll = 0;
        }
        self.walkthrough_complete = false;
    }

    pub fn go_to_step(&mut self, index: usize) {
        if index < self.walkthrough.step_count() && index != self.current_step {
            self.current_step = index;
            self.diff_scroll = 0;
            self.chat_scroll = 0;
            self.walkthrough_complete = false;
        }
    }

    pub fn complete_step_and_advance(&mut self) {
        // Mark current step as completed
        if let Some(visited) = self.visited_steps.get_mut(self.current_step) {
            *visited = true;
        }

        // If not on last step, advance
        if self.current_step < self.walkthrough.step_count().saturating_sub(1) {
            self.current_step += 1;
            self.diff_scroll = 0;
            self.chat_scroll = 0;
        } else {
            // On last step - show completion message
            self.walkthrough_complete = true;
        }
    }

    pub fn uncomplete_step(&mut self) {
        if let Some(visited) = self.visited_steps.get_mut(self.current_step) {
            *visited = false;
        }
    }

    pub fn is_walkthrough_complete(&self) -> bool {
        self.walkthrough_complete && self.visited_steps.iter().all(|&v| v)
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.diff_scroll = self.diff_scroll.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.diff_scroll = self.diff_scroll.saturating_sub(amount);
    }

    pub fn scroll_to_top(&mut self) {
        self.diff_scroll = 0;
    }

    pub fn scroll_to_bottom(&mut self, content_height: usize, viewport_height: usize) {
        if content_height > viewport_height {
            self.diff_scroll = content_height - viewport_height;
        }
    }

    pub fn current_step_data(&self) -> Option<&crate::model::Step> {
        self.walkthrough.get_step(self.current_step)
    }

    pub fn current_step_data_mut(&mut self) -> Option<&mut crate::model::Step> {
        self.walkthrough.steps.get_mut(self.current_step)
    }

    pub fn set_active_pane(&mut self, pane: ActivePane) {
        // If leaving chat, reset to vim normal mode
        if self.active_pane == ActivePane::Chat && pane != ActivePane::Chat {
            self.vim_mode = VimInputMode::Normal;
        }
        self.active_pane = pane;
    }

    pub fn send_message(&mut self) {
        let content: String = self.textarea.lines().join("\n");
        if content.trim().is_empty() {
            return;
        }

        // Clear the textarea
        self.textarea.select_all();
        self.textarea.delete_char();

        if let Some(step) = self.current_step_data_mut() {
            step.messages.push(Message::user(content));
            // Placeholder response - will be replaced with actual LLM call
            step.messages.push(Message::assistant(
                "I'm not connected to an LLM yet, but I'll be able to answer your questions soon!"
            ));
        }

        // Stay in insert mode after sending
    }

    pub fn textarea_is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    pub fn scroll_chat_up(&mut self, amount: usize) {
        self.chat_scroll = self.chat_scroll.saturating_sub(amount);
    }

    pub fn scroll_chat_down(&mut self, amount: usize) {
        self.chat_scroll = self.chat_scroll.saturating_add(amount);
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn set_left_pane_percent(&mut self, percent: u16) {
        // Clamp to reasonable bounds (20-80%)
        self.left_pane_percent = percent.clamp(20, 80);
    }

    pub fn set_minimap_percent(&mut self, percent: u16) {
        // Clamp to reasonable bounds (15-85%)
        self.minimap_percent = percent.clamp(15, 85);
    }

    pub fn start_drag(&mut self, divider: Divider) {
        self.dragging = Some(divider);
    }

    pub fn stop_drag(&mut self) {
        self.dragging = None;
    }
}
