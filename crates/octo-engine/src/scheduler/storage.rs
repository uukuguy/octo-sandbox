//! SQLite-based scheduler storage implementation

use super::*;
use rusqlite::params;
use tokio_rusqlite::Connection as TokioConnection;

/// SQLite-based scheduler storage
pub struct SqliteSchedulerStorage {
    conn: TokioConnection,
}

impl SqliteSchedulerStorage {
    /// Create storage from a tokio_rusqlite Connection
    pub fn new(conn: TokioConnection) -> Self {
        Self { conn }
    }

    /// Helper to convert a row to ScheduledTask
    fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<ScheduledTask> {
        let agent_config_json: String = row.get(4)?;
        let agent_config: AgentTaskConfig =
            serde_json::from_str(&agent_config_json).unwrap_or_default();

        Ok(ScheduledTask {
            id: row.get(0)?,
            user_id: row.get(1)?,
            name: row.get(2)?,
            cron: row.get(3)?,
            agent_config,
            enabled: row.get::<_, i32>(5)? != 0,
            last_run: row
                .get::<_, Option<String>>(6)?
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&chrono::Utc)),
            next_run: row
                .get::<_, Option<String>>(7)?
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&chrono::Utc)),
            created_at: row
                .get::<_, String>(8)?
                .parse::<i64>()
                .map(|t| chrono::DateTime::from_timestamp(t, 0))
                .unwrap_or(None)
                .unwrap_or_else(chrono::Utc::now),
            updated_at: chrono::Utc::now(),
        })
    }
}

#[async_trait]
impl SchedulerStorage for SqliteSchedulerStorage {
    async fn save_task(&self, task: &ScheduledTask) -> Result<(), SchedulerError> {
        // Clone data needed for the closure to have 'static lifetime
        let id = task.id.clone();
        let user_id = task.user_id.clone();
        let name = task.name.clone();
        let cron = task.cron.clone();
        let agent_config_json = serde_json::to_string(&task.agent_config)
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;
        let enabled = task.enabled;
        let last_run = task.last_run.map(|d| d.to_rfc3339());
        let next_run = task.next_run.map(|d| d.to_rfc3339());
        let created_at = task.created_at.to_rfc3339();
        let updated_at = task.updated_at.to_rfc3339();

        self.conn.call(move |conn| {
            conn.execute(
                r#"INSERT OR REPLACE INTO scheduled_tasks
                   (id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
                params![
                    id,
                    user_id,
                    name,
                    cron,
                    agent_config_json,
                    enabled as i32,
                    last_run,
                    next_run,
                    created_at,
                    updated_at,
                ],
            )?;
            Ok(())
        }).await.map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get_task(&self, task_id: &str) -> Result<Option<ScheduledTask>, SchedulerError> {
        let task_id = task_id.to_string();
        let result = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
                 FROM scheduled_tasks WHERE id = ?1"
            )?;

            let task = match stmt.query_row([&task_id], Self::row_to_task) {
                Ok(task) => Some(task),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(e.into()),
            };

            Ok(task)
        }).await.map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(result)
    }

    async fn get_due_tasks(&self) -> Result<Vec<ScheduledTask>, SchedulerError> {
        let now = Utc::now().to_rfc3339();
        let result = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
                 FROM scheduled_tasks
                 WHERE enabled = 1 AND (next_run IS NULL OR next_run <= ?1)"
            )?;

            let tasks = stmt.query_map([&now], Self::row_to_task)?
                .filter_map(|r| r.ok())
                .collect();

            Ok(tasks)
        }).await.map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(result)
    }

    async fn list_tasks(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<ScheduledTask>, SchedulerError> {
        let result = if let Some(user_id) = user_id {
            let user_id = user_id.to_string();
            self.conn.call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
                     FROM scheduled_tasks WHERE user_id = ?1 ORDER BY created_at DESC"
                )?;

                let tasks = stmt.query_map([&user_id], Self::row_to_task)?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(tasks)
            }).await.map_err(|e| SchedulerError::Storage(e.to_string()))?
        } else {
            self.conn.call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, user_id, name, cron, agent_config, enabled, last_run, next_run, created_at, updated_at
                     FROM scheduled_tasks ORDER BY created_at DESC"
                )?;

                let tasks = stmt.query_map([], Self::row_to_task)?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(tasks)
            }).await.map_err(|e| SchedulerError::Storage(e.to_string()))?
        };

        Ok(result)
    }

    async fn delete_task(&self, task_id: &str) -> Result<(), SchedulerError> {
        let task_id = task_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute("DELETE FROM scheduled_tasks WHERE id = ?1", [&task_id])?;
                Ok(())
            })
            .await
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn update_timing(
        &self,
        task_id: &str,
        last_run: Option<DateTime<Utc>>,
        next_run: Option<DateTime<Utc>>,
    ) -> Result<(), SchedulerError> {
        let task_id = task_id.to_string();
        let last_run = last_run.map(|d| d.to_rfc3339());
        let next_run = next_run.map(|d| d.to_rfc3339());
        let updated_at = Utc::now().to_rfc3339();

        self.conn.call(move |conn| {
            conn.execute(
                "UPDATE scheduled_tasks SET last_run = ?1, next_run = ?2, updated_at = ?3 WHERE id = ?4",
                params![last_run, next_run, updated_at, task_id],
            )?;
            Ok(())
        }).await.map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn save_execution(&self, execution: &TaskExecution) -> Result<(), SchedulerError> {
        // Clone data needed for the closure to have 'static lifetime
        let id = execution.id.clone();
        let task_id = execution.task_id.clone();
        let started_at = execution.started_at.to_rfc3339();
        let finished_at = execution.finished_at.map(|d| d.to_rfc3339());
        let status = format!("{:?}", execution.status);
        let result = execution.result.clone();
        let error = execution.error.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    r#"INSERT OR REPLACE INTO task_executions
                   (id, task_id, started_at, finished_at, status, result, error)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
                    params![id, task_id, started_at, finished_at, status, result, error],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get_executions(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskExecution>, SchedulerError> {
        let task_id = task_id.to_string();
        let limit = limit as i64;
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, task_id, started_at, finished_at, status, result, error
                 FROM task_executions WHERE task_id = ?1 ORDER BY started_at DESC LIMIT ?2",
                )?;

                let executions = stmt
                    .query_map(params![&task_id, limit], |row| {
                        let status_str: String = row.get(4)?;
                        let status = match status_str.as_str() {
                            "Running" => ExecutionStatus::Running,
                            "Success" => ExecutionStatus::Success,
                            "Failed" => ExecutionStatus::Failed,
                            "Timeout" => ExecutionStatus::Timeout,
                            "Cancelled" => ExecutionStatus::Cancelled,
                            _ => ExecutionStatus::Failed,
                        };

                        Ok(TaskExecution {
                            id: row.get(0)?,
                            task_id: row.get(1)?,
                            started_at: row
                                .get::<_, String>(2)?
                                .parse::<i64>()
                                .map(|t| chrono::DateTime::from_timestamp(t, 0))
                                .unwrap_or(None)
                                .unwrap_or_else(chrono::Utc::now),
                            finished_at: row
                                .get::<_, Option<String>>(3)?
                                .and_then(|s| s.parse::<i64>().ok())
                                .and_then(|t| chrono::DateTime::from_timestamp(t, 0)),
                            status,
                            result: row.get(5)?,
                            error: row.get(6)?,
                        })
                    })?
                    .filter_map(|r| r.ok())
                    .collect();

                Ok(executions)
            })
            .await
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;

        Ok(result)
    }
}
