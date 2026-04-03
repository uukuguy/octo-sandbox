//! SendMessageTool — fire-and-forget message delivery to user.
//!
//! Aligns with CC-OSS BriefTool (SendUserMessage): one-way push from agent
//! to user, supporting markdown formatting and status indicators.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::json;

use super::interaction::{InteractionGate, MessageStatus};
use super::traits::Tool;

pub struct SendMessageTool {
    gate: Arc<InteractionGate>,
}

impl SendMessageTool {
    pub fn new(gate: Arc<InteractionGate>) -> Self {
        Self { gate }
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn description(&self) -> &str {
        "Send a message to the user without expecting a response. Use this to deliver status \
         updates, progress notifications, or informational messages. Supports markdown formatting.\n\
         \n\
         ## Parameters\n\
         - message (required): The message content (supports markdown)\n\
         - status (optional): \"normal\" (default) or \"proactive\" for unsolicited updates\n\
         \n\
         ## When to use\n\
         - Delivering progress updates during long operations\n\
         - Sharing results or summaries\n\
         - Proactive notifications (set status: \"proactive\")\n\
         \n\
         ## When NOT to use\n\
         - When you need user input → use ask_user instead\n\
         - For routine tool output that appears naturally in the conversation"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message content to send (supports markdown)"
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "Message status: normal (reactive) or proactive (unsolicited)"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?
            .to_string();

        let status = match params.get("status").and_then(|v| v.as_str()) {
            Some("proactive") => MessageStatus::Proactive,
            _ => MessageStatus::Normal,
        };

        let request_id = format!("msg-{}", uuid::Uuid::new_v4());
        self.gate
            .send_message(&request_id, message.clone(), status)
            .await;

        let result = json!({
            "sent": true,
            "message": message,
            "status": if status == MessageStatus::Proactive { "proactive" } else { "normal" },
        });
        Ok(ToolOutput::success(serde_json::to_string(&result)?))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "interaction"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_send_message_fires_and_forgets() {
        let gate = Arc::new(InteractionGate::new());
        let tool = SendMessageTool::new(gate.clone());
        let ctx = test_ctx();

        let result = tool
            .execute(json!({"message": "Hello user!"}), &ctx)
            .await
            .unwrap();

        assert!(result.content.contains("\"sent\":true"));
        assert!(result.content.contains("Hello user!"));
        // No pending requests — message was auto-delivered
        assert_eq!(gate.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_send_message_proactive_status() {
        let gate = Arc::new(InteractionGate::new());
        let tool = SendMessageTool::new(gate);
        let ctx = test_ctx();

        let result = tool
            .execute(
                json!({"message": "Update: build passed", "status": "proactive"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.content.contains("proactive"));
    }

    #[tokio::test]
    async fn test_send_message_default_normal_status() {
        let gate = Arc::new(InteractionGate::new());
        let tool = SendMessageTool::new(gate);
        let ctx = test_ctx();

        let result = tool
            .execute(json!({"message": "Result ready"}), &ctx)
            .await
            .unwrap();

        assert!(result.content.contains("\"status\":\"normal\""));
    }

    #[tokio::test]
    async fn test_send_message_supports_markdown() {
        let gate = Arc::new(InteractionGate::new());
        let tool = SendMessageTool::new(gate);
        let ctx = test_ctx();

        let md = "## Summary\n- Item 1\n- Item 2\n\n```rust\nfn main() {}\n```";
        let result = tool
            .execute(json!({"message": md}), &ctx)
            .await
            .unwrap();

        assert!(result.content.contains("## Summary"));
    }

    #[tokio::test]
    async fn test_send_message_missing_param() {
        let gate = Arc::new(InteractionGate::new());
        let tool = SendMessageTool::new(gate);
        let ctx = test_ctx();

        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_err());
    }
}
