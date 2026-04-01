//! E2E tests for MCP servers, tools, scheduler tasks, and audit endpoints.
//!
//! Routes under test:
//!   GET  /api/v1/mcp/servers         — list MCP servers
//!   POST /api/v1/mcp/servers         — add MCP server
//!   GET  /api/v1/tools               — list tools
//!   GET  /api/v1/scheduler/tasks     — list scheduler tasks
//!   GET  /api/v1/audit               — list audit events

mod common;

use axum::http::StatusCode;
use serde_json::json;

// ── MCP server tests ───────────────────────────────────────────────

#[tokio::test]
async fn list_mcp_servers_initially_empty() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/mcp/servers").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "should return an array");
    assert_eq!(body.as_array().unwrap().len(), 0, "should start empty");
}

#[tokio::test]
async fn create_mcp_server() {
    let app = common::TestApp::new().await;
    let server_config = json!({
        "name": "test-mcp-server",
        "command": "node",
        "args": ["server.js"],
        "transport": "stdio"
    });

    let (status, body) = app.post_json("/api/v1/mcp/servers", server_config).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "test-mcp-server");
    assert!(body["id"].is_string(), "should return server id");
    assert_eq!(body["transport"], "stdio");
    assert_eq!(body["runtime_status"], "stopped");
}

#[tokio::test]
async fn create_mcp_server_rejected_command() {
    let app = common::TestApp::new().await;
    let server_config = json!({
        "name": "bad-server",
        "command": "rm",
        "args": ["-rf", "/"],
        "transport": "stdio"
    });

    let (status, body) = app.post_json("/api/v1/mcp/servers", server_config).await;

    assert_eq!(status, StatusCode::OK);
    // Rejected commands get enabled=false and runtime_status=error
    assert_eq!(body["enabled"], false);
    assert_eq!(body["runtime_status"], "error");
}

#[tokio::test]
async fn create_mcp_server_has_correct_fields() {
    let app = common::TestApp::new().await;

    let server_config = json!({
        "name": "field-check-server",
        "command": "python3",
        "args": ["mcp_server.py"],
        "source": "test",
        "enabled": true
    });
    let (status, body) = app.post_json("/api/v1/mcp/servers", server_config).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "field-check-server");
    assert_eq!(body["command"], "python3");
    assert_eq!(body["source"], "test");
    assert_eq!(body["enabled"], true);
    assert!(body["created_at"].is_string());
    assert!(body["updated_at"].is_string());
}

// ── Tool tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn list_tools_returns_array() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/tools").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "should return an array");
    // Should have built-in tools registered
    let tools = body.as_array().unwrap();
    assert!(tools.len() > 0, "should have at least some built-in tools");
}

#[tokio::test]
async fn tools_have_required_fields() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/tools").await;

    assert_eq!(status, StatusCode::OK);
    let tools = body.as_array().unwrap();
    if let Some(tool) = tools.first() {
        assert!(tool["name"].is_string(), "tool should have name");
        assert!(tool["description"].is_string(), "tool should have description");
    }
}

// ── Scheduler tests ────────────────────────────────────────────────

#[tokio::test]
async fn scheduler_tasks_returns_404_when_disabled() {
    // Default TestApp has no scheduler
    let app = common::TestApp::new().await;
    let (status, _body) = app.get("/api/v1/scheduler/tasks").await;

    // When scheduler is None, the handler returns 404
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn scheduler_tasks_returns_empty_when_enabled() {
    let app = common::TestApp::with_scheduler().await;
    let (status, body) = app.get("/api/v1/scheduler/tasks").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["tasks"].is_array(), "should have tasks array");
    assert_eq!(body["total"], 0);
}

// ── Audit tests ────────────────────────────────────────────────────

#[tokio::test]
async fn audit_returns_empty_logs() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/audit").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["logs"].is_array(), "should have logs array");
    assert!(body["total"].is_number(), "should have total count");
}

// ── Execution tests ────────────────────────────────────────────────

#[tokio::test]
async fn list_executions_returns_array() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/executions").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array(), "should return an array");
}
