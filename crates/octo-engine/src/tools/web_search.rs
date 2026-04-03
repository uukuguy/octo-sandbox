use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{debug, warn};

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolProgress, ToolSource};

use super::traits::Tool;

/// Supported web search engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchEngine {
    Jina,
    Tavily,
    Ddg,
}

impl SearchEngine {
    /// Parse from string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "jina" => Some(Self::Jina),
            "tavily" => Some(Self::Tavily),
            "ddg" | "duckduckgo" => Some(Self::Ddg),
            _ => None,
        }
    }
}

/// Default engine priority: Jina → Tavily → DDG.
pub const DEFAULT_SEARCH_PRIORITY: &[SearchEngine] = &[
    SearchEngine::Jina,
    SearchEngine::Tavily,
    SearchEngine::Ddg,
];

/// Tool for searching the web.
///
/// Engine priority is configurable via `with_priority()`.
/// Default: Jina → Tavily → DDG.
/// Requires JINA_API_KEY and/or TAVILY_API_KEY env vars.
pub struct WebSearchTool {
    client: reqwest::Client,
    jina_api_key: Option<String>,
    tavily_api_key: Option<String>,
    priority: Vec<SearchEngine>,
}

impl WebSearchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        let jina_api_key = std::env::var("JINA_API_KEY").ok().filter(|k| !k.is_empty());
        let tavily_api_key = std::env::var("TAVILY_API_KEY").ok().filter(|k| !k.is_empty());
        Self {
            client,
            jina_api_key,
            tavily_api_key,
            priority: DEFAULT_SEARCH_PRIORITY.to_vec(),
        }
    }

    /// Set custom engine priority order.
    pub fn with_priority(mut self, priority: Vec<SearchEngine>) -> Self {
        if !priority.is_empty() {
            self.priority = priority;
        }
        self
    }

    /// Parse priority from a list of engine name strings (e.g., from config).
    /// Unknown names are silently skipped.
    pub fn with_priority_strings(self, names: &[String]) -> Self {
        let engines: Vec<SearchEngine> = names
            .iter()
            .filter_map(|n| SearchEngine::from_str_loose(n))
            .collect();
        self.with_priority(engines)
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
        super::prompts::WEB_SEARCH_DESCRIPTION
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

        debug!(query, max_results, priority = ?self.priority, "performing web search");

        // Try engines in configured priority order with automatic fallback
        for (idx, engine) in self.priority.iter().enumerate() {
            let is_last = idx == self.priority.len() - 1;
            match engine {
                SearchEngine::Jina => {
                    if let Some(api_key) = &self.jina_api_key {
                        match self.search_jina(query, max_results, api_key).await {
                            Ok(output) => return Ok(output),
                            Err(e) if !is_last => {
                                warn!("Jina search failed, trying next engine: {e}");
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                SearchEngine::Tavily => {
                    if let Some(api_key) = &self.tavily_api_key {
                        match self.search_tavily(query, max_results, api_key).await {
                            Ok(output) => return Ok(output),
                            Err(e) if !is_last => {
                                warn!("Tavily search failed, trying next engine: {e}");
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                SearchEngine::Ddg => {
                    match self.search_ddg_instant(query, max_results).await {
                        Ok(output) => return Ok(output),
                        Err(e) if !is_last => {
                            warn!("DDG search failed, trying next engine: {e}");
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }

        // All engines skipped (no API keys configured)
        Ok(ToolOutput::success("No search engines available. Set JINA_API_KEY or TAVILY_API_KEY.".to_string()))
    }

    async fn execute_with_progress(
        &self,
        params: Value,
        ctx: &ToolContext,
        on_progress: Option<super::traits::ProgressCallback>,
    ) -> Result<ToolOutput> {
        if let Some(ref cb) = on_progress {
            let query = params["query"].as_str().unwrap_or("?");
            cb(ToolProgress::indeterminate(format!("searching: {query}")));
        }
        let result = self.execute(params, ctx).await;
        if let Some(ref cb) = on_progress {
            cb(ToolProgress::percent(1.0, "search complete"));
        }
        result
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl WebSearchTool {
    /// Search using Jina Search API (primary)
    async fn search_jina(
        &self,
        query: &str,
        max_results: usize,
        api_key: &str,
    ) -> Result<ToolOutput> {
        // Jina API limits num to 0..=20
        let jina_num = max_results.min(20);
        let body = json!({
            "q": query,
            "num": jina_num,
        });

        let response = self
            .client
            .post("https://s.jina.ai/")
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Jina request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Jina HTTP {status}: {text}"));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Jina JSON parse failed: {e}"))?;

        let mut output = String::new();

        if let Some(results) = data["data"].as_array() {
            if results.is_empty() {
                return Ok(ToolOutput::success("No search results found.".to_string()));
            }
            output.push_str("## Search Results\n\n");
            for (i, r) in results.iter().take(max_results).enumerate() {
                let title = r["title"].as_str().unwrap_or("(no title)");
                let url = r["url"].as_str().unwrap_or("");
                let description = r["description"].as_str().unwrap_or("");
                let content = r["content"].as_str().unwrap_or("");
                // Use description if available, otherwise first ~500 chars of content
                let snippet = if !description.is_empty() {
                    description.to_string()
                } else {
                    truncate_str(content, 500)
                };
                output.push_str(&format!(
                    "{}. **{}**\n   URL: {}\n   {}\n\n",
                    i + 1,
                    title,
                    url,
                    snippet
                ));
            }
        } else {
            return Ok(ToolOutput::success("No search results found.".to_string()));
        }

        Ok(ToolOutput::success(output.trim_end().to_string()))
    }

    /// Search using Tavily Search API (secondary)
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
            "search_depth": "advanced",
            "include_raw_content": false,
            "topic": "general",
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

        // Include direct answer prominently if available
        if let Some(answer) = data["answer"].as_str() {
            if !answer.is_empty() {
                output.push_str(&format!("## Direct Answer\n{answer}\n\n---\n\n"));
            }
        }

        // Format search results
        if let Some(results) = data["results"].as_array() {
            if results.is_empty() && output.is_empty() {
                return Ok(ToolOutput::success("No search results found.".to_string()));
            }
            if !results.is_empty() {
                output.push_str("## Search Results\n\n");
            }
            for (i, r) in results.iter().enumerate() {
                let title = r["title"].as_str().unwrap_or("(no title)");
                let url = r["url"].as_str().unwrap_or("");
                let content = r["content"].as_str().unwrap_or("");
                let score = r["score"].as_f64().map(|s| format!(" (relevance: {:.2})", s)).unwrap_or_default();
                output.push_str(&format!("{}. **{}**{}\n   URL: {}\n   {}\n\n", i + 1, title, score, url, content));
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

/// Truncate a string to at most `max_bytes` bytes at a valid UTF-8 char boundary.
fn truncate_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Find the last char boundary at or before max_bytes
    let end = s
        .char_indices()
        .take_while(|&(i, _)| i <= max_bytes)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}...", &s[..end])
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
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        };
        let result = tool.execute(json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing 'query'"));
    }

    #[test]
    fn test_api_keys_from_env() {
        // Test that constructor reads JINA_API_KEY and TAVILY_API_KEY without panic
        let tool = WebSearchTool::new();
        assert_eq!(tool.name(), "web_search");
        // Default priority: jina > tavily > ddg
        assert_eq!(tool.priority, vec![SearchEngine::Jina, SearchEngine::Tavily, SearchEngine::Ddg]);
    }

    #[test]
    fn test_custom_priority() {
        let tool = WebSearchTool::new()
            .with_priority(vec![SearchEngine::Tavily, SearchEngine::Jina]);
        assert_eq!(tool.priority, vec![SearchEngine::Tavily, SearchEngine::Jina]);
    }

    #[test]
    fn test_priority_from_strings() {
        let names = vec!["ddg".to_string(), "jina".to_string()];
        let tool = WebSearchTool::new().with_priority_strings(&names);
        assert_eq!(tool.priority, vec![SearchEngine::Ddg, SearchEngine::Jina]);
    }

    #[test]
    fn test_priority_from_strings_unknown_ignored() {
        let names = vec!["jina".to_string(), "google".to_string(), "tavily".to_string()];
        let tool = WebSearchTool::new().with_priority_strings(&names);
        // "google" is silently skipped
        assert_eq!(tool.priority, vec![SearchEngine::Jina, SearchEngine::Tavily]);
    }

    #[test]
    fn test_empty_priority_keeps_default() {
        let tool = WebSearchTool::new().with_priority(vec![]);
        assert_eq!(tool.priority, vec![SearchEngine::Jina, SearchEngine::Tavily, SearchEngine::Ddg]);
    }

    #[test]
    fn test_search_engine_from_str_loose() {
        assert_eq!(SearchEngine::from_str_loose("jina"), Some(SearchEngine::Jina));
        assert_eq!(SearchEngine::from_str_loose("TAVILY"), Some(SearchEngine::Tavily));
        assert_eq!(SearchEngine::from_str_loose("DuckDuckGo"), Some(SearchEngine::Ddg));
        assert_eq!(SearchEngine::from_str_loose("ddg"), Some(SearchEngine::Ddg));
        assert_eq!(SearchEngine::from_str_loose("google"), None);
    }
}
