use std::path::PathBuf;

use octo_engine::skills::SkillTool;
use octo_engine::tools::Tool;
use octo_types::id::SandboxId;
use octo_types::{SkillDefinition, SkillSourceType, ToolContext, TrustLevel};

fn test_skill() -> SkillDefinition {
    SkillDefinition {
        name: "test-skill".into(),
        description: "A test skill".into(),
        version: None,
        user_invocable: true,
        allowed_tools: None,
        body: "This is the test skill body.".into(),
        base_dir: PathBuf::from("/tmp/test-skill-nonexistent"),
        source_path: PathBuf::from("/tmp/test-skill-nonexistent/SKILL.md"),
        body_loaded: true,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::default(),
        triggers: vec![],
        dependencies: vec![],
        tags: vec![],
        denied_tools: None,
        execution_mode: Default::default(),
        source_type: SkillSourceType::default(),
        max_rounds: 0,
    }
}

fn test_ctx() -> ToolContext {
    ToolContext {
        sandbox_id: SandboxId::from_string("test-sandbox"),
        working_dir: PathBuf::from("/tmp"),
        path_validator: None,
    }
}

#[tokio::test]
async fn test_skill_tool_activate_returns_body() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    let params = serde_json::json!({"action": "activate"});
    let result = tool.execute(params, &test_ctx()).await.unwrap();
    assert_eq!(result.content, "This is the test skill body.");
    assert!(!result.is_error);
}

#[tokio::test]
async fn test_skill_tool_default_action_is_activate() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    let params = serde_json::json!({});
    let result = tool.execute(params, &test_ctx()).await.unwrap();
    assert_eq!(result.content, "This is the test skill body.");
    assert!(!result.is_error);
}

#[tokio::test]
async fn test_skill_tool_list_scripts_no_dir() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    let params = serde_json::json!({"action": "list_scripts"});
    let result = tool.execute(params, &test_ctx()).await.unwrap();
    assert!(result.content.contains("No scripts"));
    assert!(!result.is_error);
}

#[tokio::test]
async fn test_skill_tool_run_script_no_bridge() {
    let skill = test_skill();
    let tool = SkillTool::new(skill); // No bridge
    let params = serde_json::json!({"action": "run_script", "args": "test.py"});
    let result = tool.execute(params, &test_ctx()).await;
    // Should error because no bridge configured
    assert!(result.is_err());
}

#[tokio::test]
async fn test_skill_tool_unknown_action() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    let params = serde_json::json!({"action": "invalid"});
    let result = tool.execute(params, &test_ctx()).await.unwrap();
    assert!(result.is_error);
    assert!(result.content.contains("Unknown action"));
}

#[tokio::test]
async fn test_skill_tool_run_script_no_args() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    let params = serde_json::json!({"action": "run_script"});
    // No bridge, but should fail on missing args before checking bridge
    // Actually, bridge check comes first — this will error on no bridge
    let result = tool.execute(params, &test_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_skill_tool_source() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    assert_eq!(tool.name(), "test-skill");
    assert_eq!(tool.description(), "A test skill");
    match tool.source() {
        octo_types::ToolSource::Skill(name) => assert_eq!(name, "test-skill"),
        _ => panic!("Expected ToolSource::Skill"),
    }
}

#[tokio::test]
async fn test_skill_tool_parameters_schema() {
    let skill = test_skill();
    let tool = SkillTool::new(skill);
    let params = tool.parameters();
    let props = params.get("properties").unwrap();
    assert!(props.get("action").is_some());
    assert!(props.get("args").is_some());
}
