//! Tests for resolved NOT_IMPLEMENTED stubs:
//! - PUT /security/policy
//! - POST /hooks/wasm/:name/reload

mod common;

use axum::http::StatusCode;

// ── PUT /security/policy ─────────────────────────────────────────────

#[tokio::test]
async fn put_security_policy_valid() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .put_json(
            "/api/v1/security/policy",
            serde_json::json!({
                "autonomy_level": "full",
                "block_high_risk_commands": false,
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    let updated = body["updated_fields"].as_array().unwrap();
    assert!(updated.len() >= 1);
}

#[tokio::test]
async fn put_security_policy_invalid_autonomy() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .put_json(
            "/api/v1/security/policy",
            serde_json::json!({
                "autonomy_level": "godmode",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_value");
}

#[tokio::test]
async fn get_security_policy_reflects_put() {
    let app = common::TestApp::new().await;

    // Update policy
    let (status, _) = app
        .put_json(
            "/api/v1/security/policy",
            serde_json::json!({
                "autonomy_level": "readonly",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // GET should reflect the override
    let (status, body) = app.get("/api/v1/security/policy").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["autonomy_level"], "readonly");
}

// ── POST /hooks/wasm/:name/reload ────────────────────────────────────

#[tokio::test]
async fn reload_wasm_plugin_no_wasm_feature() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .post_json("/api/v1/hooks/wasm/test-plugin/reload", serde_json::json!({}))
        .await;

    // Without sandbox-wasm feature, returns a graceful response (not 501)
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["reloaded"], false);
    assert!(body["message"].as_str().unwrap().contains("WASM"));
}
