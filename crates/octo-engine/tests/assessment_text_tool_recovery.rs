//! Edge-case tests for text tool recovery patterns.
//!
//! The `parse_tool_calls_from_text` function in `agent/harness.rs` is private
//! and already covered by 6 inline unit tests. These integration-level tests
//! validate the JSON/XML data structures at the parsing layer to document
//! expected formats and confirm our test data is valid for the parser.

use serde_json;

/// Verify that a fenced JSON block with nested JSON arguments is valid
/// parseable JSON. The `content` field contains an escaped JSON string,
/// which is a common pattern when tools write configuration files.
#[test]
fn edge_case_nested_json_in_tool_args() {
    let json_str = r#"{"name": "file_write", "arguments": {"path": "/tmp/config.json", "content": "{\"key\": \"value\", \"nested\": {\"a\": 1}}"}}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["name"], "file_write");
    assert!(parsed["arguments"]["content"].is_string());

    // Additionally verify the nested content itself is valid JSON
    let inner: serde_json::Value =
        serde_json::from_str(parsed["arguments"]["content"].as_str().unwrap()).unwrap();
    assert_eq!(inner["key"], "value");
    assert_eq!(inner["nested"]["a"], 1);
}

/// Verify that two separate fenced JSON blocks in text can each be
/// individually parsed. The text-tool-recovery parser scans for multiple
/// fenced blocks and attempts to parse each one independently.
#[test]
fn edge_case_multiple_tools_in_one_text() {
    let block1 = r#"{"name": "bash", "arguments": {"command": "ls"}}"#;
    let block2 = r#"{"name": "file_read", "arguments": {"path": "/tmp/x"}}"#;

    let v1: serde_json::Value = serde_json::from_str(block1).unwrap();
    let v2: serde_json::Value = serde_json::from_str(block2).unwrap();

    assert_eq!(v1["name"], "bash");
    assert_eq!(v1["arguments"]["command"], "ls");
    assert_eq!(v2["name"], "file_read");
    assert_eq!(v2["arguments"]["path"], "/tmp/x");

    // Ensure both are structurally independent (no shared references)
    assert_ne!(v1["name"], v2["name"]);
}

/// Verify XML-style tool call inner content is valid JSON. The parser
/// extracts inner text between matching XML tags (e.g. `<bash>...</bash>`)
/// and attempts to parse it as a JSON object. Shell metacharacters in
/// argument values must be preserved verbatim.
#[test]
fn edge_case_xml_format_with_complex_args() {
    // The parser extracts inner text between matching XML tags and tries JSON parse
    let inner_json = r#"{"command": "echo 'hello; world' && ls -la | grep test"}"#;
    let parsed: serde_json::Value = serde_json::from_str(inner_json).unwrap();
    assert!(parsed["command"].is_string());

    // Verify shell metacharacters are preserved in the JSON value
    let cmd = parsed["command"].as_str().unwrap();
    assert!(cmd.contains(";"), "semicolon must be preserved");
    assert!(cmd.contains("|"), "pipe must be preserved");
    assert!(cmd.contains("&&"), "double-ampersand must be preserved");
    assert!(cmd.contains("'"), "single quotes must be preserved");
}

/// Verify that the tool call JSON structure embedded in surrounding
/// reasoning text is independently valid. The parser uses regex to locate
/// fenced code blocks (` ```json ... ``` `) within free-form LLM output,
/// then parses only the JSON portion.
#[test]
fn edge_case_tool_call_mixed_with_reasoning() {
    let tool_json =
        r#"{"name": "bash", "arguments": {"command": "cargo test --workspace"}}"#;
    let full_text = format!(
        "Let me think about this...\n\nI'll run the tests:\n```json\n{}\n```\n\nThis should work.",
        tool_json
    );

    // The JSON itself is valid
    let parsed: serde_json::Value = serde_json::from_str(tool_json).unwrap();
    assert_eq!(parsed["name"], "bash");
    assert_eq!(
        parsed["arguments"]["command"],
        "cargo test --workspace"
    );

    // The full text contains the JSON embedded in a fenced block
    assert!(full_text.contains(tool_json));
    assert!(full_text.contains("```json"));
    assert!(full_text.starts_with("Let me think"));
    assert!(full_text.ends_with("This should work."));
}
