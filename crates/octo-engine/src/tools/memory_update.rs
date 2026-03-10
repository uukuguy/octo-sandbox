use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use octo_types::{MemoryId, ToolContext, ToolOutput, ToolSource};

use crate::memory::store_traits::MemoryStore;

use super::traits::Tool;

pub struct MemoryUpdateTool {
    store: Arc<dyn MemoryStore>,
}

impl MemoryUpdateTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryUpdateTool {
    fn name(&self) -> &str {
        "memory_update"
    }

    fn description(&self) -> &str {
        "Update the content of an existing memory entry by its ID."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The memory ID to update"
                },
                "content": {
                    "type": "string",
                    "description": "The new content for the memory"
                }
            },
            "required": ["id", "content"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let id_str = params["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'id' parameter"))?;

        let content = params["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' parameter"))?;

        let id = MemoryId::from_string(id_str);

        // Check if memory exists
        let existing = self.store.get(&id).await?;
        if existing.is_none() {
            return Ok(ToolOutput::error(format!(
                "Memory with id '{id_str}' not found"
            )));
        }

        self.store.update(&id, content).await?;

        Ok(ToolOutput::success(format!("Updated memory {id_str}")))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}
