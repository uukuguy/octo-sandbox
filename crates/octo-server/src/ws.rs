use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Request, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use octo_engine::{AgentEvent, AgentMessage};

use crate::state::AppState;

// --- Client → Server messages ---

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "send_message")]
    SendMessage { content: String },
    #[serde(rename = "cancel")]
    Cancel,
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
    // Auth check: if auth is enabled, verify user context exists
    if state.auth_config.mode != octo_engine::auth::AuthMode::None {
        if req.extensions().get::<octo_engine::auth::UserContext>().is_none() {
            return axum::response::Response::builder()
                .status(axum::http::StatusCode::UNAUTHORIZED)
                .body(axum::body::Body::from("WebSocket authentication required"))
                .unwrap()
                .into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
        .into_response()
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
                if let Ok(text) = serde_json::to_string(&err) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }
                continue;
            }
        };

        match client_msg {
            ClientMessage::SendMessage { content } => {
                // 直接使用注入的主 Handle，不持有 AgentRuntime
                let handle = &state.agent_handle;
                let sid_str = handle.session_id.as_str().to_string();

                // 告知客户端 session_id（前端 UI 显示用）
                let created_msg = ServerMessage::SessionCreated {
                    session_id: sid_str.clone(),
                };
                if let Ok(text) = serde_json::to_string(&created_msg) {
                    let _ = sender.send(Message::Text(text.into())).await;
                }

                // 先订阅，再发消息（避免丢失事件）
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
                                AgentEvent::Done => {
                                    let done_msg = ServerMessage::Done {
                                        session_id: sid_str.clone(),
                                    };
                                    if let Ok(text) = serde_json::to_string(&done_msg) {
                                        let _ = sender.send(Message::Text(text.into())).await;
                                    }
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
            ClientMessage::Cancel => {
                let _ = state.agent_handle.send(AgentMessage::Cancel).await;
                info!("Agent cancellation requested");
            }
        }
    }

    info!("WebSocket disconnected");
}
