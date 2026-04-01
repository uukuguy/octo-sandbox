//! Integration tests for multi-session lifecycle (Phase AJ-T6/T7).
//!
//! Validates the session lifecycle API on `AgentRuntime`: creating, listing,
//! counting, limiting, stopping, and primary-session compatibility.

use std::sync::Arc;

use octo_engine::providers::ProviderConfig;
use octo_engine::{AgentCatalog, AgentRuntime, AgentRuntimeConfig, TenantContext};
use octo_types::{SandboxId, SessionId, TenantId, UserId};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Build a test `AgentRuntime` with an optional concurrent-session cap.
async fn create_test_runtime_with_limit(max_sessions: Option<usize>) -> Arc<AgentRuntime> {
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let db_path_str = db_path.to_str().unwrap().to_string();

    let catalog = Arc::new(AgentCatalog::new());
    let mut runtime_config =
        AgentRuntimeConfig::from_parts(db_path_str, ProviderConfig::default(), vec![], None, None, false);
    runtime_config.max_concurrent_sessions = max_sessions;

    let tenant_context = TenantContext::for_single_user(
        TenantId::from_string("test-tenant"),
        UserId::from_string("test-user"),
    );

    // Leak the tempdir so the directory survives the test duration.
    std::mem::forget(db_dir);

    Arc::new(
        AgentRuntime::new(catalog, runtime_config, Some(tenant_context))
            .await
            .expect("AgentRuntime::new should succeed"),
    )
}

// ---------------------------------------------------------------------------
// 1. Start multiple sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_start_multiple_sessions() {
    let runtime = create_test_runtime_with_limit(None).await;

    let s1 = SessionId::from_string("session-1");
    let s2 = SessionId::from_string("session-2");
    let s3 = SessionId::from_string("session-3");
    let user = UserId::from_string("user-a");

    runtime
        .start_session(s1.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session-1");
    runtime
        .start_session(s2.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session-2");
    runtime
        .start_session(s3.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session-3");

    // Count
    assert_eq!(runtime.active_session_count(), 3);

    // All IDs present
    let active = runtime.active_sessions();
    assert!(active.contains(&s1));
    assert!(active.contains(&s2));
    assert!(active.contains(&s3));

    // Handles retrievable
    assert!(runtime.get_session_handle(&s1).is_some());
    assert!(runtime.get_session_handle(&s2).is_some());
    assert!(runtime.get_session_handle(&s3).is_some());
}

// ---------------------------------------------------------------------------
// 2. Stop session removes from registry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stop_session_removes_from_registry() {
    let runtime = create_test_runtime_with_limit(None).await;

    let s1 = SessionId::from_string("sess-a");
    let s2 = SessionId::from_string("sess-b");
    let user = UserId::from_string("user-b");

    runtime
        .start_session(s1.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start sess-a");
    runtime
        .start_session(s2.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start sess-b");

    assert_eq!(runtime.active_session_count(), 2);

    // Stop one session
    runtime.stop_session(&s1).await;

    assert_eq!(runtime.active_session_count(), 1);
    assert!(runtime.get_session_handle(&s1).is_none(), "stopped session should not be retrievable");
    assert!(runtime.get_session_handle(&s2).is_some(), "remaining session should still exist");
}

// ---------------------------------------------------------------------------
// 3. Concurrent session limit enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_session_limit() {
    let runtime = create_test_runtime_with_limit(Some(2)).await;
    assert_eq!(runtime.max_concurrent_sessions(), 2);

    let user = UserId::from_string("user-c");

    runtime
        .start_session(
            SessionId::from_string("lim-1"),
            user.clone(),
            SandboxId::new(),
            vec![],
            None,
        )
        .await
        .expect("first session within limit");

    runtime
        .start_session(
            SessionId::from_string("lim-2"),
            user.clone(),
            SandboxId::new(),
            vec![],
            None,
        )
        .await
        .expect("second session within limit");

    // Third session should be rejected
    let result = runtime
        .start_session(
            SessionId::from_string("lim-3"),
            user.clone(),
            SandboxId::new(),
            vec![],
            None,
        )
        .await;

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("third session should exceed limit"),
    };
    let err_msg = format!("{err}");
    assert!(
        err_msg.contains("Maximum concurrent sessions reached"),
        "error should mention session limit, got: {err_msg}",
    );
}

// ---------------------------------------------------------------------------
// 4. Primary session compatibility
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_primary_session_compatibility() {
    let runtime = create_test_runtime_with_limit(None).await;

    let sid = SessionId::from_string("primary-sess");
    let user = UserId::from_string("user-d");

    let _handle = runtime
        .start_primary(sid.clone(), user, SandboxId::new(), vec![], None)
        .await;

    // primary_session_id should return the session we just started
    let primary_id = runtime.primary_session_id().await;
    assert_eq!(primary_id, Some(sid.clone()));

    // primary() should return a handle
    assert!(runtime.primary().await.is_some(), "primary() should return Some after start_primary");

    // The primary session must also appear in the multi-session registry
    let active = runtime.active_sessions();
    assert!(
        active.contains(&sid),
        "primary session should be listed in active_sessions()",
    );
}

// ---------------------------------------------------------------------------
// 5. Duplicate session ID returns existing handle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_duplicate_session_id_returns_existing() {
    let runtime = create_test_runtime_with_limit(None).await;

    let sid = SessionId::from_string("dup-session");
    let user = UserId::from_string("user-e");

    let handle1 = runtime
        .start_session(sid.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("first start");

    let handle2 = runtime
        .start_session(sid.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("second start with same ID should succeed");

    // Should still be a single session
    assert_eq!(runtime.active_session_count(), 1);

    // Both handles should point to the same session
    assert_eq!(handle1.session_id, handle2.session_id);
}
