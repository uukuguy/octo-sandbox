use eaasp_mcp_orchestrator::config::{McpServerDef, RunMode};
use eaasp_mcp_orchestrator::manager::McpManager;
use std::collections::HashMap;

fn make_server(name: &str, command: &str, args: &[&str], mode: RunMode, tags: &[&str]) -> McpServerDef {
    McpServerDef {
        name: name.to_string(),
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        transport: "stdio".to_string(),
        port: 0,
        mode,
        tags: tags.iter().map(|s| s.to_string()).collect(),
        env: HashMap::new(),
        health_endpoint: String::new(),
    }
}

#[tokio::test]
async fn manager_load_config_and_list() {
    let mgr = McpManager::new(vec![make_server(
        "erp-mcp",
        "echo",
        &["hello"],
        RunMode::Shared,
        &["erp"],
    )]);

    let servers = mgr.list_servers().await;
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "erp-mcp");
    assert!(!servers[0].running);
    assert!(servers[0].pid.is_none());
}

#[tokio::test]
async fn manager_start_stop_shared() {
    let mgr = McpManager::new(vec![make_server(
        "test-echo",
        "sleep",
        &["30"],
        RunMode::Shared,
        &[],
    )]);

    mgr.start("test-echo").await.expect("start should succeed");

    let servers = mgr.list_servers().await;
    assert!(servers[0].running, "server should be running after start");
    assert!(servers[0].pid.is_some());

    mgr.stop("test-echo").await.expect("stop should succeed");

    let servers = mgr.list_servers().await;
    assert!(!servers[0].running, "server should not be running after stop");
    assert!(servers[0].pid.is_none());
}

#[tokio::test]
async fn manager_filter_by_tags() {
    let mgr = McpManager::new(vec![
        make_server("erp-server", "echo", &["1"], RunMode::Shared, &["erp"]),
        make_server("crm-server", "echo", &["2"], RunMode::Shared, &["crm"]),
    ]);

    let erp_only = mgr.list_by_tags(&["erp"]).await;
    assert_eq!(erp_only.len(), 1);
    assert_eq!(erp_only[0].name, "erp-server");

    let crm_only = mgr.list_by_tags(&["crm"]).await;
    assert_eq!(crm_only.len(), 1);
    assert_eq!(crm_only[0].name, "crm-server");

    let both = mgr.list_by_tags(&["erp", "crm"]).await;
    assert_eq!(both.len(), 2);
}

#[tokio::test]
async fn resolve_dependencies_happy_path() {
    let servers = vec![
        make_server(
            "mock-scada",
            "mock-scada",
            &["--transport", "stdio"],
            RunMode::Shared,
            &["eaasp"],
        ),
        make_server(
            "eaasp-l2-memory",
            "eaasp-l2-memory",
            &[],
            RunMode::Shared,
            &["eaasp"],
        ),
    ];
    let mgr = McpManager::new(servers);
    let deps = vec![
        "mcp:mock-scada".to_string(),
        "mcp:eaasp-l2-memory".to_string(),
    ];
    let resolved = mgr.resolve_dependencies(&deps);
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].name, "mock-scada");
    assert_eq!(resolved[1].name, "eaasp-l2-memory");
}

#[tokio::test]
async fn resolve_dependencies_unknown_ignored() {
    let servers = vec![make_server(
        "mock-scada",
        "mock-scada",
        &[],
        RunMode::Shared,
        &[],
    )];
    let mgr = McpManager::new(servers);
    let deps = vec![
        "mcp:mock-scada".to_string(),
        "mcp:nonexistent".to_string(),
    ];
    let resolved = mgr.resolve_dependencies(&deps);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].name, "mock-scada");
}

#[tokio::test]
async fn resolve_dependencies_non_mcp_filtered() {
    let servers = vec![make_server(
        "mock-scada",
        "mock-scada",
        &[],
        RunMode::Shared,
        &[],
    )];
    let mgr = McpManager::new(servers);
    let deps = vec![
        "pip:numpy".to_string(),
        "mcp:mock-scada".to_string(),
    ];
    let resolved = mgr.resolve_dependencies(&deps);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].name, "mock-scada");
}
