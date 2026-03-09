//! Precise token counter using tiktoken-rs (requires `tiktoken` feature).
//!
//! Uses the cl100k_base encoding (compatible with Claude and GPT-4 family
//! models) for accurate token counting instead of heuristic estimation.

use tiktoken_rs::CoreBPE;

use super::manager::TokenCounter;
use octo_types::message::ChatMessage;

/// Token counter that uses tiktoken for precise token counting.
///
/// Falls back to cl100k_base encoding (used by Claude and GPT-4).
/// Each message carries an additional 4-token overhead for role tags
/// and delimiters, matching the convention in `EstimateCounter`.
pub struct TiktokenCounter {
    bpe: CoreBPE,
}

impl TiktokenCounter {
    /// Create a new counter with the default cl100k_base encoding.
    pub fn new() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().expect("Failed to load cl100k_base encoding"),
        }
    }

    /// Create a counter with a custom BPE encoding.
    pub fn with_encoding(bpe: CoreBPE) -> Self {
        Self { bpe }
    }
}

impl Default for TiktokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for TiktokenCounter {
    fn count(&self, text: &str) -> usize {
        self.bpe.encode_with_special_tokens(text).len()
    }

    fn count_messages(&self, messages: &[ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| self.count(&m.text_content()) + 4) // 4 tokens per-message overhead
            .sum()
    }
}
