//! Shared test infrastructure for octo-server E2E tests.
//!
//! Provides `TestApp` — a lightweight wrapper around the full Axum router
//! that uses `tower::ServiceExt::oneshot` for in-process HTTP testing
//! (no real port binding).

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use octo_engine::{AgentCatalog, AgentRuntime, AgentRuntimeConfig, TenantContext};
use octo_types::{TenantId, UserId};

// Re-export for test files
pub use octo_server::config::Config;
pub use octo_server::router::build_router;
pub use octo_server::state::AppState;

/// Test application wrapping the full Axum router.
///
/// On drop the temporary SQLite directory is removed automatically.
pub struct TestApp {
    router: Router,
    _db_dir: tempfile::TempDir,
}

impl TestApp {
    /// Build a fully-initialised `TestApp` backed by a temporary SQLite database.
    pub async fn new() -> Self {
        let db_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = db_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap().to_string();

        // Config with auth disabled (mode = None)
        let mut config = Config::default();
        config.auth.mode = Some(octo_engine::auth::AuthMode::None);

        // Agent catalog (empty, in-memory only — no AgentStore persistence needed)
        let catalog = Arc::new(AgentCatalog::new());

        // Runtime config with dummy provider (no real LLM calls)
        let runtime_config = AgentRuntimeConfig::from_parts(
            db_path_str.clone(),
            config.provider.clone(),
            vec![],  // no skills dirs
            None,    // no provider chain
            None,    // no working dir
            false,   // no event bus
        );

        // Tenant context for single-user workbench scenario
        let tenant_context = TenantContext::for_single_user(
            TenantId::from_string("test-tenant"),
            UserId::from_string("test-user"),
        );

        let agent_runtime = Arc::new(
            AgentRuntime::new(catalog.clone(), runtime_config, Some(tenant_context))
                .await
                .expect("failed to create AgentRuntime"),
        );

        // Start primary executor to get agent_handle
        let session_store = agent_runtime.session_store();
        let session = session_store.create_session().await;
        let history = session_store
            .get_messages(&session.session_id)
            .await
            .unwrap_or_default();

        let agent_handle = agent_runtime
            .start_primary(
                session.session_id.clone(),
                session.user_id.clone(),
                session.sandbox_id.clone(),
                history,
                None,
            )
            .await;

        let state = Arc::new(AppState::new(
            PathBuf::from(&db_path_str),
            None, // no scheduler for basic tests
            config,
            agent_runtime,
            agent_handle,
        ));

        let router = build_router(state);

        Self {
            router,
            _db_dir: db_dir,
        }
    }

    /// Build a `TestApp` with scheduler enabled.
    pub async fn with_scheduler() -> Self {
        use octo_engine::db::Database;
        use octo_engine::scheduler::{Scheduler, SchedulerConfig, SqliteSchedulerStorage};

        let db_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = db_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap().to_string();

        let mut config = Config::default();
        config.auth.mode = Some(octo_engine::auth::AuthMode::None);
        config.scheduler.enabled = true;

        let catalog = Arc::new(AgentCatalog::new());

        let runtime_config = AgentRuntimeConfig::from_parts(
            db_path_str.clone(),
            config.provider.clone(),
            vec![],
            None,
            None,
            false,
        );

        let tenant_context = TenantContext::for_single_user(
            TenantId::from_string("test-tenant"),
            UserId::from_string("test-user"),
        );

        let agent_runtime = Arc::new(
            AgentRuntime::new(catalog.clone(), runtime_config, Some(tenant_context))
                .await
                .expect("failed to create AgentRuntime"),
        );

        let session_store = agent_runtime.session_store();
        let session = session_store.create_session().await;
        let history = session_store
            .get_messages(&session.session_id)
            .await
            .unwrap_or_default();

        let agent_handle = agent_runtime
            .start_primary(
                session.session_id.clone(),
                session.user_id.clone(),
                session.sandbox_id.clone(),
                history,
                None,
            )
            .await;

        // Create scheduler with its own DB connection
        let db = Database::open(&db_path_str)
            .await
            .expect("failed to open scheduler DB");
        let conn = db.conn().clone();
        let storage = SqliteSchedulerStorage::new(conn);
        let scheduler = Arc::new(Scheduler::new(
            config.scheduler.clone(),
            Arc::new(storage),
            agent_runtime.provider().clone(),
            agent_runtime.tools().clone(),
            agent_runtime.memory().clone(),
            agent_runtime.session_store().clone(),
            Some(
                agent_runtime.security_policy().clone()
                    as std::sync::Arc<dyn octo_types::PathValidator>,
            ),
        ));

        let state = Arc::new(AppState::new(
            PathBuf::from(&db_path_str),
            Some(scheduler),
            config,
            agent_runtime,
            agent_handle,
        ));

        let router = build_router(state);

        Self {
            router,
            _db_dir: db_dir,
        }
    }

    // ── HTTP helpers ───────────────────────────────────────────────────

    /// Send a GET request and return (status, json_body).
    pub async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");
        self.send(req).await
    }

    /// Send a POST request with JSON body.
    pub async fn post_json(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("failed to build request");
        self.send(req).await
    }

    /// Send a PUT request with JSON body.
    pub async fn put_json(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .expect("failed to build request");
        self.send(req).await
    }

    /// Send a DELETE request.
    pub async fn delete(&self, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");
        self.send(req).await
    }

    /// Send a GET request and return (status, json_body, headers).
    pub async fn get_with_headers(
        &self,
        uri: &str,
    ) -> (StatusCode, Value, axum::http::HeaderMap) {
        let req = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");
        self.send_with_headers(req).await
    }

    /// Low-level: send request and return headers too.
    async fn send_with_headers(
        &self,
        req: Request<Body>,
    ) -> (StatusCode, Value, axum::http::HeaderMap) {
        let response = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed");
        let status = response.status();
        let headers = response.headers().clone();
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .expect("failed to read body")
            .to_bytes();
        let json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
        (status, json, headers)
    }

    /// Low-level: send any `Request<Body>` through the router.
    async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed");
        let status = response.status();
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .expect("failed to read body")
            .to_bytes();
        let json: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
        (status, json)
    }
}
