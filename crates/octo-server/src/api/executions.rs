use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::Json;
use octo_engine::auth::UserContext;
use octo_types::{SessionId, ToolExecution, UserId};
use tracing::{debug, error};

use super::user_context::get_user_id_from_context;
use super::PaginationParams;
use crate::state::AppState;

/// List executions for a user.
pub async fn list_user_executions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
    Extension(ctx): Extension<UserContext>,
) -> Json<Vec<ToolExecution>> {
    let user_id = get_user_id_from_context(Some(&ctx));
    let (limit, offset) = params.clamped();
    let recorder = state.agent_supervisor.recorder();
    match recorder.list_by_user(&user_id, limit, offset).await {
        Ok(execs) => Json(execs),
        Err(e) => {
            error!(error = %e, user_id = %user_id, "Failed to list user executions");
            Json(vec![])
        }
    }
}

/// List executions for a specific session (with user isolation).
pub async fn list_session_executions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<PaginationParams>,
    Extension(ctx): Extension<UserContext>,
) -> Json<Vec<ToolExecution>> {
    let user_id_str = get_user_id_from_context(Some(&ctx));
    let user_id = UserId::from_string(&user_id_str);
    let session_id_obj = SessionId::from_string(&session_id);

    // Verify the session belongs to the user (authorization check)
    let sessions = state.agent_supervisor.session_store();
    let session = sessions
        .get_session_for_user(&session_id_obj, &user_id)
        .await;
    if session.is_none() {
        debug!(session_id = %session_id, user_id = %user_id_str, "Session not found or access denied");
        return Json(vec![]);
    }

    let (limit, offset) = params.clamped();
    let recorder = state.agent_supervisor.recorder();
    match recorder.list_by_session(&session_id, limit, offset).await {
        Ok(execs) => Json(execs),
        Err(e) => {
            error!(error = %e, session_id = %session_id, "Failed to list session executions");
            Json(vec![])
        }
    }
}

pub async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let recorder = state.agent_supervisor.recorder();
    match recorder.get(&id).await {
        Ok(Some(exec)) => Json(serde_json::to_value(exec).unwrap_or_else(|e| {
            error!(error = %e, execution_id = %id, "Failed to serialize execution");
            serde_json::json!(null)
        })),
        Ok(None) => Json(serde_json::json!(null)),
        Err(e) => {
            error!(error = %e, execution_id = %id, "Failed to get execution");
            Json(serde_json::json!(null))
        }
    }
}
