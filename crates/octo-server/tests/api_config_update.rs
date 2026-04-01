mod common;

use axum::http::StatusCode;

#[tokio::test]
async fn put_config_valid_fields() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .put_json(
            "/api/v1/config",
            serde_json::json!({
                "provider_name": "openai",
                "provider_model": "gpt-4o",
                "logging_format": "json",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    let updated = body["updated_fields"].as_array().unwrap();
    assert!(updated.len() >= 2);
    assert!(!body["restart_required"].as_bool().unwrap());
}

#[tokio::test]
async fn put_config_empty_body() {
    let app = common::TestApp::new().await;

    let (status, body) = app.put_json("/api/v1/config", serde_json::json!({})).await;

    assert_eq!(status, StatusCode::OK);
    let updated = body["updated_fields"].as_array().unwrap();
    assert!(updated.is_empty());
}

#[tokio::test]
async fn put_config_rejects_non_updatable() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .put_json(
            "/api/v1/config",
            serde_json::json!({
                "port": 9999,
                "host": "0.0.0.0",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "non_updatable_fields");
}

#[tokio::test]
async fn get_config_reflects_put_changes() {
    let app = common::TestApp::new().await;

    // PUT to update provider
    let (status, _) = app
        .put_json(
            "/api/v1/config",
            serde_json::json!({
                "provider_name": "openai",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // GET should reflect the update
    let (status, body) = app.get("/api/v1/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["provider"], "openai");
}

#[tokio::test]
async fn put_config_invalid_logging_format() {
    let app = common::TestApp::new().await;

    let (status, body) = app
        .put_json(
            "/api/v1/config",
            serde_json::json!({
                "logging_format": "xml",
            }),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_value");
}
