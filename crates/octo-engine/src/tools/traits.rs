use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ApprovalRequirement, RiskLevel, ToolContext, ToolOutput, ToolSource, ToolSpec};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput>;
    fn source(&self) -> ToolSource;

    /// Returns the risk level of this tool. Defaults to LowRisk.
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::LowRisk
    }

    /// Returns the approval requirement for this tool. Defaults to Never.
    fn approval(&self) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.parameters(),
        }
    }

    /// Per-tool execution timeout (default 30 seconds).
    ///
    /// When used together with `AgentLoopConfig::tool_timeout_secs`, the
    /// effective timeout is `min(config_timeout, self.execution_timeout())`.
    fn execution_timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    /// Rate limit: maximum invocations per minute (0 = unlimited).
    fn rate_limit(&self) -> u32 {
        0
    }

    /// Parameter names whose values should be redacted in logs.
    fn sensitive_params(&self) -> Vec<&str> {
        vec![]
    }

    /// Category tag for grouping and permission control.
    fn category(&self) -> &str {
        "general"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::ToolSource;

    /// Minimal mock that only implements required methods.
    struct MockTool;

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            "mock"
        }
        fn description(&self) -> &str {
            "mock tool"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::success("ok"))
        }
        fn source(&self) -> ToolSource {
            ToolSource::BuiltIn
        }
    }

    #[test]
    fn test_default_execution_timeout() {
        let tool = MockTool;
        assert_eq!(tool.execution_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_default_rate_limit() {
        let tool = MockTool;
        assert_eq!(tool.rate_limit(), 0);
    }

    #[test]
    fn test_default_sensitive_params() {
        let tool = MockTool;
        assert!(tool.sensitive_params().is_empty());
    }

    #[test]
    fn test_default_category() {
        let tool = MockTool;
        assert_eq!(tool.category(), "general");
    }
}
