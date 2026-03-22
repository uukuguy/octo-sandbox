use octo_engine::skills::trust::{TrustError, TrustManager};
use octo_types::skill::{SkillDefinition, SkillSourceType, TrustLevel};

fn test_skill() -> SkillDefinition {
    SkillDefinition {
        name: "test".into(),
        description: "test skill".into(),
        version: None,
        user_invocable: false,
        allowed_tools: None,
        body: String::new(),
        base_dir: std::path::PathBuf::new(),
        source_path: std::path::PathBuf::new(),
        body_loaded: false,
        model: None,
        context_fork: false,
        always: false,
        trust_level: TrustLevel::Installed,
        triggers: vec![],
        dependencies: vec![],
        tags: vec![],
        denied_tools: None,
        execution_mode: Default::default(),
        source_type: SkillSourceType::ProjectLocal,
        max_rounds: 0,
    }
}

#[test]
fn test_effective_trust_project_local_trusted() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.source_type = SkillSourceType::ProjectLocal;
    skill.trust_level = TrustLevel::Trusted;
    assert_eq!(tm.effective_trust_level(&skill), TrustLevel::Trusted);
}

#[test]
fn test_effective_trust_registry_capped_at_unknown() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.source_type = SkillSourceType::Registry;
    skill.trust_level = TrustLevel::Trusted;
    assert_eq!(tm.effective_trust_level(&skill), TrustLevel::Unknown);
}

#[test]
fn test_effective_trust_user_local_capped_at_installed() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.source_type = SkillSourceType::UserLocal;
    skill.trust_level = TrustLevel::Trusted;
    assert_eq!(tm.effective_trust_level(&skill), TrustLevel::Installed);
}

#[test]
fn test_trusted_allows_all_tools() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Trusted;
    skill.source_type = SkillSourceType::ProjectLocal;
    assert!(tm.check_tool_permission(&skill, "bash").is_ok());
    assert!(tm.check_tool_permission(&skill, "file_read").is_ok());
}

#[test]
fn test_installed_allows_only_listed() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Installed;
    skill.allowed_tools = Some(vec!["read".into(), "grep".into()]);
    assert!(tm.check_tool_permission(&skill, "read").is_ok());
    assert!(tm.check_tool_permission(&skill, "bash").is_err());
}

#[test]
fn test_unknown_only_readonly() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Unknown;
    skill.source_type = SkillSourceType::Registry;
    assert!(tm.check_tool_permission(&skill, "file_read").is_ok());
    assert!(tm.check_tool_permission(&skill, "glob").is_ok());
    assert!(tm.check_tool_permission(&skill, "bash").is_err());
}

#[test]
fn test_denied_tools_override() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Trusted;
    skill.source_type = SkillSourceType::ProjectLocal;
    skill.denied_tools = Some(vec!["bash".into()]);
    assert!(tm.check_tool_permission(&skill, "bash").is_err());
    assert!(tm.check_tool_permission(&skill, "file_read").is_ok());
}

#[test]
fn test_installed_no_allowed_tools_denies_all() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Installed;
    skill.allowed_tools = None;
    let result = tm.check_tool_permission(&skill, "read");
    assert_eq!(
        result,
        Err(TrustError::NoAllowedToolsDefined {
            skill: "test".into(),
        })
    );
}

#[test]
fn test_installed_wildcard_allows_all() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Installed;
    skill.allowed_tools = Some(vec!["*".into()]);
    assert!(tm.check_tool_permission(&skill, "bash").is_ok());
    assert!(tm.check_tool_permission(&skill, "file_read").is_ok());
}

#[test]
fn test_extra_readonly_tools() {
    let tm = TrustManager::new(vec!["custom_read".into()]);
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Unknown;
    skill.source_type = SkillSourceType::Registry;
    assert!(tm.check_tool_permission(&skill, "custom_read").is_ok());
    assert!(tm.check_tool_permission(&skill, "bash").is_err());
}

#[test]
fn test_plugin_bundled_capped_at_installed() {
    let tm = TrustManager::default();
    let mut skill = test_skill();
    skill.source_type = SkillSourceType::PluginBundled;
    skill.trust_level = TrustLevel::Trusted;
    assert_eq!(tm.effective_trust_level(&skill), TrustLevel::Installed);
}
