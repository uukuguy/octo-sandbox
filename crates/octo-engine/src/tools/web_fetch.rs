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

        // SSRF protection: validate URL scheme and destination
        let parsed_url = reqwest::Url::parse(url)
            .map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;

        // Block non-HTTP schemes
        match parsed_url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Ok(ToolResult::error(format!(
                    "Blocked URL scheme '{}'. Only http and https are allowed.",
                    scheme
                )));
            }
        }

        // Block private/loopback/link-local IPs and cloud metadata endpoints
        if let Some(host) = parsed_url.host_str() {
            let is_loopback = host == "localhost"
                || host == "127.0.0.1"
                || host == "::1"
                || host == "0.0.0.0";

            let is_private_10 = host.starts_with("10.");
            let is_private_192 = host.starts_with("192.168.");
            let is_link_local = host.starts_with("169.254.");

            // Check 172.16.0.0/12 range
            let is_private_172 = if let Some(rest) = host.strip_prefix("172.") {
                rest.split('.')
                    .next()
                    .and_then(|s| s.parse::<u8>().ok())
                    .map(|n| (16..=31).contains(&n))
                    .unwrap_or(false)
            } else {
                false
            };

            // Cloud metadata endpoints
            let is_metadata =
                host == "metadata.google.internal" || host == "169.254.169.254";

            if is_loopback
                || is_private_10
                || is_private_192
                || is_link_local
                || is_private_172
                || is_metadata
            {
                return Ok(ToolResult::error(format!(
                    "Blocked request to private/internal address: {}",
                    host
                )));
            }
        }

        debug!(url, max_length, "fetching web content");

        let response = self
            .client
            .get(url)
            .header("User-Agent", "Octo-Sandbox/1.0")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch URL: {e}"))?;

        if !response.status().is_success() {
            return Ok(ToolResult::error(format!(
                "HTTP error: {} - {}",
                response.status().as_u16(),
                response
                    .status()
                    .canonical_reason()
                    .unwrap_or("Unknown error")
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
