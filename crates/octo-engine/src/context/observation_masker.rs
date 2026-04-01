use octo_types::{ChatMessage, ContentBlock, MessageRole};
use tracing::debug;

use super::tool_use_summary;

/// Tools whose output can be safely compressed (large, repetitive results).
pub const DEFAULT_COMPACTABLE_TOOLS: &[&str] = &[
    "bash", "file_read", "file_write", "file_edit",
    "grep", "glob", "find", "web_fetch", "web_search",
];

/// Configuration for observation masking
#[derive(Debug, Clone)]
pub struct ObservationMaskConfig {
    /// Number of recent assistant turns to keep full output
    pub keep_recent_turns: usize,
    /// Placeholder template for masked content (use {chars} for character count)
    pub placeholder_template: String,
    /// Minimum output length to consider for masking (shorter outputs are kept)
    pub min_mask_length: usize,
    /// Time-based trigger: mask if no assistant message for N minutes
    pub time_trigger_minutes: Option<u64>,
    /// Only mask output from these tools (None = mask all eligible tools)
    pub compactable_tools: Option<std::collections::HashSet<String>>,
}

impl Default for ObservationMaskConfig {
    fn default() -> Self {
        Self {
            keep_recent_turns: 3,
            placeholder_template: "[output hidden - {chars} chars]".to_string(),
            min_mask_length: 100,
            time_trigger_minutes: None,
            compactable_tools: None,
        }
    }
}

/// Masks tool outputs in older conversation turns to save tokens.
///
/// Strategy:
/// - Keep the most recent N turns fully intact
/// - For older turns: preserve tool name + arguments (ToolUse), mask tool results (ToolResult)
/// - Replace masked content with a placeholder showing original character count
///
/// A "turn" is defined by each assistant message. The turn includes
/// the assistant message itself and all subsequent user messages (which
/// may carry ToolResult content blocks) until the next assistant message.
pub struct ObservationMasker {
    config: ObservationMaskConfig,
}

impl ObservationMasker {
    pub fn new(config: ObservationMaskConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(ObservationMaskConfig::default())
    }

    /// Check if time-based micro-compaction should trigger.
    pub fn should_time_trigger(
        &self,
        elapsed_since_last_assistant: Option<std::time::Duration>,
    ) -> bool {
        if let (Some(threshold), Some(elapsed)) = (
            self.config.time_trigger_minutes,
            elapsed_since_last_assistant,
        ) {
            return elapsed.as_secs() / 60 >= threshold;
        }
        false
    }

    /// Apply observation masking to a conversation history.
    /// Returns a new vector with masked messages (does not modify original).
    pub fn mask(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        if messages.is_empty() {
            return Vec::new();
        }

        // Find turn boundaries: each assistant message starts a new turn
        let turn_starts: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| msg.role == MessageRole::Assistant)
            .map(|(i, _)| i)
            .collect();

        // Determine the cutoff: keep last N turns unmasked
        let mask_before_turn = turn_starts
            .len()
            .saturating_sub(self.config.keep_recent_turns);

        // Get the message index where masking stops
        let mask_before_index = if mask_before_turn > 0 {
            turn_starts[mask_before_turn]
        } else {
            0
        };

        let mut result = Vec::with_capacity(messages.len());
        let mut masked_count = 0;

        let tool_info_map: std::collections::HashMap<String, (String, String)> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|block| {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    Some((id.clone(), (name.clone(), input.to_string())))
                } else {
                    None
                }
            })
            .collect();

        for (i, msg) in messages.iter().enumerate() {
            if i < mask_before_index {
                let (masked_msg, did_mask) = self.mask_message(msg, &tool_info_map);
                if did_mask {
                    masked_count += 1;
                }
                result.push(masked_msg);
            } else {
                result.push(msg.clone());
            }
        }

        if masked_count > 0 {
            debug!(
                masked_count,
                mask_before_index, "ObservationMasker: masked tool results in old turns"
            );
        }

        result
    }

    /// Create a masked version of a message, replacing eligible ToolResult blocks.
    /// Returns (message, whether_any_block_was_masked).
    fn mask_message(
        &self,
        msg: &ChatMessage,
        tool_info_map: &std::collections::HashMap<String, (String, String)>,
    ) -> (ChatMessage, bool) {
        let mut any_masked = false;
        let new_content: Vec<ContentBlock> = msg
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    // Check tool whitelist
                    if let Some(ref whitelist) = self.config.compactable_tools {
                        if let Some((tool_name, _)) = tool_info_map.get(tool_use_id) {
                            if !whitelist.contains(tool_name) {
                                return block.clone();
                            }
                        }
                    }
                    // Check minimum length
                    if content.len() >= self.config.min_mask_length {
                        any_masked = true;
                        // Use heuristic summary if we can identify the tool
                        let placeholder =
                            if let Some((tool_name, tool_input)) = tool_info_map.get(tool_use_id) {
                                tool_use_summary::summarize_tool_output(
                                    tool_name, tool_input, content, *is_error,
                                )
                            } else {
                                self.config
                                    .placeholder_template
                                    .replace("{chars}", &content.len().to_string())
                            };
                        ContentBlock::ToolResult {
                            tool_use_id: tool_use_id.clone(),
                            content: placeholder,
                            is_error: *is_error,
                        }
                    } else {
                        block.clone()
                    }
                }
                other => other.clone(),
            })
            .collect();

        (
            ChatMessage {
                role: msg.role.clone(),
                content: new_content,
            },
            any_masked,
        )
    }

    /// Calculate token savings estimate (approximate, based on character count).
    /// Returns (original_chars, masked_chars).
    pub fn estimate_savings(&self, messages: &[ChatMessage]) -> (usize, usize) {
        let original: usize = messages
            .iter()
            .flat_map(|m| &m.content)
            .map(Self::block_char_len)
            .sum();

        let masked = self.mask(messages);
        let after: usize = masked
            .iter()
            .flat_map(|m| &m.content)
            .map(Self::block_char_len)
            .sum();

        (original, after)
    }

    fn block_char_len(block: &ContentBlock) -> usize {
        match block {
            ContentBlock::Text { text } => text.len(),
            ContentBlock::ToolUse { input, .. } => input.to_string().len(),
            ContentBlock::ToolResult { content, .. } => content.len(),
            ContentBlock::Image { data, .. } => data.len(),
            ContentBlock::Document { data, .. } => data.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_compactable_tools_list() {
        assert!(DEFAULT_COMPACTABLE_TOOLS.contains(&"bash"));
        assert!(DEFAULT_COMPACTABLE_TOOLS.contains(&"file_read"));
        assert!(!DEFAULT_COMPACTABLE_TOOLS.contains(&"memory_store"));
    }

    #[test]
    fn test_should_time_trigger_none() {
        let masker = ObservationMasker::with_defaults();
        assert!(!masker.should_time_trigger(None));
        assert!(!masker.should_time_trigger(Some(std::time::Duration::from_secs(600))));
    }

    #[test]
    fn test_should_time_trigger_configured() {
        let config = ObservationMaskConfig {
            time_trigger_minutes: Some(5),
            ..Default::default()
        };
        let masker = ObservationMasker::new(config);
        assert!(!masker.should_time_trigger(Some(std::time::Duration::from_secs(180))));
        assert!(masker.should_time_trigger(Some(std::time::Duration::from_secs(300))));
        assert!(masker.should_time_trigger(Some(std::time::Duration::from_secs(600))));
    }

    #[test]
    fn test_whitelist_filtering() {
        let mut whitelist = std::collections::HashSet::new();
        whitelist.insert("bash".to_string());

        let config = ObservationMaskConfig {
            compactable_tools: Some(whitelist),
            min_mask_length: 10,
            keep_recent_turns: 1,
            ..Default::default()
        };
        let masker = ObservationMasker::new(config);

        let messages = vec![
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![
                    ContentBlock::ToolUse {
                        id: "call_1".to_string(),
                        name: "bash".to_string(),
                        input: serde_json::json!({}),
                    },
                    ContentBlock::ToolUse {
                        id: "call_2".to_string(),
                        name: "memory_store".to_string(),
                        input: serde_json::json!({}),
                    },
                ],
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![
                    ContentBlock::ToolResult {
                        tool_use_id: "call_1".to_string(),
                        content: "x".repeat(200),
                        is_error: false,
                    },
                    ContentBlock::ToolResult {
                        tool_use_id: "call_2".to_string(),
                        content: "y".repeat(200),
                        is_error: false,
                    },
                ],
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text { text: "done".to_string() }],
            },
        ];

        let masked = masker.mask(&messages);
        let user_msg = &masked[1];

        if let ContentBlock::ToolResult { content, .. } = &user_msg.content[0] {
            // With tool_use_summary, bash output is replaced with a heuristic summary
            assert!(
                content.starts_with("[bash("),
                "bash result should be masked with summary, got: {content}"
            );
        }
        if let ContentBlock::ToolResult { content, .. } = &user_msg.content[1] {
            assert_eq!(content.len(), 200, "memory_store should not be masked");
        }
    }
}
