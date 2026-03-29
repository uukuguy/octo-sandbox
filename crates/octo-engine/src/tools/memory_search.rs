use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{SearchOptions, ToolContext, ToolOutput, ToolSource};

use crate::memory::hybrid_query::HybridQueryEngine;
use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;

use super::traits::Tool;

pub struct MemorySearchTool {
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
    hybrid_engine: Option<Arc<HybridQueryEngine>>,
}

impl MemorySearchTool {
    pub fn new(store: Arc<dyn MemoryStore>, provider: Arc<dyn Provider>) -> Self {
        Self {
            store,
            provider,
            hybrid_engine: None,
        }
    }

    /// Create with an optional HybridQueryEngine for enhanced semantic search.
    pub fn with_hybrid_engine(
        store: Arc<dyn MemoryStore>,
        provider: Arc<dyn Provider>,
        hybrid_engine: Arc<HybridQueryEngine>,
    ) -> Self {
        Self {
            store,
            provider,
            hybrid_engine: Some(hybrid_engine),
        }
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
            query_embedding: query_embedding.clone(),
            ..Default::default()
        };

        let results = self.store.search(query, opts).await?;

        // Merge HybridQueryEngine results if available
        let mut seen_ids: HashSet<String> = results.iter().map(|r| r.entry.id.to_string()).collect();
        let mut hybrid_extra = Vec::new();

        if let Some(engine) = &self.hybrid_engine {
            let emb_ref = query_embedding.as_deref();
            match engine.search(query, emb_ref, limit).await {
                Ok(hybrid_results) => {
                    for hr in hybrid_results {
                        if seen_ids.insert(hr.id.clone()) {
                            hybrid_extra.push(hr);
                        }
                    }
                    if !hybrid_extra.is_empty() {
                        debug!(
                            extra = hybrid_extra.len(),
                            "HybridQueryEngine contributed additional results"
                        );
                    }
                }
                Err(e) => {
                    debug!(error = %e, "HybridQueryEngine search failed, using store results only");
                }
            }
        }

        if results.is_empty() && hybrid_extra.is_empty() {
            return Ok(ToolOutput::success("No memories found.".to_string()));
        }

        let total = results.len() + hybrid_extra.len();
        let mut output = format!("Found {} memories:\n\n", total);
        let mut idx = 1;

        for r in results.iter() {
            output.push_str(&format!(
                "{}. [{}] (score: {:.2}, category: {})\n   {}\n\n",
                idx,
                r.entry.id,
                r.score,
                r.entry.category.as_str(),
                r.entry.content,
            ));
            idx += 1;
        }

        for hr in hybrid_extra.iter() {
            output.push_str(&format!(
                "{}. [{}] (score: {:.2}, source: {})\n   {}\n\n",
                idx, hr.id, hr.score, hr.source, hr.content,
            ));
            idx += 1;
        }

        Ok(ToolOutput::success(output))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
