use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use eaasp_skill_registry::models::SubmitDraftRequest;
use eaasp_skill_registry::routes::router;
use eaasp_skill_registry::skill_parser::{parse_v2_frontmatter, ScopedHookBody, V2ParseError};
use eaasp_skill_registry::store::SkillStore;
use tower::util::ServiceExt;

const FULL_V2_YAML: &str = r#"name: Threshold Calibration Assistant
version: 1.0.0
author: ops-team
runtime_affinity:
  preferred: null
  compatible:
    - grid-runtime
    - claude-code-runtime
    - hermes-runtime
access_scope: enterprise
scoped_hooks:
  PreToolUse:
    - name: block_write_scada
      type: command
      command: "scripts/hooks/block_write_scada.sh"
  PostToolUse:
    - name: require_evidence
      type: prompt
      prompt: "Does the tool output include an evidence_anchor_id reference?"
  Stop:
    - name: require_anchor_in_output
      type: command
      command: "scripts/hooks/check_output_anchor.sh"
dependencies: []
"#;

// ─── Parser unit tests ──────────────────────────────────────────────────────

#[test]
fn parse_full_v2_schema() {
    let fm = parse_v2_frontmatter(FULL_V2_YAML).unwrap();
    assert_eq!(fm.name.as_deref(), Some("Threshold Calibration Assistant"));
    assert_eq!(fm.runtime_affinity.preferred, None);
    assert_eq!(fm.runtime_affinity.compatible.len(), 3);
    assert_eq!(fm.access_scope.as_deref(), Some("enterprise"));
    assert_eq!(fm.scoped_hooks.pre_tool_use.len(), 1);
    assert_eq!(fm.scoped_hooks.pre_tool_use[0].name, "block_write_scada");
    match &fm.scoped_hooks.pre_tool_use[0].body {
        ScopedHookBody::Command { command } => {
            assert_eq!(command, "scripts/hooks/block_write_scada.sh")
        }
        _ => panic!("expected command hook"),
    }
    match &fm.scoped_hooks.post_tool_use[0].body {
        ScopedHookBody::Prompt { prompt } => assert!(prompt.contains("evidence_anchor_id")),
        _ => panic!("expected prompt hook"),
    }
    assert!(fm.dependencies.is_empty());
}

#[test]
fn parse_missing_optional_fields() {
    let yaml = "name: Simple\nversion: 0.1.0\n";
    let fm = parse_v2_frontmatter(yaml).unwrap();
    assert_eq!(fm.name.as_deref(), Some("Simple"));
    assert_eq!(fm.runtime_affinity.preferred, None);
    assert!(fm.runtime_affinity.compatible.is_empty());
    assert!(fm.scoped_hooks.pre_tool_use.is_empty());
    assert!(fm.dependencies.is_empty());
    assert_eq!(fm.access_scope, None);
}

#[test]
fn parse_legacy_skill_returns_ok_with_defaults() {
    let yaml = "name: Hello\nversion: 0.1.0\nauthor: legacy\n";
    let fm = parse_v2_frontmatter(yaml).unwrap();
    assert_eq!(fm.author.as_deref(), Some("legacy"));
}

#[test]
fn parse_empty_string_returns_err() {
    let result = parse_v2_frontmatter("");
    assert!(matches!(result, Err(V2ParseError::Empty)));
    let result2 = parse_v2_frontmatter("   \n  ");
    assert!(matches!(result2, Err(V2ParseError::Empty)));
}

#[test]
fn parse_threshold_calibration_example_skill() {
    // Read the example skill file and split frontmatter from prose.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/skills/threshold-calibration/SKILL.md");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    // Strip the --- frontmatter delimiters the same way store.rs does.
    assert!(
        content.starts_with("---\n"),
        "SKILL.md must start with frontmatter"
    );
    let rest = &content[4..];
    let end_idx = rest
        .find("\n---\n")
        .expect("SKILL.md must close frontmatter with ---");
    let frontmatter_yaml = &rest[..end_idx + 1];

    let fm = parse_v2_frontmatter(frontmatter_yaml)
        .expect("threshold-calibration frontmatter must parse");

    assert_eq!(fm.name.as_deref(), Some("threshold-calibration"));
    assert_eq!(fm.version.as_deref(), Some("0.1.0"));
    assert_eq!(
        fm.runtime_affinity.preferred.as_deref(),
        Some("grid-runtime")
    );
    assert!(fm
        .runtime_affinity
        .compatible
        .contains(&"grid-runtime".to_string()));
    assert!(fm
        .runtime_affinity
        .compatible
        .contains(&"claude-code-runtime".to_string()));
    assert_eq!(fm.access_scope.as_deref(), Some("org:eaasp-mvp"));

    // Scoped hooks: 1 pre + 1 post + 1 stop
    assert_eq!(fm.scoped_hooks.pre_tool_use.len(), 1);
    assert_eq!(fm.scoped_hooks.pre_tool_use[0].name, "block_write_scada");
    match &fm.scoped_hooks.pre_tool_use[0].body {
        ScopedHookBody::Command { command } => {
            assert!(command.contains("block_write_scada.sh"));
        }
        _ => panic!("PreToolUse[0] must be a command hook"),
    }

    assert_eq!(fm.scoped_hooks.post_tool_use.len(), 1);
    assert_eq!(fm.scoped_hooks.post_tool_use[0].name, "require_evidence");
    match &fm.scoped_hooks.post_tool_use[0].body {
        ScopedHookBody::Prompt { prompt } => {
            assert!(prompt.to_lowercase().contains("snapshot"));
        }
        _ => panic!("PostToolUse[0] must be a prompt hook"),
    }

    assert_eq!(fm.scoped_hooks.stop.len(), 1);
    assert_eq!(fm.scoped_hooks.stop[0].name, "require_anchor");
    match &fm.scoped_hooks.stop[0].body {
        ScopedHookBody::Command { command } => {
            assert!(command.contains("check_output_anchor.sh"));
        }
        _ => panic!("Stop[0] must be a command hook"),
    }

    assert_eq!(fm.dependencies.len(), 2);
    assert!(fm.dependencies.iter().any(|d| d.contains("mock-scada")));
    assert!(fm
        .dependencies
        .iter()
        .any(|d| d.contains("eaasp-l2-memory")));
}

#[test]
fn parse_skill_extraction_example_skill() {
    // Read the example skill file and split frontmatter from prose.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/skills/skill-extraction/SKILL.md");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    // Strip the --- frontmatter delimiters the same way store.rs does.
    assert!(
        content.starts_with("---\n"),
        "SKILL.md must start with frontmatter"
    );
    let rest = &content[4..];
    let end_idx = rest
        .find("\n---\n")
        .expect("SKILL.md must close frontmatter with ---");
    let frontmatter_yaml = &rest[..end_idx + 1];

    let fm = parse_v2_frontmatter(frontmatter_yaml)
        .expect("skill-extraction frontmatter must parse");

    assert_eq!(fm.name.as_deref(), Some("skill-extraction"));
    assert_eq!(fm.version.as_deref(), Some("0.1.0"));
    assert_eq!(fm.access_scope.as_deref(), Some("org:eaasp-mvp"));
    assert_eq!(
        fm.runtime_affinity.preferred.as_deref(),
        Some("grid-runtime")
    );
    assert!(fm
        .runtime_affinity
        .compatible
        .contains(&"grid-runtime".to_string()));
    assert!(fm
        .runtime_affinity
        .compatible
        .contains(&"claude-code-runtime".to_string()));

    // Scoped hooks: 0 pre + 1 post (verify_skill_draft) + 1 stop (check_final_output)
    assert_eq!(fm.scoped_hooks.pre_tool_use.len(), 0);
    assert_eq!(fm.scoped_hooks.post_tool_use.len(), 1);
    assert_eq!(fm.scoped_hooks.post_tool_use[0].name, "verify_skill_draft");
    match &fm.scoped_hooks.post_tool_use[0].body {
        ScopedHookBody::Command { command } => {
            assert!(command.contains("verify_skill_draft.sh"));
        }
        _ => panic!("PostToolUse[0] must be a command hook"),
    }

    assert_eq!(fm.scoped_hooks.stop.len(), 1);
    assert_eq!(fm.scoped_hooks.stop[0].name, "check_final_output");
    match &fm.scoped_hooks.stop[0].body {
        ScopedHookBody::Command { command } => {
            assert!(command.contains("check_final_output.sh"));
        }
        _ => panic!("Stop[0] must be a command hook"),
    }

    // Verify workflow.required_tools contains exactly the four tools the agent
    // actually invokes. skill_submit_draft is intentionally excluded: submission
    // to skill-registry is a human-gated step (MVP_SCOPE N14) and listing it
    // here would trap D87 continuation logic into forcing an unreachable call.
    assert!(fm.workflow.is_some());
    let workflow = fm.workflow.unwrap();
    assert_eq!(workflow.required_tools.len(), 4);
    assert!(workflow.required_tools.contains(&"memory_search".to_string()));
    assert!(workflow.required_tools.contains(&"memory_read".to_string()));
    assert!(workflow
        .required_tools
        .contains(&"memory_write_anchor".to_string()));
    assert!(workflow
        .required_tools
        .contains(&"memory_write_file".to_string()));

    // Verify dependencies: both eaasp-l2-memory (tool MCP) and eaasp-skill-registry
    // (soft intent declaration — draft output is destined for the registry even
    // though this skill never calls it directly).
    assert_eq!(fm.dependencies.len(), 2);
    assert!(fm
        .dependencies
        .iter()
        .any(|d| d.contains("eaasp-l2-memory")));
    assert!(fm
        .dependencies
        .iter()
        .any(|d| d.contains("eaasp-skill-registry")));
}

// ─── Integration tests ──────────────────────────────────────────────────────

async fn build_test_app() -> (axum::Router, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(SkillStore::open(tmp.path()).await.unwrap());
    (router(store), tmp)
}

#[tokio::test]
async fn v2_frontmatter_roundtrip_via_store() {
    let tmp = tempfile::tempdir().unwrap();
    let store = SkillStore::open(tmp.path()).await.unwrap();
    let req = SubmitDraftRequest {
        id: "calibrator".into(),
        name: "Calibrator".into(),
        description: "".into(),
        version: "1.0.0".into(),
        author: None,
        tags: None,
        frontmatter_yaml: FULL_V2_YAML.to_string(),
        prose: "body".into(),
        source_dir: None,
    };
    store.submit_draft(req).await.unwrap();
    let content = store
        .read_skill("calibrator".into(), Some("1.0.0".into()))
        .await
        .unwrap()
        .expect("should read back");
    let v2 = content.parsed_v2.expect("parsed_v2 should be populated");
    assert_eq!(v2.access_scope.as_deref(), Some("enterprise"));
    assert_eq!(v2.scoped_hooks.pre_tool_use.len(), 1);
}

#[tokio::test]
async fn get_tools_returns_seven() {
    let (app, _tmp) = build_test_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/tools")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let tools = json["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 7);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "skill_search",
        "skill_read",
        "skill_list_versions",
        "skill_submit_draft",
        "skill_promote",
        "skill_dependencies",
        "skill_usage",
    ] {
        assert!(names.contains(&expected), "missing tool: {}", expected);
    }
}

#[tokio::test]
async fn invoke_skill_dependencies_v2() {
    let (app, _tmp) = build_test_app().await;
    // First submit a v2 skill with dependencies
    let body = serde_json::json!({
        "id": "dep-skill",
        "name": "Dep Skill",
        "description": "",
        "version": "0.1.0",
        "frontmatter_yaml": "name: Dep\nversion: 0.1.0\ndependencies:\n  - foo\n  - bar\n",
        "prose": "body"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/skills/draft")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Invoke
    let invoke = serde_json::json!({"id": "dep-skill", "version": "0.1.0"});
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/tools/skill_dependencies/invoke")
                .header("content-type", "application/json")
                .body(Body::from(invoke.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let deps = json["dependencies"].as_array().unwrap();
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].as_str().unwrap(), "foo");
    assert_eq!(deps[1].as_str().unwrap(), "bar");
}

#[tokio::test]
async fn invoke_skill_usage_stub_returns_zero() {
    let (app, _tmp) = build_test_app().await;
    let body = serde_json::json!({"id": "anything"});
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/tools/skill_usage/invoke")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["session_count"].as_i64().unwrap(), 0);
    assert!(json["last_used"].is_null());
    assert!(json["note"].as_str().unwrap().contains("D9"));
}
