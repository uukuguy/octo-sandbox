use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Request, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use octo_types::SessionId;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_engine::{AgentEvent, AgentExecutorHandle, AgentMessage};

use crate::state::AppState;

// --- Client -> Server messages ---

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "send_message")]
    SendMessage { content: String },
    #[serde(rename = "cancel")]
    Cancel,
    #[serde(rename = "approval_response")]
    ApprovalResponse {
        tool_id: String,
        approved: bool,
    },
}

// --- Server -> Client messages ---

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum ServerMessage {
    #[serde(rename = "session_created")]
    SessionCreated { session_id: String },

    #[serde(rename = "text_delta")]
    TextDelta { session_id: String, text: String },

    #[serde(rename = "text_complete")]
    TextComplete { session_id: String, text: String },

    #[serde(rename = "thinking_delta")]
    ThinkingDelta { session_id: String, text: String },

    #[serde(rename = "thinking_complete")]
    ThinkingComplete { session_id: String, text: String },

    #[serde(rename = "tool_start")]
    ToolStart {
        session_id: String,
        tool_id: String,
        tool_name: String,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        session_id: String,
        tool_id: String,
        output: String,
        success: bool,
    },

    #[serde(rename = "tool_execution")]
    ToolExecutionEvent {
        session_id: String,
        execution: octo_types::ToolExecution,
    },

    #[serde(rename = "token_budget_update")]
    TokenBudgetUpdate {
        session_id: String,
        budget: octo_types::TokenBudgetSnapshot,
    },

    #[serde(rename = "typing")]
    Typing { session_id: String, state: bool },

    #[serde(rename = "error")]
    Error { session_id: String, message: String },

    #[serde(rename = "done")]
    Done { session_id: String },

    #[serde(rename = "context_degraded")]
    ContextDegraded {
        session_id: String,
        level: String,
        usage_pct: f32,
    },

    #[serde(rename = "memory_flushed")]
    MemoryFlushed {
        session_id: String,
        facts_count: usize,
    },

    #[serde(rename = "approval_required")]
    ApprovalRequired {
        session_id: String,
        tool_name: String,
        tool_id: String,
        risk_level: octo_types::RiskLevel,
    },

    #[serde(rename = "security_blocked")]
    SecurityBlocked { session_id: String, reason: String },

    #[serde(rename = "retrying_malformed_tool_call")]
    RetryingMalformedToolCall {
        session_id: String,
        attempt: u32,
        max_attempts: u32,
        reason: String,
    },

    /// Session lifecycle update (created, message_added, context_updated, closed).
    /// The actual WebSocket subscription wiring is deferred to a later phase.
    #[serde(rename = "session_update")]
    SessionUpdate {
        event_type: String,
        session_id: String,
    },
}

/// Extract a named parameter from a query string.
fn extract_query_param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if k == name { Some(v.to_string()) } else { None }
    })
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    req: Request,
) -> impl IntoResponse {
    // Auth check: if auth is enabled, verify user context exists.
    // Browser WebSocket API cannot send custom HTTP headers, so we also
    // accept the token as a query parameter: /ws?token=xxx
    if state.auth_config.mode != octo_engine::auth::AuthMode::None
        && req
            .extensions()
            .get::<octo_engine::auth::UserContext>()
            .is_none()
    {
        // Try token from query string as fallback for browser WS connections
        let query_token = req
            .uri()
            .query()
            .and_then(|q| extract_query_param(q, "token"));

        match query_token {
            Some(ref token) if state.auth_config.validate_key(token) => {
                debug!("WebSocket authenticated via query token");
                // Token is valid — proceed (no UserContext extension needed
                // since ws_handler manages its own auth gate)
            }
            Some(_) => {
                warn!("WebSocket query token validation failed");
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::UNAUTHORIZED)
                    .body(axum::body::Body::from(
                        "WebSocket authentication failed: invalid token",
                    ))
                    .unwrap_or_else(|_| {
                        axum::response::Response::new(axum::body::Body::from("Unauthorized"))
                    })
                    .into_response();
            }
            None => {
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::UNAUTHORIZED)
                    .body(axum::body::Body::from("WebSocket authentication required"))
                    .unwrap_or_else(|_| {
                        axum::response::Response::new(axum::body::Body::from("Unauthorized"))
                    })
                    .into_response();
            }
        }
    }

    // Extract session_id from query string: /ws?session_id=xxx
    let requested_sid = req
        .uri()
        .query()
        .and_then(|q| extract_query_param(q, "session_id"));

    // Resolve the AgentExecutorHandle based on session_id query param.
    // If session_id is provided, look it up in the session registry;
    // if not found (or not provided), fall back to the primary session handle.
    let handle = if let Some(ref sid) = requested_sid {
        let session_id = SessionId::from_string(sid);
        match state.agent_supervisor.get_session_handle(&session_id) {
            Some(h) => {
                info!(session_id = %sid, "WebSocket routed to existing session");
                h
            }
            None => {
                debug!(session_id = %sid, "Session not found, using primary session");
                state.agent_handle.clone()
            }
        }
    } else {
        state.agent_handle.clone()
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, handle))
        .into_response()
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, handle: AgentExecutorHandle) {
    let (mut sender, mut receiver) = socket.split();

    let sid_str = handle.session_id.as_str().to_string();
    info!(session_id = %sid_str, "WebSocket connected");

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => {
                info!(session_id = %sid_str, "WebSocket closed by client");
                break;
            }
            Err(e) => {
                warn!(session_id = %sid_str, "WebSocket error: {e}");
                break;
            }
            _ => continue,
        };

        // Touch session to reset idle timeout (AJ-D4)
        state.agent_supervisor.touch_session(&handle.session_id);

        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                let err = ServerMessage::Error {
                    session_id: sid_str.clone(),
                    message: format!("Invalid message: {e}"),
                };
                if let Ok(text) = serde_json::to_string(&err) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }
                continue;
            }
        };

        match client_msg {
            ClientMessage::SendMessage { content } => {
                // Tell client the session_id (for frontend UI display)
                let created_msg = ServerMessage::SessionCreated {
                    session_id: sid_str.clone(),
                };
                if let Ok(text) = serde_json::to_string(&created_msg) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }

                // Subscribe first, then send message (avoid missing events)
                let mut rx = handle.subscribe();

                // Forward user message to AgentExecutor
                let _ = handle
                    .send(AgentMessage::UserMessage {
                        content: content.clone(),
                        channel_id: "websocket".to_string(),
                    })
                    .await;

                // Forward agent events to WebSocket
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let server_msg = match event {
                                AgentEvent::TextDelta { text } => ServerMessage::TextDelta {
                                    session_id: sid_str.clone(),
                                    text,
                                },
                                AgentEvent::TextComplete { text } => ServerMessage::TextComplete {
                                    session_id: sid_str.clone(),
                                    text,
                                },
                                AgentEvent::ThinkingDelta { text } => {
                                    ServerMessage::ThinkingDelta {
                                        session_id: sid_str.clone(),
                                        text,
                                    }
                                }
                                AgentEvent::ThinkingComplete { text } => {
                                    ServerMessage::ThinkingComplete {
                                        session_id: sid_str.clone(),
                                        text,
                                    }
                                }
                                AgentEvent::ToolStart {
                                    tool_id,
                                    tool_name,
                                    input,
                                } => ServerMessage::ToolStart {
                                    session_id: sid_str.clone(),
                                    tool_id,
                                    tool_name,
                                    input,
                                },
                                AgentEvent::ToolResult {
                                    tool_id,
                                    output,
                                    success,
                                } => ServerMessage::ToolResult {
                                    session_id: sid_str.clone(),
                                    tool_id,
                                    output,
                                    success,
                                },
                                AgentEvent::ToolExecution { execution } => {
                                    ServerMessage::ToolExecutionEvent {
                                        session_id: sid_str.clone(),
                                        execution,
                                    }
                                }
                                AgentEvent::TokenBudgetUpdate { budget } => {
                                    ServerMessage::TokenBudgetUpdate {
                                        session_id: sid_str.clone(),
                                        budget,
                                    }
                                }
                                AgentEvent::Typing { state } => ServerMessage::Typing {
                                    session_id: sid_str.clone(),
                                    state,
                                },
                                AgentEvent::Error { message } => ServerMessage::Error {
                                    session_id: sid_str.clone(),
                                    message,
                                },
                                AgentEvent::Done | AgentEvent::Completed(_) => {
                                    let done_msg = ServerMessage::Done {
                                        session_id: sid_str.clone(),
                                    };
                                    if let Ok(text) = serde_json::to_string(&done_msg) {
                                        let _ = sender.send(Message::Text(text.into())).await;
                                    }
                                    break;
                                }
                                AgentEvent::ContextDegraded { level, usage_pct } => {
                                    ServerMessage::ContextDegraded {
                                        session_id: sid_str.clone(),
                                        level,
                                        usage_pct,
                                    }
                                }
                                AgentEvent::MemoryFlushed { facts_count } => {
                                    ServerMessage::MemoryFlushed {
                                        session_id: sid_str.clone(),
                                        facts_count,
                                    }
                                }
                                AgentEvent::ApprovalRequired {
                                    tool_name,
                                    tool_id,
                                    risk_level,
                                } => ServerMessage::ApprovalRequired {
                                    session_id: sid_str.clone(),
                                    tool_name,
                                    tool_id,
                                    risk_level,
                                },
                                AgentEvent::SecurityBlocked { reason } => {
                                    ServerMessage::SecurityBlocked {
                                        session_id: sid_str.clone(),
                                        reason,
                                    }
                                }
                                AgentEvent::RetryingMalformedToolCall {
                                    attempt,
                                    max_attempts,
                                    reason,
                                } => ServerMessage::RetryingMalformedToolCall {
                                    session_id: sid_str.clone(),
                                    attempt,
                                    max_attempts,
                                    reason,
                                },
                                // IterationStart/IterationEnd are internal -- skip
                                _ => continue,
                            };

                            if let Ok(json) = serde_json::to_string(&server_msg) {
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            debug!("Broadcast lagged by {n} messages");
                        }
                    }
                }
            }
            ClientMessage::Cancel => {
                let _ = handle.send(AgentMessage::Cancel).await;
                info!(session_id = %sid_str, "Agent cancellation requested");
            }
            ClientMessage::ApprovalResponse { tool_id, approved } => {
                if let Some(ref gate) = state.approval_gate {
                    let found = gate.respond(&tool_id, approved).await;
                    if found {
                        info!(tool_id = %tool_id, approved, "Approval response forwarded");
                    } else {
                        warn!(tool_id = %tool_id, "No pending approval for tool_id");
                    }
                } else {
                    warn!("ApprovalResponse received but no ApprovalGate configured");
                }
            }
        }
    }

    info!(session_id = %sid_str, "WebSocket disconnected");
}
