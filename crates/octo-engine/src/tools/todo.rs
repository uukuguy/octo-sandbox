//! TodoWriteTool — lightweight todo list with full-replace semantics.
//!
//! Aligns with CC-OSS TodoWriteTool: each call replaces the entire list.
//! When all items are completed, the list auto-clears.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use super::traits::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    #[serde(default)]
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
}

/// Shared todo storage. Can be passed to multiple tool instances.
pub type TodoStore = Arc<Mutex<Vec<TodoItem>>>;

pub struct TodoWriteTool {
    store: TodoStore,
}

impl TodoWriteTool {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_store(store: TodoStore) -> Self {
        Self { store }
    }
}

impl Default for TodoWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Write/replace the current todo list. Each call replaces the entire list.\n\
         When all items are marked completed, the list auto-clears.\n\
         \n\
         ## Parameters\n\
         - todos (required): Array of todo items, each with:\n\
         - content (string): What needs to be done\n\
         - status (string): \"pending\" or \"completed\"\n\
         - id (string, optional): Identifier for tracking\n\
         \n\
         ## When to use\n\
         - Track progress on multi-step work within a session\n\
         - Lighter than task_create for ephemeral checklists\n\
         \n\
         ## Auto-clear\n\
         If all todos have status \"completed\", the list is emptied."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "What needs to be done"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "completed"],
                                "description": "Todo status"
                            },
                            "id": {
                                "type": "string",
                                "description": "Optional identifier"
                            }
                        },
                        "required": ["content", "status"]
                    },
                    "description": "Complete todo list (replaces current list)"
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let todos_raw = params
            .get("todos")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: todos"))?;

        let new_todos: Vec<TodoItem> = serde_json::from_value(todos_raw.clone())
            .map_err(|e| anyhow::anyhow!("Invalid todos format: {}", e))?;

        // Assign IDs to items that don't have one
        let new_todos: Vec<TodoItem> = new_todos
            .into_iter()
            .enumerate()
            .map(|(i, mut item)| {
                if item.id.is_empty() {
                    item.id = format!("todo-{}", i + 1);
                }
                item
            })
            .collect();

        let mut store = self.store.lock().await;
        let old_todos = store.clone();

        // Auto-clear: if all completed, clear the list
        let all_completed = !new_todos.is_empty()
            && new_todos.iter().all(|t| t.status == TodoStatus::Completed);

        let final_todos = if all_completed {
            Vec::new()
        } else {
            new_todos.clone()
        };

        *store = final_todos.clone();

        let result = json!({
            "old_todos": old_todos,
            "new_todos": final_todos,
            "all_completed": all_completed,
        });
        Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_todo_write_stores_list() {
        let tool = TodoWriteTool::new();
        let result = tool
            .execute(
                json!({
                    "todos": [
                        {"content": "Fix bug", "status": "pending"},
                        {"content": "Write tests", "status": "pending"},
                        {"content": "Deploy", "status": "pending"}
                    ]
                }),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("Fix bug"));
        assert!(result.content.contains("Write tests"));
        assert!(result.content.contains("Deploy"));
    }

    #[tokio::test]
    async fn test_todo_write_replaces_list() {
        let tool = TodoWriteTool::new();
        let ctx = test_ctx();

        // First write
        tool.execute(
            json!({"todos": [{"content": "A", "status": "pending"}]}),
            &ctx,
        )
        .await
        .unwrap();

        // Second write replaces
        let result = tool
            .execute(
                json!({"todos": [{"content": "B", "status": "pending"}]}),
                &ctx,
            )
            .await
            .unwrap();

        // old_todos should contain A
        assert!(result.content.contains("\"content\": \"A\""));
        // new_todos should contain B
        assert!(result.content.contains("\"content\": \"B\""));
    }

    #[tokio::test]
    async fn test_todo_all_completed_clears() {
        let tool = TodoWriteTool::new();
        let result = tool
            .execute(
                json!({
                    "todos": [
                        {"content": "Done 1", "status": "completed"},
                        {"content": "Done 2", "status": "completed"}
                    ]
                }),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("\"all_completed\": true"));
        assert!(result.content.contains("\"new_todos\": []"));
    }

    #[tokio::test]
    async fn test_todo_auto_assigns_ids() {
        let tool = TodoWriteTool::new();
        let result = tool
            .execute(
                json!({"todos": [{"content": "Test", "status": "pending", "id": ""}]}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("todo-1"));
    }

    #[tokio::test]
    async fn test_todo_preserves_custom_ids() {
        let tool = TodoWriteTool::new();
        let result = tool
            .execute(
                json!({"todos": [{"content": "Test", "status": "pending", "id": "my-id"}]}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("my-id"));
    }
}
