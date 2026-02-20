use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VimMode {
    #[default]
    Auto,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EditorSettings {
    #[serde(default)]
    pub vim_mode: VimMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub editor: EditorSettings,
}

impl Settings {
    /// Load settings from ~/.docent/settings.json
    pub fn load() -> Self {
        Self::settings_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    fn settings_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".docent").join("settings.json"))
    }

    /// Resolve whether vim mode should be enabled.
    /// If set to Auto, checks .inputrc for "set editing-mode vi".
    pub fn vim_enabled(&self) -> bool {
        match self.editor.vim_mode {
            VimMode::Enabled => true,
            VimMode::Disabled => false,
            VimMode::Auto => detect_vim_from_inputrc(),
        }
    }
}

/// Check if .inputrc contains "set editing-mode vi"
fn detect_vim_from_inputrc() -> bool {
    let inputrc_path = dirs::home_dir().map(|h| h.join(".inputrc"));

    if let Some(path) = inputrc_path
        && let Ok(contents) = fs::read_to_string(path)
    {
        for line in contents.lines() {
            let line = line.trim();
            // Skip comments
            if line.starts_with('#') {
                continue;
            }
            // Check for "set editing-mode vi" (case-insensitive, flexible whitespace)
            if line.to_lowercase().contains("set")
                && line.to_lowercase().contains("editing-mode")
                && line.to_lowercase().contains("vi")
            {
                return true;
            }
        }
    }

    false
}
