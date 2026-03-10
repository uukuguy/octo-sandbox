//! Spinner and progress indicators

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

/// Create a spinner for long-running operations
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

/// Create a spinner for tool execution
pub fn create_tool_spinner(tool_name: &str, input_preview: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⣿⠿⢟⠯⠷ "),
    );
    pb.set_message(format!("{}({})", tool_name, input_preview));
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}
