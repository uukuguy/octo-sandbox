use octo_engine::context::{AutoCompactConfig, AutoCompactSummary};

#[test]
fn test_compact_long_content() {
    let config = AutoCompactConfig::default();
    let content = "First line of output\n".to_string() + &"x".repeat(300);
    let result = AutoCompactSummary::compact_message(&content, &config);

    assert!(result.starts_with("[Compacted:"));
    assert!(result.contains("chars"));
    assert!(result.contains("tokens saved"));
    // Original content should NOT appear verbatim
    assert!(!result.contains(&"x".repeat(100)));
}

#[test]
fn test_skip_short_content() {
    let config = AutoCompactConfig::default();
    let content = "Short result: OK";
    let result = AutoCompactSummary::compact_message(content, &config);

    assert_eq!(result, content);
}

#[test]
fn test_compact_preserves_first_line() {
    let config = AutoCompactConfig::default();
    let first_line = "File listing for /home/user";
    let content = format!("{}\n{}", first_line, "data line\n".repeat(50));
    let result = AutoCompactSummary::compact_message(&content, &config);

    assert!(
        result.contains(first_line),
        "Expected first line '{}' in result: {}",
        first_line,
        result
    );
}

#[test]
fn test_compact_config_defaults() {
    let config = AutoCompactConfig::default();
    assert_eq!(config.max_summary_tokens, 50);
    assert_eq!(config.min_content_length, 200);
}

#[test]
fn test_compact_empty_content() {
    let config = AutoCompactConfig::default();
    let result = AutoCompactSummary::compact_message("", &config);

    // Empty string is shorter than min_content_length, returned unchanged
    assert_eq!(result, "");
}

#[test]
fn test_compact_multiline_extracts_first() {
    let config = AutoCompactConfig {
        min_content_length: 50,
        ..Default::default()
    };
    let content = "HEADER: summary info\nsecond line\nthird line\nfourth line\nfifth line\n";
    let result = AutoCompactSummary::compact_message(content, &config);

    assert!(result.starts_with("[Compacted: HEADER: summary info..."));
    // Second line should NOT appear in the compacted output
    assert!(!result.contains("second line"));
}
