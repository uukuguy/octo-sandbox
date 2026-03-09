use octo_engine::tools::truncation::{truncate_output, ToolExecutionConfig, TruncationStrategy};

#[test]
fn short_content_is_not_truncated() {
    let config = ToolExecutionConfig::default();
    let result = truncate_output("hello", &config, TruncationStrategy::Head67Tail27);
    assert!(!result.was_truncated);
    assert_eq!(result.content, "hello");
    assert_eq!(result.original_size, 5);
    assert!(result.strategy_used.is_none());
}

#[test]
fn head67_tail27_truncates_long_content() {
    let config = ToolExecutionConfig {
        max_output_bytes: 200,
        max_output_lines: 2000,
    };
    let long = "abcdefghij".repeat(50); // 500 bytes
    let result = truncate_output(&long, &config, TruncationStrategy::Head67Tail27);

    assert!(result.was_truncated);
    assert_eq!(result.original_size, 500);
    assert_eq!(result.strategy_used, Some(TruncationStrategy::Head67Tail27));
    assert!(result.content.contains("truncated"));
    // Head portion should start with original content
    assert!(result.content.starts_with("abcdefghij"));
    // Tail portion should end with original content
    assert!(result.content.ends_with("abcdefghij"));
    assert!(result.content.len() <= 250); // some slack for marker
}

#[test]
fn head_only_truncates_long_content() {
    let config = ToolExecutionConfig {
        max_output_bytes: 200,
        max_output_lines: 2000,
    };
    let long = "abcdefghij".repeat(50);
    let result = truncate_output(&long, &config, TruncationStrategy::HeadOnly);

    assert!(result.was_truncated);
    assert_eq!(result.strategy_used, Some(TruncationStrategy::HeadOnly));
    assert!(result.content.starts_with("abcdefghij"));
    assert!(result.content.contains("truncated"));
    // Should NOT end with original tail content
    assert!(result.content.ends_with("..."));
}

#[test]
fn tail_only_truncates_long_content() {
    let config = ToolExecutionConfig {
        max_output_bytes: 200,
        max_output_lines: 2000,
    };
    let long = "abcdefghij".repeat(50);
    let result = truncate_output(&long, &config, TruncationStrategy::TailOnly);

    assert!(result.was_truncated);
    assert_eq!(result.strategy_used, Some(TruncationStrategy::TailOnly));
    assert!(result.content.ends_with("abcdefghij"));
    assert!(result.content.contains("truncated"));
}

#[test]
fn truncation_by_line_count() {
    let config = ToolExecutionConfig {
        max_output_bytes: 1024 * 1024, // 1MB — not the limiting factor
        max_output_lines: 10,
    };
    // 50 lines, each "line N"
    let lines: Vec<String> = (0..50).map(|i| format!("line {}", i)).collect();
    let content = lines.join("\n");

    let result = truncate_output(&content, &config, TruncationStrategy::Head67Tail27);
    assert!(result.was_truncated);
    assert_eq!(result.strategy_used, Some(TruncationStrategy::Head67Tail27));

    // Head portion should include first lines
    assert!(result.content.starts_with("line 0"));
    // Tail portion should include last lines
    assert!(result.content.contains("line 49"));
    // Omission marker present
    assert!(result.content.contains("truncated"));
}

#[test]
fn truncation_by_byte_size() {
    let config = ToolExecutionConfig {
        max_output_bytes: 100,
        max_output_lines: 100_000,
    };
    let long = "x".repeat(500);
    let result = truncate_output(&long, &config, TruncationStrategy::HeadOnly);

    assert!(result.was_truncated);
    assert_eq!(result.original_size, 500);
    assert!(result.content.contains("truncated"));
}

#[test]
fn truncation_marker_is_present() {
    let config = ToolExecutionConfig {
        max_output_bytes: 150,
        max_output_lines: 5,
    };
    let lines: Vec<String> = (0..20).map(|i| format!("data-{}", i)).collect();
    let content = lines.join("\n");

    for strategy in [
        TruncationStrategy::Head67Tail27,
        TruncationStrategy::HeadOnly,
        TruncationStrategy::TailOnly,
    ] {
        let result = truncate_output(&content, &config, strategy);
        assert!(
            result.content.contains("truncated"),
            "strategy {:?} should include truncation marker",
            strategy,
        );
    }
}

#[test]
fn exact_limit_is_not_truncated() {
    let config = ToolExecutionConfig {
        max_output_bytes: 10,
        max_output_lines: 1,
    };
    // Exactly 1 line, exactly 10 bytes
    let content = "0123456789";
    let result = truncate_output(content, &config, TruncationStrategy::HeadOnly);
    assert!(!result.was_truncated);
    assert_eq!(result.content, content);
}

#[test]
fn multibyte_chars_respected() {
    let config = ToolExecutionConfig {
        max_output_bytes: 20,
        max_output_lines: 2000,
    };
    // Each CJK char is 3 bytes in UTF-8; 10 chars = 30 bytes > 20
    let content = "你好世界测试一二三四";
    let result = truncate_output(content, &config, TruncationStrategy::HeadOnly);
    assert!(result.was_truncated);
    // The result must be valid UTF-8
    assert!(result.content.is_char_boundary(0));
}
