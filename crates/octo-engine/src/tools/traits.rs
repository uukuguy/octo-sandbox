use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolResult, ToolSource, ToolSpec};

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult>;
    fn source(&self) -> ToolSource;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: self.parameters(),
        }
    }
}
