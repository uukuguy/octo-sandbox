use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::{json, Value};

use super::traits::Tool;

/// Sleep tool -- rhythm controller for autonomous mode.
/// The actual wait is handled by the harness autonomous loop;
/// this tool just signals the intent to sleep.
pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str {
        "sleep"
    }

    fn description(&self) -> &str {
        "Wait for the specified number of seconds. Used in autonomous mode to control work rhythm. \
         Users can interrupt at any time. No tokens are consumed during sleep. \
         Prefer this tool over bash(sleep N)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "seconds": {
                    "type": "integer",
                    "description": "Number of seconds to wait (1-600)"
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for waiting (e.g., 'waiting for test completion')"
                }
            },
            "required": ["seconds"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let seconds = params["seconds"].as_u64().unwrap_or(30).min(600);
        let reason = params["reason"].as_str().unwrap_or("idle");
        Ok(ToolOutput::success(format!(
            "Sleeping for {} seconds (reason: {}). Will wake on tick or user message.",
            seconds, reason
        )))
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

    fn execution_timeout(&self) -> Duration {
        // Sleep tool returns instantly (the actual wait is external).
        Duration::from_secs(5)
    }

    fn category(&self) -> &str {
        "autonomous"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: Default::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: "/tmp".into(),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_sleep_tool_basic() {
        let tool = SleepTool;
        let params = json!({"seconds": 10, "reason": "waiting for CI"});
        let result = tool.execute(params, &make_ctx()).await.unwrap();
        assert!(result.content.contains("10 seconds"));
        assert!(result.content.contains("waiting for CI"));
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_sleep_tool_clamps_max() {
        let tool = SleepTool;
        let params = json!({"seconds": 9999});
        let result = tool.execute(params, &make_ctx()).await.unwrap();
        assert!(result.content.contains("600 seconds"));
    }

    #[tokio::test]
    async fn test_sleep_tool_defaults() {
        let tool = SleepTool;
        let params = json!({});
        let result = tool.execute(params, &make_ctx()).await.unwrap();
        // Default to 30 seconds when missing
        assert!(result.content.contains("30 seconds"));
        assert!(result.content.contains("idle"));
    }

    #[test]
    fn test_sleep_tool_metadata() {
        let tool = SleepTool;
        assert_eq!(tool.name(), "sleep");
        assert!(tool.is_read_only());
        assert_eq!(tool.category(), "autonomous");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
    }

    #[test]
    fn test_sleep_tool_parameters_schema() {
        let tool = SleepTool;
        let schema = tool.parameters();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["seconds"].is_object());
        assert!(schema["properties"]["reason"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "seconds");
    }
}
