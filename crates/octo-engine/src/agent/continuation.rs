//! Max-tokens auto-continuation strategy.
//!
//! When the LLM stops because it hit `max_tokens`, the tracker decides
//! whether to automatically re-prompt so the model can finish its output.
//! This module is intentionally standalone — it does **not** modify the
//! main agent loop.  A future integration point in `AgentLoop` can call
//! `should_continue` / `record_continuation` at the appropriate place.

/// Configuration for automatic continuation on `max_tokens` truncation.
#[derive(Debug, Clone)]
pub struct ContinuationConfig {
    /// Maximum number of continuation rounds per turn.
    pub max_continuations: u32,
    /// Cumulative character budget across all continuations (ZeroClaw: 120 K).
    pub max_total_output_chars: usize,
    /// The user-role message injected to ask the model to keep going.
    pub continuation_prompt: String,
}

impl Default for ContinuationConfig {
    fn default() -> Self {
        Self {
            max_continuations: 3,
            max_total_output_chars: 120_000,
            continuation_prompt: "Please continue where you left off.".to_string(),
        }
    }
}

/// Tracks how many continuations have been issued in the current turn and
/// how many output characters have been accumulated.
#[derive(Debug)]
pub struct ContinuationTracker {
    config: ContinuationConfig,
    continuation_count: u32,
    total_output_chars: usize,
}

impl ContinuationTracker {
    /// Create a new tracker with the given configuration.
    pub fn new(config: ContinuationConfig) -> Self {
        Self {
            config,
            continuation_count: 0,
            total_output_chars: 0,
        }
    }

    /// Returns `true` when the model should be re-prompted.
    ///
    /// Conditions:
    /// 1. The stop reason is `"max_tokens"`.
    /// 2. We have not yet reached `max_continuations`.
    /// 3. Cumulative output is still below `max_total_output_chars`.
    pub fn should_continue(&self, stop_reason: &str) -> bool {
        stop_reason == "max_tokens"
            && self.continuation_count < self.config.max_continuations
            && self.total_output_chars < self.config.max_total_output_chars
    }

    /// Record a continuation round.
    ///
    /// Increments the internal counter, adds `output_chars` to the running
    /// total, and returns the continuation prompt to inject.
    pub fn record_continuation(&mut self, output_chars: usize) -> String {
        self.continuation_count += 1;
        self.total_output_chars += output_chars;
        self.config.continuation_prompt.clone()
    }

    /// How many continuations have been issued so far in this turn.
    pub fn continuation_count(&self) -> u32 {
        self.continuation_count
    }

    /// Total output characters accumulated across all continuations.
    pub fn total_output_chars(&self) -> usize {
        self.total_output_chars
    }

    /// Reset counters — call this at the start of each new turn.
    pub fn reset(&mut self) {
        self.continuation_count = 0;
        self.total_output_chars = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = ContinuationConfig::default();
        assert_eq!(cfg.max_continuations, 3);
        assert_eq!(cfg.max_total_output_chars, 120_000);
        assert_eq!(
            cfg.continuation_prompt,
            "Please continue where you left off."
        );
    }

    #[test]
    fn should_continue_on_max_tokens() {
        let tracker = ContinuationTracker::new(ContinuationConfig::default());
        assert!(tracker.should_continue("max_tokens"));
    }

    #[test]
    fn should_not_continue_on_end_turn() {
        let tracker = ContinuationTracker::new(ContinuationConfig::default());
        assert!(!tracker.should_continue("end_turn"));
    }

    #[test]
    fn should_not_continue_after_max_reached() {
        let mut tracker = ContinuationTracker::new(ContinuationConfig {
            max_continuations: 2,
            ..Default::default()
        });
        tracker.record_continuation(100);
        tracker.record_continuation(100);
        assert!(!tracker.should_continue("max_tokens"));
    }

    #[test]
    fn should_not_continue_after_char_limit() {
        let mut tracker = ContinuationTracker::new(ContinuationConfig {
            max_total_output_chars: 500,
            ..Default::default()
        });
        tracker.record_continuation(600);
        assert!(!tracker.should_continue("max_tokens"));
    }

    #[test]
    fn record_continuation_increments_counter() {
        let mut tracker = ContinuationTracker::new(ContinuationConfig::default());
        assert_eq!(tracker.continuation_count(), 0);
        let prompt = tracker.record_continuation(42);
        assert_eq!(tracker.continuation_count(), 1);
        assert_eq!(tracker.total_output_chars(), 42);
        assert_eq!(prompt, "Please continue where you left off.");
    }

    #[test]
    fn reset_clears_state() {
        let mut tracker = ContinuationTracker::new(ContinuationConfig::default());
        tracker.record_continuation(1000);
        tracker.record_continuation(2000);
        assert_eq!(tracker.continuation_count(), 2);
        assert_eq!(tracker.total_output_chars(), 3000);

        tracker.reset();
        assert_eq!(tracker.continuation_count(), 0);
        assert_eq!(tracker.total_output_chars(), 0);
        assert!(tracker.should_continue("max_tokens"));
    }
}
