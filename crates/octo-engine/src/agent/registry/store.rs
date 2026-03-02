//! AgentRegistry SQLite persistence layer
//! Pattern mirrors McpStorage: load-on-startup, persist-on-write

use anyhow::Result;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use super::entry::{AgentEntry, AgentId, AgentManifest, AgentStatus};

pub struct AgentStore {
    conn: Arc<Mutex<Connection>>,
}

impl AgentStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS agents (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                manifest    TEXT NOT NULL,
                state       TEXT NOT NULL DEFAULT 'created',
                created_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);
        ",
        )?;
        Ok(())
    }

    pub fn save(&self, entry: &AgentEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let manifest_json = serde_json::to_string(&entry.manifest)?;
        let state = entry.state.to_string();
        conn.execute(
            "INSERT OR REPLACE INTO agents (id, name, manifest, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![entry.id.0, entry.manifest.name, manifest_json, state, entry.created_at],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &AgentId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM agents WHERE id = ?1", rusqlite::params![id.0])?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<AgentEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, manifest, state, created_at FROM agents ORDER BY created_at ASC",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, manifest_json, state_str, created_at)| {
                let manifest: AgentManifest = serde_json::from_str(&manifest_json).ok()?;
                let state = match state_str.as_str() {
                    "running" | "paused" => AgentStatus::Stopped, // reset on restart
                    "stopped" => AgentStatus::Stopped,
                    _ => AgentStatus::Created,
                };
                Some(AgentEntry { id: AgentId(id), manifest, state, created_at })
            })
            .collect();
        Ok(entries)
    }

    pub fn update_state(&self, id: &AgentId, state: &AgentStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE agents SET state = ?1 WHERE id = ?2",
            rusqlite::params![state.to_string(), id.0],
        )?;
        Ok(())
    }
}
