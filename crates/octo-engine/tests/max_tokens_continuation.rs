//! Integration tests for the max-tokens auto-continuation module.

use octo_engine::agent::{ContinuationConfig, ContinuationTracker};

#[test]
fn should_continue_returns_true_on_max_tokens() {
    let tracker = ContinuationTracker::new(ContinuationConfig::default());
    assert!(tracker.should_continue("max_tokens"));
}

#[test]
fn should_continue_returns_false_on_end_turn() {
    let tracker = ContinuationTracker::new(ContinuationConfig::default());
    assert!(!tracker.should_continue("end_turn"));
}

#[test]
fn should_continue_returns_false_on_stop_sequence() {
    let tracker = ContinuationTracker::new(ContinuationConfig::default());
    assert!(!tracker.should_continue("stop_sequence"));
}

#[test]
fn returns_false_after_max_continuations_reached() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig {
        max_continuations: 2,
        ..Default::default()
    });
    tracker.record_continuation(100);
    tracker.record_continuation(100);
    // 2 of 2 used — should not continue
    assert!(!tracker.should_continue("max_tokens"));
}

#[test]
fn returns_false_after_output_char_limit_exceeded() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig {
        max_total_output_chars: 1000,
        ..Default::default()
    });
    tracker.record_continuation(1200);
    assert!(!tracker.should_continue("max_tokens"));
}

#[test]
fn record_continuation_increments_counter_and_chars() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig::default());
    assert_eq!(tracker.continuation_count(), 0);
    assert_eq!(tracker.total_output_chars(), 0);

    let prompt = tracker.record_continuation(500);
    assert_eq!(tracker.continuation_count(), 1);
    assert_eq!(tracker.total_output_chars(), 500);
    assert_eq!(prompt, "Please continue where you left off.");

    tracker.record_continuation(300);
    assert_eq!(tracker.continuation_count(), 2);
    assert_eq!(tracker.total_output_chars(), 800);
}

#[test]
fn reset_clears_all_counters() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig::default());
    tracker.record_continuation(5000);
    tracker.record_continuation(5000);
    assert_eq!(tracker.continuation_count(), 2);
    assert_eq!(tracker.total_output_chars(), 10000);

    tracker.reset();
    assert_eq!(tracker.continuation_count(), 0);
    assert_eq!(tracker.total_output_chars(), 0);
    // After reset, continuation is possible again
    assert!(tracker.should_continue("max_tokens"));
}

#[test]
fn default_config_values_are_correct() {
    let cfg = ContinuationConfig::default();
    assert_eq!(cfg.max_continuations, 3);
    assert_eq!(cfg.max_total_output_chars, 120_000);
    assert_eq!(
        cfg.continuation_prompt,
        "Please continue where you left off."
    );
}

#[test]
fn custom_continuation_prompt() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig {
        continuation_prompt: "Keep going from where you stopped.".to_string(),
        ..Default::default()
    });
    let prompt = tracker.record_continuation(100);
    assert_eq!(prompt, "Keep going from where you stopped.");
}

#[test]
fn boundary_exactly_at_max_continuations() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig {
        max_continuations: 3,
        ..Default::default()
    });
    // Use up exactly 2 — should still allow one more
    tracker.record_continuation(10);
    tracker.record_continuation(10);
    assert!(tracker.should_continue("max_tokens"));

    // Use the 3rd
    tracker.record_continuation(10);
    assert!(!tracker.should_continue("max_tokens"));
}

#[test]
fn boundary_exactly_at_char_limit() {
    let mut tracker = ContinuationTracker::new(ContinuationConfig {
        max_total_output_chars: 100,
        ..Default::default()
    });
    // Below limit — ok
    tracker.record_continuation(99);
    assert!(tracker.should_continue("max_tokens"));

    // At limit — should stop
    tracker.record_continuation(1);
    assert!(!tracker.should_continue("max_tokens"));
}
