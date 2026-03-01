use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::Json;
use octo_engine::auth::UserContext;
use octo_types::ToolExecution;

use crate::state::AppState;
use super::PaginationParams;
use super::user_context::get_user_id_from_context;

/// List executions for a user.
pub async fn list_user_executions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
    Extension(ctx): Extension<UserContext>,
) -> Json<Vec<ToolExecution>> {
    let user_id = get_user_id_from_context(Some(&ctx));
    let (limit, offset) = params.clamped();
    match &state.recorder {
        Some(recorder) => {
            let execs = recorder
                .list_by_user(&user_id, limit, offset)
                .await
                .unwrap_or_default();
            Json(execs)
        }
        None => Json(vec![]),
    }
}

pub async fn list_session_executions(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Json<Vec<ToolExecution>> {
    let (limit, offset) = params.clamped();
    match &state.recorder {
        Some(recorder) => {
            let execs = recorder
                .list_by_session(&session_id, limit, offset)
                .await
                .unwrap_or_default();
            Json(execs)
        }
        None => Json(vec![]),
    }
}

pub async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match &state.recorder {
        Some(recorder) => {
            let exec = recorder.get(&id).await.ok().flatten();
            Json(serde_json::to_value(exec).unwrap_or_default())
        }
        None => Json(serde_json::json!(null)),
    }
}
