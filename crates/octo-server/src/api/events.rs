use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tokio_stream::Stream;

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

/// Query parameters for the SSE event stream endpoint.
#[derive(Debug, Deserialize)]
pub struct EventStreamParams {
    /// Optional session ID filter. When omitted, all events are streamed.
    pub session_id: Option<String>,
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

/// GET /api/v1/events/stream?session_id=xxx
///
/// Server-Sent Events endpoint that streams live telemetry events from the
/// TelemetryBus. When `session_id` is provided, only events matching that
/// session are forwarded. When omitted, all events are streamed.
///
/// SSE format:
/// ```text
/// event: telemetry
/// data: {"type":"ToolCallCompleted","session_id":"abc","tool_name":"bash","duration_ms":150}
/// ```
pub async fn event_stream(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventStreamParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Json<serde_json::Value>> {
    let Some(bus) = state.agent_supervisor.event_bus() else {
        return Err(Json(
            serde_json::json!({ "error": "TelemetryBus not available" }),
        ));
    };

    let mut rx = bus.subscribe();
    let session_filter = params.session_id;

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Apply session_id filter if provided
                    if let Some(ref filter) = session_filter {
                        if event.session_id() != filter {
                            continue;
                        }
                    }
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    yield Ok(Event::default().event("telemetry").data(data));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // Missed events due to slow consumer — skip and continue
                    tracing::warn!(lagged = n, "SSE consumer lagged, skipped events");
                    let msg = serde_json::json!({ "warning": format!("lagged, skipped {} events", n) });
                    yield Ok(Event::default().event("error").data(msg.to_string()));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // Channel closed — end stream
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive"),
    ))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/events", get(list_events))
        .route("/events/stream", get(event_stream))
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
