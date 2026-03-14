//! Eval-specific session endpoints for `EvalTarget::Server`.
//!
//! These endpoints create ephemeral sessions, send a single message and wait for
//! the agent loop to complete (synchronous blocking), then allow cleanup.
//!
//! Routes:
//!   POST   /api/eval/sessions               — create eval session
//!   POST   /api/eval/sessions/{id}/messages  — send message, block until done
//!   DELETE /api/eval/sessions/{id}           — delete session

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio::sync::broadcast;
use tracing::{info, warn};

use octo_engine::{AgentEvent, AgentMessage};
use octo_types::{SessionId, UserId};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateEvalSessionRequest {
    /// Optional agent ID (reserved for future multi-agent eval support)
    #[serde(default)]
    #[allow(dead_code)]
    pub agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

/// POST /api/eval/sessions — create a new eval session
pub async fn create_eval_session(
    State(state): State<Arc<AppState>>,
    Json(_body): Json<CreateEvalSessionRequest>,
) -> impl IntoResponse {
    let sessions = state.agent_supervisor.session_store();
    let user_id = UserId::from_string("eval");
    let session_data = sessions.create_session_with_user(&user_id).await;
    let session_id = session_data.session_id.as_str().to_string();

    info!(session_id = %session_id, "Eval session created");

    (
        StatusCode::OK,
        Json(serde_json::json!({ "session_id": session_id })),
    )
}

/// POST /api/eval/sessions/{id}/messages — send message and wait for completion
///
/// This is a synchronous blocking endpoint: it sends the user message to the
/// agent executor and collects all events until `Done`/`Completed`, then returns
/// the aggregated response as JSON.
pub async fn send_eval_message(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let handle = &state.agent_handle;

    // Subscribe to events BEFORE sending the message
    let mut rx = handle.subscribe();

    // Send user message to the agent executor
    if let Err(e) = handle
        .send(AgentMessage::UserMessage {
            content: body.content.clone(),
            channel_id: format!("eval-{}", session_id),
        })
        .await
    {
        warn!(error = %e, "Failed to send message to agent executor");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to send message: {}", e) })),
        );
    }

    // Collect events until Done/Completed
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<serde_json::Value> = Vec::new();
    let mut rounds: u32 = 0;
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut stop_reason = String::from("unknown");
    let mut duration_ms: u64 = 0;

    let start = std::time::Instant::now();

    loop {
        match rx.recv().await {
            Ok(event) => match event {
                AgentEvent::TextComplete { text } => {
                    text_parts.push(text);
                }
                AgentEvent::ToolStart {
                    tool_id,
                    tool_name,
                    input,
                } => {
                    tool_calls.push(serde_json::json!({
                        "name": tool_name,
                        "tool_id": tool_id,
                        "args": input,
                        "result": "",
                        "success": true,
                    }));
                }
                AgentEvent::ToolResult {
                    tool_id,
                    output,
                    success,
                } => {
                    // Update the matching tool call with its result
                    for tc in tool_calls.iter_mut().rev() {
                        if tc["tool_id"].as_str() == Some(&tool_id) {
                            tc["result"] = serde_json::Value::String(output.clone());
                            tc["success"] = serde_json::Value::Bool(success);
                            break;
                        }
                    }
                }
                AgentEvent::Completed(result) => {
                    rounds = result.rounds;
                    input_tokens = result.input_tokens;
                    output_tokens = result.output_tokens;
                    stop_reason = format!("{:?}", result.stop_reason);
                    duration_ms = start.elapsed().as_millis() as u64;
                    break;
                }
                AgentEvent::Done => {
                    duration_ms = start.elapsed().as_millis() as u64;
                    break;
                }
                AgentEvent::Error { message } => {
                    warn!(error = %message, "Agent error during eval");
                }
                _ => {} // skip other events
            },
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(lagged = n, "Eval broadcast lagged");
            }
        }
    }

    let final_text = text_parts.join("");

    // Remove internal tool_id from response
    let clean_tool_calls: Vec<serde_json::Value> = tool_calls
        .into_iter()
        .map(|mut tc| {
            if let Some(obj) = tc.as_object_mut() {
                obj.remove("tool_id");
            }
            tc
        })
        .collect();

    info!(
        session_id = %session_id,
        rounds,
        input_tokens,
        output_tokens,
        duration_ms,
        "Eval message completed"
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "text": final_text,
            "tool_calls": clean_tool_calls,
            "rounds": rounds,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "stop_reason": stop_reason,
            "duration_ms": duration_ms,
        })),
    )
}

/// DELETE /api/eval/sessions/{id} — delete eval session
pub async fn delete_eval_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let sessions = state.agent_supervisor.session_store();
    let sid = SessionId::from_string(&session_id);
    let deleted = sessions.delete_session(&sid).await;

    info!(session_id = %session_id, deleted, "Eval session deleted");

    (
        StatusCode::OK,
        Json(serde_json::json!({ "deleted": deleted })),
    )
}

/// Build the eval sessions router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/eval/sessions", post(create_eval_session))
        .route(
            "/eval/sessions/{id}/messages",
            post(send_eval_message),
        )
        .route(
            "/eval/sessions/{id}",
            delete(delete_eval_session),
        )
}
