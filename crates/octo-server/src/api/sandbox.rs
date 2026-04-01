//! Sandbox Management API — container pool observability (AO-T7)

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::state::AppState;

// ── Response types ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SandboxStatusResponse {
    pub mode: String,
    pub sandbox_available: bool,
    pub active_count: usize,
    pub active_sessions: Vec<String>,
    pub config: Option<SandboxConfigSummary>,
}

#[derive(Debug, Serialize)]
pub struct SandboxConfigSummary {
    pub image: String,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
    pub max_containers: usize,
    pub working_dir: String,
}

#[derive(Debug, Serialize)]
pub struct SandboxSessionInfo {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    pub cleaned: usize,
}

#[derive(Debug, Serialize)]
pub struct ReleaseResponse {
    pub released: bool,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ── Router ───────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/sandbox/status", get(status))
        .route("/sandbox/sessions", get(sessions))
        // cleanup BEFORE the path param so "cleanup" isn't captured as session_id
        .route("/sandbox/cleanup", post(cleanup))
        .route("/sandbox/{session_id}/release", post(release))
}

// ── Handlers ─────────────────────────────────────────────────────────

/// GET /api/v1/sandbox/status — active container count, session list, config summary
async fn status(State(state): State<Arc<AppState>>) -> Json<SandboxStatusResponse> {
    match state.agent_supervisor.session_sandbox_manager() {
        None => Json(SandboxStatusResponse {
            mode: "host".to_string(),
            sandbox_available: false,
            active_count: 0,
            active_sessions: vec![],
            config: None,
        }),
        Some(ssm) => {
            let config = ssm.config();
            Json(SandboxStatusResponse {
                mode: "sandboxed".to_string(),
                sandbox_available: true,
                active_count: ssm.active_count().await,
                active_sessions: ssm.active_sessions().await,
                config: Some(SandboxConfigSummary {
                    image: config.image.clone(),
                    idle_timeout_secs: config.idle_timeout.as_secs(),
                    max_lifetime_secs: config.max_lifetime.as_secs(),
                    max_containers: config.max_containers,
                    working_dir: config.working_dir.clone(),
                }),
            })
        }
    }
}

/// GET /api/v1/sandbox/sessions — active sandbox session detail list
async fn sessions(State(state): State<Arc<AppState>>) -> Json<Vec<SandboxSessionInfo>> {
    match state.agent_supervisor.session_sandbox_manager() {
        None => Json(vec![]),
        Some(ssm) => {
            let ids = ssm.active_sessions().await;
            Json(
                ids.into_iter()
                    .map(|session_id| SandboxSessionInfo { session_id })
                    .collect(),
            )
        }
    }
}

/// POST /api/v1/sandbox/{session_id}/release — manually release a container
async fn release(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<ReleaseResponse>, (StatusCode, Json<ErrorResponse>)> {
    let ssm = state
        .agent_supervisor
        .session_sandbox_manager()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Sandbox not available (running in host mode)".to_string(),
                }),
            )
        })?;

    ssm.release(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to release sandbox: {e}"),
            }),
        )
    })?;

    Ok(Json(ReleaseResponse {
        released: true,
        session_id,
    }))
}

/// POST /api/v1/sandbox/cleanup — trigger idle cleanup
async fn cleanup(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CleanupResponse>, (StatusCode, Json<ErrorResponse>)> {
    let ssm = state
        .agent_supervisor
        .session_sandbox_manager()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Sandbox not available (running in host mode)".to_string(),
                }),
            )
        })?;

    let cleaned = ssm.cleanup_idle().await;
    Ok(Json(CleanupResponse { cleaned }))
}
