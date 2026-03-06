//! AgentCatalog SQLite persistence layer
//! Pattern mirrors McpStorage: load-on-startup, persist-on-write

use anyhow::Result;
use octo_types::{TenantId, DEFAULT_TENANT_ID};
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
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Step 1: create table (no-op if already exists)
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS agents (
                id          TEXT PRIMARY KEY,
                tenant_id   TEXT NOT NULL DEFAULT 'default',
                name        TEXT NOT NULL,
                manifest    TEXT NOT NULL,
                state       TEXT NOT NULL DEFAULT 'created',
                created_at  INTEGER NOT NULL
            );
        ",
        )?;
        // Step 2: add tenant_id column to pre-existing databases (idempotent: ignore if exists)
        let _ = conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';",
        );
        // Step 3: create indexes
        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);
            CREATE INDEX IF NOT EXISTS idx_agents_tenant ON agents(tenant_id);
        ",
        )?;
        Ok(())
    }

    pub fn save(&self, entry: &AgentEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let manifest_json = serde_json::to_string(&entry.manifest)?;
        let state = entry.state.to_string();
        conn.execute(
            "INSERT OR REPLACE INTO agents (id, tenant_id, name, manifest, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                entry.id.0,
                entry.tenant_id.as_str(),
                entry.manifest.name,
                manifest_json,
                state,
                entry.created_at
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &AgentId) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM agents WHERE id = ?1", rusqlite::params![id.0])?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<AgentEntry>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, tenant_id, manifest, state, created_at FROM agents ORDER BY created_at ASC",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, tenant_id, manifest_json, state_str, created_at)| {
                let manifest: AgentManifest = serde_json::from_str(&manifest_json).ok()?;
                let state = match state_str.as_str() {
                    "running" | "paused" => AgentStatus::Stopped, // reset on restart
                    "stopped" => AgentStatus::Stopped,
                    _ => AgentStatus::Created,
                };
                let tenant_id = if tenant_id.is_empty() {
                    TenantId::from_string(DEFAULT_TENANT_ID)
                } else {
                    TenantId::from_string(tenant_id)
                };
                Some(AgentEntry {
                    id: AgentId(id),
                    tenant_id,
                    manifest,
                    state,
                    created_at,
                })
            })
            .collect();
        Ok(entries)
    }

    pub fn update_state(&self, id: &AgentId, state: &AgentStatus) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE agents SET state = ?1 WHERE id = ?2",
            rusqlite::params![state.to_string(), id.0],
        )?;
        Ok(())
    }
}
