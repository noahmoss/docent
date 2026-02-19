use crate::model::{Message, StepStatus, Walkthrough};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Minimap,
    Chat,
    Diff,
}

pub struct App {
    pub walkthrough: Walkthrough,
    pub current_step: usize,
    pub visited_steps: Vec<bool>,
    pub diff_scroll: usize,
    pub chat_scroll: usize,
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub active_pane: ActivePane,
    pub walkthrough_complete: bool,
}

impl App {
    pub fn new(walkthrough: Walkthrough) -> Self {
        let step_count = walkthrough.step_count();
        Self {
            walkthrough,
            current_step: 0,
            visited_steps: vec![false; step_count],
            diff_scroll: 0,
            chat_scroll: 0,
            should_quit: false,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            cursor_position: 0,
            active_pane: ActivePane::Diff,
            walkthrough_complete: false,
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

    pub fn step_status(&self, index: usize) -> StepStatus {
        if index == self.current_step {
            StepStatus::Current
        } else if self.visited_steps.get(index).copied().unwrap_or(false) {
            StepStatus::Visited
        } else {
            StepStatus::Pending
        }
    }

    pub fn current_step_data(&self) -> Option<&crate::model::Step> {
        self.walkthrough.get_step(self.current_step)
    }

    pub fn current_step_data_mut(&mut self) -> Option<&mut crate::model::Step> {
        self.walkthrough.steps.get_mut(self.current_step)
    }

    pub fn enter_insert_mode(&mut self) {
        self.input_mode = InputMode::Insert;
        self.active_pane = ActivePane::Chat;
    }

    pub fn exit_insert_mode(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    pub fn set_active_pane(&mut self, pane: ActivePane) {
        // If leaving chat while in insert mode, exit insert mode
        if self.active_pane == ActivePane::Chat && pane != ActivePane::Chat {
            self.exit_insert_mode();
        }
        self.active_pane = pane;
    }

    pub fn insert_char(&mut self, c: char) {
        self.input_buffer.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input_buffer.remove(self.cursor_position);
        }
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input_buffer.len() {
            self.cursor_position += 1;
        }
    }

    pub fn move_cursor_to_start(&mut self) {
        self.cursor_position = 0;
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor_position = self.input_buffer.len();
    }

    pub fn send_message(&mut self) {
        if self.input_buffer.trim().is_empty() {
            return;
        }

        let content = std::mem::take(&mut self.input_buffer);
        self.cursor_position = 0;

        if let Some(step) = self.current_step_data_mut() {
            step.messages.push(Message::user(content));
            // Placeholder response - will be replaced with actual LLM call
            step.messages.push(Message::assistant(
                "I'm not connected to an LLM yet, but I'll be able to answer your questions soon!"
            ));
        }

        self.exit_insert_mode();
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
}
