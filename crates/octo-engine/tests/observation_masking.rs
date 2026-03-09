use octo_engine::context::{ObservationMaskConfig, ObservationMasker};
use octo_types::{ChatMessage, ContentBlock, MessageRole};

fn user_msg(text: &str) -> ChatMessage {
    ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    }
}

fn assistant_msg(text: &str) -> ChatMessage {
    ChatMessage {
        role: MessageRole::Assistant,
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    }
}

fn tool_result_msg(id: &str, content: &str) -> ChatMessage {
    ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: id.to_string(),
            content: content.to_string(),
            is_error: false,
        }],
    }
}

fn long_content(n: usize) -> String {
    "x".repeat(n)
}

#[test]
fn test_empty_messages() {
    let masker = ObservationMasker::with_defaults();
    let result = masker.mask(&[]);
    assert!(result.is_empty());
}

#[test]
fn test_no_masking_within_recent_turns() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 3,
        ..Default::default()
    });

    // 2 turns (< 3), nothing should be masked
    let msgs = vec![
        user_msg("hello"),
        assistant_msg("hi"),
        tool_result_msg("t1", &long_content(200)),
        user_msg("next"),
        assistant_msg("reply"),
        tool_result_msg("t2", &long_content(200)),
    ];

    let result = masker.mask(&msgs);
    // All content should be identical
    for (orig, masked) in msgs.iter().zip(result.iter()) {
        assert_eq!(orig.content.len(), masked.content.len());
    }
}

#[test]
fn test_mask_old_tool_results() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 1,
        min_mask_length: 50,
        ..Default::default()
    });

    let long = long_content(200);
    let msgs = vec![
        user_msg("q1"),
        assistant_msg("a1"),
        tool_result_msg("t1", &long), // turn 1 - should be masked
        user_msg("q2"),
        assistant_msg("a2"),
        tool_result_msg("t2", &long), // turn 2 (recent) - kept
    ];

    let result = masker.mask(&msgs);

    // Check the old tool result (index 2) is masked
    match &result[2].content[0] {
        ContentBlock::ToolResult { content, .. } => {
            assert!(content.contains("hidden"));
            assert!(content.contains("200"));
        }
        _ => panic!("Expected ToolResult"),
    }

    // Check the recent tool result (index 5) is NOT masked
    match &result[5].content[0] {
        ContentBlock::ToolResult { content, .. } => {
            assert_eq!(content.len(), 200);
        }
        _ => panic!("Expected ToolResult"),
    }
}

#[test]
fn test_preserve_short_tool_results() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 1,
        min_mask_length: 100,
        ..Default::default()
    });

    let msgs = vec![
        user_msg("q1"),
        assistant_msg("a1"),
        tool_result_msg("t1", "short result"), // < 100 chars, keep even if old
        user_msg("q2"),
        assistant_msg("a2"),
    ];

    let result = masker.mask(&msgs);
    match &result[2].content[0] {
        ContentBlock::ToolResult { content, .. } => {
            assert_eq!(content, "short result");
        }
        _ => panic!("Expected ToolResult"),
    }
}

#[test]
fn test_preserve_user_and_assistant_messages() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 1,
        min_mask_length: 10,
        ..Default::default()
    });

    let msgs = vec![
        user_msg("long user message that is definitely over 10 chars"),
        assistant_msg("long assistant message that is also over 10 chars"),
        user_msg("q2"),
        assistant_msg("a2"),
    ];

    let result = masker.mask(&msgs);
    // User and assistant text should never be masked
    match &result[0].content[0] {
        ContentBlock::Text { text } => {
            assert!(text.starts_with("long user"));
        }
        _ => panic!("Expected Text"),
    }
}

#[test]
fn test_custom_placeholder() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 1,
        min_mask_length: 10,
        placeholder_template: "[MASKED: {chars} characters]".to_string(),
    });

    let msgs = vec![
        user_msg("q1"),
        assistant_msg("a1"),
        tool_result_msg("t1", &long_content(150)),
        user_msg("q2"),
        assistant_msg("a2"),
    ];

    let result = masker.mask(&msgs);
    match &result[2].content[0] {
        ContentBlock::ToolResult { content, .. } => {
            assert_eq!(content, "[MASKED: 150 characters]");
        }
        _ => panic!("Expected ToolResult"),
    }
}

#[test]
fn test_custom_keep_recent() {
    // keep_recent_turns = 2, so with 3 turns the first is masked
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 2,
        min_mask_length: 10,
        ..Default::default()
    });

    let long = long_content(200);
    let msgs = vec![
        assistant_msg("a1"),
        tool_result_msg("t1", &long), // turn 1 - masked
        assistant_msg("a2"),
        tool_result_msg("t2", &long), // turn 2 - kept
        assistant_msg("a3"),
        tool_result_msg("t3", &long), // turn 3 - kept
    ];

    let result = masker.mask(&msgs);

    // turn 1 masked
    match &result[1].content[0] {
        ContentBlock::ToolResult { content, .. } => assert!(content.contains("hidden")),
        _ => panic!("Expected ToolResult"),
    }

    // turn 2 kept
    match &result[3].content[0] {
        ContentBlock::ToolResult { content, .. } => assert_eq!(content.len(), 200),
        _ => panic!("Expected ToolResult"),
    }
}

#[test]
fn test_estimate_savings() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 1,
        min_mask_length: 10,
        ..Default::default()
    });

    let msgs = vec![
        assistant_msg("a1"),
        tool_result_msg("t1", &long_content(500)),
        assistant_msg("a2"),
    ];

    let (original, after) = masker.estimate_savings(&msgs);
    assert!(after < original, "masked should be smaller: {} < {}", after, original);
}

#[test]
fn test_default_config() {
    let config = ObservationMaskConfig::default();
    assert_eq!(config.keep_recent_turns, 3);
    assert_eq!(config.min_mask_length, 100);
    assert!(config.placeholder_template.contains("{chars}"));
}

#[test]
fn test_single_turn_no_masking() {
    let masker = ObservationMasker::new(ObservationMaskConfig {
        keep_recent_turns: 1,
        min_mask_length: 10,
        ..Default::default()
    });

    let msgs = vec![
        user_msg("hello"),
        assistant_msg("response"),
        tool_result_msg("t1", &long_content(500)),
    ];

    let result = masker.mask(&msgs);
    // Single turn = within recent, not masked
    match &result[2].content[0] {
        ContentBlock::ToolResult { content, .. } => assert_eq!(content.len(), 500),
        _ => panic!("Expected ToolResult"),
    }
}
