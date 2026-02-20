use crate::constants::{
    LEFT_PANE_MAX_PERCENT, LEFT_PANE_MIN_PERCENT, MINIMAP_MAX_PERCENT, MINIMAP_MIN_PERCENT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Minimap,
    Chat,
    Diff,
}

impl Pane {
    /// Cycle to the next pane (Minimap -> Chat -> Diff -> Minimap)
    pub fn next(self) -> Self {
        match self {
            Self::Minimap => Self::Chat,
            Self::Chat => Self::Diff,
            Self::Diff => Self::Minimap,
        }
    }

    /// Cycle to the previous pane (Minimap -> Diff -> Chat -> Minimap)
    pub fn prev(self) -> Self {
        match self {
            Self::Minimap => Self::Diff,
            Self::Chat => Self::Minimap,
            Self::Diff => Self::Chat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    Vertical,   // Between left pane and diff viewer
    Horizontal, // Between minimap and chat
}

/// Pane layout and focus state
#[derive(Debug)]
pub struct Layout {
    /// Which pane has focus
    pub active_pane: Pane,
    /// Width of left pane as percentage (20-80)
    pub left_pane_percent: u16,
    /// Height of minimap within left pane as percentage (15-85)
    pub minimap_percent: u16,
    /// Currently dragging a divider
    pub dragging: Option<Divider>,
    /// Zoomed pane (None = normal layout, Some(pane) = that pane is fullscreen)
    pub zoomed: Option<Pane>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            active_pane: Pane::Diff,
            left_pane_percent: 50,
            minimap_percent: 40,
            dragging: None,
            zoomed: None,
        }
    }
}

impl Layout {
    pub fn set_left_pane_percent(&mut self, percent: u16) {
        self.left_pane_percent = percent.clamp(LEFT_PANE_MIN_PERCENT, LEFT_PANE_MAX_PERCENT);
    }

    pub fn set_minimap_percent(&mut self, percent: u16) {
        self.minimap_percent = percent.clamp(MINIMAP_MIN_PERCENT, MINIMAP_MAX_PERCENT);
    }

    pub fn start_drag(&mut self, divider: Divider) {
        self.dragging = Some(divider);
    }

    pub fn stop_drag(&mut self) {
        self.dragging = None;
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = match self.zoomed {
            Some(_) => None,
            None => Some(self.active_pane),
        };
    }

    pub fn is_zoomed(&self) -> bool {
        self.zoomed.is_some()
    }
}
