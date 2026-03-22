use std::path::PathBuf;
use std::sync::Arc;

use octo_types::skill::{SkillDefinition, SkillSourceType, TrustLevel};

use octo_engine::skills::trust::TrustManager;
use octo_engine::tools::ToolCallInterceptor;

fn test_skill() -> SkillDefinition {
    SkillDefinition {
        name: "test-skill".into(),
        description: "test".into(),
        version: None,
        user_invocable: false,
        allowed_tools: None,
        body: String::new(),
        base_dir: PathBuf::new(),
        source_path: PathBuf::new(),
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
fn test_interceptor_no_skill_allows_all() {
    let tm = Arc::new(TrustManager::default());
    let interceptor = ToolCallInterceptor::new(tm, None);
    assert!(interceptor.check_permission("bash").is_ok());
    assert!(interceptor.check_permission("file_read").is_ok());
    assert!(interceptor.check_permission("anything").is_ok());
}

#[test]
fn test_interceptor_installed_skill_allows_listed() {
    let tm = Arc::new(TrustManager::default());
    let mut skill = test_skill();
    skill.allowed_tools = Some(vec!["read".into(), "grep".into()]);
    let interceptor = ToolCallInterceptor::new(tm, Some(skill));
    assert!(interceptor.check_permission("read").is_ok());
    assert!(interceptor.check_permission("grep").is_ok());
    assert!(interceptor.check_permission("bash").is_err());
}

#[test]
fn test_interceptor_denied_tools_override() {
    let tm = Arc::new(TrustManager::default());
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Trusted;
    skill.denied_tools = Some(vec!["http_request".into()]);
    let interceptor = ToolCallInterceptor::new(tm, Some(skill));
    assert!(interceptor.check_permission("bash").is_ok());
    assert!(interceptor.check_permission("http_request").is_err());
}

#[test]
fn test_filter_available_tools() {
    let tm = Arc::new(TrustManager::default());
    let mut skill = test_skill();
    skill.allowed_tools = Some(vec!["read".into(), "grep".into()]);
    let interceptor = ToolCallInterceptor::new(tm, Some(skill));

    let all_tools: Vec<String> = vec![
        "read".into(),
        "grep".into(),
        "bash".into(),
        "file_write".into(),
    ];
    let filtered = interceptor.filter_available_tools(&all_tools);
    assert_eq!(filtered, vec!["read".to_string(), "grep".to_string()]);
}

#[test]
fn test_interceptor_wildcard_allows_all_except_denied() {
    let tm = Arc::new(TrustManager::default());
    let mut skill = test_skill();
    skill.allowed_tools = Some(vec!["*".into()]);
    skill.denied_tools = Some(vec!["bash".into()]);
    let interceptor = ToolCallInterceptor::new(tm, Some(skill));
    assert!(interceptor.check_permission("read").is_ok());
    assert!(interceptor.check_permission("file_write").is_ok());
    assert!(interceptor.check_permission("bash").is_err());
}

#[test]
fn test_set_active_skill() {
    let tm = Arc::new(TrustManager::default());
    let mut interceptor = ToolCallInterceptor::new(tm, None);
    assert!(!interceptor.has_active_skill());

    interceptor.set_active_skill(Some(test_skill()));
    assert!(interceptor.has_active_skill());

    interceptor.set_active_skill(None);
    assert!(!interceptor.has_active_skill());
}
