use ratatui::style::Color;

// Borders
pub const BORDER_INACTIVE: Color = Color::DarkGray;
pub const BORDER_ACTIVE: Color = Color::Green;

// Diff viewer
pub const DIFF_ADDED: Color = Color::Green;
pub const DIFF_REMOVED: Color = Color::Red;
pub const DIFF_HUNK_HEADER: Color = Color::Cyan;
pub const DIFF_FILE_HEADER: Color = Color::Magenta;

// Chat
pub const CHAT_ASSISTANT_BULLET: Color = Color::Rgb(199, 199, 199);
pub const CHAT_ASSISTANT_TEXT: Color = Color::Rgb(199, 199, 199);
pub const CHAT_ASSISTANT_BOLD: Color = Color::White;
pub const CHAT_ASSISTANT_CODE: Color = Color::Rgb(147, 154, 207);
pub const CHAT_USER_TEXT: Color = Color::White;
pub const CHAT_USER_BG: Color = Color::Rgb(60, 60, 60);

// Input
pub const INPUT_PLACEHOLDER: Color = Color::DarkGray;
pub const INPUT_CURSOR_FG: Color = Color::Black;
pub const INPUT_CURSOR_BG: Color = Color::White;

// Minimap
pub const STEP_CURRENT: Color = Color::White;
pub const STEP_COMPLETED: Color = Color::Green;
pub const STEP_PENDING: Color = Color::DarkGray;
