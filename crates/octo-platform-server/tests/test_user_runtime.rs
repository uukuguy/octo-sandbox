//! Tests for UserRuntime

use octo_platform_server::{SessionStatus, UserRuntime, UserRuntimeConfig};
use std::sync::Arc;

fn create_test_config() -> UserRuntimeConfig {
    UserRuntimeConfig {
        max_concurrent_agents: 3,
        session_timeout_minutes: 30,
        db_path_template: "data-platform/test/users/{user_id}".to_string(),
    }
}

#[test]
fn test_user_runtime_creation() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config()));
    assert!(runtime.is_ok());
    let runtime = runtime.unwrap();
    assert_eq!(runtime.user_id, "test-user-1");
    assert!(runtime.sessions.is_empty());
}

#[test]
fn test_create_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None);
    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.user_id, "test-user-1");
    assert_eq!(session.status, SessionStatus::Active);
}

#[test]
fn test_create_named_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(Some("My Session".to_string()));
    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.name, Some("My Session".to_string()));
}

#[test]
fn test_concurrent_session_limit() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    // Create 3 sessions (at limit)
    for _ in 0..3 {
        let result = runtime.create_session(None);
        assert!(result.is_ok());
    }

    // Try to create 4th - should fail
    let result = runtime.create_session(None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Concurrent session limit"));
}

#[test]
fn test_get_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None).unwrap();
    let retrieved = runtime.get_session("test-user-1", &session.id);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, session.id);
}

#[test]
fn test_delete_session() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None).unwrap();
    assert_eq!(runtime.sessions.len(), 1);

    let deleted = runtime.delete_session("test-user-1", &session.id);
    assert!(deleted);
    assert_eq!(runtime.sessions.len(), 0);
}

#[test]
fn test_list_sessions() {
    let runtime = UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    runtime.create_session(None).unwrap();
    runtime.create_session(Some("Session 2".to_string())).unwrap();

    let sessions = runtime.list_sessions("test-user-1");
    assert_eq!(sessions.len(), 2);
}
