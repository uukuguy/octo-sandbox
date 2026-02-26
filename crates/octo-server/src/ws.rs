use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_engine::AgentEvent;
use octo_types::{ChatMessage, SandboxId, SessionId, ToolContext, UserId};

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
    Cancel {
        session_id: String,
    },
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

    #[serde(rename = "error")]
    Error { session_id: String, message: String },

    #[serde(rename = "done")]
    Done { session_id: String },
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
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

        match client_msg {
            ClientMessage::SendMessage {
                session_id,
                content,
            } => {
                // Get or create session
                let session = if let Some(sid) = session_id {
                    let session_id_obj = SessionId::from_string(&sid);
                    if state.sessions.get_messages(&session_id_obj).await.is_some() {
                        crate::session::SessionData {
                            session_id: session_id_obj,
                            user_id: UserId::from_string("default"),
                            sandbox_id: SandboxId::new(),
                        }
                    } else {
                        let s = state.sessions.create_session().await;
                        let msg = ServerMessage::SessionCreated {
                            session_id: s.session_id.as_str().to_string(),
                        };
                        let _ = sender
                            .send(Message::Text(
                                serde_json::to_string(&msg).unwrap().into(),
                            ))
                            .await;
                        s
                    }
                } else {
                    let s = state.sessions.create_session().await;
                    let msg = ServerMessage::SessionCreated {
                        session_id: s.session_id.as_str().to_string(),
                    };
                    let _ = sender
                        .send(Message::Text(
                            serde_json::to_string(&msg).unwrap().into(),
                        ))
                        .await;
                    s
                };

                let sid_str = session.session_id.as_str().to_string();

                // Add user message to history
                let user_msg = ChatMessage::user(&content);
                state.sessions.push_message(&session.session_id, user_msg).await;

                // Get current messages
                let mut messages = state
                    .sessions
                    .get_messages(&session.session_id)
                    .await
                    .unwrap_or_default();

                // Create broadcast channel for agent events
                let (tx, mut rx) = broadcast::channel::<AgentEvent>(256);

                let tool_ctx = ToolContext {
                    sandbox_id: session.sandbox_id.clone(),
                    working_dir: PathBuf::from("/tmp/octo-sandbox"),
                };

                // Ensure working dir exists
                let _ = tokio::fs::create_dir_all(&tool_ctx.working_dir).await;

                // Create a fresh AgentLoop per invocation (owns its own budget state)
                let provider = state.provider.clone();
                let tools = state.tools.clone();
                let memory = state.memory.clone();
                let mut agent_loop = octo_engine::AgentLoop::new(provider, tools, memory)
                    .with_memory_store(state.memory_store.clone());
                if let Some(ref recorder) = state.recorder {
                    agent_loop = agent_loop.with_recorder(recorder.clone());
                }
                if let Some(ref m) = state.model {
                    agent_loop = agent_loop.with_model(m.clone());
                }
                let session_id_clone = session.session_id.clone();
                let user_id = session.user_id.clone();
                let sandbox_id = session.sandbox_id.clone();

                // Spawn agent loop task
                let agent_handle = tokio::spawn(async move {
                    if let Err(e) = agent_loop
                        .run(
                            &session_id_clone,
                            &user_id,
                            &sandbox_id,
                            &mut messages,
                            tx,
                            tool_ctx,
                        )
                        .await
                    {
                        warn!("Agent loop error: {e}");
                    }
                    messages
                });

                // Forward agent events to WebSocket
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let server_msg = match event {
                                AgentEvent::TextDelta { text } => ServerMessage::TextDelta {
                                    session_id: sid_str.clone(),
                                    text,
                                },
                                AgentEvent::TextComplete { text } => {
                                    ServerMessage::TextComplete {
                                        session_id: sid_str.clone(),
                                        text,
                                    }
                                }
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

                // Get updated messages from agent loop
                if let Ok(updated_messages) = agent_handle.await {
                    state
                        .sessions
                        .set_messages(&session.session_id, updated_messages)
                        .await;
                }
            }
            ClientMessage::Cancel { session_id: _ } => {
                // TODO: implement cancellation via CancellationToken
                debug!("Cancel requested (not yet implemented)");
            }
        }
    }

    info!("WebSocket disconnected");
}
