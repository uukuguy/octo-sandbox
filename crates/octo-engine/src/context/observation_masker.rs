use octo_types::{ChatMessage, ContentBlock, MessageRole};
use tracing::debug;

/// Configuration for observation masking
#[derive(Debug, Clone)]
pub struct ObservationMaskConfig {
    /// Number of recent assistant turns to keep full output
    pub keep_recent_turns: usize,
    /// Placeholder template for masked content (use {chars} for character count)
    pub placeholder_template: String,
    /// Minimum output length to consider for masking (shorter outputs are kept)
    pub min_mask_length: usize,
}

impl Default for ObservationMaskConfig {
    fn default() -> Self {
        Self {
            keep_recent_turns: 3,
            placeholder_template: "[output hidden - {chars} chars]".to_string(),
            min_mask_length: 100,
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

        for (i, msg) in messages.iter().enumerate() {
            if i < mask_before_index {
                let (masked_msg, did_mask) = self.mask_message(msg);
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
    fn mask_message(&self, msg: &ChatMessage) -> (ChatMessage, bool) {
        let mut any_masked = false;
        let new_content: Vec<ContentBlock> = msg
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } if content.len() >= self.config.min_mask_length => {
                    any_masked = true;
                    let placeholder = self
                        .config
                        .placeholder_template
                        .replace("{chars}", &content.len().to_string());
                    ContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: placeholder,
                        is_error: *is_error,
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
