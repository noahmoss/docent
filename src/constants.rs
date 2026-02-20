use std::time::Duration;

// Event loop timing
pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub const EVENT_RECV_TIMEOUT: Duration = Duration::from_millis(100);

// Viewport calculations
pub const VIEWPORT_HEIGHT_OFFSET: u16 = 5;
pub const HELP_BAR_HEIGHT: u16 = 1;

// Mouse interaction
pub const DIVIDER_HIT_ZONE: u16 = 2;

// Input box sizing
pub const INPUT_MIN_LINES: u16 = 1;
pub const INPUT_MAX_LINES: u16 = 10;

// Pane layout bounds (percentages)
pub const LEFT_PANE_MIN_PERCENT: u16 = 20;
pub const LEFT_PANE_MAX_PERCENT: u16 = 80;
pub const MINIMAP_MIN_PERCENT: u16 = 15;
pub const MINIMAP_MAX_PERCENT: u16 = 85;

// Dialog sizing (percentages)
pub const LOADING_DIALOG_WIDTH: u16 = 60;
pub const LOADING_DIALOG_HEIGHT: u16 = 30;
pub const ERROR_DIALOG_WIDTH: u16 = 70;
pub const ERROR_DIALOG_HEIGHT: u16 = 40;
