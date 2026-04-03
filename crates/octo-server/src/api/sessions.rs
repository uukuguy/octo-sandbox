use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use octo_engine::agent::{AgentId, AgentMessage, SessionMetrics};
use octo_engine::auth::UserContext;
use octo_types::{SandboxId, SessionId, UserId};
use serde::{Deserialize, Serialize};

use super::user_context::get_user_id_from_context;
use super::PaginationParams;
use crate::state::AppState;

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
    Extension(ctx): Extension<UserContext>,
) -> Json<serde_json::Value> {
    let sessions = state.agent_supervisor.session_store();
    let user_id_str = get_user_id_from_context(Some(&ctx));
    let user_id = UserId::from_string(&user_id_str);
    let limit = params.limit.min(100);
    let summaries = sessions
        .list_sessions_for_user(&user_id, limit, params.offset)
        .await;
    Json(serde_json::to_value(summaries).unwrap_or_default())
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Extension(ctx): Extension<UserContext>,
) -> Json<serde_json::Value> {
    let sessions = state.agent_supervisor.session_store();
    let user_id_str = get_user_id_from_context(Some(&ctx));
    let user_id = UserId::from_string(&user_id_str);
    let session_id = SessionId::from_string(&id);

    // Use get_session_for_user to ensure user can only access their own sessions
    let session_data = sessions.get_session_for_user(&session_id, &user_id).await;
    if session_data.is_none() {
        return Json(serde_json::json!({
            "error": "Session not found or access denied"
        }));
    }

    let messages = sessions.get_messages(&session_id).await;
    Json(serde_json::json!({
        "id": id,
        "messages": messages.unwrap_or_default(),
    }))
}

// ── Phase AJ-T9: Multi-session lifecycle REST endpoints ─────────────

#[derive(Deserialize)]
pub struct StartSessionRequest {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Serialize)]
pub struct StartSessionResponse {
    pub session_id: String,
    pub status: String,
}

/// POST /api/sessions/start — Create and start a new agent session
pub async fn start_session(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<UserContext>,
    Json(req): Json<StartSessionRequest>,
) -> impl IntoResponse {
    let session_id_str = req
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let session_id = SessionId::from_string(&session_id_str);
    let user_id_str = get_user_id_from_context(Some(&ctx));
    let user_id = UserId::from_string(&user_id_str);
    let sandbox_id = SandboxId::new();
    let agent_id = req.agent_id.map(|id| AgentId(id));

    match state
        .agent_supervisor
        .start_session(
            session_id,
            user_id,
            sandbox_id,
            vec![],
            agent_id.as_ref(),
        )
        .await
    {
        Ok(_handle) => {
            let body = StartSessionResponse {
                session_id: session_id_str,
                status: "active".to_string(),
            };
            (StatusCode::CREATED, Json(serde_json::to_value(body).unwrap())).into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("Maximum concurrent sessions reached") {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": msg})),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": msg})),
                )
                    .into_response()
            }
        }
    }
}

/// DELETE /api/sessions/{id}/stop — Stop a session
pub async fn stop_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session_id = SessionId::from_string(&id);
    state.agent_supervisor.stop_session(&session_id).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "stopped"})),
    )
}

/// GET /api/sessions/active — List active (running) sessions
pub async fn list_active_sessions(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let sessions = state.agent_supervisor.active_sessions();
    let count = state.agent_supervisor.active_session_count();
    let max = state.agent_supervisor.max_concurrent_sessions();
    let session_ids: Vec<&str> = sessions.iter().map(|s| s.as_str()).collect();
    Json(serde_json::json!({
        "sessions": session_ids,
        "count": count,
        "max": max,
    }))
}

/// GET /api/sessions/{id}/status — Get session runtime status
pub async fn get_session_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session_id = SessionId::from_string(&id);
    let active = state
        .agent_supervisor
        .get_session_handle(&session_id)
        .is_some();
    Json(serde_json::json!({
        "session_id": id,
        "active": active,
    }))
}

/// GET /api/v1/sessions/metrics — Session monitoring metrics (AM-T3)
pub async fn session_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<SessionMetrics> {
    let metrics = state.agent_supervisor.session_metrics().await;
    Json(metrics)
}

// ---------------------------------------------------------------------------
// AR-T4: Session rewind / fork
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RewindRequest {
    pub to_turn: usize,
}

/// POST /api/v1/sessions/{id}/rewind — Rewind conversation to a specific turn.
pub async fn rewind_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RewindRequest>,
) -> impl IntoResponse {
    use octo_engine::agent::AgentMessage;

    let session_id = SessionId::from_string(&id);
    if let Some(handle) = state.agent_supervisor.get_session_handle(&session_id) {
        if handle.send(AgentMessage::Rewind { to_turn: body.to_turn }).await.is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send rewind message" })),
            );
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": id,
                "rewound_to_turn": body.to_turn,
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Session not found or not active" })),
        )
    }
}

#[derive(Deserialize)]
pub struct ForkRequest {
    pub at_turn: usize,
}

// ---------------------------------------------------------------------------
// AU-G4: Session-level pause/resume for autonomous mode
// ---------------------------------------------------------------------------

/// POST /api/v1/sessions/{id}/pause — Pause an autonomous session.
pub async fn pause_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session_id = SessionId::from_string(&id);
    if let Some(handle) = state.agent_supervisor.get_session_handle(&session_id) {
        if handle.send(AgentMessage::Pause).await.is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send pause message" })),
            );
        }
        // Update scheduler state
        state
            .agent_supervisor
            .autonomous_scheduler()
            .update_status(
                &session_id,
                octo_engine::agent::AutonomousStatus::Paused,
            );
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": id,
                "status": "paused",
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Session not found or not active" })),
        )
    }
}

/// POST /api/v1/sessions/{id}/resume — Resume a paused autonomous session.
pub async fn resume_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let session_id = SessionId::from_string(&id);
    if let Some(handle) = state.agent_supervisor.get_session_handle(&session_id) {
        if handle.send(AgentMessage::Resume).await.is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send resume message" })),
            );
        }
        // Update scheduler state
        state
            .agent_supervisor
            .autonomous_scheduler()
            .update_status(
                &session_id,
                octo_engine::agent::AutonomousStatus::Running,
            );
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": id,
                "status": "resumed",
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Session not found or not active" })),
        )
    }
}

/// POST /api/v1/sessions/{id}/fork — Fork conversation at a turn into a new session.
pub async fn fork_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ForkRequest>,
) -> impl IntoResponse {
    use octo_engine::agent::AgentMessage;

    let session_id = SessionId::from_string(&id);
    let new_session_id = SessionId::from_string(&uuid::Uuid::new_v4().to_string());

    if let Some(handle) = state.agent_supervisor.get_session_handle(&session_id) {
        if handle
            .send(AgentMessage::Fork {
                at_turn: body.at_turn,
                new_session_id: new_session_id.clone(),
            })
            .await
            .is_err()
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send fork message" })),
            );
        }
        (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "source_session_id": id,
                "new_session_id": new_session_id.as_str(),
                "forked_at_turn": body.at_turn,
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Session not found or not active" })),
        )
    }
}
