//! InteractionGate — async channel for agent-to-user interaction.
//!
//! Modeled after ApprovalGate (tools/approval.rs) but supports multiple
//! interaction types: Question, Select, Confirm.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, warn};

/// Default timeout for waiting on user interaction (60 seconds).
const INTERACTION_TIMEOUT_SECS: u64 = 60;

/// Message status for fire-and-forget notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    /// Normal response to user action.
    Normal,
    /// Proactive/unsolicited update from agent.
    Proactive,
}

/// Interaction request types that the agent can send to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InteractionRequest {
    /// Free-text question
    Question {
        prompt: String,
        default: Option<String>,
    },
    /// Single-select from options
    Select {
        prompt: String,
        options: Vec<String>,
    },
    /// Yes/No confirmation
    Confirm { prompt: String },
    /// Fire-and-forget message (no response expected).
    Message {
        content: String,
        status: MessageStatus,
    },
}

/// User's response to an interaction request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InteractionResponse {
    Text(String),
    Selected { index: usize, value: String },
    Confirmed(bool),
    /// Acknowledgement that a fire-and-forget message was delivered.
    Delivered,
    Timeout,
}

/// Async interaction gate — manages pending interaction requests.
///
/// Usage pattern (similar to ApprovalGate):
/// 1. Agent tool calls `gate.request(id, req)` → gets a oneshot Receiver
/// 2. AgentEvent::InteractionRequested is emitted to the TUI/WS consumer
/// 3. TUI/WS consumer calls `gate.respond(id, response)` when user answers
/// 4. The tool's awaiting receiver gets the response
#[derive(Debug, Default)]
pub struct InteractionGate {
    pending: Mutex<HashMap<String, oneshot::Sender<InteractionResponse>>>,
}

impl InteractionGate {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending interaction request, returning a receiver for the response.
    pub async fn request(
        &self,
        id: &str,
        _req: InteractionRequest,
    ) -> oneshot::Receiver<InteractionResponse> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.to_string(), tx);
        debug!(id, "InteractionGate: registered pending request");
        rx
    }

    /// Respond to a pending interaction request.
    pub async fn respond(&self, id: &str, response: InteractionResponse) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(id) {
            if tx.send(response).is_ok() {
                debug!(id, "InteractionGate: response delivered");
                return true;
            }
        }
        warn!(id, "InteractionGate: no pending request found or receiver dropped");
        false
    }

    /// Wait for a response with timeout. Returns Timeout on expiry.
    pub async fn wait_for_response(
        rx: oneshot::Receiver<InteractionResponse>,
    ) -> InteractionResponse {
        Self::wait_for_response_with_timeout(rx, Duration::from_secs(INTERACTION_TIMEOUT_SECS))
            .await
    }

    /// Wait for a response with custom timeout.
    pub async fn wait_for_response_with_timeout(
        rx: oneshot::Receiver<InteractionResponse>,
        timeout: Duration,
    ) -> InteractionResponse {
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                warn!("InteractionGate: sender dropped before responding");
                InteractionResponse::Timeout
            }
            Err(_) => {
                warn!("InteractionGate: timed out waiting for response");
                InteractionResponse::Timeout
            }
        }
    }

    /// Send a fire-and-forget message. Does not wait for a response.
    /// The request is registered and immediately auto-responded with `Delivered`.
    pub async fn send_message(&self, id: &str, content: String, status: MessageStatus) {
        let req = InteractionRequest::Message { content, status };
        let _rx = self.request(id, req).await;
        // Auto-respond immediately — the consumer sees the Message event,
        // but the sender doesn't block.
        self.respond(id, InteractionResponse::Delivered).await;
    }

    /// Check if there are any pending requests.
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_interaction_gate_question_roundtrip() {
        let gate = InteractionGate::new();
        let rx = gate
            .request(
                "q1",
                InteractionRequest::Question {
                    prompt: "What is your name?".into(),
                    default: None,
                },
            )
            .await;

        gate.respond("q1", InteractionResponse::Text("Alice".into()))
            .await;
        let response = rx.await.unwrap();
        assert!(matches!(response, InteractionResponse::Text(t) if t == "Alice"));
    }

    #[tokio::test]
    async fn test_interaction_gate_select_roundtrip() {
        let gate = InteractionGate::new();
        let rx = gate
            .request(
                "s1",
                InteractionRequest::Select {
                    prompt: "Choose color".into(),
                    options: vec!["Red".into(), "Blue".into()],
                },
            )
            .await;

        gate.respond(
            "s1",
            InteractionResponse::Selected {
                index: 1,
                value: "Blue".into(),
            },
        )
        .await;
        let response = rx.await.unwrap();
        assert!(matches!(
            response,
            InteractionResponse::Selected { index: 1, .. }
        ));
    }

    #[tokio::test]
    async fn test_interaction_gate_confirm_roundtrip() {
        let gate = InteractionGate::new();
        let rx = gate
            .request(
                "c1",
                InteractionRequest::Confirm {
                    prompt: "Continue?".into(),
                },
            )
            .await;

        gate.respond("c1", InteractionResponse::Confirmed(true))
            .await;
        let response = rx.await.unwrap();
        assert!(matches!(response, InteractionResponse::Confirmed(true)));
    }

    #[tokio::test]
    async fn test_interaction_gate_timeout() {
        let gate = InteractionGate::new();
        let rx = gate
            .request(
                "t1",
                InteractionRequest::Question {
                    prompt: "test".into(),
                    default: None,
                },
            )
            .await;

        let response =
            InteractionGate::wait_for_response_with_timeout(rx, Duration::from_millis(50)).await;
        assert!(matches!(response, InteractionResponse::Timeout));
    }

    #[tokio::test]
    async fn test_interaction_gate_respond_nonexistent() {
        let gate = InteractionGate::new();
        let result = gate
            .respond("nonexistent", InteractionResponse::Text("x".into()))
            .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_interaction_gate_concurrent_requests() {
        let gate = Arc::new(InteractionGate::new());
        let rx1 = gate
            .request(
                "a",
                InteractionRequest::Question {
                    prompt: "q1".into(),
                    default: None,
                },
            )
            .await;
        let rx2 = gate
            .request(
                "b",
                InteractionRequest::Confirm {
                    prompt: "q2".into(),
                },
            )
            .await;

        assert_eq!(gate.pending_count().await, 2);

        gate.respond("b", InteractionResponse::Confirmed(false))
            .await;
        gate.respond("a", InteractionResponse::Text("hello".into()))
            .await;

        let r1 = rx1.await.unwrap();
        let r2 = rx2.await.unwrap();
        assert!(matches!(r1, InteractionResponse::Text(_)));
        assert!(matches!(r2, InteractionResponse::Confirmed(false)));
        assert_eq!(gate.pending_count().await, 0);
    }
}
