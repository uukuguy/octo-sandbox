use octo_engine::context::{
    CompactionAction, CompactionConfig, CompactionStrategy, ContextPruner, DegradationLevel,
};
use octo_types::{ChatMessage, ContentBlock, MessageRole};

// ---------------------------------------------------------------------------
// Helper: build a conversation of N rounds, each round = user msg + assistant
// ToolUse + user ToolResult with `result_size` chars of content.
// ---------------------------------------------------------------------------

fn make_conversation(rounds: usize, result_size: usize) -> Vec<ChatMessage> {
    let mut msgs = Vec::with_capacity(rounds * 3);
    let filler: String = "x".repeat(result_size);

    for i in 0..rounds {
        // 1. User text message
        msgs.push(ChatMessage::user(format!("User question round {}", i)));

        // 2. Assistant tool-use message
        msgs.push(ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: format!("tool_{}", i),
                name: "bash".to_string(),
                input: serde_json::json!({"command": "echo hello"}),
            }],
        });

        // 3. User message carrying the tool result
        msgs.push(ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: format!("tool_{}", i),
                content: filler.clone(),
                is_error: false,
            }],
        });
    }

    msgs
}

/// Compute total character length across all content blocks in a message list.
fn total_content_chars(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .flat_map(|m| m.content.iter())
        .map(|block| match block {
            ContentBlock::Text { text } => text.len(),
            ContentBlock::ToolUse { input, name, id } => {
                name.len() + id.len() + input.to_string().len()
            }
            ContentBlock::ToolResult { content, .. } => content.len(),
            ContentBlock::Image { data, .. } => data.len(),
            ContentBlock::Document { data, .. } => data.len(),
        })
        .sum()
}

// ===========================================================================
// Test 1: DegradationLevel::None does not modify messages
// ===========================================================================

#[test]
fn level_none_does_not_modify_messages() {
    let pruner = ContextPruner::new();
    let mut msgs = make_conversation(7, 200); // 21 messages
    let original_len = msgs.len();

    let modified = pruner.apply(&mut msgs, DegradationLevel::None);

    assert_eq!(modified, 0, "None level should modify 0 blocks");
    assert_eq!(
        msgs.len(),
        original_len,
        "None level should not change message count"
    );
}

// ===========================================================================
// Test 2: SoftTrim truncates old tool results (head/tail)
// ===========================================================================

#[test]
fn level_soft_trim_truncates_old_tool_results() {
    let pruner = ContextPruner::new(); // protect_recent_rounds = 2

    // We need enough rounds so that some tool results fall outside the
    // protected boundary. The protection boundary is based on "real" user
    // messages (non-tool-result), so we need > 2 rounds of user text msgs.
    // With 6 rounds the last 2 user-text rounds are protected, leaving
    // rounds 0..3 (indices 0..12) eligible for trimming.
    //
    // Tool result size must exceed SOFT_TRIM_HEAD + SOFT_TRIM_TAIL + 100
    // = 1500 + 500 + 100 = 2100 chars.
    let mut msgs = make_conversation(6, 3000);
    let original_len = msgs.len();

    let modified = pruner.apply(&mut msgs, DegradationLevel::SoftTrim);

    assert!(
        modified > 0,
        "SoftTrim should have modified at least one tool result"
    );
    assert_eq!(
        msgs.len(),
        original_len,
        "SoftTrim should not remove messages, only truncate content"
    );

    // Verify that at least one old tool result now contains the omission marker
    let has_omitted = msgs.iter().any(|m| {
        m.content.iter().any(|b| {
            if let ContentBlock::ToolResult { content, .. } = b {
                content.contains("[... omitted")
            } else {
                false
            }
        })
    });
    assert!(
        has_omitted,
        "At least one tool result should contain '[... omitted' after SoftTrim"
    );
}

// ===========================================================================
// Test 3: AutoCompaction replaces old tool results with placeholders
// ===========================================================================

#[test]
fn level_auto_compaction_keeps_recent_messages() {
    let pruner = ContextPruner::new();

    // 20 rounds -> 60 messages; tool results have content > 100 chars.
    // AutoCompaction keeps last 10 messages, replaces tool results in the
    // first 50 with placeholders (if content > 100 chars).
    let mut msgs = make_conversation(20, 500);
    let original_len = msgs.len();

    let modified = pruner.apply(&mut msgs, DegradationLevel::AutoCompaction);

    assert!(
        modified > 0,
        "AutoCompaction should have modified old tool results"
    );
    assert_eq!(
        msgs.len(),
        original_len,
        "AutoCompaction should not remove messages"
    );

    // Verify old tool results contain the AutoCompaction placeholder
    let has_placeholder = msgs.iter().any(|m| {
        m.content.iter().any(|b| {
            if let ContentBlock::ToolResult { content, .. } = b {
                content.contains("[Tool result omitted (AutoCompaction)")
            } else {
                false
            }
        })
    });
    assert!(
        has_placeholder,
        "Old tool results should have AutoCompaction placeholder"
    );

    // Verify the last 10 messages are NOT replaced
    let recent_10 = &msgs[msgs.len() - 10..];
    for msg in recent_10 {
        for block in &msg.content {
            if let ContentBlock::ToolResult { content, .. } = block {
                assert!(
                    !content.contains("[Tool result omitted (AutoCompaction)"),
                    "Recent messages should NOT be compacted"
                );
            }
        }
    }
}

// ===========================================================================
// Test 4: OverflowCompaction drains old messages, keeps only 4
// ===========================================================================

#[test]
fn level_overflow_compaction_drains_old_messages() {
    let pruner = ContextPruner::new();

    // 20 rounds -> 60 messages; after OverflowCompaction only 4 should remain.
    let mut msgs = make_conversation(20, 200);
    assert!(msgs.len() > 4);

    let modified = pruner.apply(&mut msgs, DegradationLevel::OverflowCompaction);

    assert!(
        modified > 0,
        "OverflowCompaction should have drained messages"
    );
    assert_eq!(
        msgs.len(),
        4, // OVERFLOW_COMPACTION_KEEP = 4
        "OverflowCompaction should keep exactly 4 messages"
    );
}

// ===========================================================================
// Test 5: ToolResultTruncation caps the last tool result to 8000 chars
// ===========================================================================

#[test]
fn level_tool_result_truncation_caps_last_result() {
    let pruner = ContextPruner::new();

    // Build a conversation where the LAST tool result exceeds 8000 chars.
    let mut msgs = make_conversation(3, 12_000);
    let original_len = msgs.len();

    let modified = pruner.apply(&mut msgs, DegradationLevel::ToolResultTruncation);

    assert_eq!(
        modified, 1,
        "ToolResultTruncation should modify exactly 1 block"
    );
    assert_eq!(
        msgs.len(),
        original_len,
        "ToolResultTruncation should not remove messages"
    );

    // Find the last tool result (searching from the end)
    let last_tool_result = msgs
        .iter()
        .rev()
        .flat_map(|m| m.content.iter())
        .find_map(|b| {
            if let ContentBlock::ToolResult { content, .. } = b {
                Some(content.clone())
            } else {
                None
            }
        })
        .expect("Should have at least one tool result");

    assert!(
        last_tool_result.contains("[... truncated"),
        "Last tool result should contain '[... truncated' marker"
    );
    // The truncated result should be shorter than the original 12000 chars
    // (8000 chars of content + the truncation marker text)
    assert!(
        last_tool_result.len() < 12_000,
        "Truncated result ({}) should be shorter than original 12000",
        last_tool_result.len()
    );
}

// ===========================================================================
// Test 6: Escalating degradation progressively reduces total content
// ===========================================================================

#[test]
fn escalating_degradation_progressively_reduces_context() {
    let levels = [
        DegradationLevel::SoftTrim,
        DegradationLevel::AutoCompaction,
        DegradationLevel::OverflowCompaction,
        DegradationLevel::ToolResultTruncation,
    ];

    // Use large tool results so every level has something to work on.
    // We need enough rounds to exceed protect_recent_rounds for SoftTrim,
    // exceed AUTO_COMPACTION_KEEP for AutoCompaction, etc.
    let base_msgs = make_conversation(20, 10_000);
    let baseline_chars = total_content_chars(&base_msgs);

    let mut prev_chars = baseline_chars;

    for level in &levels {
        let mut msgs = base_msgs.clone();

        // Apply all levels up to and including the current one
        let pruner = ContextPruner::new();
        for l in &levels {
            pruner.apply(&mut msgs, *l);
            if l == level {
                break;
            }
        }

        let current_chars = total_content_chars(&msgs);
        assert!(
            current_chars < baseline_chars,
            "Level {:?} should reduce total content from baseline {} but got {}",
            level,
            baseline_chars,
            current_chars,
        );

        // Each successive level should reduce content further (or at least
        // not increase it) compared to the previous cumulative application.
        assert!(
            current_chars <= prev_chars,
            "Level {:?}: content {} should be <= previous {}",
            level,
            current_chars,
            prev_chars,
        );
        prev_chars = current_chars;
    }
}

// ===========================================================================
// Test 7: Summarize compaction strategy returns NeedsSummarize action
// ===========================================================================

#[test]
fn compaction_strategy_summarize_returns_action() {
    let pruner = ContextPruner::new().with_compaction_config(CompactionConfig {
        strategy: CompactionStrategy::Summarize,
        summary_max_tokens: 500,
    });

    // Need >= 8 messages for compaction to activate (MIN_MESSAGES_FOR_COMPACTION).
    let msgs = make_conversation(4, 200); // 12 messages

    let action = pruner.plan_compaction(&msgs);

    match action {
        CompactionAction::NeedsSummarize {
            messages_to_summarize,
            insert_position,
        } => {
            assert!(
                !messages_to_summarize.is_empty(),
                "messages_to_summarize should not be empty"
            );
            assert_eq!(
                insert_position, 0,
                "insert_position should be 0 (beginning of conversation)"
            );
        }
        other => panic!(
            "Expected CompactionAction::NeedsSummarize, got {:?}",
            other
        ),
    }
}
