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
    let runtime =
        UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None);
    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.user_id, "test-user-1");
    assert_eq!(session.status, SessionStatus::Active);
}

#[test]
fn test_create_named_session() {
    let runtime =
        UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(Some("My Session".to_string()));
    assert!(session.is_ok());
    let session = session.unwrap();
    assert_eq!(session.name, Some("My Session".to_string()));
}

#[test]
fn test_concurrent_session_limit() {
    let runtime =
        UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    // Create 3 sessions (at limit)
    for _ in 0..3 {
        let result = runtime.create_session(None);
        assert!(result.is_ok());
    }

    // Try to create 4th - should fail
    let result = runtime.create_session(None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Concurrent session limit"));
}

#[test]
fn test_get_session() {
    let runtime =
        UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None).unwrap();
    let retrieved = runtime.get_session("test-user-1", &session.id);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, session.id);
}

#[test]
fn test_delete_session() {
    let runtime =
        UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    let session = runtime.create_session(None).unwrap();
    assert_eq!(runtime.sessions.len(), 1);

    let deleted = runtime.delete_session("test-user-1", &session.id);
    assert!(deleted);
    assert_eq!(runtime.sessions.len(), 0);
}

#[test]
fn test_list_sessions() {
    let runtime =
        UserRuntime::new("test-user-1".to_string(), Arc::new(create_test_config())).unwrap();

    runtime.create_session(None).unwrap();
    runtime
        .create_session(Some("Session 2".to_string()))
        .unwrap();

    let sessions = runtime.list_sessions("test-user-1");
    assert_eq!(sessions.len(), 2);
}

/// Test that verifies User A cannot access User B's session
/// This tests the cross-user isolation property
#[test]
fn test_cross_user_isolation() {
    // Create runtime for user A
    let runtime_a = UserRuntime::new("user-a".to_string(), Arc::new(create_test_config())).unwrap();

    // Create a session for user A
    let session_a = runtime_a
        .create_session(Some("User A Session".to_string()))
        .unwrap();
    assert_eq!(session_a.user_id, "user-a");

    // Try to retrieve user A's session with user B's user_id - should return None
    let retrieved_by_wrong_user = runtime_a.get_session("user-b", &session_a.id);
    assert!(
        retrieved_by_wrong_user.is_none(),
        "User B should not be able to access User A's session"
    );

    // Verify user A can still access their own session
    let retrieved_by_correct_user = runtime_a.get_session("user-a", &session_a.id);
    assert!(retrieved_by_correct_user.is_some());
    assert_eq!(retrieved_by_correct_user.unwrap().id, session_a.id);
}

/// Test that verifies delete_session correctly prevents unauthorized deletion
/// The session should NOT be deleted when a wrong user tries to delete it.
#[test]
fn test_delete_session_wrong_owner_returns_not_found() {
    // Create runtime for user A
    let runtime_a = UserRuntime::new("user-a".to_string(), Arc::new(create_test_config())).unwrap();

    // Create a session for user A
    let session_a = runtime_a.create_session(None).unwrap();
    assert_eq!(runtime_a.sessions.len(), 1);

    // Try to delete user A's session with user B's user_id - should return false
    let deleted_by_wrong_user = runtime_a.delete_session("user-b", &session_a.id);
    assert!(
        !deleted_by_wrong_user,
        "User B should not be able to delete User A's session"
    );

    // The session should still exist because authorization failed
    assert_eq!(
        runtime_a.sessions.len(),
        1,
        "Session should remain when wrong owner tries to delete it"
    );

    // User A should still be able to access and delete their own session
    let deleted_by_owner = runtime_a.delete_session("user-a", &session_a.id);
    assert!(
        deleted_by_owner,
        "Owner should be able to delete their own session"
    );
    assert_eq!(runtime_a.sessions.len(), 0);
}
