use octo_types::{ChatMessage, ContentBlock, MessageRole};
use tracing::{debug, info};

use super::budget::DegradationLevel;

const SOFT_TRIM_HEAD: usize = 1_500;
const SOFT_TRIM_TAIL: usize = 500;

/// Prunes conversation history based on degradation level.
/// Does NOT modify the most recent `protect_recent_rounds` rounds.
pub struct ContextPruner {
    /// Number of recent agent rounds to protect from pruning.
    protect_recent_rounds: usize,
}

impl ContextPruner {
    pub fn new() -> Self {
        Self {
            protect_recent_rounds: 2,
        }
    }

    /// Apply degradation to messages in-place.
    /// Returns the number of content blocks modified.
    pub fn apply(&self, messages: &mut Vec<ChatMessage>, level: DegradationLevel) -> usize {
        match level {
            DegradationLevel::None => 0,
            DegradationLevel::SoftTrim => self.soft_trim(messages),
            DegradationLevel::HardClear => self.hard_clear(messages),
            DegradationLevel::Compact => {
                // Compact is handled externally (requires LLM call for summarization).
                // Here we just do HardClear as a pre-step.
                self.hard_clear(messages)
            }
        }
    }

    /// Level 1: Soft-trim old tool results (keep head + tail).
    fn soft_trim(&self, messages: &mut Vec<ChatMessage>) -> usize {
        let boundary = self.find_protection_boundary(messages);
        let mut modified = 0;

        for msg in messages[..boundary].iter_mut() {
            for block in msg.content.iter_mut() {
                if let ContentBlock::ToolResult { content, .. } = block {
                    if content.len() > (SOFT_TRIM_HEAD + SOFT_TRIM_TAIL + 100) {
                        let (head, tail) =
                            Self::head_tail_utf8(content, SOFT_TRIM_HEAD, SOFT_TRIM_TAIL);
                        let omitted = content.len() - SOFT_TRIM_HEAD - SOFT_TRIM_TAIL;
                        *content = format!(
                            "{}\n\n[... omitted {} chars ...]\n\n{}",
                            head, omitted, tail
                        );
                        modified += 1;
                    }
                }
            }
        }

        if modified > 0 {
            debug!(modified, "Soft-trimmed tool results");
        }
        modified
    }

    /// Level 2: Hard-clear old tool results (replace with placeholder).
    fn hard_clear(&self, messages: &mut Vec<ChatMessage>) -> usize {
        let boundary = self.find_protection_boundary(messages);
        let mut modified = 0;

        for msg in messages[..boundary].iter_mut() {
            for block in msg.content.iter_mut() {
                if let ContentBlock::ToolResult {
                    content,
                    tool_use_id,
                    ..
                } = block
                {
                    if content.len() > 100 {
                        *content = format!(
                            "[Tool result omitted, tool_use_id={}]",
                            tool_use_id
                        );
                        modified += 1;
                    }
                }
            }
        }

        if modified > 0 {
            info!(modified, "Hard-cleared tool results");
        }
        modified
    }

    /// Find the message index before which we can prune.
    /// Protects the last N "rounds" (user+assistant pairs).
    fn find_protection_boundary(&self, messages: &[ChatMessage]) -> usize {
        if messages.is_empty() {
            return 0;
        }

        let mut rounds_found = 0;
        let mut boundary = messages.len();

        for (i, msg) in messages.iter().enumerate().rev() {
            if msg.role == MessageRole::User {
                let is_tool_result_msg = msg
                    .content
                    .iter()
                    .all(|b| matches!(b, ContentBlock::ToolResult { .. }));
                if !is_tool_result_msg {
                    rounds_found += 1;
                    if rounds_found > self.protect_recent_rounds {
                        boundary = i;
                        break;
                    }
                }
            }
        }

        if rounds_found <= self.protect_recent_rounds {
            return 0;
        }

        boundary
    }

    /// Find safe compaction boundary (not in the middle of a tool chain).
    /// Returns the index at which to split: messages[..index] will be summarized.
    pub fn find_compaction_boundary(messages: &[ChatMessage], min_keep_chars: usize) -> usize {
        if messages.is_empty() {
            return 0;
        }

        let mut kept_chars: usize = 0;
        let mut candidate_boundary = 0;

        for (i, msg) in messages.iter().enumerate().rev() {
            let msg_chars: usize = msg
                .content
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                    ContentBlock::ToolResult { content, .. } => content.len(),
                })
                .sum();

            kept_chars += msg_chars;

            if kept_chars >= min_keep_chars {
                candidate_boundary = i;
                break;
            }
        }

        // Walk forward from candidate to find a safe boundary:
        // Safe = right after an Assistant message that contains Text (not just ToolUse).
        for i in candidate_boundary..messages.len() {
            if messages[i].role == MessageRole::Assistant {
                let has_text = messages[i]
                    .content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Text { .. }));
                let has_only_tool_use = messages[i]
                    .content
                    .iter()
                    .all(|b| matches!(b, ContentBlock::ToolUse { .. }));
                if has_text && !has_only_tool_use {
                    return (i + 1).min(messages.len());
                }
            }
        }

        candidate_boundary
    }

    /// UTF-8 safe head+tail extraction.
    fn head_tail_utf8(s: &str, head_chars: usize, tail_chars: usize) -> (String, String) {
        let head_end = s
            .char_indices()
            .nth(head_chars)
            .map(|(idx, _)| idx)
            .unwrap_or(s.len());

        let char_count = s.chars().count();
        let tail_start_char = char_count.saturating_sub(tail_chars);
        let tail_start = s
            .char_indices()
            .nth(tail_start_char)
            .map(|(idx, _)| idx)
            .unwrap_or(s.len());

        (s[..head_end].to_string(), s[tail_start..].to_string())
    }
}

impl Default for ContextPruner {
    fn default() -> Self {
        Self::new()
    }
}
