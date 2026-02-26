use anyhow::Result;
use tracing::debug;

use octo_types::{ExecutionStatus, ToolExecution, ToolSource};

pub struct ToolExecutionRecorder {
    conn: tokio_rusqlite::Connection,
}

impl ToolExecutionRecorder {
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Record tool execution start. Returns the execution ID.
    pub async fn record_start(
        &self,
        session_id: &str,
        tool_name: &str,
        source: &ToolSource,
        input: &serde_json::Value,
    ) -> Result<String> {
        let id = ulid::Ulid::new().to_string();
        let source_str = serde_json::to_string(source)?;
        let input_str = serde_json::to_string(input)?;
        let started_at = chrono::Utc::now().timestamp_millis();

        let id_clone = id.clone();
        let session_id = session_id.to_string();
        let tool_name_owned = tool_name.to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO tool_executions (id, session_id, tool_name, source, input, status, started_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
                    rusqlite::params![id_clone, session_id, tool_name_owned, source_str, input_str, started_at],
                )?;
                Ok(())
            })
            .await?;

        debug!(id = %id, tool = %tool_name, "Recorded tool execution start");
        Ok(id)
    }

    /// Record successful tool execution completion.
    pub async fn record_complete(
        &self,
        id: &str,
        output: &serde_json::Value,
        duration_ms: u64,
    ) -> Result<()> {
        let output_str = serde_json::to_string(output)?;
        let id = id.to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE tool_executions SET output = ?1, status = 'success', duration_ms = ?2 WHERE id = ?3",
                    rusqlite::params![output_str, duration_ms as i64, id],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    /// Record failed tool execution.
    pub async fn record_failed(
        &self,
        id: &str,
        error: &str,
        duration_ms: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let error = error.to_string();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE tool_executions SET error = ?1, status = 'failed', duration_ms = ?2 WHERE id = ?3",
                    rusqlite::params![error, duration_ms as i64, id],
                )?;
                Ok(())
            })
            .await?;

        Ok(())
    }

    /// List executions for a session.
    pub async fn list_by_session(
        &self,
        session_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ToolExecution>> {
        let session_id = session_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, tool_name, source, input, output, status, started_at, duration_ms, error
                     FROM tool_executions WHERE session_id = ?1
                     ORDER BY started_at DESC LIMIT ?2 OFFSET ?3",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![session_id, limit as i64, offset as i64], |row| {
                        Ok(Self::row_to_execution(row))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .map_err(Into::into)
    }

    /// Get a single execution by ID.
    pub async fn get(&self, id: &str) -> Result<Option<ToolExecution>> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, tool_name, source, input, output, status, started_at, duration_ms, error
                     FROM tool_executions WHERE id = ?1",
                )?;
                let result = stmt
                    .query_row(rusqlite::params![id], |row| Ok(Self::row_to_execution(row)))
                    .ok();
                Ok(result)
            })
            .await
            .map_err(Into::into)
    }

    fn row_to_execution(row: &rusqlite::Row<'_>) -> ToolExecution {
        let source_str: String = row.get(3).unwrap_or_default();
        let source: ToolSource =
            serde_json::from_str(&source_str).unwrap_or(ToolSource::BuiltIn);
        let input_str: String = row.get(4).unwrap_or_default();
        let output_str: Option<String> = row.get(5).unwrap_or(None);
        let status_str: String = row.get(6).unwrap_or_default();

        ToolExecution {
            id: row.get(0).unwrap_or_default(),
            session_id: row.get(1).unwrap_or_default(),
            tool_name: row.get(2).unwrap_or_default(),
            source,
            input: serde_json::from_str(&input_str).unwrap_or_default(),
            output: output_str.and_then(|s| serde_json::from_str(&s).ok()),
            status: match status_str.as_str() {
                "running" => ExecutionStatus::Running,
                "success" => ExecutionStatus::Success,
                "failed" => ExecutionStatus::Failed,
                "timeout" => ExecutionStatus::Timeout,
                _ => ExecutionStatus::Failed,
            },
            started_at: row.get(7).unwrap_or(0),
            duration_ms: row.get::<_, Option<i64>>(8).unwrap_or(None).map(|v| v as u64),
            error: row.get(9).unwrap_or(None),
        }
    }
}
