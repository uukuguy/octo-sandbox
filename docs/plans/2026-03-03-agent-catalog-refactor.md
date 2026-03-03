# Agent Catalog 重构实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 `AgentCatalog` 精简为纯 CRUD 数据层，把生命周期状态机（mark_*/AgentExecutorHandle）从 Catalog 移入 `AgentRuntime`，同时将 `registry/` 目录拍平为 `agent/catalog.rs + entry.rs + store.rs`，REST API 改为通过 `AgentRuntime` 的公开方法操控 agent 生命周期。

**Architecture:**
- `AgentCatalog`（`agent/catalog.rs`）：纯 CRUD，只存 `AgentEntry`（状态字段保留供查询），不再持有 `AgentExecutorHandle`，不再有 `mark_*` 方法
- `AgentRuntime`（`agent/runtime.rs`）：已有 `start/stop/pause/resume` 私有方法，改为公开；cancel token 管理移入 `AgentRuntime.handles: DashMap<AgentId, CancellationToken>`
- REST API（`api/agents.rs`）：`start_agent` 等改调 `state.agent_supervisor.start(...)` 而不是 `catalog().mark_*()`

**Tech Stack:** Rust, Tokio, DashMap

---

## 背景：当前问题

`registry/lifecycle.rs` 给 `AgentCatalog` 添加了 `mark_running/mark_stopped/mark_paused/mark_resumed` 方法，并在 `AgentCatalog` 内部存储了 `AgentExecutorHandle { cancel_token }`。

问题：
1. **职责混乱**：Catalog 是数据层，不该持有运行时句柄
2. **API 绕过 AgentRuntime**：REST API 直接调 `catalog().mark_running()`，实际上**没有 spawn AgentExecutor**，只改了状态标记——功能 bug
3. **文件结构混乱**：`registry/lifecycle.rs` 存在感弱，令人困惑

## 目标架构

```
AgentCatalog（catalog.rs）
  - by_id: DashMap<AgentId, AgentEntry>   ← 去掉 Option<AgentExecutorHandle>
  - register / get / get_by_name / list_all / unregister
  - update_state（内部用，供 AgentRuntime 调用）
  - 无 mark_* 方法，无 AgentExecutorHandle

AgentRuntime（runtime.rs）
  - handles: DashMap<SessionId, AgentExecutorHandle>   ← 已有
  - agent_handles: DashMap<AgentId, CancellationToken> ← 新增，替代 catalog 内的 handle
  - pub start(agent_id, session_id, ...) → Result<AgentExecutorHandle, AgentError>   ← 已有，改公开
  - pub stop(agent_id, session_id)       ← 已有，改公开
  - pub pause(agent_id, session_id)      ← 已有，改公开
  - pub resume(agent_id)                 ← 已有，改公开

REST API（api/agents.rs）
  - start_agent → state.agent_supervisor.start(...)
  - stop_agent  → state.agent_supervisor.stop(...)
  - pause_agent → state.agent_supervisor.pause(...)
  - resume_agent→ state.agent_supervisor.resume(...)
```

## 文件变化一览

| 操作 | 文件 |
|------|------|
| 删除 | `crates/octo-engine/src/agent/registry/lifecycle.rs` |
| 删除 | `crates/octo-engine/src/agent/registry/` 目录 |
| 新建 | `crates/octo-engine/src/agent/catalog.rs`（来自 registry/mod.rs，精简） |
| 新建 | `crates/octo-engine/src/agent/entry.rs`（来自 registry/entry.rs，原样） |
| 新建 | `crates/octo-engine/src/agent/store.rs`（来自 registry/store.rs，原样） |
| 修改 | `crates/octo-engine/src/agent/runtime.rs`（新增 agent_handles，start/stop/pause/resume 改公开） |
| 修改 | `crates/octo-engine/src/agent/mod.rs`（更新模块声明和 re-export） |
| 修改 | `crates/octo-engine/src/lib.rs`（更新 re-export） |
| 修改 | `crates/octo-server/src/api/agents.rs`（改调 AgentRuntime 方法） |

---

## Task 1：创建 `entry.rs` 和 `store.rs`（原样复制，修正 use 路径）

**Files:**
- Create: `crates/octo-engine/src/agent/entry.rs`
- Create: `crates/octo-engine/src/agent/store.rs`

### Step 1：创建 entry.rs

内容与 `registry/entry.rs` 完全一致（路径无内部引用，直接复制）：

```rust
//! AgentEntry and AgentManifest - core catalog types

use serde::{Deserialize, Serialize};

use crate::agent::AgentConfig;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub backstory: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_filter: Vec<String>,
    #[serde(default)]
    pub config: AgentConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Error(String),
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Created
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Stopped => write!(f, "stopped"),
            Self::Error(e) => write!(f, "error: {e}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub id: AgentId,
    pub manifest: AgentManifest,
    pub state: AgentStatus,
    pub created_at: i64,
}

impl AgentEntry {
    pub fn new(manifest: AgentManifest) -> Self {
        Self {
            id: AgentId::new(),
            manifest,
            state: AgentStatus::Created,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }
}
```

### Step 2：创建 store.rs

内容与 `registry/store.rs` 完全一致，只修正 use 路径（`super::entry::` → `crate::agent::entry::`，或直接用 `super::`——因为 store.rs 在 agent/ 下，super 就是 agent mod）：

```rust
//! AgentCatalog SQLite persistence layer

use anyhow::Result;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::agent::entry::{AgentEntry, AgentId, AgentManifest, AgentStatus};

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
                    "running" | "paused" => AgentStatus::Stopped,
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
```

### Step 3：编译验证（预期通过，新文件还未被引用）

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -10
```

Expected: 零错误

### Step 4：Commit

```bash
git add crates/octo-engine/src/agent/entry.rs crates/octo-engine/src/agent/store.rs
git commit -m "feat(agent): add entry.rs and store.rs (flattening registry/)"
```

---

## Task 2：创建精简版 `catalog.rs`（去掉 AgentExecutorHandle 和 mark_* 方法）

**Files:**
- Create: `crates/octo-engine/src/agent/catalog.rs`

### Step 1：创建 catalog.rs

`AgentCatalog` 保留所有 CRUD 方法，新增 `update_state`（供 AgentRuntime 调用），去掉：
- `AgentExecutorHandle` 内部类型
- `by_id` 值类型从 `(AgentEntry, Option<AgentExecutorHandle>)` → `AgentEntry`
- `mark_running/mark_stopped/mark_paused/mark_resumed` 方法
- `AgentError`（移到 `entry.rs` 或单独在 runtime 定义）

> **注意**：`AgentError` 目前在 `lifecycle.rs` 定义，REST API 和 runtime 都用它。把它移入 `entry.rs`（与数据类型放一起）。

```rust
//! AgentCatalog - concurrent multi-index store for agent definitions

use std::sync::Arc;

use dashmap::DashMap;

use crate::agent::entry::{AgentEntry, AgentId, AgentManifest, AgentStatus};
use crate::agent::store::AgentStore;

pub struct AgentCatalog {
    by_id: DashMap<AgentId, AgentEntry>,
    by_name: DashMap<String, AgentId>,
    by_tag: DashMap<String, Vec<AgentId>>,
    store: Option<Arc<AgentStore>>,
}

impl AgentCatalog {
    pub fn new() -> Self {
        Self {
            by_id: DashMap::new(),
            by_name: DashMap::new(),
            by_tag: DashMap::new(),
            store: None,
        }
    }

    pub fn with_store(mut self, store: Arc<AgentStore>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn load_from_store(&self) -> anyhow::Result<usize> {
        if let Some(store) = &self.store {
            let entries = store.load_all()?;
            let count = entries.len();
            for entry in entries {
                let id = entry.id.clone();
                let name = entry.manifest.name.clone();
                let tags = entry.manifest.tags.clone();
                self.by_id.insert(id.clone(), entry);
                self.by_name.insert(name, id.clone());
                for tag in &tags {
                    self.by_tag.entry(tag.clone()).or_default().push(id.clone());
                }
            }
            Ok(count)
        } else {
            Ok(0)
        }
    }

    pub fn register(&self, manifest: AgentManifest) -> AgentId {
        let entry = AgentEntry::new(manifest);
        let id = entry.id.clone();
        let name = entry.manifest.name.clone();
        let tags = entry.manifest.tags.clone();
        self.by_id.insert(id.clone(), entry.clone());
        self.by_name.insert(name, id.clone());
        for tag in &tags {
            self.by_tag.entry(tag.clone()).or_default().push(id.clone());
        }
        if let Some(store) = &self.store {
            if let Err(e) = store.save(&entry) {
                tracing::warn!("AgentStore.save failed for {id}: {e}");
            }
        }
        id
    }

    pub fn get(&self, id: &AgentId) -> Option<AgentEntry> {
        self.by_id.get(id).map(|r| r.value().clone())
    }

    pub fn get_by_name(&self, name: &str) -> Option<AgentEntry> {
        self.by_name.get(name).and_then(|id| self.get(id.value()))
    }

    pub fn get_by_tag(&self, tag: &str) -> Vec<AgentEntry> {
        self.by_tag
            .get(tag)
            .map(|ids| ids.value().iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn list_all(&self) -> Vec<AgentEntry> {
        self.by_id.iter().map(|r| r.value().clone()).collect()
    }

    pub fn unregister(&self, id: &AgentId) -> Option<AgentEntry> {
        let entry = {
            let slot = self.by_id.get(id)?;
            slot.value().clone()
        };
        self.by_name.remove(&entry.manifest.name);
        for tag in &entry.manifest.tags {
            if let Some(mut ids) = self.by_tag.get_mut(tag) {
                ids.retain(|i| i != id);
            }
        }
        let removed = self.by_id.remove(id).map(|(_, e)| e);
        if removed.is_some() {
            if let Some(store) = &self.store {
                if let Err(e) = store.delete(id) {
                    tracing::warn!("AgentStore.delete failed for {id}: {e}");
                }
            }
        }
        removed
    }

    /// Update agent state in memory and persist to store.
    /// Called by AgentRuntime on lifecycle transitions.
    pub fn update_state(&self, id: &AgentId, state: AgentStatus) {
        if let Some(mut slot) = self.by_id.get_mut(id) {
            slot.value_mut().state = state.clone();
        }
        if let Some(store) = &self.store {
            if let Err(e) = store.update_state(id, &state) {
                tracing::warn!("AgentStore.update_state failed for {id}: {e}");
            }
        }
    }

    pub fn state(&self, id: &AgentId) -> Option<AgentStatus> {
        self.by_id.get(id).map(|r| r.value().state.clone())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

impl Default for AgentCatalog {
    fn default() -> Self {
        Self::new()
    }
}
```

### Step 2：将 `AgentError` 追加到 `entry.rs`

在 `entry.rs` 末尾追加：

```rust
#[derive(Debug)]
pub enum AgentError {
    NotFound(AgentId),
    InvalidTransition { from: AgentStatus, action: &'static str },
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "agent not found: {id}"),
            Self::InvalidTransition { from, action } => {
                write!(f, "cannot {action} agent in state {from}")
            }
        }
    }
}

impl std::error::Error for AgentError {}
```

### Step 3：编译验证（预期通过，新文件还未被引用）

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -10
```

Expected: 零错误

### Step 4：Commit

```bash
git add crates/octo-engine/src/agent/catalog.rs crates/octo-engine/src/agent/entry.rs
git commit -m "feat(agent): add catalog.rs (pure CRUD, no lifecycle) and AgentError in entry.rs"
```

---

## Task 3：更新 `agent/mod.rs` — 切换到新模块，删除 registry/

**Files:**
- Modify: `crates/octo-engine/src/agent/mod.rs`
- Delete: `crates/octo-engine/src/agent/registry/` 目录

### Step 1：重写 mod.rs

```rust
pub mod cancellation;
pub mod catalog;
pub mod config;
pub mod context;
pub mod entry;
pub mod executor;
pub mod extension;
pub mod loop_;
pub mod loop_guard;
pub mod parallel;
pub mod queue;
pub mod runtime;
pub mod store;

pub use cancellation::{CancellationToken, ChildCancellationToken};
pub use catalog::AgentCatalog;
pub use config::AgentConfig;
pub use entry::{AgentEntry, AgentError, AgentId, AgentManifest, AgentStatus};
pub use executor::{AgentExecutor, AgentExecutorHandle, AgentMessage};
pub use extension::{AgentExtension, ExtensionEvent, ExtensionRegistry};
pub use loop_::{AgentEvent, AgentLoop};
pub use queue::{MessageQueue, QueueKind, QueueMode};
pub use runtime::AgentRuntime;
pub use store::AgentStore;
```

### Step 2：删除 registry/ 目录

```bash
git rm -r crates/octo-engine/src/agent/registry/
```

### Step 3：编译验证

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

Expected: `runtime.rs` 中引用 `registry::` 相关类型的 use 路径报错（下一 Task 修复），其他零错误。

### Step 4：Commit（即使有编译错误）

```bash
git add crates/octo-engine/src/agent/mod.rs
git commit -m "refactor(agent): flatten registry/ into agent/ — update mod.rs, remove registry dir"
```

---

## Task 4：更新 `runtime.rs` — 修正 use 路径，新增 agent_handles，start/stop/pause/resume 改公开

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

### Step 1：修正 use 路径

将：
```rust
use crate::agent::{AgentCatalog, AgentConfig, AgentError, AgentEvent, AgentId, AgentManifest, AgentMessage, AgentExecutor, AgentExecutorHandle, CancellationToken};
```

改为：
```rust
use crate::agent::{
    AgentCatalog, AgentConfig, AgentEntry, AgentError, AgentEvent, AgentId, AgentManifest,
    AgentMessage, AgentExecutor, AgentExecutorHandle, AgentStatus, CancellationToken,
};
```

### Step 2：新增 `agent_handles` 字段

在 `AgentRuntime` struct 中新增字段（在 `handles` 之后）：

```rust
/// AgentId → CancellationToken，用于 stop/pause 时取消正在运行的 AgentExecutor
agent_handles: DashMap<AgentId, CancellationToken>,
```

在 `AgentRuntime::new()` 的 `Self { ... }` 中初始化：

```rust
agent_handles: DashMap::new(),
```

### Step 3：修改 `get_or_spawn` 中的 catalog 调用

将：
```rust
if let Some(id) = agent_id {
    let cancel_token = CancellationToken::new();
    let _ = self.catalog.mark_running(id, cancel_token);
}
```

改为：
```rust
if let Some(id) = agent_id {
    let cancel_token = CancellationToken::new();
    self.agent_handles.insert(id.clone(), cancel_token);
    self.catalog.update_state(id, AgentStatus::Running);
}
```

### Step 4：将 `start/stop/pause/resume` 改为 `pub`，更新内部 catalog 调用

**`start` 方法**（已有，改 pub）：
```rust
pub fn start(
    &self,
    agent_id: &AgentId,
    session_id: SessionId,
    user_id: UserId,
    sandbox_id: SandboxId,
    initial_history: Vec<ChatMessage>,
) -> Result<AgentExecutorHandle, AgentError> {
    // 验证 agent 存在且状态合法
    let entry = self.catalog
        .get(agent_id)
        .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
    match entry.state {
        AgentStatus::Created | AgentStatus::Paused => {}
        ref other => return Err(AgentError::InvalidTransition {
            from: other.clone(),
            action: "start",
        }),
    }
    let handle = self.get_or_spawn(
        session_id,
        user_id,
        sandbox_id,
        initial_history,
        Some(agent_id),
    );
    Ok(handle)
}
```

**`stop` 方法**（已有 async，改 pub）：
```rust
pub async fn stop(&self, agent_id: &AgentId, session_id: &SessionId) -> Result<(), AgentError> {
    // 验证状态
    let entry = self.catalog
        .get(agent_id)
        .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
    if entry.state == AgentStatus::Stopped {
        return Err(AgentError::InvalidTransition {
            from: AgentStatus::Stopped,
            action: "stop",
        });
    }
    // 取消运行中的 executor
    if let Some((_, token)) = self.agent_handles.remove(agent_id) {
        token.cancel();
    }
    if let Some(handle) = self.get(session_id) {
        let _ = handle.send(AgentMessage::Cancel).await;
    }
    self.remove(session_id);
    self.catalog.update_state(agent_id, AgentStatus::Stopped);
    Ok(())
}
```

**`pause` 方法**（已有 async，改 pub）：
```rust
pub async fn pause(&self, agent_id: &AgentId, session_id: &SessionId) -> Result<(), AgentError> {
    let entry = self.catalog
        .get(agent_id)
        .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
    if entry.state != AgentStatus::Running {
        return Err(AgentError::InvalidTransition {
            from: entry.state.clone(),
            action: "pause",
        });
    }
    if let Some((_, token)) = self.agent_handles.remove(agent_id) {
        token.cancel();
    }
    if let Some(handle) = self.get(session_id) {
        let _ = handle.send(AgentMessage::Cancel).await;
    }
    self.catalog.update_state(agent_id, AgentStatus::Paused);
    Ok(())
}
```

**`resume` 方法**（已有，改 pub）：
```rust
pub fn resume(&self, agent_id: &AgentId) -> Result<(), AgentError> {
    let entry = self.catalog
        .get(agent_id)
        .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?;
    if entry.state != AgentStatus::Paused {
        return Err(AgentError::InvalidTransition {
            from: entry.state.clone(),
            action: "resume",
        });
    }
    let cancel_token = CancellationToken::new();
    self.agent_handles.insert(agent_id.clone(), cancel_token);
    self.catalog.update_state(agent_id, AgentStatus::Running);
    Ok(())
}
```

### Step 5：编译验证

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

Expected: 零错误（或只剩 api/agents.rs 的调用报错，那是下一 Task）

### Step 6：Commit

```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -m "refactor(runtime): add agent_handles, expose start/stop/pause/resume as pub, use catalog.update_state"
```

---

## Task 5：更新 `lib.rs` — 修正 re-export

**Files:**
- Modify: `crates/octo-engine/src/lib.rs`

### Step 1：将 agent re-export 行改为

```rust
pub use agent::{
    AgentCatalog, AgentEntry, AgentError, AgentEvent, AgentId, AgentLoop,
    AgentManifest, AgentMessage, AgentExecutor, AgentExecutorHandle,
    AgentStatus, AgentStore, AgentRuntime,
};
```

（与当前基本一致，确保 `AgentError` 从 `entry` 而不是 `lifecycle` 来）

### Step 2：编译验证

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -10
```

Expected: 零错误

### Step 3：Commit

```bash
git add crates/octo-engine/src/lib.rs
git commit -m "refactor(lib): update agent re-exports after registry/ flatten"
```

---

## Task 6：更新 `api/agents.rs` — 改调 AgentRuntime 方法

**Files:**
- Modify: `crates/octo-server/src/api/agents.rs`

### Step 1：删除 CancellationToken import

将：
```rust
use octo_engine::{agent::CancellationToken, AgentEntry, AgentError, AgentId, AgentManifest};
```

改为：
```rust
use octo_engine::{AgentEntry, AgentError, AgentId, AgentManifest};
```

### Step 2：重写 start_agent

```rust
async fn start_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    // 用默认 session（与 ws.rs 一致的主 session）
    // start() 验证状态、spawn AgentExecutor、更新 catalog
    // 注意：REST 触发的 start 使用空 history，实际对话通过 WebSocket 进行
    // TODO: 当需要多 session 支持时，从请求体中读取 session_id
    use octo_types::{SandboxId, SessionId, UserId};
    let session_id = SessionId::new();
    let user_id = UserId::from("api");
    let sandbox_id = SandboxId::from("default");
    s.agent_supervisor
        .start(&agent_id, session_id, user_id, sandbox_id, vec![])
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
```

### Step 3：重写 stop_agent

```rust
async fn stop_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    use octo_types::SessionId;
    // TODO: 从请求体或 session 映射中获取 session_id
    let session_id = SessionId::from(agent_id.0.clone());
    s.agent_supervisor
        .stop(&agent_id, &session_id)
        .await
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
```

### Step 4：重写 pause_agent

```rust
async fn pause_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    use octo_types::SessionId;
    let session_id = SessionId::from(agent_id.0.clone());
    s.agent_supervisor
        .pause(&agent_id, &session_id)
        .await
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
```

### Step 5：重写 resume_agent

```rust
async fn resume_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_supervisor
        .resume(&agent_id)
        .map_err(agent_err_to_status)?;
    s.agent_supervisor
        .catalog()
        .get(&agent_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
```

### Step 6：全量编译验证

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

Expected: 零错误

### Step 7：Commit

```bash
git add crates/octo-server/src/api/agents.rs
git commit -m "refactor(agents-api): call AgentRuntime.start/stop/pause/resume instead of catalog.mark_*()"
```

---

## Task 7：验证与收尾

### Step 1：全量编译

```bash
cargo check --workspace 2>&1 | grep "^error"
```

Expected: 无输出

### Step 2：验证 registry/ 已完全删除

```bash
ls crates/octo-engine/src/agent/registry/ 2>&1
```

Expected: `No such file or directory`

### Step 3：验证 catalog 无 mark_* 方法

```bash
grep -n "mark_" crates/octo-engine/src/agent/catalog.rs
```

Expected: 无输出

### Step 4：验证 API 层不再直接调 catalog 的生命周期方法

```bash
grep -rn "catalog()\.mark_\|catalog()\.update_state" crates/octo-server/src/api/
```

Expected: 无输出（`update_state` 只由 runtime 内部调用）

### Step 5：运行测试

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: 所有测试通过

### Step 6：最终 Commit

```bash
git add docs/plans/.checkpoint.json
git commit -m "checkpoint: agent catalog refactor complete — registry/ flattened, lifecycle moved to AgentRuntime"
```
