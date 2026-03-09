use octo_engine::mcp::{McpToolAnnotations, McpToolInfo};
use octo_types::RiskLevel;

fn tool_with_annotations(ann: Option<McpToolAnnotations>) -> McpToolInfo {
    McpToolInfo {
        name: "test-tool".to_string(),
        description: Some("A test tool".to_string()),
        input_schema: serde_json::json!({"type": "object"}),
        annotations: ann,
    }
}

/// Helper: compute risk level from annotations using the same logic as McpToolBridge
fn risk_from_annotations(annotations: &Option<McpToolAnnotations>) -> RiskLevel {
    match annotations {
        Some(ann) if ann.destructive => RiskLevel::Destructive,
        Some(ann) if ann.open_world => RiskLevel::HighRisk,
        Some(ann) if ann.read_only => RiskLevel::ReadOnly,
        _ => RiskLevel::LowRisk,
    }
}

#[test]
fn test_annotations_default() {
    let ann = McpToolAnnotations::default();
    assert!(!ann.read_only);
    assert!(!ann.destructive);
    assert!(!ann.open_world);
    assert!(ann.title.is_none());
}

#[test]
fn test_tool_info_no_annotations() {
    let tool = tool_with_annotations(None);
    assert!(tool.annotations.is_none());
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::LowRisk);
}

#[test]
fn test_tool_info_read_only_annotation() {
    let ann = McpToolAnnotations {
        read_only: true,
        ..Default::default()
    };
    let tool = tool_with_annotations(Some(ann));
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::ReadOnly);
}

#[test]
fn test_tool_info_destructive_annotation() {
    let ann = McpToolAnnotations {
        destructive: true,
        ..Default::default()
    };
    let tool = tool_with_annotations(Some(ann));
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::Destructive);
}

#[test]
fn test_tool_info_open_world_annotation() {
    let ann = McpToolAnnotations {
        open_world: true,
        ..Default::default()
    };
    let tool = tool_with_annotations(Some(ann));
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::HighRisk);
}

#[test]
fn test_destructive_takes_priority_over_open_world() {
    let ann = McpToolAnnotations {
        destructive: true,
        open_world: true,
        ..Default::default()
    };
    let tool = tool_with_annotations(Some(ann));
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::Destructive);
}

#[test]
fn test_destructive_takes_priority_over_read_only() {
    let ann = McpToolAnnotations {
        destructive: true,
        read_only: true,
        ..Default::default()
    };
    let tool = tool_with_annotations(Some(ann));
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::Destructive);
}

#[test]
fn test_open_world_takes_priority_over_read_only() {
    let ann = McpToolAnnotations {
        open_world: true,
        read_only: true,
        ..Default::default()
    };
    let tool = tool_with_annotations(Some(ann));
    assert_eq!(risk_from_annotations(&tool.annotations), RiskLevel::HighRisk);
}

#[test]
fn test_annotations_with_title() {
    let ann = McpToolAnnotations {
        read_only: true,
        title: Some("Read Database".to_string()),
        ..Default::default()
    };
    assert_eq!(ann.title.as_deref(), Some("Read Database"));
}

#[test]
fn test_annotations_serialize_deserialize() {
    let ann = McpToolAnnotations {
        read_only: true,
        destructive: false,
        open_world: true,
        title: Some("Test".to_string()),
    };
    let json = serde_json::to_string(&ann).unwrap();
    let parsed: McpToolAnnotations = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.read_only, true);
    assert_eq!(parsed.destructive, false);
    assert_eq!(parsed.open_world, true);
    assert_eq!(parsed.title.as_deref(), Some("Test"));
}

#[test]
fn test_tool_info_deserialize_without_annotations() {
    let json = r#"{"name":"foo","description":"bar","input_schema":{}}"#;
    let tool: McpToolInfo = serde_json::from_str(json).unwrap();
    assert_eq!(tool.name, "foo");
    assert!(tool.annotations.is_none());
}

#[test]
fn test_tool_info_deserialize_with_annotations() {
    let json = r#"{
        "name": "foo",
        "description": "bar",
        "input_schema": {},
        "annotations": {
            "read_only": true,
            "destructive": false,
            "open_world": false
        }
    }"#;
    let tool: McpToolInfo = serde_json::from_str(json).unwrap();
    assert!(tool.annotations.is_some());
    let ann = tool.annotations.unwrap();
    assert!(ann.read_only);
    assert!(!ann.destructive);
}
