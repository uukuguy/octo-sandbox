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
}
