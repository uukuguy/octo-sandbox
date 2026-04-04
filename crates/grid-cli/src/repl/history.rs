//! Session history management with XDG path support

use std::path::PathBuf;

/// Get the history file path for the REPL.
///
/// Uses XDG data directory: `~/.local/share/grid-cli/history.txt` (Linux)
/// or `~/Library/Application Support/com.grid.grid-cli/history.txt` (macOS)
pub fn history_file_path() -> PathBuf {
    let dir = history_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join("history.txt")
}

/// Get the history directory (XDG data dir).
pub fn history_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "grid", "grid-cli")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Get the data directory for grid-cli (for other persistent data).
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "grid", "grid-cli")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Get the config directory for grid-cli.
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "grid", "grid-cli")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Get the history file path for a specific GridRoot project.
pub fn history_file_path_for(root: &grid_engine::GridRoot) -> PathBuf {
    let dir = root.history_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join("history.txt")
}
