/// Auto-compact summary strategy for context pruning.
///
/// Produces a concise heuristic summary placeholder for long tool results,
/// preserving the first line as a title hint. Designed so a real LLM
/// summarizer can be plugged in later by replacing `compact_message`.
/// Configuration for the auto-compact summary strategy.
#[derive(Debug, Clone)]
pub struct AutoCompactConfig {
    /// Maximum tokens the summary placeholder should occupy (approximate).
    pub max_summary_tokens: usize,
    /// Minimum content length (in chars) before compaction kicks in.
    /// Content shorter than this is returned unchanged.
    pub min_content_length: usize,
}

impl Default for AutoCompactConfig {
    fn default() -> Self {
        Self {
            max_summary_tokens: 50,
            min_content_length: 200,
        }
    }
}

/// Heuristic auto-compact summarizer.
///
/// Extracts the first line as a title hint and produces a short placeholder
/// that captures content size. The API is intentionally simple so a real
/// LLM-based summarizer can replace the implementation later.
pub struct AutoCompactSummary;

impl AutoCompactSummary {
    /// Compact a content string according to the given config.
    ///
    /// - If `content` is shorter than `config.min_content_length`, it is
    ///   returned unchanged.
    /// - Otherwise, the first line is extracted (truncated to 60 chars),
    ///   and a summary placeholder is produced.
    pub fn compact_message(content: &str, config: &AutoCompactConfig) -> String {
        if content.len() < config.min_content_length {
            return content.to_string();
        }

        let first_line = content.lines().next().unwrap_or("").trim();

        let title = if first_line.len() > 60 {
            // Truncate at a char boundary <= 60 chars
            let end = first_line
                .char_indices()
                .take_while(|(i, _)| *i < 60)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(60.min(first_line.len()));
            &first_line[..end]
        } else {
            first_line
        };

        let char_count = content.chars().count();
        let tokens_saved = char_count / 4; // same chars/4 heuristic used elsewhere

        format!(
            "[Compacted: {}... ({} chars -> {} tokens saved)]",
            title, char_count, tokens_saved
        )
    }
}
