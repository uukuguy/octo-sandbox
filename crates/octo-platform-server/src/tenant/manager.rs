use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use rusqlite::Connection;

use super::models::ResourceQuota;
use super::runtime::TenantRuntime;

pub struct TenantManager {
    conn: Arc<Mutex<Connection>>,
    runtimes: DashMap<String, Arc<TenantRuntime>>,
}

impl fmt::Debug for TenantManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TenantManager")
            .field("runtimes", &self.runtimes.len())
            .finish()
    }
}

impl TenantManager {
    pub fn new(data_dir: PathBuf) -> Result<Self, anyhow::Error> {
        let conn = Connection::open(data_dir.join("platform.db"))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                plan TEXT NOT NULL DEFAULT 'free',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tenant_quotas (
                tenant_id TEXT PRIMARY KEY REFERENCES tenants(id),
                max_agents INTEGER NOT NULL DEFAULT 5,
                max_sessions_per_user INTEGER NOT NULL DEFAULT 10,
                max_api_calls_per_day INTEGER NOT NULL DEFAULT 1000,
                max_memory_mb INTEGER NOT NULL DEFAULT 1024,
                max_mcp_servers INTEGER NOT NULL DEFAULT 5
            );
            ",
        )?;

        // Create default tenant
        Self::ensure_default_tenant(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            runtimes: DashMap::new(),
        })
    }

    fn ensure_default_tenant(conn: &Connection) -> Result<(), anyhow::Error> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tenants WHERE id = 'default'",
            [],
            |row| row.get(0),
        )?;

        if count == 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                "INSERT INTO tenants (id, name, slug, plan, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                rusqlite::params!["default", "Default Tenant", "default", "free", now, now],
            )?;

            conn.execute(
                "INSERT INTO tenant_quotas (tenant_id, max_agents, max_sessions_per_user, max_api_calls_per_day, max_memory_mb, max_mcp_servers) VALUES (?, ?, ?, ?, ?, ?)",
                rusqlite::params!["default", 5, 10, 1000, 1024, 5],
            )?;
        }
        Ok(())
    }

    pub fn get_or_create_runtime(&self, tenant_id: &str) -> Arc<TenantRuntime> {
        self.runtimes
            .entry(tenant_id.to_string())
            .or_insert_with(|| {
                let quota = self.get_quota(tenant_id).unwrap_or_default();
                Arc::new(TenantRuntime::new(tenant_id.to_string(), quota))
            })
            .value()
            .clone()
    }

    pub fn get_quota(&self, tenant_id: &str) -> Result<ResourceQuota, anyhow::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT max_agents, max_sessions_per_user, max_api_calls_per_day, max_memory_mb, max_mcp_servers FROM tenant_quotas WHERE tenant_id = ?",
        )?;

        let quota = stmt.query_row(rusqlite::params![tenant_id], |row| {
            Ok(ResourceQuota {
                max_agents: row.get(0)?,
                max_sessions_per_user: row.get(1)?,
                max_api_calls_per_day: row.get(2)?,
                max_memory_mb: row.get(3)?,
                max_mcp_servers: row.get(4)?,
            })
        })?;

        Ok(quota)
    }
}
