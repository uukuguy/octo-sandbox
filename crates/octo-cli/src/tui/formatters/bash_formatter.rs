//! Bash tool output formatter.
//!
//! Formats command execution results with exit code coloring:
//! green for success (exit 0), red for failure (nonzero).

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::base::{FormattedOutput, ToolFormatter};
use super::style_tokens;

/// Formatter for Bash/command execution tool output.
pub struct BashFormatter;

impl BashFormatter {
    /// Parse exit code from output text.
    fn parse_exit_code(output: &str) -> Option<i32> {
        for line in output.lines().rev() {
            let trimmed = line.trim().to_lowercase();
            if let Some(rest) = trimmed.strip_prefix("exit code:") {
                if let Ok(code) = rest.trim().parse::<i32>() {
                    return Some(code);
                }
            }
            if let Some(rest) = trimmed.strip_prefix("exit_code:") {
                if let Ok(code) = rest.trim().parse::<i32>() {
                    return Some(code);
                }
            }
        }
        None
    }

    /// Extract command line from the output (first line if it looks like a command).
    fn extract_command(output: &str) -> Option<&str> {
        let first = output.lines().next()?;
        let trimmed = first.trim();
        if trimmed.starts_with('$') || trimmed.starts_with('>') {
            Some(trimmed.trim_start_matches(['$', '>']).trim())
        } else {
            None
        }
    }
}

impl ToolFormatter for BashFormatter {
    fn format<'a>(&self, _tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let exit_code = Self::parse_exit_code(output);
        let success = exit_code.is_none() || exit_code == Some(0);
        let status_color = if success {
            style_tokens::SUCCESS
        } else {
            style_tokens::ERROR
        };

        let cmd = Self::extract_command(output);
        let header = Line::from(vec![
            Span::styled("  $ ".to_string(), Style::default().fg(style_tokens::GREY)),
            Span::styled(
                cmd.unwrap_or("command").to_string(),
                Style::default().fg(style_tokens::WARNING),
            ),
        ]);

        let lines: Vec<&str> = output.lines().collect();
        let start = if cmd.is_some() { 1 } else { 0 };
        let end = if exit_code.is_some() && lines.len() > start {
            let mut e = lines.len();
            for i in (start..lines.len()).rev() {
                let t = lines[i].trim().to_lowercase();
                if t.starts_with("exit code:") || t.starts_with("exit_code:") {
                    e = i;
                    break;
                }
            }
            e
        } else {
            lines.len()
        };

        let body: Vec<Line<'a>> = lines[start..end]
            .iter()
            .map(|l| Line::from(Span::raw(format!("    {l}"))))
            .collect();

        let footer = exit_code.map(|code| {
            Line::from(vec![
                Span::styled(
                    "  \u{2500} exit ".to_string(),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(code.to_string(), Style::default().fg(status_color)),
            ])
        });

        FormattedOutput {
            header,
            body,
            footer,
        }
    }

    fn handles(&self, tool_name: &str) -> bool {
        matches!(tool_name, "Bash" | "bash" | "run_command" | "bash_execute")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handles() {
        let f = BashFormatter;
        assert!(f.handles("Bash"));
        assert!(f.handles("run_command"));
        assert!(f.handles("bash_execute"));
        assert!(!f.handles("read_file"));
    }

    #[test]
    fn test_format_success() {
        let f = BashFormatter;
        let output = "$ ls -la\nfile1.rs\nfile2.rs\nExit code: 0";
        let result = f.format("Bash", output);

        let header_text: String = result
            .header
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(header_text.contains("ls -la"));
        assert_eq!(result.body.len(), 2);

        let footer = result.footer.unwrap();
        let footer_text: String = footer.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(footer_text.contains("0"));
    }

    #[test]
    fn test_format_failure() {
        let f = BashFormatter;
        let output = "$ bad_command\ncommand not found\nExit code: 127";
        let result = f.format("Bash", output);

        let footer = result.footer.unwrap();
        let code_span = &footer.spans[1];
        assert_eq!(code_span.content.as_ref(), "127");
        assert_eq!(code_span.style.fg, Some(style_tokens::ERROR));
    }

    #[test]
    fn test_format_no_exit_code() {
        let f = BashFormatter;
        let output = "some output\nmore output";
        let result = f.format("Bash", output);
        assert!(result.footer.is_none());
        assert_eq!(result.body.len(), 2);
    }

    #[test]
    fn test_parse_exit_code() {
        assert_eq!(
            BashFormatter::parse_exit_code("output\nExit code: 0"),
            Some(0)
        );
        assert_eq!(
            BashFormatter::parse_exit_code("output\nexit_code: 42"),
            Some(42)
        );
        assert_eq!(BashFormatter::parse_exit_code("no code here"), None);
    }
}
