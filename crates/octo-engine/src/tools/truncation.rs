/// Tool execution output configuration for truncation limits.
#[derive(Debug, Clone)]
pub struct ToolExecutionConfig {
    /// Maximum output size in bytes (default: 50KB).
    pub max_output_bytes: usize,
    /// Maximum number of output lines (default: 2000).
    pub max_output_lines: usize,
}

impl Default for ToolExecutionConfig {
    fn default() -> Self {
        Self {
            max_output_bytes: 50 * 1024,
            max_output_lines: 2000,
        }
    }
}

/// Strategy for truncating oversized tool output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncationStrategy {
    /// Keep head 67% + tail 27%, insert an omission marker in between (~6%).
    Head67Tail27,
    /// Keep only the beginning of the output.
    HeadOnly,
    /// Keep only the end of the output.
    TailOnly,
}

/// Result of a truncation operation.
#[derive(Debug, Clone)]
pub struct TruncationResult {
    /// The (possibly truncated) content.
    pub content: String,
    /// Whether the content was actually truncated.
    pub was_truncated: bool,
    /// Original size in bytes before truncation.
    pub original_size: usize,
    /// Which strategy was applied, if any.
    pub strategy_used: Option<TruncationStrategy>,
}

const OMISSION_MARKER: &str = "\n\n... [truncated: middle section omitted] ...\n\n";

/// Truncate tool output according to the given configuration and strategy.
///
/// The function checks both byte-size and line-count limits. If either limit
/// is exceeded the content is truncated using `strategy`. When neither limit
/// is exceeded the original content is returned unchanged.
pub fn truncate_output(
    content: &str,
    config: &ToolExecutionConfig,
    strategy: TruncationStrategy,
) -> TruncationResult {
    let original_size = content.len();

    // Determine effective limit — take the stricter of bytes vs lines.
    let byte_limit_exceeded = original_size > config.max_output_bytes;
    let lines: Vec<&str> = content.lines().collect();
    let line_limit_exceeded = lines.len() > config.max_output_lines;

    if !byte_limit_exceeded && !line_limit_exceeded {
        return TruncationResult {
            content: content.to_string(),
            was_truncated: false,
            original_size,
            strategy_used: None,
        };
    }

    // Decide the target size (in lines) we want to keep.
    let target_lines = if line_limit_exceeded {
        config.max_output_lines
    } else {
        lines.len() // lines are fine; we'll trim by bytes later
    };

    // First pass: trim by lines.
    let trimmed = truncate_lines(&lines, target_lines, strategy);

    // Second pass: if still over byte budget, trim by bytes.
    let result_content = if trimmed.len() > config.max_output_bytes {
        truncate_bytes(&trimmed, config.max_output_bytes, strategy)
    } else {
        trimmed
    };

    TruncationResult {
        content: result_content,
        was_truncated: true,
        original_size,
        strategy_used: Some(strategy),
    }
}

/// Truncate a set of lines down to `target` lines using the given strategy.
fn truncate_lines(lines: &[&str], target: usize, strategy: TruncationStrategy) -> String {
    if lines.len() <= target {
        return lines.join("\n");
    }

    match strategy {
        TruncationStrategy::HeadOnly => {
            let mut out = lines[..target].join("\n");
            out.push_str(&format!(
                "\n\n... [truncated: {} lines omitted] ...",
                lines.len() - target,
            ));
            out
        }
        TruncationStrategy::TailOnly => {
            let skip = lines.len() - target;
            let mut out = format!("... [truncated: {} lines omitted] ...\n\n", skip,);
            out.push_str(&lines[skip..].join("\n"));
            out
        }
        TruncationStrategy::Head67Tail27 => {
            let head_count = (target as f64 * 0.67).floor() as usize;
            let tail_count = (target as f64 * 0.27).floor() as usize;
            let omitted = lines.len() - head_count - tail_count;

            let mut out = lines[..head_count].join("\n");
            out.push_str(&format!(
                "\n\n... [truncated: {} lines omitted] ...\n\n",
                omitted,
            ));
            out.push_str(&lines[lines.len() - tail_count..].join("\n"));
            out
        }
    }
}

/// Truncate a string down to approximately `budget` bytes using the given
/// strategy, splitting on char boundaries.
fn truncate_bytes(content: &str, budget: usize, strategy: TruncationStrategy) -> String {
    let marker_len = OMISSION_MARKER.len();
    // Ensure budget can at least fit the marker.
    if budget <= marker_len {
        return content.chars().take(budget).collect::<String>();
    }

    match strategy {
        TruncationStrategy::HeadOnly => {
            let usable = budget.saturating_sub(marker_len);
            let head = safe_slice(content, usable);
            format!("{}{}", head, OMISSION_MARKER.trim_end())
        }
        TruncationStrategy::TailOnly => {
            let usable = budget.saturating_sub(marker_len);
            let tail = safe_tail_slice(content, usable);
            format!("{}{}", OMISSION_MARKER.trim_start(), tail)
        }
        TruncationStrategy::Head67Tail27 => {
            let usable = budget.saturating_sub(marker_len);
            let head_budget = (usable as f64 * 0.67).floor() as usize;
            let tail_budget = (usable as f64 * 0.27).floor() as usize;

            let head = safe_slice(content, head_budget);
            let tail = safe_tail_slice(content, tail_budget);
            format!("{}{}{}", head, OMISSION_MARKER, tail)
        }
    }
}

/// Return up to `max_bytes` from the start of `s`, respecting char boundaries.
fn safe_slice(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Return up to `max_bytes` from the end of `s`, respecting char boundaries.
fn safe_tail_slice(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut start = s.len() - max_bytes;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_content_not_truncated() {
        let config = ToolExecutionConfig::default();
        let result = truncate_output("hello world", &config, TruncationStrategy::Head67Tail27);
        assert!(!result.was_truncated);
        assert_eq!(result.content, "hello world");
        assert!(result.strategy_used.is_none());
    }

    #[test]
    fn truncation_result_contains_marker() {
        let config = ToolExecutionConfig {
            max_output_bytes: 100,
            max_output_lines: 2000,
        };
        let long = "x".repeat(200);
        let result = truncate_output(&long, &config, TruncationStrategy::Head67Tail27);
        assert!(result.was_truncated);
        assert!(result.content.contains("truncated"));
    }
}
