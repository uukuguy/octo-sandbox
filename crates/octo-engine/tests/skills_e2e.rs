//! End-to-end tests for the Skills system (Phase V).
//!
//! Tests cover: builtin skill loading, SkillSelector matching,
//! ExecuteSkillTool execution, system prompt injection, and SubAgent constraints.

use std::path::PathBuf;
use std::sync::Arc;

use octo_engine::skills::{
    sync_builtin_skills, builtin_skill_names, ExecuteSkillTool, SkillSelector,
    SkillRegistry, TrustManager,
};
use octo_engine::skills::loader::SkillLoader;
use octo_engine::Tool;
use octo_types::skill::{ExecutionMode, SkillDefinition, SkillTrigger, TrustLevel};

// ── Builtin Skills Loading ──────────────────────────────────────────

#[test]
fn test_builtin_skills_sync_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    sync_builtin_skills(dir.path()).unwrap();

    let names = builtin_skill_names();
    assert!(names.len() >= 2, "Expected at least 2 embedded fallback skills, got {}", names.len());

    for name in &names {
        let skill_dir = dir.path().join(name);
        assert!(skill_dir.exists(), "Skill dir missing: {}", name);
        let skill_file = skill_dir.join("SKILL.md");
        assert!(skill_file.exists(), "SKILL.md missing for: {}", name);
    }
}

#[test]
fn test_builtin_skills_parse_valid() {
    let dir = tempfile::tempdir().unwrap();
    sync_builtin_skills(dir.path()).unwrap();

    for name in builtin_skill_names() {
        let skill_file = dir.path().join(&name).join("SKILL.md");
        let result = SkillLoader::parse_skill(&skill_file);
        assert!(result.is_ok(), "Failed to parse skill '{}': {:?}", name, result.err());

        let skill = result.unwrap();
        assert_eq!(skill.name, name);
        assert!(!skill.description.is_empty(), "Skill '{}' has empty description", name);
        assert!(!skill.body.is_empty(), "Skill '{}' has empty body", name);
    }
}

#[test]
fn test_builtin_skills_execution_modes() {
    let dir = tempfile::tempdir().unwrap();
    sync_builtin_skills(dir.path()).unwrap();

    // filesystem — only SKILL.md, no extra files → Knowledge (auto-inferred)
    let skill_file = dir.path().join("filesystem").join("SKILL.md");
    let skill = SkillLoader::parse_skill(&skill_file).unwrap();
    assert_eq!(
        skill.execution_mode,
        ExecutionMode::Knowledge,
        "Seeded filesystem (SKILL.md only) should be Knowledge mode"
    );

    // web-search — only SKILL.md in seeded fallback → Knowledge
    let skill_file = dir.path().join("web-search").join("SKILL.md");
    let skill = SkillLoader::parse_skill(&skill_file).unwrap();
    assert_eq!(
        skill.execution_mode,
        ExecutionMode::Knowledge,
        "Seeded web-search (SKILL.md only) should be Knowledge mode"
    );
}

// ── SkillSelector Pipeline ──────────────────────────────────────────

fn make_test_skill(name: &str, mode: ExecutionMode, triggers: Vec<SkillTrigger>) -> SkillDefinition {
    SkillDefinition {
        name: name.into(),
        description: format!("Test skill: {}", name),
        version: None,
        user_invocable: true,
        allowed_tools: None,
        body: "Test body".into(),
        base_dir: PathBuf::new(),
        source_path: PathBuf::new(),
        body_loaded: true,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::Installed,
        triggers,
        dependencies: vec![],
        tags: vec![],
        denied_tools: None,
        execution_mode: mode,
        source_type: octo_types::skill::SkillSourceType::ProjectLocal,
    }
}

#[test]
fn test_selector_slash_command_match() {
    let skills = vec![
        make_test_skill("filesystem", ExecutionMode::Playbook, vec![
            SkillTrigger::Keyword { keyword: "file".into() },
        ]),
        make_test_skill("code-review", ExecutionMode::Knowledge, vec![
            SkillTrigger::Keyword { keyword: "review".into() },
        ]),
    ];

    let trust = TrustManager::default();
    let selector = SkillSelector::new(8000, trust);

    // Slash command match should score 900
    let selected = selector.select(&skills, "/filesystem list files");
    assert!(!selected.is_empty(), "Should match /filesystem");
    assert_eq!(selected[0].name, "filesystem");
    assert_eq!(selected[0].score, 900);
}

#[test]
fn test_selector_keyword_match() {
    let skills = vec![
        make_test_skill("code-review", ExecutionMode::Knowledge, vec![
            SkillTrigger::Keyword { keyword: "review".into() },
            SkillTrigger::Keyword { keyword: "code".into() },
        ]),
    ];

    let trust = TrustManager::default();
    let selector = SkillSelector::new(8000, trust);

    let selected = selector.select(&skills, "please review my code");
    assert!(!selected.is_empty());
    assert_eq!(selected[0].name, "code-review");
    assert!(selected[0].score >= 20, "Should score at least 20 for two keyword hits");
}

#[test]
fn test_selector_no_match() {
    let skills = vec![
        make_test_skill("filesystem", ExecutionMode::Playbook, vec![
            SkillTrigger::Keyword { keyword: "file".into() },
        ]),
    ];

    let trust = TrustManager::default();
    let selector = SkillSelector::new(8000, trust);

    let selected = selector.select(&skills, "hello world");
    assert!(selected.is_empty(), "Should not match any skill for 'hello world'");
}

// ── SkillRegistry ──────────────────────────────────────────────────

#[test]
fn test_registry_register_and_get() {
    let registry = SkillRegistry::new();
    let skill = make_test_skill("test-skill", ExecutionMode::Knowledge, vec![]);
    registry.register(skill);

    let retrieved = registry.get("test-skill");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "test-skill");
}

#[test]
fn test_registry_list_all() {
    let registry = SkillRegistry::new();
    registry.register(make_test_skill("a", ExecutionMode::Knowledge, vec![]));
    registry.register(make_test_skill("b", ExecutionMode::Playbook, vec![]));

    let all = registry.list_all();
    assert_eq!(all.len(), 2);
}

// ── ExecuteSkillTool (KNOWLEDGE mode) ──────────────────────────────

#[tokio::test]
async fn test_execute_skill_tool_knowledge() {
    let registry = Arc::new(SkillRegistry::new());
    let mut skill = make_test_skill("code-review", ExecutionMode::Knowledge, vec![]);
    skill.body = "## Code Review Guidelines\n\n1. Check formatting\n2. Check logic".into();
    registry.register(skill);

    let tool = ExecuteSkillTool::new(registry);

    let params = serde_json::json!({
        "skill_name": "code-review",
        "request": "Review my main.rs file"
    });

    let ctx = octo_types::ToolContext {
        sandbox_id: octo_types::SandboxId::from_string("test"),
        working_dir: PathBuf::from("."),
        path_validator: None,
    };

    let result = tool.execute(params, &ctx).await.unwrap();
    let output = &result.content;
    assert!(output.contains("Code Review Guidelines"), "Should contain skill body");
    assert!(output.contains("Review my main.rs"), "Should contain the request");
}

#[tokio::test]
async fn test_execute_skill_tool_not_found() {
    let registry = Arc::new(SkillRegistry::new());
    let tool = ExecuteSkillTool::new(registry);

    let params = serde_json::json!({
        "skill_name": "nonexistent",
        "request": "do something"
    });

    let ctx = octo_types::ToolContext {
        sandbox_id: octo_types::SandboxId::from_string("test"),
        working_dir: PathBuf::from("."),
        path_validator: None,
    };

    let result = tool.execute(params, &ctx).await.unwrap();
    assert!(result.is_error);
}

#[tokio::test]
async fn test_execute_skill_tool_missing_params() {
    let registry = Arc::new(SkillRegistry::new());
    let tool = ExecuteSkillTool::new(registry);

    let ctx = octo_types::ToolContext {
        sandbox_id: octo_types::SandboxId::from_string("test"),
        working_dir: PathBuf::from("."),
        path_validator: None,
    };

    // Missing skill_name
    let result = tool.execute(serde_json::json!({"request": "test"}), &ctx).await.unwrap();
    assert!(result.is_error);

    // Missing request
    let result = tool.execute(serde_json::json!({"skill_name": "test"}), &ctx).await.unwrap();
    assert!(result.is_error);
}

// ── System Prompt Injection ─────────────────────────────────────────

#[test]
fn test_system_prompt_skill_index() {
    use octo_engine::context::NewSystemPromptBuilder;

    let skills = vec![
        make_test_skill("filesystem", ExecutionMode::Playbook, vec![]),
        make_test_skill("code-review", ExecutionMode::Knowledge, vec![]),
    ];

    let prompt = NewSystemPromptBuilder::new()
        .with_skill_index(&skills)
        .build();

    assert!(prompt.contains("filesystem"), "Should contain filesystem skill");
    assert!(prompt.contains("code-review"), "Should contain code-review skill");
    assert!(prompt.contains("playbook"), "Should show execution mode");
    assert!(prompt.contains("execute_skill"), "Should mention execute_skill tool");
}

#[test]
fn test_system_prompt_active_skill_injection() {
    use octo_engine::context::NewSystemPromptBuilder;

    let mut skill = make_test_skill("code-review", ExecutionMode::Knowledge, vec![]);
    skill.body = "Always check for proper error handling.".into();

    let prompt = NewSystemPromptBuilder::new()
        .with_active_skill(&skill)
        .build();

    assert!(prompt.contains("Always check for proper error handling"), "Should contain skill body");
}

// ── ExecutionMode Auto-Inference ────────────────────────────────────

#[test]
fn test_execution_mode_auto_infer_playbook() {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("my-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();

    // Create SKILL.md without explicit execution-mode
    let skill_md = r#"---
name: my-skill
description: A test skill
---

Do something useful.
"#;
    std::fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();
    // Add an extra file to trigger Playbook auto-inference
    std::fs::write(skill_dir.join("helper.py"), "print('hello')").unwrap();

    let result = SkillLoader::parse_skill(&skill_dir.join("SKILL.md"));
    assert!(result.is_ok());
    let skill = result.unwrap();
    assert_eq!(skill.execution_mode, ExecutionMode::Playbook,
        "Should auto-infer Playbook when extra files exist");
}

#[test]
fn test_execution_mode_stays_knowledge_without_extra_files() {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("my-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();

    let skill_md = r#"---
name: my-skill
description: A test skill
---

Knowledge instructions only.
"#;
    std::fs::write(skill_dir.join("SKILL.md"), skill_md).unwrap();

    let result = SkillLoader::parse_skill(&skill_dir.join("SKILL.md"));
    assert!(result.is_ok());
    let skill = result.unwrap();
    assert_eq!(skill.execution_mode, ExecutionMode::Knowledge,
        "Should stay Knowledge when no extra files");
}
