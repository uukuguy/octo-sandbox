use octo_engine::agent::loop_steps;
use octo_types::{ChatMessage, ContentBlock, MessageRole};

#[test]
fn test_should_execute_parallel_multi_tools_enabled() {
    assert!(loop_steps::should_execute_parallel(3, true));
}

#[test]
fn test_should_execute_parallel_single_tool() {
    assert!(!loop_steps::should_execute_parallel(1, true));
}

#[test]
fn test_should_execute_parallel_disabled() {
    assert!(!loop_steps::should_execute_parallel(3, false));
}

#[test]
fn test_inject_zone_b_empty() {
    let mut messages = vec![];
    loop_steps::inject_zone_b(&mut messages, "");
    assert!(messages.is_empty());
}

#[test]
fn test_inject_zone_b_prepends() {
    let mut messages = vec![ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text {
            text: "hello".into(),
        }],
    }];
    loop_steps::inject_zone_b(&mut messages, "<context>memory</context>");
    assert_eq!(messages.len(), 2);
    assert!(
        matches!(&messages[0].content[0], ContentBlock::Text { text } if text.starts_with("<context>"))
    );
}

#[test]
fn test_inject_zone_b_replaces_existing() {
    let mut messages = vec![ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text {
            text: "<context>old</context>".into(),
        }],
    }];
    loop_steps::inject_zone_b(&mut messages, "<context>new</context>");
    assert_eq!(messages.len(), 1); // replaced, not prepended
    assert!(matches!(&messages[0].content[0], ContentBlock::Text { text } if text.contains("new")));
}

#[test]
fn test_effective_max_rounds_zero_is_unlimited() {
    assert_eq!(loop_steps::effective_max_rounds(0), u32::MAX);
}

#[test]
fn test_effective_max_rounds_normal() {
    assert_eq!(loop_steps::effective_max_rounds(30), 30);
}

#[test]
fn test_maybe_trim_tool_result_under_limit() {
    let result = "short result";
    assert_eq!(loop_steps::maybe_trim_tool_result(result, 1000), result);
}

#[test]
fn test_maybe_trim_tool_result_over_limit() {
    let long = "x".repeat(50000);
    let trimmed = loop_steps::maybe_trim_tool_result(&long, 30000);
    assert!(trimmed.len() < long.len());
    assert!(trimmed.contains("omitted"));
}

#[test]
fn test_check_loop_guard_verdict_allow() {
    use octo_engine::agent::loop_guard::LoopGuardVerdict;
    assert!(loop_steps::check_loop_guard_verdict(&LoopGuardVerdict::Allow).is_none());
}

#[test]
fn test_check_loop_guard_verdict_warn() {
    use octo_engine::agent::loop_guard::LoopGuardVerdict;
    let result =
        loop_steps::check_loop_guard_verdict(&LoopGuardVerdict::Warn("test warning".into()));
    assert!(result.is_none());
}

#[test]
fn test_check_loop_guard_verdict_block() {
    use octo_engine::agent::loop_guard::LoopGuardVerdict;
    let result = loop_steps::check_loop_guard_verdict(&LoopGuardVerdict::Block("blocked!".into()));
    assert!(result.is_some());
    assert!(result.unwrap().contains("Loop Guard"));
}

#[test]
fn test_check_loop_guard_verdict_circuit_break() {
    use octo_engine::agent::loop_guard::LoopGuardVerdict;
    let result =
        loop_steps::check_loop_guard_verdict(&LoopGuardVerdict::CircuitBreak("overload".into()));
    assert!(result.is_some());
    assert!(result.unwrap().contains("Loop Guard"));
}

// ── should_force_text_only ──────────────────────────────────────────

#[test]
fn test_force_text_only_before_last_round() {
    // round 2 < max_iterations(5) - 1 = 4 → false
    assert!(!loop_steps::should_force_text_only(2, 5, true));
}

#[test]
fn test_force_text_only_at_last_round() {
    // round 4 == max_iterations(5) - 1 → true
    assert!(loop_steps::should_force_text_only(4, 5, true));
}

#[test]
fn test_force_text_only_past_last_round() {
    // round 6 > max_iterations(5) - 1 → true (safety net)
    assert!(loop_steps::should_force_text_only(6, 5, true));
}

#[test]
fn test_force_text_only_disabled() {
    // force_text_at_last == false → always false
    assert!(!loop_steps::should_force_text_only(4, 5, false));
    assert!(!loop_steps::should_force_text_only(0, 1, false));
}

#[test]
fn test_force_text_only_max_zero_unlimited() {
    // max_iterations == 0 means unlimited → false
    assert!(!loop_steps::should_force_text_only(0, 0, true));
    assert!(!loop_steps::should_force_text_only(999, 0, true));
}

#[test]
fn test_force_text_only_max_one_round_zero() {
    // max_iterations == 1, round 0 → 0 >= 1-1 → true
    assert!(loop_steps::should_force_text_only(0, 1, true));
}

// ── generate_error_hint ───────────────────────────────────────────────

#[test]
fn test_generate_error_hint_contains_tool_name_and_error() {
    let hint = loop_steps::generate_error_hint("bash", "permission denied");
    assert!(hint.contains("bash"));
    assert!(hint.contains("permission denied"));
}

#[test]
fn test_generate_error_hint_contains_alternatives() {
    let hint = loop_steps::generate_error_hint("file_read", "not found");
    assert!(hint.contains("alternative approaches"));
    assert!(hint.contains("different tool"));
    assert!(hint.contains("Modify the parameters"));
    assert!(hint.contains("smaller steps"));
    assert!(hint.contains("clarification"));
}

// ── should_append_error_hint ──────────────────────────────────────────

#[test]
fn test_should_append_error_hint_true_on_error() {
    assert!(loop_steps::should_append_error_hint(true, 1));
}

#[test]
fn test_should_append_error_hint_false_when_no_error() {
    assert!(!loop_steps::should_append_error_hint(false, 0));
    assert!(!loop_steps::should_append_error_hint(false, 1));
    assert!(!loop_steps::should_append_error_hint(false, 5));
}

#[test]
fn test_should_append_error_hint_false_after_three_consecutive() {
    // consecutive_errors > 3 → suppress
    assert!(!loop_steps::should_append_error_hint(true, 4));
    assert!(!loop_steps::should_append_error_hint(true, 10));
}

#[test]
fn test_should_append_error_hint_true_at_zero_consecutive() {
    assert!(loop_steps::should_append_error_hint(true, 0));
}
