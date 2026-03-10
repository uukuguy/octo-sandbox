use octo_types::{Artifact, ToolOutput};
use serde_json::json;

#[test]
fn test_tool_output_success() {
    let output = ToolOutput::success("hello world");
    assert_eq!(output.content, "hello world");
    assert!(!output.is_error);
    assert!(output.artifacts.is_empty());
    assert!(output.metadata.is_none());
    assert!(!output.truncated);
    assert!(output.original_size.is_none());
    assert_eq!(output.duration_ms, 0);
}

#[test]
fn test_tool_output_error() {
    let output = ToolOutput::error("something failed");
    assert_eq!(output.content, "something failed");
    assert!(output.is_error);
    assert!(output.artifacts.is_empty());
}

#[test]
fn test_tool_output_with_artifact() {
    let artifact = Artifact {
        name: "result.json".to_string(),
        content_type: "application/json".to_string(),
        data: r#"{"key":"value"}"#.to_string(),
    };
    let output = ToolOutput::success("done").with_artifact(artifact);
    assert_eq!(output.artifacts.len(), 1);
    assert_eq!(output.artifacts[0].name, "result.json");
    assert_eq!(output.artifacts[0].content_type, "application/json");
}

#[test]
fn test_tool_output_multiple_artifacts() {
    let output = ToolOutput::success("done")
        .with_artifact(Artifact {
            name: "a.txt".to_string(),
            content_type: "text/plain".to_string(),
            data: "aaa".to_string(),
        })
        .with_artifact(Artifact {
            name: "b.png".to_string(),
            content_type: "image/png".to_string(),
            data: "base64data".to_string(),
        });
    assert_eq!(output.artifacts.len(), 2);
    assert_eq!(output.artifacts[1].name, "b.png");
}

#[test]
fn test_tool_output_with_metadata() {
    let meta = json!({"version": 2, "tags": ["fast", "cached"]});
    let output = ToolOutput::success("ok").with_metadata(meta.clone());
    assert_eq!(output.metadata, Some(meta));
}

#[test]
fn test_tool_output_with_duration() {
    let output = ToolOutput::success("ok").with_duration(142);
    assert_eq!(output.duration_ms, 142);
}

#[test]
fn test_tool_output_mark_truncated() {
    let output = ToolOutput::success("first 1000 chars...").mark_truncated(50_000);
    assert!(output.truncated);
    assert_eq!(output.original_size, Some(50_000));
}

#[test]
fn test_tool_output_builder_chain() {
    let output = ToolOutput::success("result")
        .with_artifact(Artifact {
            name: "out.csv".to_string(),
            content_type: "text/csv".to_string(),
            data: "a,b\n1,2".to_string(),
        })
        .with_metadata(json!({"rows": 1}))
        .with_duration(300)
        .mark_truncated(100_000);

    assert_eq!(output.content, "result");
    assert!(!output.is_error);
    assert_eq!(output.artifacts.len(), 1);
    assert!(output.metadata.is_some());
    assert_eq!(output.duration_ms, 300);
    assert!(output.truncated);
    assert_eq!(output.original_size, Some(100_000));
}

#[test]
fn test_tool_output_serialize_deserialize() {
    let output = ToolOutput::success("hello")
        .with_artifact(Artifact {
            name: "file.txt".to_string(),
            content_type: "text/plain".to_string(),
            data: "content".to_string(),
        })
        .with_metadata(json!({"key": 42}))
        .with_duration(100)
        .mark_truncated(5000);

    let json_str = serde_json::to_string(&output).expect("serialize");
    let deserialized: ToolOutput = serde_json::from_str(&json_str).expect("deserialize");

    assert_eq!(deserialized.content, "hello");
    assert!(!deserialized.is_error);
    assert_eq!(deserialized.artifacts.len(), 1);
    assert_eq!(deserialized.artifacts[0].name, "file.txt");
    assert_eq!(deserialized.metadata, Some(json!({"key": 42})));
    assert!(deserialized.truncated);
    assert_eq!(deserialized.original_size, Some(5000));
    assert_eq!(deserialized.duration_ms, 100);
}

#[test]
fn test_artifact_serialize_deserialize() {
    let artifact = Artifact {
        name: "image.png".to_string(),
        content_type: "image/png".to_string(),
        data: "base64encodeddata".to_string(),
    };

    let json_str = serde_json::to_string(&artifact).expect("serialize");
    let deserialized: Artifact = serde_json::from_str(&json_str).expect("deserialize");

    assert_eq!(deserialized.name, "image.png");
    assert_eq!(deserialized.content_type, "image/png");
    assert_eq!(deserialized.data, "base64encodeddata");
}
