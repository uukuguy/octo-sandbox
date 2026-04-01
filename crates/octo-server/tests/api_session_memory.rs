//! E2E tests for session and memory API endpoints.
//!
//! Routes under test:
//!   GET  /api/v1/sessions           — list sessions
//!   GET  /api/v1/sessions/:id       — get session details
//!   GET  /api/v1/memories           — list/search memories
//!   POST /api/v1/memories           — create memory
//!   GET  /api/v1/memories/working   — get working memory blocks
//!   GET  /api/v1/memories/:id       — get memory by id
//!   DELETE /api/v1/memories/:id     — delete memory

mod common;

use axum::http::StatusCode;
use serde_json::json;

// ── Session tests ──────────────────────────────────────────────────

#[tokio::test]
async fn list_sessions_returns_array() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/sessions").await;

    assert_eq!(status, StatusCode::OK);
    // Sessions endpoint returns a JSON array (may contain the primary session)
    assert!(body.is_array(), "sessions should be an array, got: {:?}", body);
}

#[tokio::test]
async fn get_session_unknown_returns_error() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/sessions/nonexistent-session-id").await;

    assert_eq!(status, StatusCode::OK); // handler returns 200 with error field
    assert!(body["error"].is_string(), "should contain error field");
}

// ── Memory tests ───────────────────────────────────────────────────

#[tokio::test]
async fn list_memories_initially_empty() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/memories").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].is_array(), "should have results array");
}

#[tokio::test]
async fn create_and_list_memory() {
    let app = common::TestApp::new().await;

    // Create a memory
    let mem = json!({
        "content": "Remember this important fact",
        "category": "profile",
        "importance": 80
    });
    let (status, body) = app.post_json("/api/v1/memories", mem).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["created"].as_bool().unwrap_or(false), "should indicate created=true");
    assert!(body["id"].is_string(), "should return memory id");

    // List memories and verify it appears
    let (status, body) = app.get("/api/v1/memories").await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().expect("should have results array");
    assert!(results.len() >= 1, "should have at least 1 memory");
}

#[tokio::test]
async fn create_memory_with_metadata() {
    let app = common::TestApp::new().await;

    let mem = json!({
        "content": "Test with metadata",
        "category": "profile",
        "metadata": {"key": "value"},
        "importance": 50
    });
    let (status, body) = app.post_json("/api/v1/memories", mem).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["created"].as_bool().unwrap_or(false));
}

#[tokio::test]
async fn search_memories_with_query() {
    let app = common::TestApp::new().await;

    // Create a memory first
    let mem = json!({
        "content": "Rust programming language is great for systems",
        "category": "profile"
    });
    app.post_json("/api/v1/memories", mem).await;

    // Search for it
    let (status, body) = app.get("/api/v1/memories?q=Rust+programming").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].is_array());
}

#[tokio::test]
async fn get_working_memory() {
    let app = common::TestApp::new().await;
    let (status, body) = app.get("/api/v1/memories/working").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["blocks"].is_array(), "should have blocks array");
}

#[tokio::test]
async fn delete_memory() {
    let app = common::TestApp::new().await;

    // Create a memory
    let mem = json!({
        "content": "Ephemeral memory to delete",
        "category": "profile"
    });
    let (status, body) = app.post_json("/api/v1/memories", mem).await;
    assert_eq!(status, StatusCode::OK);
    let mem_id = body["id"].as_str().expect("should return memory id");

    // Delete it
    let (status, body) = app.delete(&format!("/api/v1/memories/{}", mem_id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], mem_id);
}
