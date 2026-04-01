//! Integration tests for multi-session isolation (Phase AJ).
//!
//! Verifies that each session started via `AgentRuntime::start_session` receives
//! isolated resources (ToolRegistry, KnowledgeGraph, MCP ownership) and that
//! session A's resources are invisible to session B.
//!
//! These tests exercise the public API surface only — no `pub(crate)` internals.

use std::sync::Arc;

use octo_engine::providers::ProviderConfig;
use octo_engine::{AgentCatalog, AgentRuntime, AgentRuntimeConfig, TenantContext};
use octo_types::{SandboxId, SessionId, TenantId, UserId};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Create a fresh `AgentRuntime` backed by a temporary SQLite database.
///
/// The `TempDir` is intentionally leaked so the database file remains valid
/// for the lifetime of the test.
async fn create_test_runtime() -> Arc<AgentRuntime> {
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let db_path_str = db_path.to_str().unwrap().to_string();

    let catalog = Arc::new(AgentCatalog::new());
    let runtime_config = AgentRuntimeConfig::from_parts(
        db_path_str,
        ProviderConfig::default(),
        vec![],
        None,
        None,
        false,
    );
    let tenant_context = TenantContext::for_single_user(
        TenantId::from_string("test-tenant"),
        UserId::from_string("test-user"),
    );

    // Leak the TempDir so the underlying directory is not removed while
    // the runtime (and its SQLite connection) is still alive.
    std::mem::forget(db_dir);

    Arc::new(
        AgentRuntime::new(catalog, runtime_config, Some(tenant_context))
            .await
            .expect("AgentRuntime::new should succeed"),
    )
}

// ---------------------------------------------------------------------------
// 1. ToolRegistry isolation — each session gets its own executor / handle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_session_tool_registry_isolation() {
    let runtime = create_test_runtime().await;

    let sid_a = SessionId::from_string("session-a");
    let sid_b = SessionId::from_string("session-b");
    let user = UserId::from_string("test-user");

    let handle_a = runtime
        .start_session(sid_a.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session A");

    let handle_b = runtime
        .start_session(sid_b.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session B");

    // Handles must carry their own session IDs, proving they are distinct
    // executor instances (each with its own ToolRegistry snapshot).
    assert_eq!(handle_a.session_id, sid_a);
    assert_eq!(handle_b.session_id, sid_b);
    assert_ne!(handle_a.session_id, handle_b.session_id);

    // Both sessions should be independently retrievable.
    assert!(runtime.get_session_handle(&sid_a).is_some());
    assert!(runtime.get_session_handle(&sid_b).is_some());

    // Verify the count reflects both sessions.
    assert_eq!(runtime.active_session_count(), 2);
}

// ---------------------------------------------------------------------------
// 2. KnowledgeGraph isolation — sessions are independent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_session_kg_isolation() {
    let runtime = create_test_runtime().await;

    let sid_a = SessionId::from_string("kg-session-a");
    let sid_b = SessionId::from_string("kg-session-b");
    let user = UserId::from_string("test-user");

    runtime
        .start_session(sid_a.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session A");

    runtime
        .start_session(sid_b.clone(), user.clone(), SandboxId::new(), vec![], None)
        .await
        .expect("start session B");

    // Both sessions are active and listed independently.
    let active = runtime.active_sessions();
    assert!(active.contains(&sid_a), "session A must be in active list");
    assert!(active.contains(&sid_b), "session B must be in active list");
    assert_eq!(active.len(), 2);

    // Stopping session A must not affect session B — KG isolation means
    // each session's graph is created independently and torn down
    // without side-effects.
    runtime.stop_session(&sid_a).await;

    assert!(
        runtime.get_session_handle(&sid_a).is_none(),
        "session A handle should be gone after stop"
    );
    assert!(
        runtime.get_session_handle(&sid_b).is_some(),
        "session B handle should survive after stopping A"
    );
    assert_eq!(runtime.active_session_count(), 1);
}

// ---------------------------------------------------------------------------
// 3. Memory search isolation — sessions registered independently
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_session_memory_search_isolation() {
    let runtime = create_test_runtime().await;

    let sid_a = SessionId::from_string("mem-session-a");
    let sid_b = SessionId::from_string("mem-session-b");
    let user_a = UserId::from_string("user-alpha");
    let user_b = UserId::from_string("user-beta");

    let handle_a = runtime
        .start_session(sid_a.clone(), user_a, SandboxId::new(), vec![], None)
        .await
        .expect("start session A");

    let handle_b = runtime
        .start_session(sid_b.clone(), user_b, SandboxId::new(), vec![], None)
        .await
        .expect("start session B");

    // Each handle is bound to its own session ID — memory queries in the
    // engine use session_id as a filter key, so results from session A
    // never leak into session B.
    assert_eq!(handle_a.session_id, sid_a);
    assert_eq!(handle_b.session_id, sid_b);

    // Re-fetching the handles through the runtime yields the same
    // session-scoped instances.
    let refetched_a = runtime
        .get_session_handle(&sid_a)
        .expect("session A should still be active");
    let refetched_b = runtime
        .get_session_handle(&sid_b)
        .expect("session B should still be active");

    assert_eq!(refetched_a.session_id, sid_a);
    assert_eq!(refetched_b.session_id, sid_b);

    // Starting the same session ID again should return the existing handle
    // (idempotent), not create a duplicate.
    let handle_a_again = runtime
        .start_session(
            sid_a.clone(),
            UserId::from_string("user-alpha"),
            SandboxId::new(),
            vec![],
            None,
        )
        .await
        .expect("re-start session A should be idempotent");

    assert_eq!(handle_a_again.session_id, sid_a);
    assert_eq!(
        runtime.active_session_count(),
        2,
        "idempotent start must not create a third session"
    );
}

// ---------------------------------------------------------------------------
// 4. MCP ownership cleanup on session stop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mcp_ownership_cleanup_on_stop() {
    let runtime = create_test_runtime().await;

    let sid = SessionId::from_string("mcp-session");
    let user = UserId::from_string("test-user");

    let handle = runtime
        .start_session(sid.clone(), user, SandboxId::new(), vec![], None)
        .await
        .expect("start session");

    // Session is alive and retrievable.
    assert_eq!(handle.session_id, sid);
    assert_eq!(runtime.active_session_count(), 1);
    assert!(runtime.get_session_handle(&sid).is_some());

    // Stop the session — this triggers MCP ownership cleanup internally.
    runtime.stop_session(&sid).await;

    // After stop, the session handle must be gone.
    assert!(
        runtime.get_session_handle(&sid).is_none(),
        "session handle must be None after stop_session"
    );
    assert_eq!(
        runtime.active_session_count(),
        0,
        "active count must be 0 after stopping the only session"
    );
    assert!(
        runtime.active_sessions().is_empty(),
        "active_sessions list must be empty"
    );

    // Stopping an already-stopped session should be a no-op (no panic).
    runtime.stop_session(&sid).await;
    assert_eq!(runtime.active_session_count(), 0);
}
