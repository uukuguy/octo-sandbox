use octo_engine::hooks::{HookContext, HookPoint};

#[test]
fn test_skills_activated_hook_point_exists() {
    let point = HookPoint::SkillsActivated;
    assert_eq!(point, HookPoint::SkillsActivated);
}

#[test]
fn test_skill_deactivated_hook_point_exists() {
    let point = HookPoint::SkillDeactivated;
    assert_eq!(point, HookPoint::SkillDeactivated);
}

#[test]
fn test_skill_script_started_hook_point_exists() {
    let point = HookPoint::SkillScriptStarted;
    assert_eq!(point, HookPoint::SkillScriptStarted);
}

#[test]
fn test_tool_constraint_violated_hook_point_exists() {
    let point = HookPoint::ToolConstraintViolated;
    assert_eq!(point, HookPoint::ToolConstraintViolated);
}

#[test]
fn test_hook_points_are_distinct() {
    assert_ne!(HookPoint::SkillsActivated, HookPoint::SkillDeactivated);
    assert_ne!(
        HookPoint::SkillScriptStarted,
        HookPoint::ToolConstraintViolated
    );
    assert_ne!(HookPoint::SkillsActivated, HookPoint::PreToolUse);
}

#[test]
fn test_context_with_skill() {
    let ctx = HookContext::new().with_skill("my-skill");
    assert_eq!(ctx.skill_name.as_deref(), Some("my-skill"));
}

#[test]
fn test_context_with_activated_skills() {
    let skills = vec!["skill-a".to_string(), "skill-b".to_string()];
    let ctx = HookContext::new().with_activated_skills(skills.clone(), "how do I search the web?");

    assert_eq!(ctx.activated_skills, Some(skills));
    assert_eq!(
        ctx.activation_query.as_deref(),
        Some("how do I search the web?")
    );
}

#[test]
fn test_context_with_script() {
    let ctx = HookContext::new().with_script("/path/to/script.py", "python");

    assert_eq!(ctx.script_path.as_deref(), Some("/path/to/script.py"));
    assert_eq!(ctx.runtime_type.as_deref(), Some("python"));
}

#[test]
fn test_context_with_constraint_violation() {
    let ctx = HookContext::new().with_constraint_violation(
        "bash",
        "safe-skill",
        "tool not in allowed_tools list",
    );

    assert_eq!(ctx.tool_name.as_deref(), Some("bash"));
    assert_eq!(ctx.skill_name.as_deref(), Some("safe-skill"));
    assert_eq!(
        ctx.constraint_reason.as_deref(),
        Some("tool not in allowed_tools list")
    );
}

#[test]
fn test_context_skill_fields_default_none() {
    let ctx = HookContext::new();
    assert!(ctx.skill_name.is_none());
    assert!(ctx.activated_skills.is_none());
    assert!(ctx.activation_query.is_none());
    assert!(ctx.script_path.is_none());
    assert!(ctx.runtime_type.is_none());
    assert!(ctx.constraint_reason.is_none());
}

#[test]
fn test_context_builder_chaining() {
    let ctx = HookContext::new()
        .with_session("session-1")
        .with_skill("my-skill")
        .with_script("/path/script.py", "python")
        .with_agent("agent-1");

    assert_eq!(ctx.session_id.as_deref(), Some("session-1"));
    assert_eq!(ctx.skill_name.as_deref(), Some("my-skill"));
    assert_eq!(ctx.script_path.as_deref(), Some("/path/script.py"));
    assert_eq!(ctx.runtime_type.as_deref(), Some("python"));
    assert_eq!(ctx.agent_id.as_deref(), Some("agent-1"));
}

#[test]
fn test_hook_point_clone_and_copy() {
    let point = HookPoint::SkillsActivated;
    let cloned = point.clone();
    let copied = point;
    assert_eq!(point, cloned);
    assert_eq!(point, copied);
}

#[test]
fn test_hook_point_debug() {
    let debug_str = format!("{:?}", HookPoint::SkillsActivated);
    assert_eq!(debug_str, "SkillsActivated");
}
