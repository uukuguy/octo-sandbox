//! Active tool spinner rendering for conversation display.
//!
//! Shows animated spinners for tools that are currently executing,
//! rendered below the message history.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use std::collections::HashMap;

use crate::tui::formatters::style_tokens;
use crate::tui::formatters::tool_registry::format_tool_call_parts;

/// Represents an active tool execution (for spinner display).
#[derive(Debug, Clone)]
pub struct ActiveTool {
    pub name: String,
    pub args: serde_json::Value,
    pub elapsed_secs: u64,
}

/// Render an active tool as a spinner line.
pub(super) fn render_active_tool(tool: &ActiveTool, spinner_char: char) -> Line<'static> {
    let args: HashMap<String, serde_json::Value> = if let Some(obj) = tool.args.as_object() {
        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    } else {
        HashMap::new()
    };
    let (verb, arg) = format_tool_call_parts(&tool.name, &args);

    Line::from(vec![
        Span::styled(
            format!("{spinner_char} "),
            Style::default().fg(style_tokens::BLUE_BRIGHT),
        ),
        Span::styled(
            verb,
            Style::default()
                .fg(style_tokens::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({arg})"),
            Style::default().fg(style_tokens::SUBTLE),
        ),
        Span::styled(
            format!(" ({}s)", tool.elapsed_secs),
            Style::default().fg(style_tokens::GREY),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_active_tool() {
        let tool = ActiveTool {
            name: "bash".into(),
            args: serde_json::json!({"command": "ls"}),
            elapsed_secs: 5,
        };
        let line = render_active_tool(&tool, '\u{280b}');
        assert!(!line.spans.is_empty());
    }
}
