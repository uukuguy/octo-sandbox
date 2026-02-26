use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Serialize;

use octo_types::SessionId;

use crate::state::AppState;
use super::PaginationParams;

#[derive(Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: i64,
    pub message_count: usize,
}

pub async fn list_sessions(
    State(_state): State<Arc<AppState>>,
    Query(_params): Query<PaginationParams>,
) -> Json<Vec<SessionSummary>> {
    // SessionStore trait does not expose a list_sessions method.
    // Return empty list until the trait is extended.
    Json(vec![])
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
