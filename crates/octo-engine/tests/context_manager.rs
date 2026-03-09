use octo_engine::context::{ContextManager, EstimateCounter, TokenCounter};
use octo_types::message::ChatMessage;

// ---------------------------------------------------------------------------
// EstimateCounter — pure-English text
// ---------------------------------------------------------------------------

#[test]
fn estimate_counter_ascii_text() {
    let counter = EstimateCounter;
    // 12 ASCII chars → 12 * 0.25 = 3.0 → ceil = 3
    let tokens = counter.count("hello world!");
    assert_eq!(tokens, 3);
}

#[test]
fn estimate_counter_ascii_longer() {
    let counter = EstimateCounter;
    // 20 ASCII chars → 20 * 0.25 = 5.0 → 5
    let tokens = counter.count("abcdefghij0123456789");
    assert_eq!(tokens, 5);
}

// ---------------------------------------------------------------------------
// EstimateCounter — Chinese text
// ---------------------------------------------------------------------------

#[test]
fn estimate_counter_chinese_text() {
    let counter = EstimateCounter;
    // 4 CJK chars → 4 * 0.67 = 2.68 → ceil = 3
    let tokens = counter.count("\u{4f60}\u{597d}\u{4e16}\u{754c}"); // 你好世界
    assert_eq!(tokens, 3);
}

// ---------------------------------------------------------------------------
// EstimateCounter — mixed text
// ---------------------------------------------------------------------------

#[test]
fn estimate_counter_mixed_text() {
    let counter = EstimateCounter;
    // "hi你好" → 2 ASCII (0.5) + 2 CJK (1.34) = 1.84 → ceil = 2
    let tokens = counter.count("hi\u{4f60}\u{597d}");
    assert_eq!(tokens, 2);
}

// ---------------------------------------------------------------------------
// EstimateCounter — empty text
// ---------------------------------------------------------------------------

#[test]
fn estimate_counter_empty() {
    let counter = EstimateCounter;
    assert_eq!(counter.count(""), 0);
}

// ---------------------------------------------------------------------------
// EstimateCounter — count_messages
// ---------------------------------------------------------------------------

#[test]
fn estimate_counter_messages() {
    let counter = EstimateCounter;
    let messages = vec![
        ChatMessage::user("hello"),   // 5 * 0.25 = 1.25 → ceil 2, + 4 = 6
        ChatMessage::assistant("hi"), // 2 * 0.25 = 0.5 → ceil 1, + 4 = 5
    ];
    let total = counter.count_messages(&messages);
    assert_eq!(total, 11); // 6 + 5
}

// ---------------------------------------------------------------------------
// ContextManager — budget_snapshot basics
// ---------------------------------------------------------------------------

#[test]
fn budget_snapshot_basic() {
    let mgr = ContextManager::new(Box::new(EstimateCounter), 1000);
    let messages = vec![ChatMessage::user("test")];
    let snap = mgr.budget_snapshot("system prompt", &messages);

    assert_eq!(snap.total_budget, 1000);
    assert!(snap.system_tokens > 0);
    assert!(snap.message_tokens > 0);
    // tool_tokens = ceil(1000 * 0.15) = 150
    assert_eq!(snap.tool_tokens, 150);
    let used = snap.system_tokens + snap.message_tokens + snap.tool_tokens;
    assert_eq!(snap.remaining, 1000 - used);
    let expected_pct = used as f32 / 1000.0;
    assert!((snap.usage_pct - expected_pct).abs() < 0.001);
}

// ---------------------------------------------------------------------------
// ContextManager — needs_pruning threshold
// ---------------------------------------------------------------------------

#[test]
fn needs_pruning_below_threshold() {
    let mgr = ContextManager::new(Box::new(EstimateCounter), 10000);
    let snap = mgr.budget_snapshot("hi", &[]);
    // usage should be very low
    assert!(!mgr.needs_pruning(&snap));
}

#[test]
fn needs_pruning_above_threshold() {
    // Use a tiny budget so even a small prompt exceeds 85%.
    let mgr = ContextManager::new(Box::new(EstimateCounter), 10);
    // system_tokens ~ ceil(44*0.25)=11, tool_tokens = ceil(10*0.15)=2 → already > 10
    let snap = mgr.budget_snapshot("a]long system prompt that is big", &[]);
    assert!(mgr.needs_pruning(&snap));
}

// ---------------------------------------------------------------------------
// ContextManager — available_tokens
// ---------------------------------------------------------------------------

#[test]
fn available_tokens_matches_remaining() {
    let mgr = ContextManager::new(Box::new(EstimateCounter), 5000);
    let snap = mgr.budget_snapshot("sys", &[ChatMessage::user("msg")]);
    assert_eq!(mgr.available_tokens(&snap), snap.remaining);
}
