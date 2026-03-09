//! WebSocket handler for real-time agent communication

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
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{
    agent_pool::{AgentPool, InstanceId},
    AppState, AuthExtractor, ErrorResponse,
};

/// Maximum message size limit (1MB)
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// WebSocket message from client
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "chat")]
    Chat { content: String },
    #[serde(rename = "ping")]
    Ping,
}

/// WebSocket message to client
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "response")]
    Response { content: String, done: bool },
    #[serde(rename = "error")]
    Error { message: String },
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

    // Save instance_id for returning to pool
    let instance_id = instance.id.clone();
    let pool = Arc::clone(pool);

    ws.on_upgrade(move |socket| handle_socket(session_id, socket, instance_id, pool))
}

async fn handle_socket(
    session_id: String,
    socket: WebSocket,
    instance_id: InstanceId,
    pool: Arc<AgentPool>,
) {
    let (mut sender, mut receiver) = socket.split();

    // Create a channel for sending messages back to client
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    // Spawn task to forward messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Issue #1: Handle serialization errors properly instead of silently returning empty string
            let text = match serde_json::to_string(&msg) {
                Ok(json) => json,
                Err(e) => {
                    warn!("Failed to serialize WebSocket message: {}", e);
                    continue;
                }
            };
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Issue #4: Add message size limit validation
                if text.len() > MAX_MESSAGE_SIZE {
                    let error_msg = ServerMessage::Error {
                        message: format!("Message too large (max {} bytes)", MAX_MESSAGE_SIZE),
                    };
                    let _ = tx.send(error_msg).await;
                    continue;
                }

                // Issue #3: Log malformed JSON instead of silently ignoring
                let client_msg = match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(msg) => msg,
                    Err(e) => {
                        debug!("Failed to parse client WebSocket message: {}", e);
                        continue;
                    }
                };

                match client_msg {
                    ClientMessage::Chat { content } => {
                        // =============================================================================
                        // STUB: AgentRuntime Integration (P1-4)
                        // =============================================================================
                        // For now, just echo the message back. Full integration will include:
                        // - Create/update AgentRuntime per session
                        // - Stream agent responses via AgentRuntime::run()
                        // - Handle tool execution results
                        // - Support conversation context and memory layers
                        //
                        // See P1-4 plan for implementation details.
                        let response = ServerMessage::Response {
                            content: format!("[Stub] Received: {}", content),
                            done: true,
                        };
                        let _ = tx.send(response).await;
                    }
                    ClientMessage::Ping => {
                        let _ = tx.send(ServerMessage::Pong).await;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    send_task.abort();

    // Return instance to pool (ignore errors - pool will handle cleanup)
    let _ = pool.release_instance(instance_id).await;

    tracing::info!(
        "WebSocket closed for session: {}, instance returned to pool",
        session_id
    );
}
