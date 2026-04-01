//! E2E tests for Secret Vault API endpoints (AO-T6)

mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn secrets_list_returns_ok() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/secrets").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["secrets"].is_array());
    assert!(body["count"].is_number());
}

#[tokio::test]
async fn secrets_verify_returns_status() {
    let app = common::TestApp::new().await;
    let (status, body) = app.post_json("/api/v1/secrets/verify", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("unlocked").is_some());
}

#[tokio::test]
async fn secrets_store_without_vault_returns_error() {
    let app = common::TestApp::new().await;
    let (status, _body) = app
        .post_json(
            "/api/v1/secrets",
            serde_json::json!({
                "name": "test-secret",
                "value": "test-value"
            }),
        )
        .await;
    // TestApp doesn't initialize a vault, so expect 503
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn secrets_delete_without_vault_returns_error() {
    let app = common::TestApp::new().await;
    let (status, _body) = app.delete("/api/v1/secrets/test-key").await;
    // TestApp doesn't initialize a vault, so expect 503
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
