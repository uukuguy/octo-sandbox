//! LLM tools for task management: task_create, task_update, task_list.
//!
//! These tools allow agents to track progress on complex multi-step work,
//! coordinate task ownership across sessions, and maintain a shared task board.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde_json::json;

use crate::agent::task_tracker::{TaskStatus, TaskTracker};
use crate::tools::traits::Tool;

// ─── task_create ────────────────────────────────────────────────────────────

pub struct TaskCreateTool {
    tracker: Arc<TaskTracker>,
}

impl TaskCreateTool {
    pub fn new(tracker: Arc<TaskTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "task_create"
    }

    fn description(&self) -> &str {
        r#"Create a tracked task to manage progress on multi-step work.

## When to use
- You receive a request with 2+ distinct deliverables
- You want to coordinate work ownership across multiple sessions/agents
- You need a shared task board visible to all agents in a team

## Parameters
- subject (required): Short title for the task (< 80 chars)
- description (required): What needs to be done, acceptance criteria
- team (optional): Team name to associate the task with

## Returns
The created task object with a unique ID (task-N).

## Example
{"subject": "Implement auth middleware", "description": "Add JWT validation to /api routes", "team": "backend"}

## Anti-patterns
- Don't create tasks for trivial one-step operations
- Don't create duplicate tasks — check task_list first
- Don't leave tasks in pending forever — update status as you work"#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "Short title for the task"
                },
                "description": {
                    "type": "string",
                    "description": "What needs to be done"
                },
                "team": {
                    "type": "string",
                    "description": "Team name to associate with (optional)"
                }
            },
            "required": ["subject", "description"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let subject = params["subject"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: subject"))?;
        let description = params["description"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: description"))?;
        let team = params["team"].as_str();

        let task = self.tracker.create(subject, description, team);
        Ok(ToolOutput::success(serde_json::to_string_pretty(&task)?))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── task_update ────────────────────────────────────────────────────────────

pub struct TaskUpdateTool {
    tracker: Arc<TaskTracker>,
}

impl TaskUpdateTool {
    pub fn new(tracker: Arc<TaskTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "task_update"
    }

    fn description(&self) -> &str {
        r#"Update a tracked task's status or owner.

## When to use
- Starting work on a task → status: "in_progress"
- Finished a task → status: "completed"
- Blocked by a dependency → status: "blocked"
- Assigning/reassigning ownership

## Parameters
- task_id (required): The task ID (e.g., "task-1")
- status (optional): New status — "pending", "in_progress", "completed", "blocked"
- owner (optional): Agent name or session ID that owns this task

## Example
{"task_id": "task-1", "status": "in_progress", "owner": "coder-session-abc"}"#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to update"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "blocked"],
                    "description": "New task status"
                },
                "owner": {
                    "type": "string",
                    "description": "Agent name or session ID that owns the task"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task_id"))?;
        let status = params["status"]
            .as_str()
            .and_then(TaskStatus::from_str_opt);
        let owner = params["owner"].as_str();

        match self.tracker.update(task_id, status, owner) {
            Ok(task) => Ok(ToolOutput::success(serde_json::to_string_pretty(&task)?)),
            Err(e) => Ok(ToolOutput::error(e)),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── task_list ──────────────────────────────────────────────────────────────

pub struct TaskListTool {
    tracker: Arc<TaskTracker>,
}

impl TaskListTool {
    pub fn new(tracker: Arc<TaskTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn description(&self) -> &str {
        r#"List all tracked tasks, optionally filtered by team.

## When to use
- Before creating tasks (check for duplicates)
- To review progress across all tasks
- To find tasks assigned to a specific team

## Parameters
- team (optional): Filter by team name

## Returns
Array of task objects with id, subject, status, owner, team."#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "team": {
                    "type": "string",
                    "description": "Filter by team name"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let team = params["team"].as_str();
        let tasks = self.tracker.list(team);
        let (pending, in_progress, completed, blocked) = self.tracker.count_by_status();
        let result = json!({
            "tasks": tasks,
            "summary": {
                "total": tasks.len(),
                "pending": pending,
                "in_progress": in_progress,
                "completed": completed,
                "blocked": blocked,
            }
        });
        Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── task_get ──────────────────────────────────────────────────────────────

pub struct TaskGetTool {
    tracker: Arc<TaskTracker>,
}

impl TaskGetTool {
    pub fn new(tracker: Arc<TaskTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "task_get"
    }

    fn description(&self) -> &str {
        r#"Retrieve a single task by its ID.

## Parameters
- task_id (required): The task ID (e.g., "task-1")

## Returns
The task object with id, subject, description, status, owner, team.
Returns null if the task does not exist."#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to retrieve"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task_id"))?;

        match self.tracker.get(task_id) {
            Some(task) => Ok(ToolOutput::success(serde_json::to_string_pretty(&task)?)),
            None => Ok(ToolOutput::success("null".to_string())),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── task_stop ─────────────────────────────────────────────────────────────

pub struct TaskStopTool {
    tracker: Arc<TaskTracker>,
}

impl TaskStopTool {
    pub fn new(tracker: Arc<TaskTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "task_stop"
    }

    fn description(&self) -> &str {
        r#"Stop/cancel a running or pending task.

## Parameters
- task_id (required): The task ID to stop (e.g., "task-1")

## Behavior
- Cancels tasks in pending, in_progress, or blocked status
- Cannot cancel already completed or cancelled tasks
- Returns the updated task with status "cancelled""#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to stop"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task_id"))?;

        match self.tracker.cancel(task_id) {
            Ok(task) => Ok(ToolOutput::success(serde_json::to_string_pretty(&task)?)),
            Err(e) => Ok(ToolOutput::error(e)),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

// ─── task_output ───────────────────────────────────────────────────────────

pub struct TaskOutputTool {
    tracker: Arc<TaskTracker>,
}

impl TaskOutputTool {
    pub fn new(tracker: Arc<TaskTracker>) -> Self {
        Self { tracker }
    }
}

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "task_output"
    }

    fn description(&self) -> &str {
        r#"Retrieve a task's current status and details as structured output.

## Parameters
- task_id (required): The task ID to retrieve output for

## Returns
Task object with id, status, subject, description, owner, team, timestamps."#
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to get output for"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: task_id"))?;

        match self.tracker.get(task_id) {
            Some(task) => {
                let output = json!({
                    "task_id": task.id,
                    "status": task.status,
                    "subject": task.subject,
                    "description": task.description,
                    "owner": task.owner,
                    "team": task.team,
                    "created_at": task.created_at,
                    "updated_at": task.updated_at,
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&output)?))
            }
            None => Ok(ToolOutput::error(format!("Task '{}' not found", task_id))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "coordination"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::{SandboxId, ToolContext};
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: SandboxId::from_string("test"),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[tokio::test]
    async fn test_task_create_tool() {
        let tracker = Arc::new(TaskTracker::new());
        let tool = TaskCreateTool::new(tracker.clone());
        let result = tool
            .execute(
                json!({"subject": "Test", "description": "Test desc"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("task-1"));
        assert!(result.content.contains("pending"));
    }

    #[tokio::test]
    async fn test_task_update_tool() {
        let tracker = Arc::new(TaskTracker::new());
        tracker.create("Test", "desc", None);
        let tool = TaskUpdateTool::new(tracker);
        let result = tool
            .execute(
                json!({"task_id": "task-1", "status": "in_progress", "owner": "agent-1"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("in_progress"));
        assert!(result.content.contains("agent-1"));
    }

    #[tokio::test]
    async fn test_task_list_tool() {
        let tracker = Arc::new(TaskTracker::new());
        tracker.create("A", "a", Some("team-x"));
        tracker.create("B", "b", None);
        let tool = TaskListTool::new(tracker);
        let result = tool
            .execute(json!({}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("\"total\": 2"));
    }

    #[tokio::test]
    async fn test_task_get_found() {
        let tracker = Arc::new(TaskTracker::new());
        tracker.create("Test", "desc", None);
        let tool = TaskGetTool::new(tracker);
        let result = tool
            .execute(json!({"task_id": "task-1"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("task-1"));
        assert!(result.content.contains("Test"));
    }

    #[tokio::test]
    async fn test_task_get_not_found() {
        let tracker = Arc::new(TaskTracker::new());
        let tool = TaskGetTool::new(tracker);
        let result = tool
            .execute(json!({"task_id": "task-999"}), &test_ctx())
            .await
            .unwrap();
        assert_eq!(result.content, "null");
    }

    #[tokio::test]
    async fn test_task_stop_cancels() {
        let tracker = Arc::new(TaskTracker::new());
        tracker.create("Test", "desc", None);
        let tool = TaskStopTool::new(tracker);
        let result = tool
            .execute(json!({"task_id": "task-1"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("cancelled"));
    }

    #[tokio::test]
    async fn test_task_stop_completed_fails() {
        let tracker = Arc::new(TaskTracker::new());
        tracker.create("Test", "desc", None);
        tracker.update("task-1", Some(TaskStatus::Completed), None).unwrap();
        let tool = TaskStopTool::new(tracker);
        let result = tool
            .execute(json!({"task_id": "task-1"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_task_output_returns_details() {
        let tracker = Arc::new(TaskTracker::new());
        tracker.create("Build", "build project", Some("team-1"));
        let tool = TaskOutputTool::new(tracker);
        let result = tool
            .execute(json!({"task_id": "task-1"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.content.contains("Build"));
        assert!(result.content.contains("build project"));
        assert!(result.content.contains("team-1"));
    }

    #[tokio::test]
    async fn test_task_output_not_found() {
        let tracker = Arc::new(TaskTracker::new());
        let tool = TaskOutputTool::new(tracker);
        let result = tool
            .execute(json!({"task_id": "task-99"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.is_error);
    }
}
