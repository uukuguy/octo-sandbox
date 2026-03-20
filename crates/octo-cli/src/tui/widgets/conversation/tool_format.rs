//! Tool call and result formatting for conversation display.
//!
//! Adapted from opendev-tui. Uses serde_json::Value for tool inputs
//! and String for tool results (matching octo-types ContentBlock).

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use std::collections::HashMap;

use crate::tui::formatters::style_tokens;
use crate::tui::formatters::tool_registry::format_tool_call_parts;
use crate::tui::widgets::spinner::COMPLETED_CHAR;

/// Convert a serde_json::Value to HashMap for tool_registry API.
fn value_to_hashmap(input: &serde_json::Value) -> HashMap<String, serde_json::Value> {
    if let Some(obj) = input.as_object() {
        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    } else {
        HashMap::new()
    }
}

/// Format a tool call as a styled line.
pub(super) fn format_tool_call(name: &str, input: &serde_json::Value) -> Line<'static> {
    let args = value_to_hashmap(input);
    let (verb, arg) = format_tool_call_parts(name, &args);

    Line::from(vec![
        Span::styled(
            format!("{COMPLETED_CHAR} "),
            Style::default().fg(style_tokens::GREEN_BRIGHT),
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
    ])
}

/// Format a tool result as styled lines.
pub(super) fn format_tool_result(content: &str, is_error: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let color = if is_error {
        style_tokens::ERROR
    } else {
        style_tokens::SUBTLE
    };

    // Truncate long results
    let max_lines = 20;
    let result_lines: Vec<&str> = content.lines().collect();
    let truncated = result_lines.len() > max_lines;
    let display_lines = if truncated {
        &result_lines[..max_lines]
    } else {
        &result_lines[..]
    };

    for line in display_lines {
        lines.push(Line::from(Span::styled(
            format!("     {line}"),
            Style::default().fg(color),
        )));
    }

    if truncated {
        lines.push(Line::from(Span::styled(
            format!("     ... ({} more lines)", result_lines.len() - max_lines),
            Style::default().fg(style_tokens::GREY),
        )));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tool_call_bash() {
        let input = serde_json::json!({"command": "ls -la"});
        let line = format_tool_call("bash", &input);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_format_tool_result_success() {
        let lines = format_tool_result("output line 1\noutput line 2", false);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_format_tool_result_error() {
        let lines = format_tool_result("error message", true);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_format_tool_result_truncation() {
        let content: String = (0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let lines = format_tool_result(&content, false);
        // 20 displayed + 1 truncation notice
        assert_eq!(lines.len(), 21);
    }
}
