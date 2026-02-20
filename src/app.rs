use std::cell::Cell;
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
    // Chat state - Some(step_index) when waiting for response
    pub chat_pending: Option<usize>,
    // Pending chat request to be processed by main loop (step_index, walkthrough, messages)
    pub chat_request: Option<(usize, Walkthrough, Vec<Message>)>,
    // When true, user is in scrollback mode (manual scroll, no auto-follow)
    pub chat_scrollback_mode: bool,
    // Last known max scroll value (updated by render via Cell for interior mutability)
    pub chat_max_scroll: Cell<usize>,
    // Set to true when user requests retry from error state
    pub retry_requested: bool,
    // Original diff input for retry
    pub diff_input: Option<String>,
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
            chat_pending: None,
            chat_request: None,
            chat_scrollback_mode: false,
            chat_max_scroll: Cell::new(0),
            retry_requested: false,
            diff_input: None,
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
            chat_pending: None,
            chat_request: None,
            chat_scrollback_mode: false,
            chat_max_scroll: Cell::new(0),
            retry_requested: false,
            diff_input: None,
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

    /// Check if in error state
    pub fn is_error(&self) -> bool {
        matches!(self.state, AppState::Error { .. })
    }

    /// Request retry - transitions back to loading state
    pub fn request_retry(&mut self) {
        self.state = AppState::Loading {
            status: "Retrying...".to_string(),
            steps_received: 0,
        };
        self.retry_requested = true;
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

    pub fn toggle_step_reviewed(&mut self) {
        if let Some(visited) = self.visited_steps.get_mut(self.current_step) {
            *visited = !*visited;
        }
        self.walkthrough_complete = false;
    }

    pub fn is_walkthrough_complete(&self) -> bool {
        self.walkthrough_complete && self.visited_steps.iter().all(|&v| v)
    }

    /// Count diff lines in a step
    fn step_diff_lines(step: &crate::model::Step) -> usize {
        step.hunks.iter().map(|h| h.content.lines().count()).sum()
    }

    /// Total diff lines across all steps
    pub fn total_diff_lines(&self) -> usize {
        self.walkthrough.steps.iter().map(Self::step_diff_lines).sum()
    }

    /// Diff lines in completed steps
    pub fn reviewed_diff_lines(&self) -> usize {
        self.walkthrough
            .steps
            .iter()
            .enumerate()
            .filter(|(i, _)| self.visited_steps.get(*i).copied().unwrap_or(false))
            .map(|(_, step)| Self::step_diff_lines(step))
            .sum()
    }

    pub fn scroll_down(&mut self, amount: usize, viewport_height: usize) {
        let content_lines = self.diff_content_lines();
        let max_scroll = content_lines.saturating_sub(viewport_height);
        self.diff_scroll = self.diff_scroll.saturating_add(amount).min(max_scroll);
    }

    /// Calculate total lines of diff content for current step
    fn diff_content_lines(&self) -> usize {
        if let Some(step) = self.current_step_data() {
            let mut count = 0;
            for hunk in &step.hunks {
                count += 2; // header + blank line
                count += hunk.content.lines().count();
                count += 1; // trailing blank
            }
            count
        } else {
            1
        }
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

    #[allow(dead_code)]
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

    /// Sends the current message and queues a chat request for the main loop.
    pub fn send_message(&mut self) {
        let content: String = self.textarea.lines().join("\n");
        if content.trim().is_empty() {
            return;
        }

        // Don't allow sending while a chat is pending
        if self.chat_pending.is_some() {
            return;
        }

        // Clear the textarea
        self.textarea.select_all();
        self.textarea.delete_char();

        let step_index = self.current_step;
        if let Some(step) = self.walkthrough.steps.get_mut(step_index) {
            step.messages.push(Message::user(content));
            let messages_clone = step.messages.clone();
            // Clone walkthrough for context (includes all steps and their conversations)
            let walkthrough_clone = self.walkthrough.clone();
            // Set pending state and queue request
            self.chat_pending = Some(step_index);
            self.chat_request = Some((step_index, walkthrough_clone, messages_clone));
        }
    }

    /// Called when a streaming chat chunk is received
    pub fn receive_chat_chunk(&mut self, step_index: usize, chunk: String) {
        if self.chat_pending != Some(step_index) {
            return;
        }
        if let Some(step) = self.walkthrough.steps.get_mut(step_index) {
            // Check if we already have an assistant message being built
            if let Some(last_msg) = step.messages.last_mut() {
                if last_msg.role == crate::model::MessageRole::Assistant {
                    // Append to existing message
                    last_msg.content.push_str(&chunk);
                } else {
                    // First chunk - create new assistant message
                    step.messages.push(Message::assistant(chunk));
                }
            } else {
                // First chunk - create new assistant message
                step.messages.push(Message::assistant(chunk));
            }
            // Render will auto-scroll if not in scrollback mode
        }
    }

    /// Called when streaming chat completes
    pub fn receive_chat_complete(&mut self, step_index: usize) {
        if self.chat_pending == Some(step_index) {
            self.chat_pending = None;
        }
    }

    /// Called when chat request fails
    pub fn receive_chat_error(&mut self, step_index: usize, error: String) {
        if self.chat_pending == Some(step_index) {
            self.chat_pending = None;
            if let Some(step) = self.walkthrough.steps.get_mut(step_index) {
                step.messages.push(Message::assistant(format!("Error: {}", error)));
            }
        }
    }

    pub fn textarea_is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Scroll up (towards older content). chat_scroll = lines from bottom.
    pub fn scroll_chat_up(&mut self, amount: usize) {
        let max = self.chat_max_scroll.get();
        if max == 0 {
            return; // Nothing to scroll
        }
        self.chat_scrollback_mode = true;
        self.chat_scroll = self.chat_scroll.saturating_add(amount).min(max);
    }

    /// Scroll down (towards newer content).
    pub fn scroll_chat_down(&mut self, amount: usize) {
        self.chat_scroll = self.chat_scroll.saturating_sub(amount);
        // If we've scrolled back to bottom, exit scrollback mode
        if self.chat_scroll == 0 {
            self.chat_scrollback_mode = false;
        }
    }

    pub fn exit_chat_scrollback(&mut self) {
        self.chat_scrollback_mode = false;
        self.chat_scroll = 0; // Back to bottom
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
