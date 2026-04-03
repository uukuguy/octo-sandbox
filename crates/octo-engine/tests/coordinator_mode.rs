//! AV-T5: Tests for coordinator mode.

use octo_engine::agent::coordinator::{build_coordinator_prompt, CoordinatorConfig};

#[test]
fn test_coordinator_prompt_contains_orchestration_tools() {
    let config = CoordinatorConfig::default();
    let prompt = build_coordinator_prompt(&config);
    assert!(prompt.contains("agent_spawn"));
    assert!(prompt.contains("send_message"));
    assert!(prompt.contains("task_stop"));
}

#[test]
fn test_coordinator_prompt_lists_worker_tools() {
    let config = CoordinatorConfig {
        worker_tools: vec!["bash".into(), "file_read".into()],
        mcp_servers: vec![],
    };
    let prompt = build_coordinator_prompt(&config);
    assert!(prompt.contains("bash, file_read"));
}

#[test]
fn test_coordinator_prompt_with_mcp_servers() {
    let config = CoordinatorConfig {
        worker_tools: CoordinatorConfig::default_worker_tools(),
        mcp_servers: vec!["postgres".into(), "redis".into()],
    };
    let prompt = build_coordinator_prompt(&config);
    assert!(prompt.contains("postgres, redis"));
    assert!(!prompt.contains("None"));
}

#[test]
fn test_coordinator_default_excludes_agent_spawn() {
    let defaults = CoordinatorConfig::default_worker_tools();
    assert!(!defaults.contains(&"agent_spawn".to_string()));
    assert!(!defaults.contains(&"send_message".to_string()));
    assert!(!defaults.contains(&"task_stop".to_string()));
}

#[test]
fn test_coordinator_config_serialization() {
    let config = CoordinatorConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("worker_tools"));
    let decoded: CoordinatorConfig = serde_json::from_str(&json).unwrap();
    assert!(!decoded.worker_tools.is_empty());
}
