use crate::diff::FileFilter;
use crate::editor::Editor;
use crate::layout::{Divider, Layout, Pane};
use crate::model::{Message, Walkthrough};
use crate::scroll::{ChatScroll, DiffScroll};
use crate::settings::Settings;

/// Application state machine
#[derive(Debug, Clone, Default)]
pub enum AppState {
    /// Generating walkthrough from diff
    Loading {
        status: String,
        steps_received: usize,
    },
    /// Walkthrough is ready for viewing
    #[default]
    Ready,
    /// An error occurred
    Error { message: String },
}

pub struct App<'a> {
    pub state: AppState,
    pub walkthrough: Walkthrough,
    pub current_step: usize,
    pub visited_steps: Vec<bool>,
    pub walkthrough_complete: bool,
    pub diff_scroll: DiffScroll,
    pub chat_scroll: ChatScroll,
    pub layout: Layout,
    pub editor: Editor<'a>,
    pub should_quit: bool,
    pub quit_pending: bool,
    // Chat state - Some(step_index) when waiting for response
    pub chat_pending: Option<usize>,
    // Pending chat request to be processed by main loop (step_index, walkthrough, messages)
    pub chat_request: Option<(usize, Walkthrough, Vec<Message>)>,
    // Set to true when user requests retry from error state
    pub retry_requested: bool,
    // Original diff input for retry
    pub diff_input: Option<String>,
    // File filter for retry
    pub diff_filter: FileFilter,
}

impl<'a> App<'a> {
    pub fn new(walkthrough: Walkthrough, settings: &Settings) -> Self {
        let step_count = walkthrough.step_count();
        Self {
            state: AppState::Ready,
            walkthrough,
            current_step: 0,
            visited_steps: vec![false; step_count],
            walkthrough_complete: false,
            diff_scroll: DiffScroll::new(),
            chat_scroll: ChatScroll::new(),
            layout: Layout::default(),
            editor: Editor::new(settings.vim_enabled()),
            should_quit: false,
            quit_pending: false,
            chat_pending: None,
            chat_request: None,
            retry_requested: false,
            diff_input: None,
            diff_filter: FileFilter::default(),
        }
    }

    /// Create an app in loading state (empty walkthrough)
    pub fn loading(settings: &Settings) -> Self {
        Self {
            state: AppState::Loading {
                status: "Initializing...".to_string(),
                steps_received: 0,
            },
            walkthrough: Walkthrough { steps: vec![] },
            current_step: 0,
            visited_steps: vec![],
            walkthrough_complete: false,
            diff_scroll: DiffScroll::new(),
            chat_scroll: ChatScroll::new(),
            layout: Layout::default(),
            editor: Editor::new(settings.vim_enabled()),
            should_quit: false,
            quit_pending: false,
            chat_pending: None,
            chat_request: None,
            retry_requested: false,
            diff_input: None,
            diff_filter: FileFilter::default(),
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
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        }
    }

    pub fn prev_step(&mut self) {
        if self.current_step > 0 {
            self.current_step -= 1;
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        }
        self.walkthrough_complete = false;
    }

    pub fn go_to_step(&mut self, index: usize) {
        if index < self.walkthrough.step_count() && index != self.current_step {
            self.current_step = index;
            self.diff_scroll.reset();
            self.chat_scroll.reset();
            self.walkthrough_complete = false;
        }
    }

    pub fn complete_step_and_advance(&mut self) {
        self.set_step_visited(self.current_step, true);

        if self.current_step < self.walkthrough.step_count().saturating_sub(1) {
            self.current_step += 1;
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        } else {
            self.walkthrough_complete = true;
        }
    }

    pub fn toggle_step_reviewed(&mut self) {
        let current = self.is_step_visited(self.current_step);
        self.set_step_visited(self.current_step, !current);
        self.walkthrough_complete = false;
    }

    pub fn is_walkthrough_complete(&self) -> bool {
        self.walkthrough_complete && self.visited_steps.iter().all(|&v| v)
    }

    pub fn is_step_visited(&self, index: usize) -> bool {
        self.visited_steps.get(index).copied().unwrap_or(false)
    }

    pub fn set_step_visited(&mut self, index: usize, visited: bool) {
        if let Some(v) = self.visited_steps.get_mut(index) {
            *v = visited;
        }
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
            .filter(|(i, _)| self.is_step_visited(*i))
            .map(|(_, step)| Self::step_diff_lines(step))
            .sum()
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.diff_scroll.add(amount);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.diff_scroll.sub(amount);
    }

    pub fn scroll_to_top(&mut self) {
        self.diff_scroll.reset();
    }

    pub fn scroll_to_bottom(&mut self, content_height: usize, viewport_height: usize) {
        if content_height > viewport_height {
            self.diff_scroll.set(content_height - viewport_height);
        }
    }

    pub fn current_step_data(&self) -> Option<&crate::model::Step> {
        self.walkthrough.get_step(self.current_step)
    }

    #[allow(dead_code)]
    pub fn current_step_data_mut(&mut self) -> Option<&mut crate::model::Step> {
        self.walkthrough.steps.get_mut(self.current_step)
    }

    pub fn set_active_pane(&mut self, pane: Pane) {
        // If leaving chat, reset to vim normal mode
        if self.layout.active_pane == Pane::Chat && pane != Pane::Chat {
            self.editor.reset_mode();
        }
        self.layout.active_pane = pane;
    }

    /// Sends the current message and queues a chat request for the main loop.
    pub fn send_message(&mut self) {
        let content: String = self.editor.textarea.lines().join("\n");
        if content.trim().is_empty() {
            return;
        }

        // Don't allow sending while a chat is pending
        if self.chat_pending.is_some() {
            return;
        }

        // Clear the textarea
        self.editor.textarea.select_all();
        self.editor.textarea.delete_char();

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
        self.editor.is_empty()
    }

    pub fn scroll_chat_up(&mut self, amount: usize) {
        self.chat_scroll.scroll_up(amount);
    }

    pub fn scroll_chat_down(&mut self, amount: usize) {
        self.chat_scroll.scroll_down(amount);
    }

    pub fn exit_chat_scrollback(&mut self) {
        self.chat_scroll.jump_to_bottom();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    pub fn set_left_pane_percent(&mut self, percent: u16) {
        self.layout.set_left_pane_percent(percent);
    }

    pub fn set_minimap_percent(&mut self, percent: u16) {
        self.layout.set_minimap_percent(percent);
    }

    pub fn start_drag(&mut self, divider: Divider) {
        self.layout.start_drag(divider);
    }

    pub fn stop_drag(&mut self) {
        self.layout.stop_drag();
    }
}
