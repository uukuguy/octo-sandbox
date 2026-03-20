//! File operation output formatter.
//!
//! Handles Read (shows content with line numbers), Write/Edit (shows diff
//! with +/- coloring).

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{truncate_lines, FormattedOutput, ToolFormatter};
use super::style_tokens;

/// Formatter for file read/write/edit tool output.
pub struct FileFormatter;

/// Maximum lines to show before truncating.
const MAX_DISPLAY_LINES: usize = 50;

impl FileFormatter {
    fn format_read<'a>(output: &str) -> FormattedOutput<'a> {
        let truncated = truncate_lines(output, MAX_DISPLAY_LINES);
        let total_lines = output.lines().count();

        let header = Line::from(vec![
            Span::styled(
                "  \u{1f4c4} ".to_string(),
                Style::default().fg(style_tokens::BLUE_PATH),
            ),
            Span::styled(
                format!("File content ({total_lines} lines)"),
                Style::default().fg(style_tokens::BLUE_PATH),
            ),
        ]);

        let body: Vec<Line<'a>> = truncated
            .lines()
            .enumerate()
            .map(|(i, line)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {:<4} ", i + 1),
                        Style::default().fg(style_tokens::GREY),
                    ),
                    Span::raw(line.to_string()),
                ])
            })
            .collect();

        let footer = if total_lines > MAX_DISPLAY_LINES {
            Some(Line::from(Span::styled(
                format!("  ... {total_lines} total lines"),
                Style::default().fg(style_tokens::SUBTLE),
            )))
        } else {
            None
        };

        FormattedOutput {
            header,
            body,
            footer,
        }
    }

    fn format_diff<'a>(tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let verb = if tool_name == "Write" || tool_name == "write_file" {
            "Written"
        } else {
            "Edited"
        };

        let header = Line::from(vec![
            Span::styled(
                "  \u{270e} ".to_string(),
                Style::default().fg(style_tokens::SUCCESS),
            ),
            Span::styled(
                format!("{verb} file"),
                Style::default().fg(style_tokens::SUCCESS),
            ),
        ]);

        let mut additions = 0usize;
        let mut removals = 0usize;

        let body: Vec<Line<'a>> = output
            .lines()
            .map(|line| {
                if let Some(rest) = line.strip_prefix('+') {
                    additions += 1;
                    Line::from(Span::styled(
                        format!("    +{rest}"),
                        Style::default().fg(style_tokens::SUCCESS),
                    ))
                } else if let Some(rest) = line.strip_prefix('-') {
                    removals += 1;
                    Line::from(Span::styled(
                        format!("    -{rest}"),
                        Style::default().fg(style_tokens::ERROR),
                    ))
                } else {
                    Line::from(Span::raw(format!("    {line}")))
                }
            })
            .collect();

        let footer = Some(Line::from(vec![
            Span::styled(
                format!("  +{additions} "),
                Style::default().fg(style_tokens::SUCCESS),
            ),
            Span::styled(
                format!("-{removals}"),
                Style::default().fg(style_tokens::ERROR),
            ),
        ]));

        FormattedOutput {
            header,
            body,
            footer,
        }
    }
}

impl ToolFormatter for FileFormatter {
    fn format<'a>(&self, tool_name: &str, output: &str) -> FormattedOutput<'a> {
        match tool_name {
            "Read" | "read_file" | "read_pdf" | "file_read" => Self::format_read(output),
            _ => Self::format_diff(tool_name, output),
        }
    }

    fn handles(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "Read"
                | "Write"
                | "Edit"
                | "read_file"
                | "write_file"
                | "edit_file"
                | "file_read"
                | "file_write"
                | "file_edit"
                | "read_pdf"
                | "patch_file"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handles() {
        let f = FileFormatter;
        assert!(f.handles("Read"));
        assert!(f.handles("Write"));
        assert!(f.handles("Edit"));
        assert!(f.handles("read_file"));
        assert!(!f.handles("Bash"));
    }

    #[test]
    fn test_format_read() {
        let f = FileFormatter;
        let output = "fn main() {\n    println!(\"hello\");\n}";
        let result = f.format("Read", output);

        let header_text: String = result
            .header
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(header_text.contains("3 lines"));
        assert_eq!(result.body.len(), 3);
    }

    #[test]
    fn test_format_edit_diff() {
        let f = FileFormatter;
        let output = " context line\n-old line\n+new line\n context again";
        let result = f.format("Edit", output);

        let header_text: String = result
            .header
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(header_text.contains("Edited"));

        let footer = result.footer.unwrap();
        let footer_text: String = footer.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(footer_text.contains("+1"));
        assert!(footer_text.contains("-1"));
    }

    #[test]
    fn test_format_write() {
        let f = FileFormatter;
        let result = f.format("Write", "+new content");

        let header_text: String = result
            .header
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(header_text.contains("Written"));
    }

    #[test]
    fn test_format_read_truncation() {
        let f = FileFormatter;
        let lines: Vec<String> = (0..100).map(|i| format!("line {i}")).collect();
        let output = lines.join("\n");
        let result = f.format("Read", &output);
        assert!(result.footer.is_some());
    }
}
