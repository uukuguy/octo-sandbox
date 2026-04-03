//! AR-T5 + AU-G5: Webhook trigger endpoint for autonomous agent sessions.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use octo_engine::agent::{AutonomousConfig, AutonomousState};
use octo_types::SessionId;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    /// Optional existing session to trigger autonomous mode on.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Override for max autonomous rounds.
    #[serde(default)]
    pub max_rounds: Option<u32>,
    /// Override for idle sleep duration in seconds.
    #[serde(default)]
    pub idle_sleep_secs: Option<u64>,
    /// Arbitrary payload passed to the agent.
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// POST /api/v1/autonomous/trigger — Webhook endpoint that triggers autonomous mode.
///
/// Registers the session with the AutonomousScheduler and returns configuration details.
/// Full session creation + executor startup is deferred to AU-D1 (requires runtime.start_session
/// to accept AutonomousConfig).
pub async fn trigger_autonomous(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TriggerRequest>,
) -> impl IntoResponse {
    let session_id_str = body
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let session_id = SessionId::from_string(&session_id_str);

    let config = AutonomousConfig {
        enabled: true,
        max_autonomous_rounds: body.max_rounds.unwrap_or(100),
        idle_sleep_secs: body.idle_sleep_secs.unwrap_or(30),
        ..Default::default()
    };

    // Register with AutonomousScheduler
    let auto_state = AutonomousState::new(session_id.clone(), config.clone());
    state
        .agent_supervisor
        .autonomous_scheduler()
        .register(auto_state);

    // TODO(AU-D1): Actually start a session via runtime.start_session() with autonomous config.
    // For now, registration is complete. The session can be started via the standard session
    // creation API, and the autonomous config will be picked up from the scheduler.

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "session_id": session_id_str,
            "status": "registered",
            "autonomous": {
                "enabled": true,
                "max_rounds": config.max_autonomous_rounds,
                "idle_sleep_secs": config.idle_sleep_secs,
            },
        })),
    )
}
