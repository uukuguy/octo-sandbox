use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Request, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_engine::auth::{get_user_context, UserContext};
use octo_engine::{AgentEvent, AgentMessage};
use octo_types::{SessionId, UserId};

use crate::state::AppState;

// --- Client → Server messages ---

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "send_message")]
    SendMessage {
        session_id: Option<String>,
        content: String,
    },
    #[serde(rename = "cancel")]
    Cancel { session_id: String },
}

// --- Server → Client messages ---

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
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
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    req: Request,
) -> impl IntoResponse {
    // Extract UserContext from request extensions (injected by auth middleware)
    let user_ctx: UserContext = get_user_context(&req).unwrap_or_else(|| UserContext {
        user_id: None,
        permissions: vec![],
    });

    ws.on_upgrade(move |socket| handle_socket(socket, state, user_ctx))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, user_ctx: UserContext) {
    let (mut sender, mut receiver) = socket.split();

    info!("WebSocket connected");

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => {
                info!("WebSocket closed by client");
                break;
            }
            Err(e) => {
                warn!("WebSocket error: {e}");
                break;
            }
            _ => continue,
        };

        let client_msg: ClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                let err = ServerMessage::Error {
                    session_id: String::new(),
                    message: format!("Invalid message: {e}"),
                };
                let _ = sender
                    .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                    .await;
                continue;
            }
        };

        // Convert user_id from Option<String> to Option<UserId>
        let user_id_opt = user_ctx.user_id.as_ref().map(|s| UserId::from_string(s));

        match client_msg {
            ClientMessage::SendMessage {
                session_id,
                content,
            } => {
                // Get or create session with user isolation
                // Handle both authenticated (with user_id) and unauthenticated modes
                let session = match (&session_id, &user_id_opt) {
                    // Case 1: Session ID provided with user_id - use user-aware methods
                    (Some(sid), Some(uid)) => {
                        let session_id_obj = SessionId::from_string(sid);
                        // Use get_session_for_user to ensure user can only access their own sessions
                        if let Some(existing) = state
                            .sessions
                            .get_session_for_user(&session_id_obj, uid)
                            .await
                        {
                            existing
                        } else {
                            // Session not found or doesn't belong to user - create new session for this user
                            let s = state.sessions.create_session_with_user(uid).await;
                            let msg = ServerMessage::SessionCreated {
                                session_id: s.session_id.as_str().to_string(),
                            };
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                                .await;
                            s
                        }
                    }
                    // Case 2: No session ID, but user_id exists - create new session for user
                    (None, Some(uid)) => {
                        let s = state.sessions.create_session_with_user(uid).await;
                        let msg = ServerMessage::SessionCreated {
                            session_id: s.session_id.as_str().to_string(),
                        };
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                        s
                    }
                    // Case 3: No user_id (auth disabled) - use original methods without user filtering
                    (Some(sid), None) => {
                        let session_id_obj = SessionId::from_string(sid);
                        if let Some(existing) = state.sessions.get_session(&session_id_obj).await {
                            existing
                        } else {
                            // Session not found - create new session
                            let s = state.sessions.create_session().await;
                            let msg = ServerMessage::SessionCreated {
                                session_id: s.session_id.as_str().to_string(),
                            };
                            let _ = sender
                                .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                                .await;
                            s
                        }
                    }
                    // Case 4: No session_id and no user_id - create new session
                    (None, None) => {
                        let s = state.sessions.create_session().await;
                        let msg = ServerMessage::SessionCreated {
                            session_id: s.session_id.as_str().to_string(),
                        };
                        let _ = sender
                            .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                            .await;
                        s
                    }
                };

                let sid_str = session.session_id.as_str().to_string();

                // Get current message history for initial seeding (only needed on first spawn)
                let initial_history = state
                    .sessions
                    .get_messages(&session.session_id)
                    .await
                    .unwrap_or_default();

                // Get or spawn persistent AgentRuntime for this session
                let handle = state.runtime_registry.get_or_spawn(
                    session.session_id.clone(),
                    session.user_id.clone(),
                    session.sandbox_id.clone(),
                    initial_history,
                    state.agent_runner.provider(),
                    state.agent_runner.build_tool_registry(&[]),
                    state.agent_runner.memory(),
                    Some(state.memory_store.clone()),
                    state.model.clone(),
                    Some(state.sessions.clone()),
                );

                // Subscribe before sending to avoid missing events
                let mut rx = handle.subscribe();

                // Forward user message to AgentRuntime
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
                                AgentEvent::Done => {
                                    let done_msg = ServerMessage::Done {
                                        session_id: sid_str.clone(),
                                    };
                                    let _ = sender
                                        .send(Message::Text(
                                            serde_json::to_string(&done_msg).unwrap().into(),
                                        ))
                                        .await;
                                    break;
                                }
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
            ClientMessage::Cancel { session_id } => {
                let sid = SessionId::from_string(&session_id);
                if let Some(handle) = state.runtime_registry.get(&sid) {
                    let _ = handle.send(AgentMessage::Cancel).await;
                }
                info!("Agent cancellation requested for session {session_id}");
            }
        }
    }

    info!("WebSocket disconnected");
}
