use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{MemoryCategory, MemoryEntry, MemorySource, ToolContext, ToolResult, ToolSource};

use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;

use super::traits::Tool;

pub struct MemoryStoreTool {
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
}

impl MemoryStoreTool {
    pub fn new(store: Arc<dyn MemoryStore>, provider: Arc<dyn Provider>) -> Self {
        Self { store, provider }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Store a piece of information in long-term memory for future retrieval. Use this to remember important facts, user preferences, patterns, or debugging insights."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The information to store"
                },
                "category": {
                    "type": "string",
                    "enum": ["profile", "preferences", "tools", "debug", "patterns"],
                    "description": "Category of the memory"
                },
                "importance": {
                    "type": "number",
                    "description": "Importance score from 0.0 to 1.0 (default: 0.5)"
                }
            },
            "required": ["content", "category"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let content = params["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' parameter"))?;

        let category_str = params["category"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'category' parameter"))?;

        let category = MemoryCategory::parse(category_str)
            .ok_or_else(|| anyhow::anyhow!("invalid category: {category_str}"))?;

        let importance = params["importance"]
            .as_f64()
            .map(|f| (f as f32).clamp(0.0, 1.0))
            .unwrap_or(0.5);

        // Try to generate embedding (best-effort)
        let embedding = match self.provider.embed(&[content.to_string()]).await {
            Ok(mut embeddings) if !embeddings.is_empty() => Some(embeddings.remove(0)),
            _ => {
                debug!("Embedding not available, storing without vector");
                None
            }
        };

        let mut entry = MemoryEntry::new("default", category, content);
        entry.importance = importance;
        entry.source_type = MemorySource::Manual;
        entry.embedding = embedding;

        let id = self.store.store(entry).await?;

        Ok(ToolResult::success(format!("Stored memory with id: {id}")))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
