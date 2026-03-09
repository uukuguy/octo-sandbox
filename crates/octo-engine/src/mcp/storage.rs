use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub source: String,
    pub command: String,
    pub args: String,
    pub env: String,
    pub enabled: bool,
    pub transport: Option<String>,
    pub url: Option<String>,
    pub user_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpExecutionRecord {
    pub id: String,
    pub server_id: String,
    pub tool_name: String,
    pub params: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub user_id: String,
    pub executed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpLogRecord {
    pub id: String,
    pub server_id: String,
    pub level: String,
    pub direction: String,
    pub method: Option<String>,
    pub params: Option<String>,
    pub result: Option<String>,
    pub raw_data: Option<String>,
    pub duration_ms: Option<i64>,
    pub user_id: String,
    pub logged_at: String,
}

pub struct McpStorage {
    conn: Connection,
}

impl McpStorage {
    pub fn new(db_path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        Ok(Self { conn })
    }

    // Server CRUD
    pub fn list_servers(&self) -> rusqlite::Result<Vec<McpServerRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, command, args, env, enabled, COALESCE(transport, 'stdio') as transport, url, COALESCE(user_id, 'default') as user_id, created_at, updated_at FROM mcp_servers ORDER BY name"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(McpServerRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                command: row.get(3)?,
                args: row.get(4)?,
                env: row.get(5)?,
                enabled: row.get::<_, i32>(6)? == 1,
                transport: row.get(7)?,
                url: row.get(8)?,
                user_id: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.collect()
    }

    /// List servers filtered by user_id
    pub fn list_servers_for_user(&self, user_id: &str) -> rusqlite::Result<Vec<McpServerRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, command, args, env, enabled, COALESCE(transport, 'stdio') as transport, url, COALESCE(user_id, 'default') as user_id, created_at, updated_at FROM mcp_servers WHERE user_id = ? OR user_id = 'default' ORDER BY name"
        )?;
        let rows = stmt.query_map([user_id], |row| {
            Ok(McpServerRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                command: row.get(3)?,
                args: row.get(4)?,
                env: row.get(5)?,
                enabled: row.get::<_, i32>(6)? == 1,
                transport: row.get(7)?,
                url: row.get(8)?,
                user_id: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_server(&self, id: &str) -> rusqlite::Result<Option<McpServerRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, command, args, env, enabled, COALESCE(transport, 'stdio') as transport, url, COALESCE(user_id, 'default') as user_id, created_at, updated_at FROM mcp_servers WHERE id = ?"
        )?;
        let mut rows = stmt.query_map([id], |row| {
            Ok(McpServerRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                command: row.get(3)?,
                args: row.get(4)?,
                env: row.get(5)?,
                enabled: row.get::<_, i32>(6)? == 1,
                transport: row.get(7)?,
                url: row.get(8)?,
                user_id: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.next().transpose()
    }

    /// Get server only if it belongs to the user
    pub fn get_server_for_user(
        &self,
        id: &str,
        user_id: &str,
    ) -> rusqlite::Result<Option<McpServerRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, command, args, env, enabled, COALESCE(transport, 'stdio') as transport, url, COALESCE(user_id, 'default') as user_id, created_at, updated_at FROM mcp_servers WHERE id = ? AND (user_id = ? OR user_id = 'default')"
        )?;
        let mut rows = stmt.query_map(rusqlite::params![id, user_id], |row| {
            Ok(McpServerRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                command: row.get(3)?,
                args: row.get(4)?,
                env: row.get(5)?,
                enabled: row.get::<_, i32>(6)? == 1,
                transport: row.get(7)?,
                url: row.get(8)?,
                user_id: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.next().transpose()
    }

    pub fn insert_server(&self, server: &McpServerRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_servers (id, name, source, command, args, env, enabled, transport, url, user_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                server.id,
                server.name,
                server.source,
                server.command,
                server.args,
                server.env,
                server.enabled as i32,
                server.transport,
                server.url,
                server.user_id,
                server.created_at,
                server.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn update_server(&self, server: &McpServerRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE mcp_servers SET name = ?, source = ?, command = ?, args = ?, env = ?, enabled = ?, transport = ?, url = ?, user_id = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![
                server.name,
                server.source,
                server.command,
                server.args,
                server.env,
                server.enabled as i32,
                server.transport,
                server.url,
                server.user_id,
                server.updated_at,
                server.id
            ],
        )?;
        Ok(())
    }

    pub fn delete_server(&self, id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM mcp_servers WHERE id = ?", [id])?;
        Ok(())
    }

    // Execution records
    pub fn insert_execution(&self, exec: &McpExecutionRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_executions (id, server_id, tool_name, params, result, error, duration_ms, user_id, executed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                exec.id,
                exec.server_id,
                exec.tool_name,
                exec.params,
                exec.result,
                exec.error,
                exec.duration_ms,
                exec.user_id,
                exec.executed_at
            ],
        )?;
        Ok(())
    }

    pub fn list_executions(
        &self,
        server_id: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<McpExecutionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, server_id, tool_name, params, result, error, duration_ms, COALESCE(user_id, 'default') as user_id, executed_at FROM mcp_executions WHERE server_id = ? ORDER BY executed_at DESC LIMIT ?"
        )?;
        let rows = stmt.query_map(rusqlite::params![server_id, limit as i64], |row| {
            Ok(McpExecutionRecord {
                id: row.get(0)?,
                server_id: row.get(1)?,
                tool_name: row.get(2)?,
                params: row.get(3)?,
                result: row.get(4)?,
                error: row.get(5)?,
                duration_ms: row.get(6)?,
                user_id: row.get(7)?,
                executed_at: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    // Log records
    pub fn insert_log(&self, log: &McpLogRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_logs (id, server_id, level, direction, method, params, result, raw_data, duration_ms, user_id, logged_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                log.id,
                log.server_id,
                log.level,
                log.direction,
                log.method,
                log.params,
                log.result,
                log.raw_data,
                log.duration_ms,
                log.user_id,
                log.logged_at
            ],
        )?;
        Ok(())
    }

    pub fn list_logs(
        &self,
        server_id: &str,
        level: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<McpLogRecord>> {
        let mut sql = String::from(
            "SELECT id, server_id, level, direction, method, params, result, raw_data, duration_ms, COALESCE(user_id, 'default') as user_id, logged_at FROM mcp_logs WHERE server_id = ?"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(server_id.to_string())];

        if let Some(l) = level {
            sql.push_str(" AND level = ?");
            params.push(Box::new(l.to_string()));
        }

        sql.push_str(" ORDER BY logged_at DESC LIMIT ? OFFSET ?");
        params.push(Box::new(limit as i64));
        params.push(Box::new(offset as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(McpLogRecord {
                id: row.get(0)?,
                server_id: row.get(1)?,
                level: row.get(2)?,
                direction: row.get(3)?,
                method: row.get(4)?,
                params: row.get(5)?,
                result: row.get(6)?,
                raw_data: row.get(7)?,
                duration_ms: row.get(8)?,
                user_id: row.get(9)?,
                logged_at: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn clear_logs(&self, server_id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM mcp_logs WHERE server_id = ?", [server_id])?;
        Ok(())
    }
}
