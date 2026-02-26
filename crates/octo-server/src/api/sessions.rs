use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;

use octo_types::SessionId;

use crate::state::AppState;
use super::PaginationParams;

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Json<serde_json::Value> {
    let limit = params.limit.min(100);
    let summaries = state.sessions.list_sessions(limit, params.offset).await;
    Json(serde_json::to_value(summaries).unwrap_or_default())
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let session_id = SessionId::from_string(&id);
    let messages = state.sessions.get_messages(&session_id).await;
    Json(serde_json::json!({
        "id": id,
        "messages": messages.unwrap_or_default(),
    }))
}
