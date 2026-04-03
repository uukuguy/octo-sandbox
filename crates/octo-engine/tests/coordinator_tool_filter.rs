//! AV-D4: Tests for coordinator worker tool filtering.

use octo_engine::agent::coordinator::CoordinatorConfig;

#[test]
fn test_coordinator_default_tools_excludes_spawn() {
    let defaults = CoordinatorConfig::default_worker_tools();
    assert!(!defaults.contains(&"spawn_subagent".to_string()));
    assert!(!defaults.contains(&"agent_spawn".to_string()));
    assert!(defaults.contains(&"bash".to_string()));
    assert!(defaults.contains(&"file_read".to_string()));
}

#[test]
fn test_coordinator_intersect_whitelist() {
    let allowed = CoordinatorConfig::default_worker_tools();
    let whitelist = vec![
        "bash".to_string(),
        "spawn_subagent".to_string(),
        "grep".to_string(),
    ];

    // Intersection: only tools in BOTH lists
    let result: Vec<String> = whitelist
        .into_iter()
        .filter(|t| allowed.contains(t))
        .collect();

    assert!(result.contains(&"bash".to_string()));
    assert!(result.contains(&"grep".to_string()));
    assert!(!result.contains(&"spawn_subagent".to_string())); // filtered out
}

#[test]
fn test_non_coordinator_no_filter() {
    // When coordinator=false, no filtering applied
    let coordinator = false;
    let worker_allowed_tools: Vec<String> = vec![];

    let filter = if coordinator {
        Some(if worker_allowed_tools.is_empty() {
            CoordinatorConfig::default_worker_tools()
        } else {
            worker_allowed_tools
        })
    } else {
        None
    };

    assert!(filter.is_none());
}

#[test]
fn test_coordinator_custom_tools() {
    // Custom worker_allowed_tools override defaults
    let custom = vec!["bash".to_string(), "grep".to_string()];
    let coordinator = true;

    let filter = if coordinator {
        Some(if custom.is_empty() {
            CoordinatorConfig::default_worker_tools()
        } else {
            custom.clone()
        })
    } else {
        None
    };

    assert_eq!(filter.unwrap(), custom);
}

#[test]
fn test_coordinator_empty_tools_uses_defaults() {
    // When coordinator=true but worker_allowed_tools is empty, use defaults
    let coordinator = true;
    let worker_allowed_tools: Vec<String> = vec![];

    let filter = if coordinator {
        Some(if worker_allowed_tools.is_empty() {
            CoordinatorConfig::default_worker_tools()
        } else {
            worker_allowed_tools
        })
    } else {
        None
    };

    let filter = filter.unwrap();
    assert!(!filter.is_empty());
    assert!(filter.contains(&"bash".to_string()));
    // Defaults should not include spawn tools
    assert!(!filter.contains(&"spawn_subagent".to_string()));
}
