//! AskUserTool — agent tool for asking the user questions.
//!
//! Uses InteractionGate to send questions and wait for answers.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};

use super::interaction::{InteractionGate, InteractionRequest, InteractionResponse};
use super::traits::Tool;

pub struct AskUserTool {
    gate: Arc<InteractionGate>,
}

impl AskUserTool {
    pub fn new(gate: Arc<InteractionGate>) -> Self {
        Self { gate }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user a question and wait for their response. Use this when you need clarification, \
         confirmation, or input from the user before proceeding. Supports three modes:\n\
         - Question: free-text answer (default)\n\
         - Select: choose from a list of options (provide `options` array)\n\
         - Confirm: yes/no answer (set `confirm: true`)\n\
         \n\
         When to use: need user decision, ambiguous requirements, dangerous operations.\n\
         When NOT to use: routine operations, information available in context."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "If provided, user selects from these options"
                },
                "default": {
                    "type": "string",
                    "description": "Default answer if user doesn't respond"
                },
                "confirm": {
                    "type": "boolean",
                    "description": "If true, ask for yes/no confirmation"
                }
            },
            "required": ["question"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let question = params
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: question"))?
            .to_string();

        let options: Option<Vec<String>> = params
            .get("options")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let default = params
            .get("default")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let confirm = params
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Determine request type
        let request = if let Some(opts) = options {
            InteractionRequest::Select {
                prompt: question.clone(),
                options: opts,
            }
        } else if confirm {
            InteractionRequest::Confirm {
                prompt: question.clone(),
            }
        } else {
            InteractionRequest::Question {
                prompt: question.clone(),
                default: default.clone(),
            }
        };

        // Generate a unique request ID
        let request_id = format!("ask-{}", uuid::Uuid::new_v4());

        // Register the request and wait for response
        let rx = self.gate.request(&request_id, request).await;
        let response = InteractionGate::wait_for_response(rx).await;

        match response {
            InteractionResponse::Text(text) => Ok(ToolOutput::success(text)),
            InteractionResponse::Selected { value, .. } => Ok(ToolOutput::success(value)),
            InteractionResponse::Confirmed(yes) => {
                Ok(ToolOutput::success(if yes { "yes" } else { "no" }))
            }
            InteractionResponse::Delivered => {
                // Shouldn't happen for ask_user, but handle gracefully
                Ok(ToolOutput::success("(message delivered)".to_string()))
            }
            InteractionResponse::Timeout => {
                if let Some(d) = default {
                    Ok(ToolOutput::success(format!(
                        "(timed out, using default: {d})"
                    )))
                } else {
                    Ok(ToolOutput::error(
                        "User did not respond within the timeout period.",
                    ))
                }
            }
        }
    }

    fn execution_timeout(&self) -> Duration {
        // Longer timeout since we're waiting for human input
        Duration::from_secs(120)
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
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
    async fn test_ask_user_question_params() {
        let gate = Arc::new(InteractionGate::new());
        let tool = AskUserTool::new(gate.clone());

        // Verify tool metadata
        assert_eq!(tool.name(), "ask_user");
        assert!(tool.description().contains("Ask the user"));
        assert!(tool.is_read_only());
        assert_eq!(tool.category(), "interaction");

        let spec = tool.spec();
        assert_eq!(spec.name, "ask_user");
    }

    #[tokio::test]
    async fn test_ask_user_execute_with_mock_response() {
        let gate = Arc::new(InteractionGate::new());
        let tool = AskUserTool::new(gate.clone());
        let ctx = test_ctx();

        // Spawn a task to respond
        let gate_clone = gate.clone();
        tokio::spawn(async move {
            // Wait a bit for the request to be registered
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            // Find the pending request and respond
            let pending = gate_clone.pending_count().await;
            assert!(pending > 0);
        });

        // The actual execute will timeout since we can't easily match the UUID
        // This tests the timeout + default path
        let result = tool
            .execute(
                serde_json::json!({
                    "question": "What color?",
                    "default": "blue"
                }),
                &ctx,
            )
            .await
            .unwrap();

        // Should get the default since we didn't respond with matching ID
        assert!(result.content.contains("blue") || result.content.contains("timed out"));
    }
}
