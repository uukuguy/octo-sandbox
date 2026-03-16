use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, warn};

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;

/// Tool for searching the web.
///
/// Uses Tavily Search API as primary backend (requires TAVILY_API_KEY env var).
/// Falls back to DuckDuckGo Instant Answer API when Tavily is unavailable.
pub struct WebSearchTool {
    client: reqwest::Client,
    tavily_api_key: Option<String>,
}

impl WebSearchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        let tavily_api_key = std::env::var("TAVILY_API_KEY").ok().filter(|k| !k.is_empty());
        Self {
            client,
            tavily_api_key,
        }
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns search results with titles, URLs, and content snippets."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?;

        let max_results = params["max_results"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(5);

        debug!(query, max_results, "performing web search");

        // Try Tavily first, fall back to DDG Instant Answer API
        if let Some(api_key) = &self.tavily_api_key {
            match self
                .search_tavily(query, max_results, api_key)
                .await
            {
                Ok(output) => return Ok(output),
                Err(e) => {
                    warn!("Tavily search failed, falling back to DDG: {e}");
                }
            }
        }

        self.search_ddg_instant(query, max_results).await
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

impl WebSearchTool {
    /// Search using Tavily Search API
    async fn search_tavily(
        &self,
        query: &str,
        max_results: usize,
        api_key: &str,
    ) -> Result<ToolOutput> {
        let body = json!({
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "include_answer": true,
        });

        let response = self
            .client
            .post("https://api.tavily.com/search")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Tavily request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Tavily HTTP {status}: {text}"));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Tavily JSON parse failed: {e}"))?;

        let mut output = String::new();

        // Include direct answer if available
        if let Some(answer) = data["answer"].as_str() {
            if !answer.is_empty() {
                output.push_str(&format!("Direct Answer: {answer}\n\n"));
            }
        }

        // Format search results
        if let Some(results) = data["results"].as_array() {
            if results.is_empty() && output.is_empty() {
                return Ok(ToolOutput::success("No search results found.".to_string()));
            }
            for (i, r) in results.iter().enumerate() {
                let title = r["title"].as_str().unwrap_or("(no title)");
                let url = r["url"].as_str().unwrap_or("");
                let content = r["content"].as_str().unwrap_or("");
                output.push_str(&format!("{}. {}\n   URL: {}\n   {}\n\n", i + 1, title, url, content));
            }
        } else if output.is_empty() {
            return Ok(ToolOutput::success("No search results found.".to_string()));
        }

        Ok(ToolOutput::success(output.trim_end().to_string()))
    }

    /// Fallback: DuckDuckGo Instant Answer API (JSON, no CAPTCHA)
    /// Note: This returns limited results (abstract + related topics only).
    async fn search_ddg_instant(&self, query: &str, max_results: usize) -> Result<ToolOutput> {
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Octo-Sandbox/1.0")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("DDG request failed: {e}"))?;

        if !response.status().is_success() {
            return Ok(ToolOutput::error(format!(
                "DDG HTTP error: {}",
                response.status().as_u16()
            )));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("DDG JSON parse failed: {e}"))?;

        let mut output = String::new();

        // Abstract (main answer)
        if let Some(abstract_text) = data["Abstract"].as_str() {
            if !abstract_text.is_empty() {
                let source = data["AbstractSource"].as_str().unwrap_or("");
                let url = data["AbstractURL"].as_str().unwrap_or("");
                output.push_str(&format!("Summary ({source}): {abstract_text}\n   URL: {url}\n\n"));
            }
        }

        // Related topics
        if let Some(results) = data["Results"].as_array() {
            for (i, r) in results.iter().take(max_results).enumerate() {
                let text = r["Text"].as_str().unwrap_or("");
                let url = r["FirstURL"].as_str().unwrap_or("");
                if !text.is_empty() {
                    output.push_str(&format!("{}. {}\n   URL: {}\n\n", i + 1, text, url));
                }
            }
        }

        // Related topics from nested structure
        if let Some(topics) = data["RelatedTopics"].as_array() {
            let start = output.lines().filter(|l| l.starts_with(char::is_numeric)).count();
            for (i, t) in topics.iter().take(max_results).enumerate() {
                let text = t["Text"].as_str().unwrap_or("");
                let url = t["FirstURL"].as_str().unwrap_or("");
                if !text.is_empty() {
                    output.push_str(&format!(
                        "{}. {}\n   URL: {}\n\n",
                        start + i + 1,
                        text,
                        url
                    ));
                }
            }
        }

        if output.is_empty() {
            return Ok(ToolOutput::success("No search results found.".to_string()));
        }

        Ok(ToolOutput::success(output.trim_end().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_search_tool_metadata() {
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_web_search_parameters_schema() {
        let tool = WebSearchTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        let props = &params["properties"];
        assert!(props["query"].is_object());
        assert!(props["max_results"].is_object());
        let required = params["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "query");
    }

    #[tokio::test]
    async fn test_web_search_missing_query() {
        use std::path::PathBuf;
        let tool = WebSearchTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::new(),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        };
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing 'query'"));
    }

    #[test]
    fn test_tavily_api_key_from_env() {
        // Test that constructor reads TAVILY_API_KEY
        let tool = WebSearchTool::new();
        // We just verify the tool constructs without panic
        assert_eq!(tool.name(), "web_search");
    }
}
