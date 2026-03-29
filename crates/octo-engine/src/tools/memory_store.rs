use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{
    MemoryCategory, MemoryEntry, MemorySource, SearchOptions, ToolContext, ToolOutput, ToolSource,
};

use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;

use super::traits::Tool;

/// Similarity threshold for conflict detection (0.0–1.0).
/// Entries scoring above this against the new content are considered conflicts.
const CONFLICT_SCORE_THRESHOLD: f32 = 0.7;

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
        "Store information in long-term memory. Automatically detects similar existing memories and handles conflicts via on_conflict strategy (replace/skip/force)."
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
                },
                "on_conflict": {
                    "type": "string",
                    "enum": ["replace", "skip", "force"],
                    "description": "Conflict resolution: replace=update existing, skip=keep existing, force=always insert new (default: replace)"
                }
            },
            "required": ["content", "category"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
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

        let on_conflict = params["on_conflict"].as_str().unwrap_or("replace");

        // Try to generate embedding (best-effort)
        let embedding = match self.provider.embed(&[content.to_string()]).await {
            Ok(mut embeddings) if !embeddings.is_empty() => Some(embeddings.remove(0)),
            _ => {
                debug!("Embedding not available, storing without vector");
                None
            }
        };

        // Conflict detection (skip if on_conflict=force)
        if on_conflict != "force" {
            let search_opts = SearchOptions {
                user_id: "default".to_string(),
                limit: 3,
                categories: Some(vec![category.clone()]),
                query_embedding: embedding.clone(),
                min_score: Some(CONFLICT_SCORE_THRESHOLD),
                ..Default::default()
            };

            let similar = self.store.search(content, search_opts).await?;
            if let Some(best) = similar.first() {
                let existing_id = best.entry.id.to_string();
                let existing_score = best.score;

                match on_conflict {
                    "skip" => {
                        debug!(
                            existing_id = %existing_id,
                            score = existing_score,
                            "Conflict detected, skipping (on_conflict=skip)"
                        );
                        return Ok(ToolOutput::success(format!(
                            "Similar memory already exists (id: {existing_id}, similarity: {existing_score:.2}). Skipped."
                        )));
                    }
                    _ => {
                        // "replace" — update existing entry
                        debug!(
                            existing_id = %existing_id,
                            score = existing_score,
                            "Conflict detected, replacing existing memory"
                        );
                        self.store.update(&best.entry.id, content).await?;
                        return Ok(ToolOutput::success(format!(
                            "Updated existing memory {existing_id} (similarity: {existing_score:.2})"
                        )));
                    }
                }
            }
        }

        // No conflict or force mode — insert new
        let mut entry = MemoryEntry::new("default", category, content);
        entry.importance = importance;
        entry.source_type = MemorySource::Manual;
        entry.embedding = embedding;

        let id = self.store.store(entry).await?;

        Ok(ToolOutput::success(format!("Stored memory with id: {id}")))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
