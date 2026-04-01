//! E2E tests for health, config, budget, and metrics endpoints.

mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn health_returns_ok_with_status_and_version() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string(), "version should be a string");
    assert!(body["uptime_secs"].is_number(), "uptime_secs should be a number");
    assert!(body["mcp_servers"].is_array(), "mcp_servers should be an array");
}

#[tokio::test]
async fn config_returns_provider_info() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/config").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["provider"].is_string(), "provider should be a string");
    assert!(body["port"].is_number(), "port should be a number");
    assert!(body["host"].is_string(), "host should be a string");
    assert!(body["api_url"].is_string(), "api_url should be a string");
    assert!(body["ws_url"].is_string(), "ws_url should be a string");
}

#[tokio::test]
async fn budget_returns_token_budget() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/budget").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["total"].is_number(), "total should be a number");
    assert!(body["free"].is_number(), "free should be a number");
    assert!(body["usage_percent"].is_number(), "usage_percent should be a number");
}

#[tokio::test]
async fn metrics_returns_snapshot() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/metrics").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["timestamp"].is_string(), "timestamp should be a string");
    assert!(body["counters"].is_array(), "counters should be an array");
    assert!(body["gauges"].is_array(), "gauges should be an array");
    assert!(body["histograms"].is_array(), "histograms should be an array");
}
