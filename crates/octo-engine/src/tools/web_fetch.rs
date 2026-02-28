use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{ToolContext, ToolResult, ToolSource};

use super::traits::Tool;

/// Tool for fetching web content from a URL
pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch content from a URL. Returns the raw HTML/text content of the page."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum number of characters to return (default: 50000)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'url' parameter"))?;

        let max_length = params["max_length"].as_u64().map(|v| v as usize);

        debug!(url, max_length, "fetching web content");

        let response = self.client
            .get(url)
            .header("User-Agent", "Octo-Sandbox/1.0")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch URL: {e}"))?;

        if !response.status().is_success() {
            return Ok(ToolResult::error(format!(
                "HTTP error: {} - {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("Unknown error")
            )));
        }

        let mut content = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;

        // Truncate if necessary
        let max = max_length.unwrap_or(50000);
        if content.len() > max {
            content.truncate(max);
            content.push_str("\n... (content truncated)");
        }

        Ok(ToolResult::success(content))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
