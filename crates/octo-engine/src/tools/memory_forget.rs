use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{MemoryCategory, MemoryFilter, MemoryId, ToolContext, ToolResult, ToolSource};

use crate::memory::store_traits::MemoryStore;

use super::traits::Tool;

pub struct MemoryForgetTool {
    store: Arc<dyn MemoryStore>,
}

impl MemoryForgetTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryForgetTool {
    fn name(&self) -> &str {
        "memory_forget"
    }

    fn description(&self) -> &str {
        "Delete one or more memories by ID or by filter criteria (category). Use to remove outdated or incorrect information."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Delete a single memory by ID"
                },
                "category": {
                    "type": "string",
                    "enum": ["profile", "preferences", "tools", "debug", "patterns"],
                    "description": "Delete all memories in this category"
                }
            }
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let id = params["id"].as_str();
        let category = params["category"].as_str();

        if id.is_none() && category.is_none() {
            return Ok(ToolResult::error(
                "At least one of 'id' or 'category' must be provided".to_string(),
            ));
        }

        // Single delete by ID
        if let Some(id_str) = id {
            let mem_id = MemoryId::from_string(id_str);
            let existing = self.store.get(&mem_id).await?;
            if existing.is_none() {
                return Ok(ToolResult::error(format!(
                    "Memory with id '{id_str}' not found"
                )));
            }
            self.store.delete(&mem_id).await?;
            debug!(id = id_str, "Forgot memory");
            return Ok(ToolResult::success(format!("Deleted memory {id_str}")));
        }

        // Bulk delete by category
        if let Some(cat_str) = category {
            let cat = MemoryCategory::parse(cat_str)
                .ok_or_else(|| anyhow::anyhow!("invalid category: {cat_str}"))?;

            let filter = MemoryFilter {
                user_id: "default".to_string(),
                categories: Some(vec![cat]),
                ..Default::default()
            };

            let count = self.store.delete_by_filter(filter).await?;
            debug!(category = cat_str, count, "Forgot memories by category");
            return Ok(ToolResult::success(format!(
                "Deleted {count} memories in category '{cat_str}'"
            )));
        }

        Ok(ToolResult::error(
            "No valid delete criteria provided".to_string(),
        ))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
