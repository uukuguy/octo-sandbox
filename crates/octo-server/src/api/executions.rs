use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;

use octo_types::ToolExecution;

use crate::state::AppState;
use super::PaginationParams;

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
