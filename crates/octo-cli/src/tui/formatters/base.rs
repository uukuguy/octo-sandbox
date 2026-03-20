//! Base formatter traits and types.

use ratatui::text::Line;

/// Result of formatting a tool output for display.
#[derive(Debug, Clone)]
pub struct FormattedOutput<'a> {
    /// Header line (tool name, file path, etc.)
    pub header: Line<'a>,
    /// Body lines (the actual content)
    pub body: Vec<Line<'a>>,
    /// Footer line (summary, stats, etc.)
    pub footer: Option<Line<'a>>,
}

/// Trait for tool-specific output formatters.
pub trait ToolFormatter {
    /// Format a tool result for display.
    fn format<'a>(&self, tool_name: &str, output: &str) -> FormattedOutput<'a>;

    /// Whether this formatter handles the given tool name.
    fn handles(&self, tool_name: &str) -> bool;
}

/// Truncate text to a maximum number of lines, adding a summary.
pub fn truncate_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }

    let head_count = max_lines * 2 / 3;
    let tail_count = max_lines - head_count - 1;
    let omitted = lines.len() - head_count - tail_count;

    let mut result = lines[..head_count].join("\n");
    result.push_str(&format!("\n... ({omitted} lines omitted) ...\n"));
    result.push_str(&lines[lines.len() - tail_count..].join("\n"));
    result
}

/// Indent each line by the given number of spaces.
pub fn indent(text: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_lines_short() {
        let text = "line 1\nline 2\nline 3";
        assert_eq!(truncate_lines(text, 10), text);
    }

    #[test]
    fn test_truncate_lines_long() {
        let lines: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
        let text = lines.join("\n");
        let result = truncate_lines(&text, 20);
        assert!(result.contains("omitted"));
        assert!(result.lines().count() <= 21);
    }

    #[test]
    fn test_indent() {
        let text = "line 1\nline 2\n\nline 3";
        let result = indent(text, 4);
        assert!(result.starts_with("    line 1"));
        assert!(result.contains("\n    line 2"));
        assert!(result.contains("\n\n"));
    }
}
