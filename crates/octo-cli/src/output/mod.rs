//! Output format system — text, json, stream-json

pub mod json;
pub mod stream_json;
pub mod text;

use serde::Serialize;

/// Output format for CLI commands
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text (default)
    #[default]
    Text,
    /// JSON output
    Json,
    /// Line-delimited JSON stream
    StreamJson,
}

/// Output configuration
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub color: bool,
    pub quiet: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        use std::io::IsTerminal;
        Self {
            format: OutputFormat::Text,
            color: std::io::stdout().is_terminal(),
            quiet: false,
        }
    }
}

/// Unified output writer — dispatches to the correct format
pub fn print_output<T: Serialize + TextOutput>(value: &T, config: &OutputConfig) {
    match config.format {
        OutputFormat::Text => text::print_text(value),
        OutputFormat::Json => json::print_json(value),
        OutputFormat::StreamJson => stream_json::print_stream_json(value),
    }
}

/// Trait for types that can render as human-readable text
pub trait TextOutput {
    fn to_text(&self) -> String;
}
