//! Dedicated token counting implementations.
//!
//! Provides both a lightweight estimate-based counter (no external deps)
//! and a trait for plugging in more accurate counters (e.g. tiktoken-rs).

use super::manager::{EstimateCounter, TokenCounter};
use octo_types::message::ChatMessage;

/// A CJK-aware token counter that distinguishes between ASCII and
/// multi-byte characters for more accurate estimation.
///
/// Rules:
/// - ASCII characters: ~0.25 tokens each (4 chars ≈ 1 token)
/// - CJK / non-ASCII: ~0.67 tokens each (1.5 chars ≈ 1 token)
/// - Per-message overhead: 4 tokens (role tag, delimiters)
///
/// This is the same algorithm as `EstimateCounter`, re-exported here
/// for discoverability as the P2-6 token counting solution.
pub type CjkAwareCounter = EstimateCounter;

/// Count tokens for a slice of messages using the given counter.
pub fn count_messages_tokens(counter: &dyn TokenCounter, messages: &[ChatMessage]) -> usize {
    counter.count_messages(messages)
}

/// Count tokens for plain text using the given counter.
pub fn count_text_tokens(counter: &dyn TokenCounter, text: &str) -> usize {
    counter.count(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cjk_aware_counter_ascii() {
        let counter = EstimateCounter;
        // "hello" = 5 ASCII chars → 5 * 0.25 = 1.25 → ceil = 2
        assert_eq!(counter.count("hello"), 2);
    }

    #[test]
    fn test_cjk_aware_counter_cjk() {
        let counter = EstimateCounter;
        // "你好" = 2 CJK chars → 2 * 0.67 = 1.34 → ceil = 2
        assert_eq!(counter.count("你好"), 2);
    }

    #[test]
    fn test_cjk_aware_counter_mixed() {
        let counter = EstimateCounter;
        // "hello你好" = 5 ASCII (1.25) + 2 CJK (1.34) = 2.59 → ceil = 3
        assert_eq!(counter.count("hello你好"), 3);
    }

    #[test]
    fn test_cjk_aware_counter_empty() {
        let counter = EstimateCounter;
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_count_helpers() {
        let counter = EstimateCounter;
        assert_eq!(count_text_tokens(&counter, "test"), 1);
    }
}
