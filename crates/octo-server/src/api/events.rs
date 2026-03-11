use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub after_sequence: i64,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// GET /api/events?after_sequence=N&limit=100
pub async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventsQuery>,
) -> Json<serde_json::Value> {
    let Some(store) = state.agent_supervisor.event_store() else {
        return Json(serde_json::json!({ "error": "EventStore not available" }));
    };

    match store
        .read_after(params.after_sequence, params.limit.clamp(1, 1000))
        .await
    {
        Ok(events) => Json(serde_json::json!({ "events": events })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

/// GET /api/events/session/{session_id}?limit=100
pub async fn list_session_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<EventsQuery>,
) -> Json<serde_json::Value> {
    let Some(store) = state.agent_supervisor.event_store() else {
        return Json(serde_json::json!({ "error": "EventStore not available" }));
    };

    match store
        .read_by_session(&session_id, params.limit.clamp(1, 1000))
        .await
    {
        Ok(events) => Json(serde_json::json!({ "events": events })),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}

/// GET /api/events/stats
pub async fn event_stats(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let Some(store) = state.agent_supervisor.event_store() else {
        return Json(serde_json::json!({ "error": "EventStore not available" }));
    };

    let count = store.count().await.unwrap_or(0);
    let latest_sequence = store.latest_sequence().await.unwrap_or(0);

    Json(serde_json::json!({
        "count": count,
        "latest_sequence": latest_sequence,
    }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/events", get(list_events))
        .route("/events/session/{session_id}", get(list_session_events))
        .route("/events/stats", get(event_stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_events_query_defaults() {
        let q: EventsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.after_sequence, 0);
        assert_eq!(q.limit, 100);
    }

    #[test]
    fn test_events_query_custom() {
        let q: EventsQuery =
            serde_json::from_str(r#"{"after_sequence": 42, "limit": 10}"#).unwrap();
        assert_eq!(q.after_sequence, 42);
        assert_eq!(q.limit, 10);
    }
}
