use crate::diff::FileFilter;
use crate::editor::Editor;
use crate::layout::{Divider, Layout, Pane};
use crate::model::{Message, ReviewMode, Step, Walkthrough};
use crate::scroll::{ChatScroll, DiffScroll};
use crate::search::SearchState;
use crate::settings::{ApiKeySource, Settings};

/// Application state machine
#[derive(Debug, Clone, Default)]
pub enum AppState {
    /// Start screen for mode selection and API key entry
    Setup,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupFocus {
    Review,
    Walkthrough,
    ApiKey,
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
    pub chat_pending: Option<usize>,
    pub chat_request: Option<(usize, Walkthrough, Vec<Message>)>,
    pub rechunk_pending: bool,
    pub rechunk_request: Option<(usize, Step, Option<String>)>,
    pub retry_requested: bool,
    pub generation_requested: bool,
    pub diff_input: Option<String>,
    pub diff_filter: FileFilter,
    pub search: SearchState,
    pub review_mode: ReviewMode,
    pub setup_focus: SetupFocus,
    pub api_key_input: String,
    pub api_key_source: ApiKeySource,
}

impl<'a> App<'a> {
    pub fn new(walkthrough: Walkthrough, settings: &Settings, mode: ReviewMode) -> Self {
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
            rechunk_pending: false,
            rechunk_request: None,
            retry_requested: false,
            generation_requested: false,
            diff_input: None,
            diff_filter: FileFilter::default(),
            search: SearchState::new(),
            review_mode: mode,
            setup_focus: SetupFocus::Review,
            api_key_input: String::new(),
            api_key_source: ApiKeySource::Missing,
        }
    }

    pub fn setup(settings: &Settings, mode: ReviewMode) -> Self {
        let (api_key, source) = settings.resolve_api_key();
        let api_key_input = api_key.unwrap_or_default();
        let focus = if source == ApiKeySource::Missing {
            SetupFocus::ApiKey
        } else {
            match mode {
                ReviewMode::Review => SetupFocus::Review,
                ReviewMode::Walkthrough => SetupFocus::Walkthrough,
            }
        };
        Self {
            state: AppState::Setup,
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
            rechunk_pending: false,
            rechunk_request: None,
            retry_requested: false,
            generation_requested: false,
            diff_input: None,
            diff_filter: FileFilter::default(),
            search: SearchState::new(),
            review_mode: mode,
            setup_focus: focus,
            api_key_input,
            api_key_source: source,
        }
    }

    pub fn confirm_setup(&mut self) {
        if self.api_key_input.trim().is_empty() {
            return;
        }
        self.state = AppState::Loading {
            status: "Initializing...".to_string(),
            steps_received: 0,
        };
        self.generation_requested = true;
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
        self.sync_parent_completion(self.current_step);

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
        self.sync_parent_completion(self.current_step);
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

    pub fn request_rechunk(&mut self) {
        if self.rechunk_pending || self.chat_pending.is_some() {
            return;
        }
        if let Some(step) = self.current_step_data() {
            if step.hunks.is_empty() {
                return;
            }
            let step = step.clone();
            let step_index = self.current_step;
            let diff_text = self.diff_input.clone();
            self.rechunk_pending = true;
            self.rechunk_request = Some((step_index, step, diff_text));
        }
    }

    pub fn receive_rechunk_complete(&mut self, step_index: usize, sub_steps: Vec<Step>) {
        self.rechunk_pending = false;
        if sub_steps.len() <= 1 {
            return;
        }

        let parent_depth = self.walkthrough.steps[step_index].depth;

        // Clear parent's hunks — it becomes a group header
        self.walkthrough.steps[step_index].hunks.clear();

        // Insert sub-steps after the parent with incremented depth
        let insert_pos = step_index + 1;
        for (i, mut sub_step) in sub_steps.into_iter().enumerate() {
            sub_step.depth = parent_depth + 1;
            self.walkthrough.steps.insert(insert_pos + i, sub_step);
            self.visited_steps.insert(insert_pos + i, false);
        }

        // Point to first child
        self.current_step = insert_pos;

        self.renumber_steps();
        self.diff_scroll.reset();
        self.chat_scroll.reset();
    }

    fn renumber_steps(&mut self) {
        let mut counters: Vec<usize> = vec![0]; // stack of counters per depth
        for step in &mut self.walkthrough.steps {
            let d = step.depth as usize;
            // Grow or shrink the counter stack to match current depth
            counters.truncate(d + 1);
            while counters.len() <= d {
                counters.push(0);
            }
            counters[d] += 1;

            step.id = if d == 0 {
                format!("{}", counters[0])
            } else {
                counters[..=d]
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            };
        }
    }

    /// When a child step is marked visited, auto-complete its parent if all siblings are done.
    fn sync_parent_completion(&mut self, child_index: usize) {
        let child_depth = self.walkthrough.steps[child_index].depth;
        if child_depth == 0 {
            return;
        }

        // Find parent: walk backwards to the first step with lower depth
        let parent_index = (0..child_index)
            .rev()
            .find(|&i| self.walkthrough.steps[i].depth < child_depth);
        let Some(parent_index) = parent_index else {
            return;
        };

        // Check if all children of this parent are visited
        let all_done = self.walkthrough.steps[parent_index + 1..]
            .iter()
            .enumerate()
            .take_while(|(_, s)| s.depth > self.walkthrough.steps[parent_index].depth)
            .filter(|(_, s)| s.depth == child_depth)
            .all(|(i, _)| self.is_step_visited(parent_index + 1 + i));

        if all_done {
            self.set_step_visited(parent_index, true);
        }
    }

    pub fn receive_rechunk_error(&mut self, error: String) {
        self.rechunk_pending = false;
        if let Some(step) = self.walkthrough.steps.get_mut(self.current_step) {
            step.messages
                .push(Message::assistant(format!("Error splitting step: {}", error)));
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

    // Search functionality

    /// Get the diff content as lines for searching
    pub fn diff_lines(&self) -> Vec<String> {
        let Some(step) = self.current_step_data() else {
            return Vec::new();
        };

        let mut lines = Vec::new();
        for hunk in &step.hunks {
            // File header
            lines.push(format!("─── {} ───", hunk.file_path));
            lines.push(String::new());

            // Diff content
            for line in hunk.content.lines() {
                lines.push(line.to_string());
            }
            lines.push(String::new());
        }
        lines
    }

    /// Start search input mode
    pub fn start_search(&mut self) {
        self.search.start();
    }

    /// Execute the current search (finalizes query and exits input mode)
    pub fn execute_search(&mut self) {
        let lines = self.diff_lines();
        self.search.execute(&lines);
        self.scroll_to_current_match();
    }

    /// Execute incremental search while typing (doesn't exit input mode)
    pub fn execute_search_incremental(&mut self) {
        let lines = self.diff_lines();
        self.search.execute_incremental(&lines);
        self.scroll_to_current_match();
    }

    /// Go to next search match
    pub fn next_search_match(&mut self) {
        self.search.next_match();
        self.scroll_to_current_match();
    }

    /// Go to previous search match
    pub fn prev_search_match(&mut self) {
        self.search.prev_match();
        self.scroll_to_current_match();
    }

    /// Scroll the diff view to show the current match
    fn scroll_to_current_match(&mut self) {
        if let Some(m) = self.search.current_match() {
            // Set scroll position to show the match line near the top
            // with a few lines of context above
            let target = m.line.saturating_sub(3);
            self.diff_scroll.set(target);
        }
    }

    /// Clear search state
    pub fn clear_search(&mut self) {
        self.search.clear();
    }
}
