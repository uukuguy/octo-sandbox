//! SQLite-based scheduler storage implementation

use super::*;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

/// SQLite-based scheduler storage
pub struct SqliteSchedulerStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSchedulerStorage {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }
}

#[async_trait]
impl SchedulerStorage for SqliteSchedulerStorage {
    async fn save_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        let conn = self.conn.lock().await;
        let agent_config_json = serde_json::to_string(&task.agent_config)
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;

        conn.execute(
            r#"INSERT OR REPLACE INTO scheduled_tasks
               (id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
            (
                &task.id,
                &task.user_id,
                &task.name,
                &task.cron,
                agent_config_json,
                task.enabled as i32,
                task.last_run.map(|d| d.to_rfc3339()),
                task.next_run.map(|d| d.to_rfc3339()),
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
            ),
        ).map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get_task(&self, task_id: &str) -> Result<Option<ScheduledTask>, SchedulerError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
             FROM scheduled_tasks WHERE id = ?1"
        ).map_err(|e| SchedulerError::Storage(e.to_string()))?;

        let task = stmt.query_row([task_id], |row| {
            Ok(row_to_task(row))
        }).ok();

        Ok(task)
    }

    async fn list_tasks(&self, user_id: Option<&str>) -> Result<Vec<ScheduledTask>, SchedulerError> {
        let conn = self.conn.lock().await;

        let tasks: Vec<ScheduledTask> = match user_id {
            Some(uid) => {
                let mut stmt = conn.prepare(
                    "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
                     FROM scheduled_tasks WHERE user_id = ?1"
                ).map_err(|e| SchedulerError::Storage(e.to_string()))?;
                let result = stmt.query_map([uid], |row| Ok(row_to_task(row)))
                    .map_err(|e| SchedulerError::Storage(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                result
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
                     FROM scheduled_tasks"
                ).map_err(|e| SchedulerError::Storage(e.to_string()))?;
                let result = stmt.query_map([], |row| Ok(row_to_task(row)))
                    .map_err(|e| SchedulerError::Storage(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                result
            }
        };

        Ok(tasks)
    }

    async fn delete_task(&self, task_id: &str) -> Result<(), SchedulerError> {
        let conn = self.conn.lock().await;
        conn.execute("DELETE FROM scheduled_tasks WHERE id = ?1", [task_id])
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn update_timing(
        &self,
        task_id: &str,
        last_run: Option<DateTime<Utc>>,
        next_run: Option<DateTime<Utc>>,
    ) -> Result<(), SchedulerError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE scheduled_tasks SET last_run = ?1, next_run = ?2, updated_at = ?3 WHERE id = ?4",
            (
                last_run.map(|d| d.to_rfc3339()),
                next_run.map(|d| d.to_rfc3339()),
                Utc::now().to_rfc3339(),
                task_id,
            ),
        ).map_err(|e| SchedulerError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn save_execution(&self, execution: &TaskExecution) -> Result<(), SchedulerError> {
        let conn = self.conn.lock().await;
        conn.execute(
            r#"INSERT OR REPLACE INTO task_executions
               (id, task_id, started_at, finished_at, status, result, error)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            (
                &execution.id,
                &execution.task_id,
                execution.started_at.to_rfc3339(),
                execution.finished_at.map(|d| d.to_rfc3339()),
                serde_json::to_string(&execution.status).unwrap_or_default(),
                &execution.result,
                &execution.error,
            ),
        ).map_err(|e| SchedulerError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get_executions(&self, task_id: &str, limit: usize) -> Result<Vec<TaskExecution>, SchedulerError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, started_at, finished_at, status, result, error
             FROM task_executions WHERE task_id = ?1 ORDER BY started_at DESC LIMIT ?2"
        ).map_err(|e| SchedulerError::Storage(e.to_string()))?;

        let executions = stmt.query_map([task_id, &limit.to_string()], |row| {
            Ok(row_to_execution(row))
        }).map_err(|e| SchedulerError::Storage(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(executions)
    }

    async fn get_due_tasks(&self) -> Result<Vec<ScheduledTask>, SchedulerError> {
        let conn = self.conn.lock().await;
        let now = Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
             FROM scheduled_tasks WHERE enabled = 1 AND next_run IS NOT NULL AND next_run <= ?1"
        ).map_err(|e| SchedulerError::Storage(e.to_string()))?;

        let tasks = stmt.query_map([&now], |row| {
            Ok(row_to_task(row))
        }).map_err(|e| SchedulerError::Storage(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(tasks)
    }
}

fn row_to_task(row: &rusqlite::Row) -> ScheduledTask {
    let agent_config_json: String = row.get(4).unwrap_or_default();
    let agent_config: AgentTaskConfig = serde_json::from_str(&agent_config_json).unwrap_or_default();

    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
    }

    ScheduledTask {
        id: row.get(0).unwrap_or_default(),
        user_id: row.get(1).ok(),
        name: row.get(2).unwrap_or_default(),
        cron: row.get(3).unwrap_or_default(),
        agent_config,
        enabled: row.get::<_, i32>(5).unwrap_or(1) == 1,
        last_run: row.get::<_, Option<String>>(6).ok().flatten().and_then(|s| parse_datetime(&s)),
        next_run: row.get::<_, Option<String>>(7).ok().flatten().and_then(|s| parse_datetime(&s)),
        created_at: row.get::<_, String>(8).ok().and_then(|s| parse_datetime(&s)).unwrap_or_else(Utc::now),
        updated_at: row.get::<_, String>(9).ok().and_then(|s| parse_datetime(&s)).unwrap_or_else(Utc::now),
    }
}

fn row_to_execution(row: &rusqlite::Row) -> TaskExecution {
    let status_str: String = row.get(4).unwrap_or_default();
    let status: ExecutionStatus = serde_json::from_str(&status_str).unwrap_or(ExecutionStatus::Failed);

    fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
    }

    TaskExecution {
        id: row.get(0).unwrap_or_default(),
        task_id: row.get(1).unwrap_or_default(),
        started_at: row.get::<_, String>(2).ok().and_then(|s| parse_datetime(&s)).unwrap_or_else(Utc::now),
        finished_at: row.get::<_, Option<String>>(3).ok().flatten().and_then(|s| parse_datetime(&s)),
        status,
        result: row.get(5).ok(),
        error: row.get(6).ok(),
    }
}
