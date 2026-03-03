use crate::editor::Editor;
use crate::layout::{Layout, Pane};
use crate::model::ReviewMode;
#[cfg(debug_assertions)]
use crate::model::Walkthrough;
use crate::scroll::{ChatScroll, DiffScroll};
use crate::search::SearchState;
use crate::session::Session;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupFocus {
    Review,
    Walkthrough,
    ApiKey,
}

pub struct App<'a> {
    pub session: Session,
    pub diff_scroll: DiffScroll,
    pub chat_scroll: ChatScroll,
    pub layout: Layout,
    pub editor: Editor<'a>,
    pub should_quit: bool,
    pub quit_pending: bool,
    pub search: SearchState,
    pub show_help: bool,
    pub setup_focus: SetupFocus,
}

impl<'a> App<'a> {
    #[cfg(debug_assertions)]
    pub fn new(walkthrough: Walkthrough, settings: &Settings, mode: ReviewMode) -> Self {
        Self {
            session: Session::new(walkthrough, mode),
            diff_scroll: DiffScroll::new(),
            chat_scroll: ChatScroll::new(),
            layout: Layout::default(),
            editor: Editor::new(settings.vim_enabled()),
            should_quit: false,
            quit_pending: false,
            search: SearchState::new(),
            show_help: false,
            setup_focus: SetupFocus::Review,
        }
    }

    pub fn setup(settings: &Settings, mode: ReviewMode) -> Self {
        let (api_key, source) = settings.resolve_api_key();
        let api_key_input = api_key.unwrap_or_default();
        let focus = if source == crate::settings::ApiKeySource::Missing {
            SetupFocus::ApiKey
        } else {
            match mode {
                ReviewMode::Review => SetupFocus::Review,
                ReviewMode::Walkthrough => SetupFocus::Walkthrough,
            }
        };
        Self {
            session: Session::setup(api_key_input, source, mode),
            diff_scroll: DiffScroll::new(),
            chat_scroll: ChatScroll::new(),
            layout: Layout::default(),
            editor: Editor::new(settings.vim_enabled()),
            should_quit: false,
            quit_pending: false,
            search: SearchState::new(),
            show_help: false,
            setup_focus: focus,
        }
    }

    // --- Delegated navigation (with scroll reset) ---

    pub fn next_step(&mut self) {
        if self.session.next_step() {
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        }
    }

    pub fn prev_step(&mut self) {
        if self.session.prev_step() {
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        }
    }

    pub fn go_to_step(&mut self, index: usize) {
        if self.session.go_to_step(index) {
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        }
    }

    pub fn complete_step_and_advance(&mut self) {
        if self.session.complete_step_and_advance() {
            self.diff_scroll.reset();
            self.chat_scroll.reset();
        }
    }

    pub fn receive_rechunk_complete(
        &mut self,
        step_index: usize,
        sub_steps: Vec<crate::model::Step>,
    ) {
        self.session.receive_rechunk_complete(step_index, sub_steps);
        self.diff_scroll.reset();
        self.chat_scroll.reset();
    }

    // --- TUI-only methods ---

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

    pub fn set_active_pane(&mut self, pane: Pane) {
        if self.layout.active_pane == Pane::Chat && pane != Pane::Chat {
            self.editor.reset_mode();
        }
        self.layout.active_pane = pane;
    }

    /// Sends the current editor content as a chat message.
    pub fn send_message(&mut self) {
        let content: String = self.editor.textarea.lines().join("\n");
        if content.trim().is_empty() {
            return;
        }
        if self.session.chat_pending.is_some() {
            return;
        }

        self.editor.textarea.select_all();
        self.editor.textarea.delete_char();

        self.session.send_message(content);
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

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    // --- Search ---

    fn diff_lines(&self) -> Vec<String> {
        self.session
            .current_step_data()
            .map(|step| step.display_lines())
            .unwrap_or_default()
    }

    pub fn execute_search(&mut self) {
        let lines = self.diff_lines();
        self.search.execute(&lines);
        self.scroll_to_current_match();
    }

    pub fn execute_search_incremental(&mut self) {
        let lines = self.diff_lines();
        self.search.execute_incremental(&lines);
        self.scroll_to_current_match();
    }

    pub fn next_search_match(&mut self) {
        self.search.next_match();
        self.scroll_to_current_match();
    }

    pub fn prev_search_match(&mut self) {
        self.search.prev_match();
        self.scroll_to_current_match();
    }

    fn scroll_to_current_match(&mut self) {
        if let Some(m) = self.search.current_match() {
            let target = m.line.saturating_sub(3);
            self.diff_scroll.set(target);
        }
    }

}
