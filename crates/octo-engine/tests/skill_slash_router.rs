use std::path::PathBuf;

use octo_engine::skills::{SkillSlashRouter, SlashCommand};
use octo_types::skill::{SkillDefinition, SkillSourceType, SkillTrigger, TrustLevel};

fn make_skill(name: &str, triggers: Vec<SkillTrigger>) -> SkillDefinition {
    SkillDefinition {
        name: name.to_string(),
        description: format!("Test skill {}", name),
        version: None,
        user_invocable: false,
        allowed_tools: None,
        denied_tools: None,
        body: String::new(),
        base_dir: PathBuf::from("/test"),
        source_path: PathBuf::from("/test/SKILL.md"),
        body_loaded: false,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::default(),
        triggers,
        dependencies: vec![],
        tags: vec![],
        execution_mode: Default::default(),
        source_type: SkillSourceType::default(),
        max_rounds: 0,
    }
}

#[test]
fn test_basic_slash_route() {
    let skills = vec![make_skill("test", vec![])];
    let router = SkillSlashRouter::build(&skills);
    let result = router.route("/test");
    assert_eq!(
        result,
        Some(SlashCommand {
            skill_name: "test".to_string(),
            args: vec![],
        })
    );
}

#[test]
fn test_slash_with_args() {
    let skills = vec![make_skill("deploy", vec![])];
    let router = SkillSlashRouter::build(&skills);
    let result = router.route("/deploy arg1 arg2");
    assert_eq!(
        result,
        Some(SlashCommand {
            skill_name: "deploy".to_string(),
            args: vec!["arg1".to_string(), "arg2".to_string()],
        })
    );
}

#[test]
fn test_no_slash_returns_none() {
    let skills = vec![make_skill("test", vec![])];
    let router = SkillSlashRouter::build(&skills);
    assert_eq!(router.route("hello"), None);
    assert_eq!(router.route("no slash here"), None);
}

#[test]
fn test_command_trigger_route() {
    let skills = vec![make_skill(
        "formatter",
        vec![SkillTrigger::Command {
            command: "/fmt".to_string(),
        }],
    )];
    let router = SkillSlashRouter::build(&skills);
    // Both /formatter and /fmt should route to the skill
    assert_eq!(
        router.route("/formatter"),
        Some(SlashCommand {
            skill_name: "formatter".to_string(),
            args: vec![],
        })
    );
    assert_eq!(
        router.route("/fmt"),
        Some(SlashCommand {
            skill_name: "formatter".to_string(),
            args: vec![],
        })
    );
}

#[test]
fn test_case_insensitive() {
    let skills = vec![make_skill("Test", vec![])];
    let router = SkillSlashRouter::build(&skills);
    let result = router.route("/test");
    assert_eq!(
        result,
        Some(SlashCommand {
            skill_name: "Test".to_string(),
            args: vec![],
        })
    );
    let result2 = router.route("/TEST");
    assert_eq!(
        result2,
        Some(SlashCommand {
            skill_name: "Test".to_string(),
            args: vec![],
        })
    );
}

#[test]
fn test_unknown_command() {
    let skills = vec![make_skill("test", vec![])];
    let router = SkillSlashRouter::build(&skills);
    assert_eq!(router.route("/unknown"), None);
}

#[test]
fn test_list_routes() {
    let skills = vec![
        make_skill("alpha", vec![]),
        make_skill(
            "beta",
            vec![SkillTrigger::Command {
                command: "/b".to_string(),
            }],
        ),
    ];
    let router = SkillSlashRouter::build(&skills);
    let mut routes = router.list_routes();
    routes.sort_by_key(|(cmd, _)| cmd.to_string());
    assert_eq!(routes.len(), 3); // alpha, beta, b
    assert!(routes.contains(&("alpha", "alpha")));
    assert!(routes.contains(&("beta", "beta")));
    assert!(routes.contains(&("b", "beta")));
}
