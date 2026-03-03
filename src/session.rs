use crate::diff::FileFilter;
use crate::model::{CommitInfo, Message, ReviewMode, Step, Walkthrough};
use crate::settings::ApiKeySource;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionState {
    Setup,
    Loading {
        status: String,
        steps_received: usize,
    },
    #[default]
    Ready,
    Error {
        message: String,
    },
}

pub struct Session {
    pub state: SessionState,
    pub walkthrough: Walkthrough,
    pub current_step: usize,
    pub reviewed_steps: Vec<bool>,
    pub walkthrough_complete: bool,
    pub review_mode: ReviewMode,
    pub chat_pending: Option<usize>,
    pub chat_request: Option<(usize, Walkthrough, Vec<Message>)>,
    pub rechunk_pending: bool,
    pub rechunk_request: Option<(usize, Step, Option<String>)>,
    pub retry_requested: bool,
    pub generation_requested: bool,
    pub diff_input: Option<String>,
    pub commits: Vec<CommitInfo>,
    pub diff_filter: FileFilter,
    pub api_key_input: String,
    pub api_key_source: ApiKeySource,
}

impl Session {
    pub fn new(walkthrough: Walkthrough, mode: ReviewMode) -> Self {
        let step_count = walkthrough.step_count();
        Self {
            state: SessionState::Ready,
            walkthrough,
            current_step: 0,
            reviewed_steps: vec![false; step_count],
            walkthrough_complete: false,
            review_mode: mode,
            chat_pending: None,
            chat_request: None,
            rechunk_pending: false,
            rechunk_request: None,
            retry_requested: false,
            generation_requested: false,
            diff_input: None,
            commits: vec![],
            diff_filter: FileFilter::default(),
            api_key_input: String::new(),
            api_key_source: ApiKeySource::Missing,
        }
    }

    pub fn setup(api_key_input: String, api_key_source: ApiKeySource, mode: ReviewMode) -> Self {
        Self {
            state: SessionState::Setup,
            walkthrough: Walkthrough { steps: vec![] },
            current_step: 0,
            reviewed_steps: vec![],
            walkthrough_complete: false,
            review_mode: mode,
            chat_pending: None,
            chat_request: None,
            rechunk_pending: false,
            rechunk_request: None,
            retry_requested: false,
            generation_requested: false,
            diff_input: None,
            commits: vec![],
            diff_filter: FileFilter::default(),
            api_key_input,
            api_key_source,
        }
    }

    // --- State transitions ---

    pub fn confirm_setup(&mut self) {
        if self.api_key_input.trim().is_empty() {
            return;
        }
        self.state = SessionState::Loading {
            status: "Initializing...".to_string(),
            steps_received: 0,
        };
        self.generation_requested = true;
    }

    pub fn set_ready(&mut self, walkthrough: Walkthrough) {
        let step_count = walkthrough.step_count();
        self.walkthrough = walkthrough;
        self.reviewed_steps = vec![false; step_count];
        self.current_step = 0;
        self.state = SessionState::Ready;
    }

    pub fn set_error(&mut self, message: String) {
        self.state = SessionState::Error { message };
    }

    pub fn is_error(&self) -> bool {
        matches!(self.state, SessionState::Error { .. })
    }

    pub fn request_retry(&mut self) {
        self.state = SessionState::Loading {
            status: "Retrying...".to_string(),
            steps_received: 0,
        };
        self.retry_requested = true;
    }

    pub fn set_loading_status(&mut self, status: String) {
        if let SessionState::Loading { steps_received, .. } = &self.state {
            self.state = SessionState::Loading {
                status,
                steps_received: *steps_received,
            };
        }
    }

    // --- Navigation ---

    /// Returns true if the step actually changed.
    pub fn next_step(&mut self) -> bool {
        if self.current_step < self.walkthrough.step_count().saturating_sub(1) {
            self.current_step += 1;
            true
        } else {
            false
        }
    }

    /// Returns true if the step actually changed.
    pub fn prev_step(&mut self) -> bool {
        if self.current_step > 0 {
            self.current_step -= 1;
            self.walkthrough_complete = false;
            true
        } else {
            false
        }
    }

    /// Returns true if the step actually changed.
    pub fn go_to_step(&mut self, index: usize) -> bool {
        if index < self.walkthrough.step_count() && index != self.current_step {
            self.current_step = index;
            self.walkthrough_complete = false;
            true
        } else {
            false
        }
    }

    /// Returns true if the step actually changed.
    pub fn complete_step_and_advance(&mut self) -> bool {
        self.set_step_reviewed(self.current_step, true);
        self.sync_parent_completion(self.current_step);

        if self.current_step < self.walkthrough.step_count().saturating_sub(1) {
            self.current_step += 1;
            true
        } else {
            self.walkthrough_complete = true;
            false
        }
    }

    pub fn toggle_step_reviewed(&mut self) {
        let current = self.is_step_reviewed(self.current_step);
        self.set_step_reviewed(self.current_step, !current);
        self.sync_parent_completion(self.current_step);
        self.walkthrough_complete = false;
    }

    // --- Queries ---

    pub fn is_walkthrough_complete(&self) -> bool {
        self.walkthrough_complete && self.reviewed_steps.iter().all(|&v| v)
    }

    pub fn is_step_reviewed(&self, index: usize) -> bool {
        self.reviewed_steps.get(index).copied().unwrap_or(false)
    }

    pub fn set_step_reviewed(&mut self, index: usize, reviewed: bool) {
        if let Some(v) = self.reviewed_steps.get_mut(index) {
            *v = reviewed;
        }
    }

    pub fn current_step_data(&self) -> Option<&Step> {
        self.walkthrough.get_step(self.current_step)
    }

    pub fn total_diff_lines(&self) -> usize {
        self.walkthrough
            .steps
            .iter()
            .map(|s| s.diff_line_count())
            .sum()
    }

    pub fn reviewed_diff_lines(&self) -> usize {
        self.walkthrough
            .steps
            .iter()
            .enumerate()
            .filter(|(i, _)| self.is_step_reviewed(*i))
            .map(|(_, step)| step.diff_line_count())
            .sum()
    }

    // --- Chat ---

    /// Sends a user message (content provided directly). Used by headless mode.
    pub fn send_message(&mut self, content: String) {
        if content.trim().is_empty() {
            return;
        }
        if self.chat_pending.is_some() {
            return;
        }

        let step_index = self.current_step;
        if let Some(step) = self.walkthrough.steps.get_mut(step_index) {
            step.messages.push(Message::user(content));
            let messages_clone = step.messages.clone();
            let walkthrough_clone = self.walkthrough.clone();
            self.chat_pending = Some(step_index);
            self.chat_request = Some((step_index, walkthrough_clone, messages_clone));
        }
    }

    pub fn receive_chat_chunk(&mut self, step_index: usize, chunk: String) {
        if self.chat_pending != Some(step_index) {
            return;
        }
        if let Some(step) = self.walkthrough.steps.get_mut(step_index) {
            if let Some(last_msg) = step.messages.last_mut() {
                if last_msg.role == crate::model::MessageRole::Assistant {
                    last_msg.content.push_str(&chunk);
                } else {
                    step.messages.push(Message::assistant(chunk));
                }
            } else {
                step.messages.push(Message::assistant(chunk));
            }
        }
    }

    pub fn receive_chat_complete(&mut self, step_index: usize) {
        if self.chat_pending == Some(step_index) {
            self.chat_pending = None;
        }
    }

    pub fn receive_chat_error(&mut self, step_index: usize, error: String) {
        if self.chat_pending == Some(step_index) {
            self.chat_pending = None;
            if let Some(step) = self.walkthrough.steps.get_mut(step_index) {
                step.messages
                    .push(Message::assistant(format!("Error: {}", error)));
            }
        }
    }

    // --- Rechunk ---

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

        self.walkthrough.steps[step_index].hunks.clear();

        let insert_pos = step_index + 1;
        for (i, mut sub_step) in sub_steps.into_iter().enumerate() {
            sub_step.depth = parent_depth + 1;
            self.walkthrough.steps.insert(insert_pos + i, sub_step);
            self.reviewed_steps.insert(insert_pos + i, false);
        }

        self.current_step = insert_pos;
        self.renumber_steps();
    }

    fn renumber_steps(&mut self) {
        let mut counters: Vec<usize> = vec![0];
        for step in &mut self.walkthrough.steps {
            let d = step.depth as usize;
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

    fn sync_parent_completion(&mut self, child_index: usize) {
        let child_depth = self.walkthrough.steps[child_index].depth;
        if child_depth == 0 {
            return;
        }

        let parent_index = (0..child_index)
            .rev()
            .find(|&i| self.walkthrough.steps[i].depth < child_depth);
        let Some(parent_index) = parent_index else {
            return;
        };

        let all_done = self.walkthrough.steps[parent_index + 1..]
            .iter()
            .enumerate()
            .take_while(|(_, s)| s.depth > self.walkthrough.steps[parent_index].depth)
            .filter(|(_, s)| s.depth == child_depth)
            .all(|(i, _)| self.is_step_reviewed(parent_index + 1 + i));

        if all_done {
            self.set_step_reviewed(parent_index, true);
        }
    }

    pub fn receive_rechunk_error(&mut self, error: String) {
        self.rechunk_pending = false;
        if let Some(step) = self.walkthrough.steps.get_mut(self.current_step) {
            step.messages.push(Message::assistant(format!(
                "Error splitting step: {}",
                error
            )));
        }
    }
}
