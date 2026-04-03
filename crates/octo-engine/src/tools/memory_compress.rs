//! Memory compression tool — summarizes multiple memories in a category into a single condensed entry.
//!
//! Loads all memories matching the given category, sends them to an LLM for summarization,
//! deletes the originals, and stores the compressed summary as a new memory entry.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{
    ChatMessage, CompletionRequest, ContentBlock, MemoryCategory, MemoryEntry, MemoryFilter,
    MemorySource, ToolContext, ToolOutput, ToolProgress, ToolSource,
};

use crate::memory::store_traits::MemoryStore;
use crate::providers::Provider;

use super::traits::Tool;

pub struct MemoryCompressTool {
    store: Arc<dyn MemoryStore>,
    provider: Arc<dyn Provider>,
}

impl MemoryCompressTool {
    pub fn new(store: Arc<dyn MemoryStore>, provider: Arc<dyn Provider>) -> Self {
        Self { store, provider }
    }
}

#[async_trait]
impl Tool for MemoryCompressTool {
    fn name(&self) -> &str {
        "memory_compress"
    }

    fn description(&self) -> &str {
        "Compress multiple memories in a category into a single summary. Reduces memory count while preserving key information. Use when a category has accumulated many entries."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "enum": ["profile", "preferences", "tools", "debug", "patterns"],
                    "description": "Category of memories to compress"
                },
                "max_entries": {
                    "type": "integer",
                    "description": "Only compress if category has more than this many entries (default: 5)"
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Preview the summary without deleting originals (default: false)"
                }
            },
            "required": ["category"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let category_str = params["category"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'category' parameter"))?;

        let category = MemoryCategory::parse(category_str)
            .ok_or_else(|| anyhow::anyhow!("invalid category: {category_str}"))?;

        let max_entries = params["max_entries"].as_u64().unwrap_or(5) as usize;
        let dry_run = params["dry_run"].as_bool().unwrap_or(false);

        // Load all memories in this category
        let filter = MemoryFilter {
            user_id: ctx.user_id.as_str().to_string(),
            categories: Some(vec![category.clone()]),
            limit: 200,
            ..Default::default()
        };

        let entries = self.store.list(filter).await?;

        if entries.len() <= max_entries {
            return Ok(ToolOutput::success(format!(
                "Category '{}' has {} entries (threshold: {}). No compression needed.",
                category_str,
                entries.len(),
                max_entries,
            )));
        }

        // Build content for LLM summarization
        let mut input_text = String::new();
        for (i, entry) in entries.iter().enumerate() {
            input_text.push_str(&format!(
                "{}. [importance={:.2}] {}\n",
                i + 1,
                entry.importance,
                entry.content,
            ));
        }

        let prompt = format!(
            "Summarize the following {} memory entries from the '{}' category into a single concise summary.\n\
             Preserve all important facts, preferences, and patterns. Remove duplicates and outdated information.\n\
             Output ONLY the summary text, no preamble.\n\n\
             Entries:\n{}",
            entries.len(),
            category_str,
            input_text,
        );

        let request = CompletionRequest {
            model: String::new(), // use default
            system: Some("You are a memory compression assistant. Produce a concise, factual summary.".into()),
            messages: vec![ChatMessage::user(prompt)],
            max_tokens: 2048,
            temperature: Some(0.0),
            ..Default::default()
        };

        let response = self.provider.complete(request).await?;
        let summary = response
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();

        if summary.trim().is_empty() {
            return Ok(ToolOutput::error(
                "LLM returned empty summary, compression aborted.".to_string(),
            ));
        }

        if dry_run {
            return Ok(ToolOutput::success(format!(
                "[dry_run] Would compress {} entries in '{}' into:\n\n{}\n\n({} originals would be deleted)",
                entries.len(),
                category_str,
                summary.trim(),
                entries.len(),
            )));
        }

        // Delete originals
        let mut deleted = 0;
        for entry in &entries {
            if self.store.delete(&entry.id).await.is_ok() {
                deleted += 1;
            }
        }

        // Compute max importance from originals
        let max_importance = entries
            .iter()
            .map(|e| e.importance)
            .fold(0.0_f32, f32::max);

        // Generate embedding for the summary
        let embedding = match self.provider.embed(&[summary.clone()]).await {
            Ok(mut embeddings) if !embeddings.is_empty() => Some(embeddings.remove(0)),
            _ => None,
        };

        // Store compressed summary
        let mut compressed = MemoryEntry::new(ctx.user_id.as_str(), category, summary.trim());
        compressed.importance = max_importance;
        compressed.source_type = MemorySource::Extracted;
        compressed.embedding = embedding;

        let new_id = self.store.store(compressed).await?;

        debug!(
            category = category_str,
            original_count = entries.len(),
            deleted,
            new_id = %new_id,
            "Memory compression complete"
        );

        Ok(ToolOutput::success(format!(
            "Compressed {} memories in '{}' into 1 summary (id: {new_id}). Deleted {deleted} originals.",
            entries.len(),
            category_str,
        )))
    }

    async fn execute_with_progress(
        &self,
        params: Value,
        ctx: &ToolContext,
        on_progress: Option<super::traits::ProgressCallback>,
    ) -> Result<ToolOutput> {
        if let Some(ref cb) = on_progress {
            let cat = params["category"].as_str().unwrap_or("?");
            cb(ToolProgress::indeterminate(format!("compressing '{cat}' memories...")));
        }
        let result = self.execute(params, ctx).await;
        if let Some(ref cb) = on_progress {
            cb(ToolProgress::percent(1.0, "compression complete"));
        }
        result
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
