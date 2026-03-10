use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{SearchOptions, ToolContext, ToolOutput, ToolSource};

use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;

use super::traits::Tool;

pub struct MemorySearchTool {
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
}

impl MemorySearchTool {
    pub fn new(store: Arc<dyn MemoryStore>, provider: Arc<dyn Provider>) -> Self {
        Self { store, provider }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant information. Uses hybrid full-text and semantic search."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 10)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?;

        let limit = params["limit"].as_u64().unwrap_or(10) as usize;

        // Try to generate query embedding (best-effort)
        let query_embedding = match self.provider.embed(&[query.to_string()]).await {
            Ok(mut embeddings) if !embeddings.is_empty() => Some(embeddings.remove(0)),
            _ => {
                debug!("Query embedding not available, using FTS-only search");
                None
            }
        };

        let opts = SearchOptions {
            user_id: "default".to_string(),
            limit,
            query_embedding,
            ..Default::default()
        };

        let results = self.store.search(query, opts).await?;

        if results.is_empty() {
            return Ok(ToolOutput::success("No memories found.".to_string()));
        }

        let mut output = format!("Found {} memories:\n\n", results.len());
        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!(
                "{}. [{}] (score: {:.2}, category: {})\n   {}\n\n",
                i + 1,
                r.entry.id,
                r.score,
                r.entry.category.as_str(),
                r.entry.content,
            ));
        }

        Ok(ToolOutput::success(output))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
