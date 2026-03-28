//! Agent-facing tool for managing scheduled tasks (CRUD via SchedulerStorage).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;
use crate::scheduler::{AgentTaskConfig, CronParser, ScheduledTask, SchedulerStorage};

/// Single action-dispatch tool for managing scheduled tasks.
pub struct ScheduleTaskTool {
    storage: Arc<dyn SchedulerStorage>,
    cron_parser: CronParser,
}

impl ScheduleTaskTool {
    pub fn new(storage: Arc<dyn SchedulerStorage>) -> Self {
        Self {
            storage,
            cron_parser: CronParser::new(),
        }
    }

    async fn handle_create(&self, params: &Value) -> Result<ToolOutput> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'name' for create action"))?;
        let cron = params["cron"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'cron' for create action"))?;
        let input = params["input"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'input' for create action"))?;

        // Validate cron expression
        if let Err(e) = self.cron_parser.validate(cron) {
            return Ok(ToolOutput::error(format!("Invalid cron expression: {e}")));
        }

        let now = Utc::now();
        let next_run = self.cron_parser.parse_next(cron, now).ok();

        let system_prompt = params["system_prompt"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let max_rounds = params["max_rounds"].as_u64().unwrap_or(50) as u32;
        let timeout_secs = params["timeout_secs"].as_u64().unwrap_or(300);

        let task = ScheduledTask {
            id: Uuid::new_v4().to_string(),
            user_id: None,
            name: name.to_string(),
            cron: cron.to_string(),
            agent_config: AgentTaskConfig {
                system_prompt,
                input: input.to_string(),
                max_rounds,
                timeout_secs,
                ..AgentTaskConfig::default()
            },
            enabled: true,
            last_run: None,
            next_run,
            created_at: now,
            updated_at: now,
        };

        if let Err(e) = self.storage.save_task(&task).await {
            return Ok(ToolOutput::error(format!("Failed to save task: {e}")));
        }

        Ok(ToolOutput::success(format!(
            "Created scheduled task '{}' (id: {})\nCron: {}\nNext run: {}\nInput: {}",
            task.name,
            task.id,
            task.cron,
            next_run
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string()),
            task.agent_config.input,
        )))
    }

    async fn handle_list(&self) -> Result<ToolOutput> {
        let tasks = self
            .storage
            .list_tasks(None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list tasks: {e}"))?;

        if tasks.is_empty() {
            return Ok(ToolOutput::success("No scheduled tasks found."));
        }

        let mut lines = Vec::with_capacity(tasks.len());
        for t in &tasks {
            lines.push(format!(
                "- [{}] {} (id: {})\n  Cron: {} | Enabled: {} | Next: {} | Last: {}",
                if t.enabled { "ON" } else { "OFF" },
                t.name,
                t.id,
                t.cron,
                t.enabled,
                t.next_run
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string()),
                t.last_run
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }

        Ok(ToolOutput::success(format!(
            "{} scheduled task(s):\n{}",
            tasks.len(),
            lines.join("\n")
        )))
    }

    async fn handle_get(&self, params: &Value) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'task_id' for get action"))?;

        match self.storage.get_task(task_id).await {
            Ok(Some(t)) => Ok(ToolOutput::success(format!(
                "Task: {}\n  ID: {}\n  Cron: {}\n  Enabled: {}\n  Input: {}\n  System Prompt: {}\n  Max Rounds: {}\n  Timeout: {}s\n  Next Run: {}\n  Last Run: {}\n  Created: {}",
                t.name,
                t.id,
                t.cron,
                t.enabled,
                t.agent_config.input,
                if t.agent_config.system_prompt.is_empty() { "(default)" } else { &t.agent_config.system_prompt },
                t.agent_config.max_rounds,
                t.agent_config.timeout_secs,
                t.next_run.map(|d| d.to_rfc3339()).unwrap_or_else(|| "-".to_string()),
                t.last_run.map(|d| d.to_rfc3339()).unwrap_or_else(|| "-".to_string()),
                t.created_at.to_rfc3339(),
            ))),
            Ok(None) => Ok(ToolOutput::error(format!("Task not found: {task_id}"))),
            Err(e) => Ok(ToolOutput::error(format!("Failed to get task: {e}"))),
        }
    }

    async fn handle_delete(&self, params: &Value) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'task_id' for delete action"))?;

        // Check existence first
        match self.storage.get_task(task_id).await {
            Ok(Some(t)) => {
                if let Err(e) = self.storage.delete_task(task_id).await {
                    return Ok(ToolOutput::error(format!("Failed to delete task: {e}")));
                }
                Ok(ToolOutput::success(format!(
                    "Deleted scheduled task '{}' (id: {})",
                    t.name, t.id
                )))
            }
            Ok(None) => Ok(ToolOutput::error(format!("Task not found: {task_id}"))),
            Err(e) => Ok(ToolOutput::error(format!("Failed to get task: {e}"))),
        }
    }

    async fn handle_toggle(&self, params: &Value, enable: bool) -> Result<ToolOutput> {
        let task_id = params["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'task_id'"))?;

        match self.storage.get_task(task_id).await {
            Ok(Some(mut t)) => {
                t.enabled = enable;
                t.updated_at = Utc::now();
                if enable {
                    t.next_run = self.cron_parser.parse_next(&t.cron, Utc::now()).ok();
                }
                if let Err(e) = self.storage.save_task(&t).await {
                    return Ok(ToolOutput::error(format!("Failed to update task: {e}")));
                }
                let action = if enable { "Enabled" } else { "Disabled" };
                Ok(ToolOutput::success(format!(
                    "{action} scheduled task '{}' (id: {})",
                    t.name, t.id
                )))
            }
            Ok(None) => Ok(ToolOutput::error(format!("Task not found: {task_id}"))),
            Err(e) => Ok(ToolOutput::error(format!("Failed to get task: {e}"))),
        }
    }
}

#[async_trait]
impl Tool for ScheduleTaskTool {
    fn name(&self) -> &str {
        "schedule_task"
    }

    fn description(&self) -> &str {
        "Manage scheduled agent tasks. Use action=\"create\" to schedule a recurring task with a cron expression, \"list\" to view all tasks, \"get\" to see details, \"delete\" to remove, or \"enable\"/\"disable\" to toggle. Example: {\"action\": \"create\", \"name\": \"PR check\", \"cron\": \"0 0 9 * * *\", \"input\": \"Check for new PRs and summarize them\"}"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "get", "delete", "enable", "disable"],
                    "description": "Operation to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Task name (required for create)"
                },
                "cron": {
                    "type": "string",
                    "description": "Cron expression in 7-field format: sec min hour day month weekday year. Example: '0 0 9 * * * *' for daily at 9AM. (required for create)"
                },
                "input": {
                    "type": "string",
                    "description": "The prompt/instruction for the agent to execute on each run (required for create)"
                },
                "system_prompt": {
                    "type": "string",
                    "description": "Optional system prompt override for the scheduled agent"
                },
                "max_rounds": {
                    "type": "integer",
                    "description": "Max conversation rounds per execution (default: 50)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Execution timeout in seconds (default: 300)"
                },
                "task_id": {
                    "type": "string",
                    "description": "Task ID (required for get, delete, enable, disable)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let action = params["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'action' parameter"))?;

        match action {
            "create" => self.handle_create(&params).await,
            "list" => self.handle_list().await,
            "get" => self.handle_get(&params).await,
            "delete" => self.handle_delete(&params).await,
            "enable" => self.handle_toggle(&params, true).await,
            "disable" => self.handle_toggle(&params, false).await,
            other => Ok(ToolOutput::error(format!(
                "Unknown action '{}'. Valid actions: create, list, get, delete, enable, disable",
                other
            ))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::HighRisk
    }

    fn category(&self) -> &str {
        "scheduler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::SqliteSchedulerStorage;

    #[test]
    fn test_schedule_task_tool_metadata() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let storage: Arc<dyn SchedulerStorage> = rt.block_on(async {
            let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
            conn.call(|c| {
                c.execute_batch(
                    "CREATE TABLE IF NOT EXISTS scheduled_tasks (
                        id TEXT PRIMARY KEY,
                        user_id TEXT,
                        name TEXT NOT NULL,
                        cron TEXT NOT NULL,
                        agent_config TEXT NOT NULL,
                        enabled INTEGER NOT NULL DEFAULT 1,
                        last_run TEXT,
                        next_run TEXT,
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL
                    )",
                )?;
                Ok(())
            })
            .await
            .unwrap();
            Arc::new(SqliteSchedulerStorage::new(conn)) as Arc<dyn SchedulerStorage>
        });

        let tool = ScheduleTaskTool::new(storage);
        assert_eq!(tool.name(), "schedule_task");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
        assert_eq!(tool.risk_level(), RiskLevel::HighRisk);
        assert_eq!(tool.category(), "scheduler");

        let params = tool.parameters();
        assert!(params["properties"]["action"].is_object());
        assert!(params["properties"]["name"].is_object());
        assert!(params["properties"]["cron"].is_object());
        assert!(params["properties"]["input"].is_object());
    }

    #[tokio::test]
    async fn test_schedule_task_list_empty() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS scheduled_tasks (
                    id TEXT PRIMARY KEY,
                    user_id TEXT,
                    name TEXT NOT NULL,
                    cron TEXT NOT NULL,
                    agent_config TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    last_run TEXT,
                    next_run TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let storage: Arc<dyn SchedulerStorage> =
            Arc::new(SqliteSchedulerStorage::new(conn));
        let tool = ScheduleTaskTool::new(storage);
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: std::path::PathBuf::from("."),
            path_validator: None,
        };

        let result = tool.execute(json!({"action": "list"}), &ctx).await.unwrap();
        assert!(result.content.contains("No scheduled tasks"));
    }

    #[tokio::test]
    async fn test_schedule_task_create_and_get() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS scheduled_tasks (
                    id TEXT PRIMARY KEY,
                    user_id TEXT,
                    name TEXT NOT NULL,
                    cron TEXT NOT NULL,
                    agent_config TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    last_run TEXT,
                    next_run TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let storage: Arc<dyn SchedulerStorage> =
            Arc::new(SqliteSchedulerStorage::new(conn));
        let tool = ScheduleTaskTool::new(storage);
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: std::path::PathBuf::from("."),
            path_validator: None,
        };

        // Create
        let result = tool
            .execute(
                json!({
                    "action": "create",
                    "name": "test task",
                    "cron": "0 0 9 * * * *",
                    "input": "check git status"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("Created scheduled task"));
        assert!(result.content.contains("test task"));

        // List should show 1 task
        let result = tool.execute(json!({"action": "list"}), &ctx).await.unwrap();
        assert!(result.content.contains("1 scheduled task(s)"));
        assert!(result.content.contains("test task"));
    }

    #[tokio::test]
    async fn test_schedule_task_invalid_cron() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS scheduled_tasks (
                    id TEXT PRIMARY KEY,
                    user_id TEXT,
                    name TEXT NOT NULL,
                    cron TEXT NOT NULL,
                    agent_config TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    last_run TEXT,
                    next_run TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let storage: Arc<dyn SchedulerStorage> =
            Arc::new(SqliteSchedulerStorage::new(conn));
        let tool = ScheduleTaskTool::new(storage);
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: std::path::PathBuf::from("."),
            path_validator: None,
        };

        let result = tool
            .execute(
                json!({
                    "action": "create",
                    "name": "bad task",
                    "cron": "not a cron",
                    "input": "whatever"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("Invalid cron"));
    }

    #[tokio::test]
    async fn test_schedule_task_unknown_action() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();
        conn.call(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS scheduled_tasks (
                    id TEXT PRIMARY KEY,
                    user_id TEXT,
                    name TEXT NOT NULL,
                    cron TEXT NOT NULL,
                    agent_config TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    last_run TEXT,
                    next_run TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let storage: Arc<dyn SchedulerStorage> =
            Arc::new(SqliteSchedulerStorage::new(conn));
        let tool = ScheduleTaskTool::new(storage);
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::from_string("test"),
            working_dir: std::path::PathBuf::from("."),
            path_validator: None,
        };

        let result = tool
            .execute(json!({"action": "bogus"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("Unknown action"));
    }
}
