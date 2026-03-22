use std::path::PathBuf;

use octo_engine::skills::{ConstraintResult, ToolConstraintEnforcer};
use octo_types::skill::{SkillDefinition, SkillSourceType, TrustLevel};

fn make_skill(
    name: &str,
    allowed: Option<Vec<&str>>,
    denied: Option<Vec<&str>>,
) -> SkillDefinition {
    SkillDefinition {
        name: name.to_string(),
        description: format!("Test skill {}", name),
        version: None,
        user_invocable: false,
        allowed_tools: allowed.map(|v| v.into_iter().map(String::from).collect()),
        denied_tools: denied.map(|v| v.into_iter().map(String::from).collect()),
        body: String::new(),
        base_dir: PathBuf::from("/test"),
        source_path: PathBuf::from("/test/SKILL.md"),
        body_loaded: false,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::default(),
        triggers: vec![],
        dependencies: vec![],
        tags: vec![],
        execution_mode: Default::default(),
        source_type: SkillSourceType::default(),
        max_rounds: 0,
    }
}

#[test]
fn test_no_constraints_allows_all() {
    let skills: Vec<SkillDefinition> = vec![];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    assert_eq!(enforcer.check("any_tool"), ConstraintResult::Allowed);
    assert_eq!(enforcer.check("another_tool"), ConstraintResult::Allowed);
}

#[test]
fn test_allowed_tools_restrict() {
    let skills = vec![make_skill("s1", Some(vec!["bash", "file_read"]), None)];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    assert_eq!(enforcer.check("bash"), ConstraintResult::Allowed);
    assert_eq!(enforcer.check("file_read"), ConstraintResult::Allowed);
    assert!(matches!(
        enforcer.check("file_write"),
        ConstraintResult::Denied(_)
    ));
}

#[test]
fn test_denied_tools_override() {
    let skills = vec![make_skill(
        "s1",
        Some(vec!["bash", "file_read", "file_write"]),
        Some(vec!["file_write"]),
    )];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    assert_eq!(enforcer.check("bash"), ConstraintResult::Allowed);
    assert_eq!(enforcer.check("file_read"), ConstraintResult::Allowed);
    assert!(matches!(
        enforcer.check("file_write"),
        ConstraintResult::Denied(_)
    ));
}

#[test]
fn test_glob_pattern_match() {
    let skills = vec![make_skill("s1", Some(vec!["mcp:server:*"]), None)];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    assert_eq!(
        enforcer.check("mcp:server:tool1"),
        ConstraintResult::Allowed
    );
    assert_eq!(
        enforcer.check("mcp:server:tool2"),
        ConstraintResult::Allowed
    );
    assert!(matches!(
        enforcer.check("mcp:other:tool1"),
        ConstraintResult::Denied(_)
    ));
}

#[test]
fn test_wildcard_allows_all() {
    let skills = vec![make_skill("s1", Some(vec!["*"]), None)];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    assert_eq!(enforcer.check("anything"), ConstraintResult::Allowed);
    assert_eq!(enforcer.check("really_anything"), ConstraintResult::Allowed);
}

#[test]
fn test_filter_tools() {
    let skills = vec![make_skill("s1", Some(vec!["bash", "file_read"]), None)];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    let tools = vec![
        "bash".to_string(),
        "file_read".to_string(),
        "file_write".to_string(),
        "exec".to_string(),
    ];
    let filtered = enforcer.filter_tools(&tools);
    assert_eq!(filtered, vec!["bash".to_string(), "file_read".to_string()]);
}

#[test]
fn test_merge_multiple_skills() {
    let skills = vec![
        make_skill("s1", Some(vec!["bash"]), None),
        make_skill("s2", Some(vec!["file_read"]), Some(vec!["bash"])),
    ];
    let enforcer = ToolConstraintEnforcer::from_active_skills(&skills);
    // bash is in allowed (from s1) but also in denied (from s2) — denied wins
    assert!(matches!(
        enforcer.check("bash"),
        ConstraintResult::Denied(_)
    ));
    // file_read is allowed
    assert_eq!(enforcer.check("file_read"), ConstraintResult::Allowed);
    // unknown tool not in allowed list
    assert!(matches!(
        enforcer.check("exec"),
        ConstraintResult::Denied(_)
    ));
}
