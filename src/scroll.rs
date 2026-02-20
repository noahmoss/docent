use std::cell::Cell;

/// Core scroll state with render-time clamping.
///
/// Uses `Cell` because render needs to clamp the scroll position to the actual
/// content height (only known at render time). This prevents "phantom scrolling"
/// where the user would have to scroll back through positions that don't exist.
#[derive(Debug, Default)]
pub struct Scroll {
    offset: Cell<usize>,
}

impl Scroll {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> usize {
        self.offset.get()
    }

    pub fn set(&self, value: usize) {
        self.offset.set(value);
    }

    pub fn add(&self, amount: usize) {
        self.offset.set(self.offset.get().saturating_add(amount));
    }

    pub fn sub(&self, amount: usize) {
        self.offset.set(self.offset.get().saturating_sub(amount));
    }

    pub fn reset(&self) {
        self.offset.set(0);
    }

    /// Clamp offset to max, persist, and return clamped value.
    pub fn clamped(&self, max: usize) -> usize {
        let clamped = self.offset.get().min(max);
        self.offset.set(clamped);
        clamped
    }
}

/// Diff scroll: top-anchored, offset = lines from top.
pub type DiffScroll = Scroll;

/// Chat scroll with "anchor to bottom" semantics.
///
/// The scroll position is stored as an offset from the bottom of content,
/// which allows new messages to appear automatically when at the bottom
/// (offset = 0) without needing to adjust the scroll position.
///
/// - `offset = 0`: Viewing the latest content (auto-follows new messages)
/// - `offset > 0`: Scrolled up into history (scrollback mode)
///
/// Uses `Cell` for `in_scrollback` to allow render-time correction when
/// there's nothing to scroll (content fits in viewport).
#[derive(Debug, Default)]
pub struct ChatScroll {
    /// Lines scrolled up from bottom. 0 = at bottom (auto-follow mode).
    scroll: Scroll,
    /// True when user has manually scrolled up into history.
    in_scrollback: Cell<bool>,
}

impl ChatScroll {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn in_scrollback(&self) -> bool {
        self.in_scrollback.get()
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.in_scrollback.set(true);
        self.scroll.add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll.sub(lines);
        if self.scroll.get() == 0 {
            self.in_scrollback.set(false);
        }
    }

    pub fn jump_to_bottom(&mut self) {
        self.in_scrollback.set(false);
        self.scroll.reset();
    }

    pub fn reset(&mut self) {
        self.scroll.reset();
        self.in_scrollback.set(false);
    }

    /// Get scroll position from top for rendering.
    /// Clamps offset to max_offset and resets scrollback mode if nothing to scroll.
    pub fn position_from_top(&self, max_offset: usize) -> usize {
        let clamped = self.scroll.clamped(max_offset);
        // If there's nothing to scroll, we're not in scrollback mode
        if max_offset == 0 {
            self.in_scrollback.set(false);
        }
        max_offset.saturating_sub(clamped)
    }
}
