//! WebSocket handler for real-time agent communication (T6)

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use octo_engine::{AgentEvent, AgentMessage};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::{
    agent_pool::{AgentInstance, AgentPool, InstanceId},
    AppState, AuthExtractor, ErrorResponse,
};

/// Maximum message size limit (1MB)
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

// --- Client -> Server messages ---

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "chat")]
    Chat { content: String },
    #[serde(rename = "cancel")]
    Cancel,
    #[serde(rename = "approval_response")]
    ApprovalResponse {
        tool_id: String,
        approved: bool,
    },
    #[serde(rename = "ping")]
    Ping,
}

// --- Server -> Client messages ---

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
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
    Error { message: String },

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

    #[serde(rename = "pong")]
    Pong,
}

/// WebSocket handler
pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Validate session ownership before allowing WebSocket connection
    let user_runtime = match state.get_or_create_user_runtime(&auth.user_id) {
        Ok(runtime) => runtime,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to access user runtime".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Check if session exists and belongs to the user
    let session = user_runtime.get_session(&auth.user_id, &session_id);
    if session.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Session not found or access denied".to_string(),
            }),
        )
            .into_response();
    }

    // Get AgentPool from state
    let pool = state.agent_pool();

    // Get instance from pool
    let instance = match pool.get_instance(&auth.user_id).await {
        Ok(i) => i,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: format!("Agent pool exhausted: {}", e),
                }),
            )
                .into_response();
        }
    };

    let pool = Arc::clone(pool);

    ws.on_upgrade(move |socket| handle_socket(session_id, socket, instance, pool))
}

async fn handle_socket(
    session_id: String,
    socket: WebSocket,
    instance: AgentInstance,
    pool: Arc<AgentPool>,
) {
    let (mut sender, mut receiver) = socket.split();
    let instance_id = instance.id.clone();

    info!(
        session_id = %session_id,
        instance_id = %instance_id,
        "WebSocket connected"
    );

    // Get AgentRuntime from the pool instance
    let runtime = match instance.runtime {
        Some(ref rt) => Arc::clone(rt),
        None => {
            let err = ServerMessage::Error {
                message: "Agent runtime not available for this instance".to_string(),
            };
            if let Ok(text) = serde_json::to_string(&err) {
                let _ = sender.send(Message::Text(text.into())).await;
            }
            let _ = pool.release_instance(instance_id).await;
            return;
        }
    };

    // Start the primary executor for this session (or get existing handle)
    let session_id_typed = octo_types::SessionId::from_string(&session_id);
    let user_id = instance
        .workspace
        .as_ref()
        .map(|w| octo_types::UserId::from_string(&w.user_id))
        .unwrap_or_default();
    let sandbox_id = octo_types::SandboxId::from_string(&format!("platform-{}", instance_id));

    let handle = runtime
        .start_primary(
            session_id_typed,
            user_id,
            sandbox_id,
            Vec::new(), // Empty initial history (session store manages persistence)
            None,       // No specific agent_id
        )
        .await;

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => {
                info!(session_id = %session_id, "WebSocket closed by client");
                break;
            }
            Err(e) => {
                warn!(session_id = %session_id, "WebSocket error: {e}");
                break;
            }
            _ => continue,
        };

        // Message size limit validation
        if msg.len() > MAX_MESSAGE_SIZE {
            let err = ServerMessage::Error {
                message: format!("Message too large (max {} bytes)", MAX_MESSAGE_SIZE),
            };
            if let Ok(text) = serde_json::to_string(&err) {
                let _ = sender.send(Message::Text(text.into())).await;
            }
            continue;
        }

        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                debug!(session_id = %session_id, "Failed to parse client message: {e}");
                let err = ServerMessage::Error {
                    message: format!("Invalid message: {e}"),
                };
                if let Ok(text) = serde_json::to_string(&err) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }
                continue;
            }
        };

        match client_msg {
            ClientMessage::Chat { content } => {
                let sid_str = session_id.clone();

                // Notify client about session
                let created_msg = ServerMessage::SessionCreated {
                    session_id: sid_str.clone(),
                };
                if let Ok(text) = serde_json::to_string(&created_msg) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }

                // Subscribe to events BEFORE sending message (avoid losing events)
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
                                AgentEvent::Error { message } => ServerMessage::Error { message },
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
                                // IterationStart/IterationEnd are internal — skip
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
                info!(session_id = %session_id, "Agent cancellation requested");
            }
            ClientMessage::ApprovalResponse { tool_id, approved } => {
                // Forward approval response to the AgentRuntime's ApprovalGate
                if let Some(gate) = runtime.approval_gate() {
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
            ClientMessage::Ping => {
                let pong = ServerMessage::Pong;
                if let Ok(text) = serde_json::to_string(&pong) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }
            }
        }
    }

    // Return instance to pool on disconnect
    let _ = pool.release_instance(instance_id.clone()).await;

    info!(
        session_id = %session_id,
        instance_id = %instance_id,
        "WebSocket closed, instance returned to pool"
    );
}
