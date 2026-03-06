use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::Json;
use octo_engine::auth::UserContext;
use octo_types::{SessionId, UserId};

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
