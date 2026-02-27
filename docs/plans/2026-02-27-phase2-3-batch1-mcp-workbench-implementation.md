# MCP Workbench Implementation Plan

> **For REQUIRED SUB-SKILL:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement MCP Workbench with server management, tool invocation, and log viewing capabilities.

**Architecture:** Extend existing MCP infrastructure (McpManager, McpClient traits) with persistence layer (SQLite), REST APIs, and frontend UI components. Backend uses stdio transport; frontend uses tab-based navigation.

**Tech Stack:** Rust (axum, tokio, rusqlite), TypeScript (React, Jotai, Tailwind)

---

## Task 1: Database Schema Extension (Migration V3)

**Files:**
- Modify: `crates/octo-engine/src/db/migrations.rs:4`
- Modify: `crates/octo-engine/src/db/migrations.rs:118-139`

**Step 1: Add migration V3 constant**

Add after MIGRATION_V2:

```rust
const MIGRATION_V3: &str = "
-- MCP Server configurations
CREATE TABLE IF NOT EXISTS mcp_servers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    source      TEXT NOT NULL DEFAULT 'manual',
    command     TEXT NOT NULL,
    args        TEXT NOT NULL DEFAULT '[]',
    env         TEXT NOT NULL DEFAULT '{}',
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- MCP tool execution history
CREATE TABLE IF NOT EXISTS mcp_executions (
    id          TEXT PRIMARY KEY,
    server_id   TEXT NOT NULL,
    tool_name   TEXT NOT NULL,
    params      TEXT NOT NULL,
    result      TEXT,
    error       TEXT,
    duration_ms INTEGER,
    executed_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (server_id) REFERENCES mcp_servers(id)
);

CREATE INDEX IF NOT EXISTS idx_mcp_executions_server
    ON mcp_executions(server_id);
CREATE INDEX IF NOT EXISTS idx_mcp_executions_time
    ON mcp_executions(executed_at DESC);

-- MCP communication logs
CREATE TABLE IF NOT EXISTS mcp_logs (
    id          TEXT PRIMARY KEY,
    server_id   TEXT NOT NULL,
    level       TEXT NOT NULL,
    direction   TEXT NOT NULL,
    method      TEXT,
    params      TEXT,
    result      TEXT,
    raw_data    TEXT,
    duration_ms INTEGER,
    logged_at   TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (server_id) REFERENCES mcp_servers(id)
);

CREATE INDEX IF NOT EXISTS idx_mcp_logs_server_time
    ON mcp_logs(server_id, logged_at);
";
```

**Step 2: Update CURRENT_VERSION**

Change line 4:
```rust
const CURRENT_VERSION: u32 = 3;
```

**Step 3: Add migration V3 execution**

Add in `migrate()` function after V2:
```rust
if version < 3 {
    conn.execute_batch(MIGRATION_V3)?;
    info!("Applied migration v3");
}
```

**Step 4: Verify build**

Run: `cargo check -p octo-engine`
Expected: SUCCESS

**Step 5: Commit**

```bash
git add crates/octo-engine/src/db/migrations.rs
git commit -m "feat(db): add MCP tables migration v3"
```

---

## Task 2: Create MCP Storage Module

**Files:**
- Create: `crates/octo-engine/src/mcp/storage.rs`
- Modify: `crates/octo-engine/src/mcp/mod.rs`

**Step 1: Create storage.rs**

```rust
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
            "SELECT id, name, source, command, args, env, enabled, created_at, updated_at FROM mcp_servers ORDER BY name"
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
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_server(&self, id: &str) -> rusqlite::Result<Option<McpServerRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, source, command, args, env, enabled, created_at, updated_at FROM mcp_servers WHERE id = ?"
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
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn insert_server(&self, server: &McpServerRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_servers (id, name, source, command, args, env, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                server.id,
                server.name,
                server.source,
                server.command,
                server.args,
                server.env,
                server.enabled as i32,
                server.created_at,
                server.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn update_server(&self, server: &McpServerRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE mcp_servers SET name = ?, source = ?, command = ?, args = ?, env = ?, enabled = ?, updated_at = ? WHERE id = ?",
            rusqlite::params![
                server.name,
                server.source,
                server.command,
                server.args,
                server.env,
                server.enabled as i32,
                server.updated_at,
                server.id
            ],
        )?;
        Ok(())
    }

    pub fn delete_server(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM mcp_servers WHERE id = ?", [id])?;
        Ok(())
    }

    // Execution records
    pub fn insert_execution(&self, exec: &McpExecutionRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_executions (id, server_id, tool_name, params, result, error, duration_ms, executed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                exec.id,
                exec.server_id,
                exec.tool_name,
                exec.params,
                exec.result,
                exec.error,
                exec.duration_ms,
                exec.executed_at
            ],
        )?;
        Ok(())
    }

    pub fn list_executions(&self, server_id: &str, limit: usize) -> rusqlite::Result<Vec<McpExecutionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, server_id, tool_name, params, result, error, duration_ms, executed_at FROM mcp_executions WHERE server_id = ? ORDER BY executed_at DESC LIMIT ?"
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
                executed_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    // Log records
    pub fn insert_log(&self, log: &McpLogRecord) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO mcp_logs (id, server_id, level, direction, method, params, result, raw_data, duration_ms, logged_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                log.logged_at
            ],
        )?;
        Ok(())
    }

    pub fn list_logs(&self, server_id: &str, level: Option<&str>, limit: usize, offset: usize) -> rusqlite::Result<Vec<McpLogRecord>> {
        let mut sql = String::from(
            "SELECT id, server_id, level, direction, method, params, result, raw_data, duration_ms, logged_at FROM mcp_logs WHERE server_id = ?"
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
                logged_at: row.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn clear_logs(&self, server_id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM mcp_logs WHERE server_id = ?", [server_id])?;
        Ok(())
    }
}
```

**Step 2: Update mod.rs**

Add to `crates/octo-engine/src/mcp/mod.rs`:
```rust
pub mod storage;
pub use storage::McpStorage;
```

**Step 3: Verify build**

Run: `cargo check -p octo-engine`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/octo-engine/src/mcp/storage.rs crates/octo-engine/src/mcp/mod.rs
git commit -m "feat(mcp): add MCP storage module with SQLite persistence"
```

---

## Task 3: Add McpManager Extensions for Runtime

**Files:**
- Modify: `crates/octo-engine/src/mcp/manager.rs`
- Modify: `crates/octo-engine/src/mcp/traits.rs`

**Step 1: Extend McpServerConfig with ID**

Add to `crates/octo-engine/src/mcp/traits.rs`:
```rust
/// Configuration for an MCP server (persisted version with ID).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigV2 {
    pub id: String,
    pub name: String,
    pub source: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

impl From<McpServerConfigV2> for McpServerConfig {
    fn from(v2: McpServerConfigV2) -> Self {
        Self {
            name: v2.name,
            command: v2.command,
            args: v2.args,
            env: v2.env,
        }
    }
}
```

**Step 2: Add runtime state tracking to McpManager**

Add to `crates/octo-engine/src/mcp/manager.rs`:
```rust
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::tools::ToolRegistry;

use super::bridge::McpToolBridge;
use super::stdio::StdioMcpClient;
use super::traits::{McpClient, McpServerConfig, McpServerConfigV2, McpToolInfo};
use super::storage::McpStorage;

/// Runtime state of an MCP server.
#[derive(Debug, Clone)]
pub enum ServerRuntimeState {
    Stopped,
    Starting,
    Running { pid: u32 },
    Error { message: String },
}

/// Manages multiple MCP server connections.
pub struct McpManager {
    clients: HashMap<String, Arc<RwLock<Box<dyn McpClient>>>>,
    tool_infos: HashMap<String, Vec<McpToolInfo>>,
    runtime_states: HashMap<String, ServerRuntimeState>,
    storage: Option<Arc<McpStorage>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            tool_infos: HashMap::new(),
            runtime_states: HashMap::new(),
            storage: None,
        }
    }

    /// Create with storage for persistence.
    pub fn with_storage(storage: Arc<McpStorage>) -> Self {
        Self {
            clients: HashMap::new(),
            tool_infos: HashMap::new(),
            runtime_states: HashMap::new(),
            storage: Some(storage),
        }
    }

    /// Get all server configs from storage.
    pub async fn load_servers(&self) -> Result<Vec<McpServerConfigV2>> {
        if let Some(storage) = &self.storage {
            let records = storage.list_servers()?;
            Ok(records
                .into_iter()
                .map(|r| McpServerConfigV2 {
                    id: r.id,
                    name: r.name,
                    source: r.source,
                    command: r.command,
                    args: serde_json::from_str(&r.args).unwrap_or_default(),
                    env: serde_json::from_str(&r.env).unwrap_or_default(),
                    enabled: r.enabled,
                })
                .collect())
        } else {
            Ok(vec![])
        }
    }

    /// Set runtime state.
    pub fn set_runtime_state(&mut self, name: &str, state: ServerRuntimeState) {
        self.runtime_states.insert(name.to_string(), state);
    }

    /// Get runtime state.
    pub fn get_runtime_state(&self, name: &str) -> ServerRuntimeState {
        self.runtime_states
            .get(name)
            .cloned()
            .unwrap_or(ServerRuntimeState::Stopped)
    }

    /// Get all runtime states.
    pub fn all_runtime_states(&self) -> HashMap<String, ServerRuntimeState> {
        self.runtime_states.clone()
    }

    /// ... (keep existing methods: add_server, remove_server, bridge_tools, shutdown_all, server_count)
}
```

**Step 3: Verify build**

Run: `cargo check -p octo-engine`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/octo-engine/src/mcp/manager.rs crates/octo-engine/src/mcp/traits.rs
git commit -m "feat(mcp): add runtime state tracking to McpManager"
```

---

## Task 4: Create MCP Server API Module

**Files:**
- Create: `crates/octo-server/src/api/mcp_servers.rs`
- Modify: `crates/octo-server/src/api/mod.rs`

**Step 1: Create mcp_servers.rs**

```rust
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use chrono::Utc;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerConfigRequest {
    pub name: String,
    pub source: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerResponse {
    pub id: String,
    pub name: String,
    pub source: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub enabled: bool,
    pub runtime_status: String,
    pub tool_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerStatusResponse {
    pub id: String,
    pub name: String,
    pub status: String,
    pub pid: Option<u32>,
    pub error: Option<String>,
    pub tool_count: usize,
}

// List all MCP servers
pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<McpServerResponse>> {
    // TODO: Implement with storage
    Json(vec![])
}

// Get single server
pub async fn get_server(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Option<McpServerResponse>> {
    // TODO: Implement with storage
    Json(None)
}

// Create new server
pub async fn create_server(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<McpServerConfigRequest>,
) -> Json<McpServerResponse> {
    let now = Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    Json(McpServerResponse {
        id,
        name: req.name,
        source: req.source.unwrap_or_else(|| "manual".to_string()),
        command: req.command,
        args: req.args,
        env: req.env.unwrap_or_default(),
        enabled: req.enabled.unwrap_or(true),
        runtime_status: "stopped".to_string(),
        tool_count: 0,
        created_at: now.clone(),
        updated_at: now,
    })
}

// Update server
pub async fn update_server(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<McpServerConfigRequest>,
) -> Json<Option<McpServerResponse>> {
    // TODO: Implement with storage
    Json(None)
}

// Delete server
pub async fn delete_server(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    // TODO: Implement with storage
    Json(serde_json::json!({"deleted": id}))
}

// Start server
pub async fn start_server(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    // TODO: Implement with McpManager
    Json(serde_json::json!({"started": id}))
}

// Stop server
pub async fn stop_server(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    // TODO: Implement with McpManager
    Json(serde_json::json!({"stopped": id}))
}

// Get server status
pub async fn get_server_status(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Option<McpServerStatusResponse>> {
    // TODO: Implement with McpManager
    Json(None)
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/servers", get(list_servers))
        .route("/mcp/servers", post(create_server))
        .route("/mcp/servers/{id}", get(get_server))
        .route("/mcp/servers/{id}", put(update_server))
        .route("/mcp/servers/{id}", delete(delete_server))
        .route("/mcp/servers/{id}/start", post(start_server))
        .route("/mcp/servers/{id}/stop", post(stop_server))
        .route("/mcp/servers/{id}/status", get(get_server_status))
}
```

**Step 2: Update mod.rs**

Add to `crates/octo-server/src/api/mod.rs`:
```rust
pub mod mcp_servers;
```

And add route in `routes()` function:
```rust
.merge(mcp_servers::routes())
```

**Step 3: Verify build**

Run: `cargo check -p octo-server`
Expected: SUCCESS (may need to add uuid dependency)

**Step 4: Add missing dependencies**

Run: `cargo add uuid -p octo-server`
Run: `cargo add chrono -p octo-server`

**Step 5: Verify build again**

Run: `cargo check -p octo-server`
Expected: SUCCESS

**Step 6: Commit**

```bash
git add crates/octo-server/src/api/mcp_servers.rs crates/octo-server/src/api/mod.rs
git commit -m "feat(api): add MCP server CRUD endpoints"
```

---

## Task 5: Create MCP Tools API Module

**Files:**
- Create: `crates/octo-server/src/api/mcp_tools.rs`
- Modify: `crates/octo-server/src/api/mod.rs`

**Step 1: Create mcp_tools.rs**

```rust
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolCallRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolCallResponse {
    pub id: String,
    pub server_id: String,
    pub tool_name: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub executed_at: String,
}

// List tools for a server
pub async fn list_tools(
    State(_state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<Vec<McpToolInfo>> {
    // TODO: Get from McpManager
    Json(vec![])
}

// Call a tool
pub async fn call_tool(
    State(_state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(req): Json<McpToolCallRequest>,
) -> Json<McpToolCallResponse> {
    let now = chrono::Utc::now();

    Json(McpToolCallResponse {
        id: uuid::Uuid::new_v4().to_string(),
        server_id,
        tool_name: req.tool_name,
        result: None,
        error: Some("Not implemented".to_string()),
        duration_ms: 0,
        executed_at: now.to_rfc3339(),
    })
}

// List execution history
pub async fn list_executions(
    State(_state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<Vec<McpToolCallResponse>> {
    // TODO: Get from storage
    Json(vec![])
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/servers/{server_id}/tools", get(list_tools))
        .route("/mcp/servers/{server_id}/call", post(call_tool))
        .route("/mcp/servers/{server_id}/executions", get(list_executions))
}
```

**Step 2: Update mod.rs**

Add to `crates/octo-server/src/api/mod.rs`:
```rust
pub mod mcp_tools;
```

And add route:
```rust
.merge(mcp_tools::routes())
```

**Step 3: Verify build**

Run: `cargo check -p octo-server`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/octo-server/src/api/mcp_tools.rs crates/octo-server/src/api/mod.rs
git commit -m "feat(api): add MCP tools API endpoints"
```

---

## Task 6: Create MCP Logs API Module

**Files:**
- Create: `crates/octo-server/src/api/mcp_logs.rs`
- Modify: `crates/octo-server/src/api/mod.rs`

**Step 1: Create mcp_logs.rs**

```rust
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LogQueryParams {
    pub level: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
pub struct McpLogEntry {
    pub id: String,
    pub server_id: String,
    pub level: String,
    pub direction: String,
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub raw_data: Option<String>,
    pub duration_ms: Option<i64>,
    pub logged_at: String,
}

// List logs
pub async fn list_logs(
    State(_state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(params): Query<LogQueryParams>,
) -> Json<Vec<McpLogEntry>> {
    let limit = params.limit.unwrap_or(100);
    let offset = params.offset.unwrap_or(0);

    // TODO: Get from storage
    Json(vec![])
}

// Clear logs
pub async fn clear_logs(
    State(_state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<serde_json::Value> {
    // TODO: Clear from storage
    Json(serde_json::json!({"cleared": server_id}))
}

// Export logs
pub async fn export_logs(
    State(_state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Json<serde_json::Value> {
    // TODO: Export to JSON
    Json(serde_json::json!({"exported": server_id, "format": "json"}))
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp/servers/{server_id}/logs", get(list_logs))
        .route("/mcp/servers/{server_id}/logs", delete(clear_logs))
        .route("/mcp/servers/{server_id}/logs/export", get(export_logs))
}
```

**Step 2: Update mod.rs**

Add:
```rust
pub mod mcp_logs;
.merge(mcp_logs::routes())
```

**Step 3: Verify build**

Run: `cargo check -p octo-server`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/octo-server/src/api/mcp_logs.rs crates/octo-server/src/api/mod.rs
git commit -m "feat(api): add MCP logs API endpoints"
```

---

## Task 7: Frontend - Add MCP Tab

**Files:**
- Modify: `web/src/atoms/ui.ts`
- Modify: `web/src/components/layout/TabBar.tsx`
- Modify: `web/src/App.tsx`

**Step 1: Update TabId type**

Change `web/src/atoms/ui.ts`:
```typescript
export type TabId = "chat" | "tools" | "debug" | "memory" | "mcp";
```

**Step 2: Update TabBar**

Add new tab in `web/src/components/layout/TabBar.tsx`:
```tsx
const tabs = [
  { id: "chat" as const, label: "Chat", icon: MessageSquare },
  { id: "tools" as const, label: "Tools", icon: Wrench },
  { id: "memory" as const, label: "Memory", icon: Brain },
  { id: "debug" as const, label: "Debug", icon: Bug },
  { id: "mcp" as const, label: "MCP", icon: Server },
];
```

**Step 3: Update App.tsx**

Add import and render:
```tsx
import McpWorkbench from "./pages/McpWorkbench";

// In render:
{activeTab === "mcp" && <McpWorkbench />}
```

**Step 4: Verify build**

Run: `cd web && pnpm build`
Expected: SUCCESS (may show error about missing McpWorkbench)

**Step 5: Commit**

```bash
git add web/src/atoms/ui.ts web/src/components/layout/TabBar.tsx web/src/App.tsx
git commit -m "feat(web): add MCP tab to navigation"
```

---

## Task 8: Frontend - Create McpWorkbench Page

**Files:**
- Create: `web/src/pages/McpWorkbench.tsx`

**Step 1: Create basic page structure**

```tsx
import { useState } from "react";

type Tab = "servers" | "invoker" | "logs";

export default function McpWorkbench() {
  const [activeTab, setActiveTab] = useState<Tab>("servers");

  return (
    <div className="h-full flex flex-col">
      {/* Tab Navigation */}
      <div className="flex border-b border-gray-700">
        <button
          className={`px-4 py-2 text-sm font-medium ${
            activeTab === "servers"
              ? "text-blue-400 border-b-2 border-blue-400"
              : "text-gray-400 hover:text-gray-200"
          }`}
          onClick={() => setActiveTab("servers")}
        >
          Servers
        </button>
        <button
          className={`px-4 py-2 text-sm font-medium ${
            activeTab === "invoker"
              ? "text-blue-400 border-b-2 border-blue-400"
              : "text-gray-400 hover:text-gray-200"
          }`}
          onClick={() => setActiveTab("invoker")}
        >
          Tool Invoker
        </button>
        <button
          className={`px-4 py-2 text-sm font-medium ${
            activeTab === "logs"
              ? "text-blue-400 border-b-2 border-blue-400"
              : "text-gray-400 hover:text-gray-200"
          }`}
          onClick={() => setActiveTab("logs")}
        >
          Logs
        </button>
      </div>

      {/* Tab Content */}
      <div className="flex-1 overflow-auto p-4">
        {activeTab === "servers" && <ServerList />}
        {activeTab === "invoker" && <ToolInvoker />}
        {activeTab === "logs" && <LogViewer />}
      </div>
    </div>
  );
}

// Placeholder components
function ServerList() {
  return (
    <div className="text-gray-400">
      <h2 className="text-lg font-medium mb-4">MCP Servers</h2>
      <p>Server list will be implemented here.</p>
    </div>
  );
}

function ToolInvoker() {
  return (
    <div className="text-gray-400">
      <h2 className="text-lg font-medium mb-4">Tool Invoker</h2>
      <p>Tool invocation UI will be implemented here.</p>
    </div>
  );
}

function LogViewer() {
  return (
    <div className="text-gray-400">
      <h2 className="text-lg font-medium mb-4">Logs</h2>
      <p>Log viewer will be implemented here.</p>
    </div>
  );
}
```

**Step 2: Verify build**

Run: `cd web && pnpm build`
Expected: SUCCESS

**Step 3: Commit**

```bash
git add web/src/pages/McpWorkbench.tsx
git commit -m "feat(web): create McpWorkbench page with tabs"
```

---

## Task 9: Frontend - Implement Server List Component

**Files:**
- Create: `web/src/components/mcp/ServerList.tsx`
- Modify: `web/src/pages/McpWorkbench.tsx`

**Step 1: Create ServerList.tsx**

```tsx
import { useState, useEffect } from "react";

interface McpServer {
  id: string;
  name: string;
  source: string;
  command: string;
  args: string[];
  enabled: boolean;
  runtime_status: string;
  tool_count: number;
}

const mockServers: McpServer[] = [
  {
    id: "1",
    name: "filesystem",
    source: "template",
    command: "npx",
    args: ["-y", "@anthropic/mcp-server-filesystem", "/tmp"],
    enabled: true,
    runtime_status: "running",
    tool_count: 5,
  },
  {
    id: "2",
    name: "memory",
    source: "template",
    command: "npx",
    args: ["-y", "@anthropic/mcp-server-memory"],
    enabled: true,
    runtime_status: "stopped",
    tool_count: 0,
  },
];

export function ServerList() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // TODO: Fetch from API
    setTimeout(() => {
      setServers(mockServers);
      setLoading(false);
    }, 500);
  }, []);

  const getStatusIcon = (status: string) => {
    switch (status) {
      case "running":
        return "🟢";
      case "stopped":
        return "⚪";
      case "error":
        return "🔴";
      case "starting":
        return "⏳";
      default:
        return "⚪";
    }
  };

  const getStatusText = (status: string) => {
    switch (status) {
      case "running":
        return "运行中";
      case "stopped":
        return "已停止";
      case "error":
        return "错误";
      case "starting":
        return "启动中";
      default:
        return status;
    }
  };

  if (loading) {
    return <div className="text-gray-400">加载中...</div>;
  }

  return (
    <div>
      <div className="flex justify-between items-center mb-4">
        <h2 className="text-lg font-medium">MCP Servers</h2>
        <div className="flex gap-2">
          <button className="px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 rounded">
            扫描
          </button>
          <button className="px-3 py-1 text-sm bg-blue-600 hover:bg-blue-500 rounded">
            添加
          </button>
        </div>
      </div>

      <div className="space-y-2">
        {servers.map((server) => (
          <div
            key={server.id}
            className="bg-gray-800 rounded-lg p-4 flex items-center justify-between"
          >
            <div className="flex items-center gap-3">
              <span className="text-xl">{getStatusIcon(server.runtime_status)}</span>
              <div>
                <div className="font-medium">{server.name}</div>
                <div className="text-sm text-gray-400">
                  {server.command} {server.args.join(" ")}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-4">
              <span className="text-sm text-gray-400">
                {server.tool_count} tools
              </span>
              <button className="px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 rounded">
                {server.runtime_status === "running" ? "停止" : "启动"}
              </button>
              <button className="px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 rounded">
                调用
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
```

**Step 2: Update McpWorkbench.tsx**

Replace ServerList placeholder:
```tsx
import { ServerList } from "../components/mcp/ServerList";

// In ServerList component:
{activeTab === "servers" && <ServerList />}
```

**Step 3: Verify build**

Run: `cd web && pnpm build`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add web/src/components/mcp/ServerList.tsx web/src/pages/McpWorkbench.tsx
git commit -m "feat(web): add ServerList component to MCP Workbench"
```

---

## Task 10: Frontend - Implement Tool Invoker Component

**Files:**
- Create: `web/src/components/mcp/ToolInvoker.tsx`

**Step 1: Create ToolInvoker.tsx**

```tsx
import { useState } from "react";

interface Tool {
  name: string;
  description?: string;
  input_schema: object;
}

interface Server {
  id: string;
  name: string;
  tools: Tool[];
}

const mockServers: Server[] = [
  {
    id: "1",
    name: "filesystem",
    tools: [
      { name: "read_file", description: "Read a file", input_schema: { path: "string" } },
      { name: "write_file", description: "Write a file", input_schema: { path: "string", content: "string" } },
      { name: "list_directory", description: "List directory", input_schema: { path: "string" } },
    ],
  },
  {
    id: "2",
    name: "memory",
    tools: [
      { name: "memory_search", description: "Search memories", input_schema: { query: "string" } },
      { name: "memory_read", description: "Read memory", input_schema: { id: "string" } },
    ],
  },
];

export function ToolInvoker() {
  const [selectedServer, setSelectedServer] = useState<string>("");
  const [selectedTool, setSelectedTool] = useState<string>("");
  const [params, setParams] = useState<string>("{\n  \n}");
  const [result, setResult] = useState<string>("");
  const [loading, setLoading] = useState(false);

  const server = mockServers.find((s) => s.id === selectedServer);
  const tool = server?.tools.find((t) => t.name === selectedTool);

  const handleServerChange = (serverId: string) => {
    setSelectedServer(serverId);
    setSelectedTool("");
    setResult("");
  };

  const handleExecute = () => {
    setLoading(true);
    // TODO: Call API
    setTimeout(() => {
      setResult(JSON.stringify({ success: true, message: "Tool executed" }, null, 2));
      setLoading(false);
    }, 1000);
  };

  return (
    <div className="h-full flex flex-col">
      {/* Server & Tool Selection */}
      <div className="flex gap-4 mb-4">
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Server</label>
          <select
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2"
            value={selectedServer}
            onChange={(e) => handleServerChange(e.target.value)}
          >
            <option value="">选择 Server...</option>
            {mockServers.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Tool</label>
          <select
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2"
            value={selectedTool}
            onChange={(e) => setSelectedTool(e.target.value)}
            disabled={!selectedServer}
          >
            <option value="">选择 Tool...</option>
            {server?.tools.map((t) => (
              <option key={t.name} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Parameters */}
      {tool && (
        <div className="mb-4">
          <label className="block text-sm text-gray-400 mb-1">
            Parameters (JSON)
          </label>
          <textarea
            className="w-full h-32 bg-gray-800 border border-gray-700 rounded px-3 py-2 font-mono text-sm"
            value={params}
            onChange={(e) => setParams(e.target.value)}
          />
        </div>
      )}

      {/* Execute Button */}
      <div className="mb-4">
        <button
          className="px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded font-medium disabled:opacity-50"
          onClick={handleExecute}
          disabled={!selectedServer || !selectedTool || loading}
        >
          {loading ? "执行中..." : "执行"}
        </button>
      </div>

      {/* Result */}
      {result && (
        <div className="flex-1">
          <label className="block text-sm text-gray-400 mb-1">Result</label>
          <pre className="bg-gray-900 border border-gray-700 rounded p-4 text-sm overflow-auto h-64">
            {result}
          </pre>
        </div>
      )}
    </div>
  );
}
```

**Step 2: Update McpWorkbench.tsx**

Add import and replace placeholder.

**Step 3: Verify build**

Run: `cd web && pnpm build`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add web/src/components/mcp/ToolInvoker.tsx web/src/pages/McpWorkbench.tsx
git commit -m "feat(web): add ToolInvoker component to MCP Workbench"
```

---

## Task 11: Frontend - Implement Log Viewer Component

**Files:**
- Create: `web/src/components/mcp/LogViewer.tsx`

**Step 1: Create LogViewer.tsx**

```tsx
import { useState } from "react";

type LogLevel = "all" | "info" | "debug" | "warn" | "error" | "raw";

interface LogEntry {
  id: string;
  level: LogLevel;
  direction: "request" | "response" | "system";
  method?: string;
  message: string;
  timestamp: string;
}

const mockLogs: LogEntry[] = [
  {
    id: "1",
    level: "info",
    direction: "request",
    method: "tools/call",
    message: '{"name": "read_file", "arguments": {"path": "/tmp/test.txt"}}',
    timestamp: "12:30:15",
  },
  {
    id: "2",
    level: "debug",
    direction: "response",
    method: "tools/call",
    message: '{"content": [{"type": "text", "text": "Hello world"}]}',
    timestamp: "12:30:15",
  },
  {
    id: "3",
    level: "error",
    direction: "response",
    message: '{"code": -32602, "message": "File not found"}',
    timestamp: "12:30:20",
  },
];

export function LogViewer() {
  const [level, setLevel] = useState<LogLevel>("all");
  const [logs, setLogs] = useState<LogEntry[]>(mockLogs);

  const filteredLogs = level === "all" ? logs : logs.filter((l) => l.level === level);

  const getLevelColor = (l: LogLevel) => {
    switch (l) {
      case "info":
        return "text-blue-400";
      case "debug":
        return "text-green-400";
      case "warn":
        return "text-yellow-400";
      case "error":
        return "text-red-400";
      case "raw":
        return "text-gray-400";
      default:
        return "text-gray-400";
    }
  };

  const getDirectionIcon = (d: string) => {
    switch (d) {
      case "request":
        return "→";
      case "response":
        return "←";
      default:
        return "•";
    }
  };

  return (
    <div className="h-full flex flex-col">
      {/* Toolbar */}
      <div className="flex justify-between items-center mb-4">
        <div className="flex gap-2">
          {(["all", "info", "debug", "warn", "error", "raw"] as LogLevel[]).map((l) => (
            <button
              key={l}
              className={`px-3 py-1 text-sm rounded ${
                level === l ? "bg-blue-600" : "bg-gray-700 hover:bg-gray-600"
              }`}
              onClick={() => setLevel(l)}
            >
              {l.toUpperCase()}
            </button>
          ))}
        </div>
        <div className="flex gap-2">
          <button
            className="px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 rounded"
            onClick={() => setLogs([])}
          >
            清空
          </button>
          <button className="px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 rounded">
            导出
          </button>
        </div>
      </div>

      {/* Log List */}
      <div className="flex-1 overflow-auto bg-gray-900 rounded border border-gray-700">
        {filteredLogs.length === 0 ? (
          <div className="p-4 text-gray-400">暂无日志</div>
        ) : (
          filteredLogs.map((log) => (
            <div key={log.id} className="border-b border-gray-800 p-2 font-mono text-sm">
              <span className="text-gray-500">{log.timestamp}</span>
              <span className={`mx-2 ${getLevelColor(log.level)}`}>
                {log.level.toUpperCase()}
              </span>
              <span className="text-gray-400 mx-1">{getDirectionIcon(log.direction)}</span>
              {log.method && <span className="text-blue-300 mr-2">{log.method}</span>}
              <span className="text-gray-300">{log.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
```

**Step 2: Update McpWorkbench.tsx**

Add import and replace placeholder.

**Step 3: Verify build**

Run: `cd web && pnpm build`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add web/src/components/mcp/LogViewer.tsx web/src/pages/McpWorkbench.tsx
git commit -m "feat(web): add LogViewer component to MCP Workbench"
```

---

## Task 12: Full Stack Integration - Connect API to Frontend

**Files:**
- Modify: `web/src/pages/McpWorkbench.tsx`

**Step 1: Add API hooks**

Update ServerList to fetch from real API:
```tsx
useEffect(() => {
  fetch("/api/mcp/servers")
    .then((res) => res.json())
    .then(setServers)
    .catch(console.error);
}, []);
```

**Step 2: Test full flow**

Run: `cargo run -p octo-server &`
Run: `cd web && pnpm dev`

Expected: MCP tab visible, can see mock servers, can switch tabs

**Step 3: Commit**

```bash
git add web/src/pages/McpWorkbench.tsx
git commit -m "feat(web): connect MCP Workbench to API"
```

---

## Summary

**Completed Tasks:** 12
**Files Created:** 9 (storage.rs, mcp_servers.rs, mcp_tools.rs, mcp_logs.rs, McpWorkbench.tsx, ServerList.tsx, ToolInvoker.tsx, LogViewer.tsx)
**Files Modified:** 6 (migrations.rs, mod.rs, manager.rs, traits.rs, ui.ts, App.tsx, TabBar.tsx)

**Next Steps:**
1. Implement full McpManager integration with storage
2. Add WebSocket events for real-time updates
3. Implement server start/stop with process management
4. Add template list and .mcp.json scan features

---

## Execution Options

**Plan complete and saved to `docs/plans/2026-02-27-phase2-3-batch1-mcp-workbench-implementation.md`. Two execution options:**

**1. Subagent-Driven (this session)** - I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** - Open new session with executing-plans, batch execution with checkpoints

**Which approach?**
