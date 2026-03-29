//! memory_edit tool — allows agents to update, append to, or clear Working Memory blocks.
//!
//! Follows the Letta/MemGPT pattern where agents actively manage their own
//! context by editing working memory blocks. This enables persistent state
//! that survives across conversation turns within a session.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use octo_types::{SandboxId, ToolContext, ToolOutput, ToolSource, UserId};

use crate::memory::WorkingMemory;

use super::traits::Tool;

pub struct MemoryEditTool {
    memory: Arc<dyn WorkingMemory>,
}

impl MemoryEditTool {
    pub fn new(memory: Arc<dyn WorkingMemory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryEditTool {
    fn name(&self) -> &str {
        "memory_edit"
    }

    fn description(&self) -> &str {
        "Edit Working Memory blocks (user_profile, task_context, or custom blocks). \
         Use to update context that persists across conversation turns. \
         Actions: update (replace content), append (add to existing), clear (empty block)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["update", "append", "clear"],
                    "description": "Action to perform: update (replace), append (add), clear (empty)"
                },
                "block": {
                    "type": "string",
                    "description": "Block ID: user_profile, task_context, or custom:{name}"
                },
                "content": {
                    "type": "string",
                    "description": "New content (required for update/append, ignored for clear)"
                }
            },
            "required": ["action", "block"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'action' parameter"))?;

        let block_id = params["block"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'block' parameter"))?;

        let content = params["content"].as_str().unwrap_or("");

        // Check if the block exists and is not readonly
        let blocks = self
            .memory
            .get_blocks(&UserId::default(), &SandboxId::default())
            .await?;

        let existing = blocks.iter().find(|b| b.id == block_id);

        // Validate readonly
        if let Some(block) = existing {
            if block.is_readonly {
                return Ok(ToolOutput::error(format!(
                    "Block '{}' is read-only and cannot be edited.",
                    block_id
                )));
            }
        }

        match action {
            "update" => {
                if content.is_empty() {
                    return Ok(ToolOutput::error(
                        "Content is required for 'update' action.".to_string(),
                    ));
                }
                self.memory.update_block(block_id, content).await?;
                Ok(ToolOutput::success(format!(
                    "Block '{}' updated ({} chars).",
                    block_id,
                    content.len()
                )))
            }
            "append" => {
                if content.is_empty() {
                    return Ok(ToolOutput::error(
                        "Content is required for 'append' action.".to_string(),
                    ));
                }
                let current_value = existing
                    .map(|b| b.value.clone())
                    .unwrap_or_default();
                let new_value = if current_value.is_empty() {
                    content.to_string()
                } else {
                    format!("{}\n{}", current_value, content)
                };
                self.memory.update_block(block_id, &new_value).await?;
                Ok(ToolOutput::success(format!(
                    "Appended to block '{}' (now {} chars).",
                    block_id,
                    new_value.len()
                )))
            }
            "clear" => {
                self.memory.update_block(block_id, "").await?;
                Ok(ToolOutput::success(format!("Block '{}' cleared.", block_id)))
            }
            other => Ok(ToolOutput::error(format!(
                "Unknown action '{}'. Use: update, append, clear.",
                other
            ))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::InMemoryWorkingMemory;

    fn make_tool() -> MemoryEditTool {
        MemoryEditTool::new(Arc::new(InMemoryWorkingMemory::new()))
    }

    fn ctx() -> ToolContext {
        ToolContext {
            sandbox_id: SandboxId::default(),
            working_dir: std::path::PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_update_block() {
        let tool = make_tool();
        let result = tool
            .execute(
                json!({"action": "update", "block": "user_profile", "content": "Likes Rust"}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("updated"));

        // Verify the block was updated
        let blocks = tool
            .memory
            .get_blocks(&UserId::default(), &SandboxId::default())
            .await
            .unwrap();
        let profile = blocks.iter().find(|b| b.id == "user_profile").unwrap();
        assert_eq!(profile.value, "Likes Rust");
    }

    #[tokio::test]
    async fn test_append_block() {
        let tool = make_tool();
        // First update
        tool.execute(
            json!({"action": "update", "block": "task_context", "content": "Line 1"}),
            &ctx(),
        )
        .await
        .unwrap();

        // Then append
        let result = tool
            .execute(
                json!({"action": "append", "block": "task_context", "content": "Line 2"}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("Appended"));

        let blocks = tool
            .memory
            .get_blocks(&UserId::default(), &SandboxId::default())
            .await
            .unwrap();
        let task = blocks.iter().find(|b| b.id == "task_context").unwrap();
        assert_eq!(task.value, "Line 1\nLine 2");
    }

    #[tokio::test]
    async fn test_clear_block() {
        let tool = make_tool();
        // Set content first
        tool.execute(
            json!({"action": "update", "block": "user_profile", "content": "Some data"}),
            &ctx(),
        )
        .await
        .unwrap();

        // Clear it
        let result = tool
            .execute(
                json!({"action": "clear", "block": "user_profile"}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("cleared"));

        let blocks = tool
            .memory
            .get_blocks(&UserId::default(), &SandboxId::default())
            .await
            .unwrap();
        let profile = blocks.iter().find(|b| b.id == "user_profile").unwrap();
        assert!(profile.value.is_empty());
    }

    #[tokio::test]
    async fn test_update_empty_content_error() {
        let tool = make_tool();
        let result = tool
            .execute(
                json!({"action": "update", "block": "user_profile", "content": ""}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_unknown_action_error() {
        let tool = make_tool();
        let result = tool
            .execute(
                json!({"action": "delete", "block": "user_profile"}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_append_to_empty_block() {
        let tool = make_tool();
        let result = tool
            .execute(
                json!({"action": "append", "block": "task_context", "content": "First line"}),
                &ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("Appended"));

        let blocks = tool
            .memory
            .get_blocks(&UserId::default(), &SandboxId::default())
            .await
            .unwrap();
        let task = blocks.iter().find(|b| b.id == "task_context").unwrap();
        assert_eq!(task.value, "First line");
    }
}
