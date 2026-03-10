use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{MemoryId, SearchOptions, ToolContext, ToolOutput, ToolSource};

use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;

use super::traits::Tool;

pub struct MemoryRecallTool {
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
}

impl MemoryRecallTool {
    pub fn new(store: Arc<dyn MemoryStore>, provider: Arc<dyn Provider>) -> Self {
        Self { store, provider }
    }
}

#[async_trait]
impl Tool for MemoryRecallTool {
    fn name(&self) -> &str {
        "memory_recall"
    }

    fn description(&self) -> &str {
        "Recall a specific memory by its ID, optionally including related memories found via semantic similarity."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The memory ID to recall"
                },
                "include_related": {
                    "type": "boolean",
                    "description": "Whether to include semantically related memories (default: false)"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let id_str = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'id' parameter"))?;

        let include_related = params["include_related"].as_bool().unwrap_or(false);

        let id = MemoryId::from_string(id_str);
        let entry = self.store.get(&id).await?;

        let entry = match entry {
            Some(e) => e,
            None => {
                return Ok(ToolOutput::error(format!(
                    "Memory with id '{id_str}' not found"
                )));
            }
        };

        let mut output = format!(
            "Memory [{}]:\n  Category: {}\n  Importance: {:.2}\n  Source: {}\n  Created: {}\n  Content: {}\n",
            entry.id,
            entry.category.as_str(),
            entry.importance,
            entry.source_type.as_str(),
            entry.timestamps.created_at,
            entry.content,
        );

        if include_related {
            let query_embedding = match self
                .provider
                .embed(std::slice::from_ref(&entry.content))
                .await
            {
                Ok(mut embeddings) if !embeddings.is_empty() => Some(embeddings.remove(0)),
                _ => {
                    debug!("Embedding not available for related search, using FTS");
                    None
                }
            };

            let opts = SearchOptions {
                user_id: entry.user_id.clone(),
                limit: 5,
                query_embedding,
                ..Default::default()
            };

            let related = self.store.search(&entry.content, opts).await?;
            let related: Vec<_> = related
                .into_iter()
                .filter(|r| r.entry.id.as_str() != id_str)
                .collect();

            if !related.is_empty() {
                output.push_str(&format!("\nRelated memories ({}):\n", related.len()));
                for (i, r) in related.iter().enumerate() {
                    output.push_str(&format!(
                        "  {}. [{}] (score: {:.2}) {}\n",
                        i + 1,
                        r.entry.id,
                        r.score,
                        r.entry.content,
                    ));
                }
            }
        }

        Ok(ToolOutput::success(output))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
