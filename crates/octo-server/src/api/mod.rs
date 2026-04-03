pub mod agents;
pub mod audit;
pub mod autonomous;
pub mod budget;
pub mod collaboration;
pub mod context;
pub mod error;
pub mod eval_sessions;
pub mod events;
pub mod hooks;
pub mod config;
pub mod executions;
pub mod knowledge_graph;
pub mod mcp_logs;
pub mod mcp_servers;
pub mod mcp_tools;
pub mod memories;
pub mod metering;
pub mod metrics;
pub mod providers;
pub mod sandbox;
pub mod scheduler;
pub mod secrets;
pub mod security;
pub mod sessions;
pub mod skills;
pub mod sync;
pub mod tasks;
pub mod tools;
pub mod user_context;

use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::AppState;

/// Pagination query params.
#[derive(Debug, serde::Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

impl PaginationParams {
    pub fn clamped(&self) -> (usize, usize) {
        (self.limit.clamp(1, 200), self.offset)
    }
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // More specific routes first — multi-session lifecycle (AJ-T9)
        .route("/sessions/start", post(sessions::start_session))
        .route("/sessions/active", get(sessions::list_active_sessions))
        .route("/sessions/metrics", get(sessions::session_metrics))
        .route(
            "/sessions/{id}/stop",
            delete(sessions::stop_session),
        )
        .route(
            "/sessions/{id}/status",
            get(sessions::get_session_status),
        )
        // Existing session routes
        .route(
            "/sessions/{id}/executions",
            get(executions::list_session_executions),
        )
        .route("/sessions/{id}", get(sessions::get_session))
        // AR-T4: Session fork/rewind
        .route("/sessions/{id}/rewind", post(sessions::rewind_session))
        .route("/sessions/{id}/fork", post(sessions::fork_session))
        // AU-G4: Session-level pause/resume for autonomous mode
        .route("/sessions/{id}/pause", post(sessions::pause_session))
        .route("/sessions/{id}/resume", post(sessions::resume_session))
        // Then less specific
        .route("/sessions", get(sessions::list_sessions))
        .route("/executions", get(executions::list_user_executions))
        .route("/executions/{id}", get(executions::get_execution))
        .route("/tools", get(tools::list_tools))
        .route("/config", get(config::get_config).put(config::update_config))
        .route(
            "/memories",
            get(memories::search_memories)
                .post(memories::create_memory)
                .delete(memories::delete_memories_by_filter),
        )
        .route("/memories/working", get(memories::get_working_memory))
        .route(
            "/memories/{id}",
            get(memories::get_memory).delete(memories::delete_memory),
        )
        .route("/budget", get(budget::get_budget))
        // Metrics, Audit, and Events
        .merge(metrics::router())
        .merge(metering::router())
        .merge(audit::router())
        .merge(events::router())
        // MCP servers
        .merge(mcp_servers::routes())
        .merge(mcp_tools::routes())
        .merge(mcp_logs::routes())
        // Scheduler
        .nest("/scheduler", scheduler::create_router())
        // Background tasks
        .merge(tasks::router())
        // Provider chain
        .merge(providers::router())
        // Agent registry and lifecycle
        .merge(agents::router())
        // Skill catalog
        .merge(skills::router())
        // Collaboration dashboard (T9)
        .merge(collaboration::router())
        // Offline sync (D6)
        .merge(sync::router())
        // Eval session endpoints (Phase G2)
        .merge(eval_sessions::router())
        // Knowledge Graph (AO-T2)
        .merge(knowledge_graph::router())
        // Hooks Management (AO-T3)
        .merge(hooks::router())
        // Secret Vault (AO-T6)
        .merge(secrets::router())
        // Sandbox Management (AO-T7)
        .merge(sandbox::router())
        // Security Policy + AI Defence (AO-T4 + T5)
        .merge(security::router())
        // Context Observability (AO-T10)
        .merge(context::router())
        // AR-T5: Autonomous webhook trigger
        .route("/autonomous/trigger", post(autonomous::trigger_autonomous))
}
