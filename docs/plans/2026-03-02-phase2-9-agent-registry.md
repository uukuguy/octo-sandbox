# Phase 2.11: AgentRegistry + 上下文工程重构

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**：引入完整的多代理注册表（AgentRegistry）和上下文工程重构，对齐 OpenFang 架构设计。

**核心设计决策**：
- Agent 身份由 `AgentManifest`（role/goal/backstory）定义，存入 SQLite 持久化
- System prompt 按优先级构建：`system_prompt` > `role/goal/backstory` > `SOUL.md` > `CORE_INSTRUCTIONS`
- Working memory（Zone B）改为注入首条 Human Message，不再混入 system prompt
- `AgentRunner` 负责启动逻辑，`AgentRegistry` 只管注册表状态
- per-agent `ToolRegistry`：按 `tool_filter` 白名单裁剪全局 tools

**技术栈**：Rust async/tokio、DashMap、Arc、SQLite（rusqlite）

---

## 架构总览

```
AppState
  └─ agent_runner: Arc<AgentRunner>
       ├─ registry: Arc<AgentRegistry>    ← DashMap 三索引 + SQLite 持久化
       ├─ provider: Arc<dyn Provider>
       ├─ tools: Arc<ToolRegistry>        ← 全局工具集（用于裁剪）
       ├─ memory: Arc<dyn WorkingMemory>
       ├─ model: String                   ← 默认模型
       └─ event_bus: Option<Arc<EventBus>>

AgentEntry（Registry 存储单元）
  ├─ id: AgentId
  ├─ manifest: AgentManifest
  │    ├─ name: String
  │    ├─ tags: Vec<String>
  │    ├─ role: Option<String>            ← 三段式身份（CrewAI 模式）
  │    ├─ goal: Option<String>
  │    ├─ backstory: Option<String>
  │    ├─ system_prompt: Option<String>   ← 直接覆盖（最高优先级）
  │    ├─ model: Option<String>           ← 覆盖默认模型
  │    ├─ tool_filter: Vec<String>        ← 工具白名单（空=全部）
  │    └─ config: AgentConfig            ← 运行时行为参数
  ├─ state: AgentStatus
  └─ created_at: i64

System Prompt 构建（Zone A，静态）
  优先级: system_prompt > role/goal/backstory > SOUL.md > CORE_INSTRUCTIONS
  + Bootstrap 文件（AGENTS.md 等）
  + 工具规范 / 输出格式

Dynamic Context（Zone B，每轮首条 Human Message）
  <context>
    <datetime>...</datetime>
    <user_profile>...</user_profile>
    <task_context>...</task_context>
    <memory>...</memory>
  </context>
```

---

## 文件索引

### 新增文件
| 文件 | Task | 说明 |
|------|------|------|
| `crates/octo-engine/src/agent/registry/mod.rs` | Task 1 | AgentRegistry 核心（DashMap 三索引） |
| `crates/octo-engine/src/agent/registry/entry.rs` | Task 1 | AgentEntry + AgentManifest + AgentStatus |
| `crates/octo-engine/src/agent/registry/store.rs` | Task 2 | SQLite 持久化（参考 McpStorage 模式） |
| `crates/octo-engine/src/agent/runner.rs` | Task 3 | AgentRunner（启动/停止/pause/resume） |
| `crates/octo-server/src/api/agents.rs` | Task 6 | REST API 端点 |

### 修改文件
| 文件 | Task | 说明 |
|------|------|------|
| `crates/octo-engine/src/agent/mod.rs` | Task 1 | 导出 registry/runner |
| `crates/octo-engine/src/lib.rs` | Task 1 | pub use 新类型 |
| `crates/octo-types/src/memory.rs` | Task 4 | 删除 AgentPersona，SandboxContext 标记 deprecated |
| `crates/octo-engine/src/memory/working.rs` | Task 4 | 默认 blocks 清理 |
| `crates/octo-engine/src/memory/injector.rs` | Task 4 | 输出改为 `<context>` 包裹 |
| `crates/octo-engine/src/context/builder.rs` | Task 5 | SystemPromptBuilder 接入 AgentLoop |
| `crates/octo-engine/src/agent/loop_.rs` | Task 5 | Zone A/B 分离，接入 AgentManifest |
| `crates/octo-server/src/state.rs` | Task 6 | agent_runner 替代 agent_registry |
| `crates/octo-server/src/router.rs` | Task 6 | 注册 /api/v1/agents 路由 |
| `crates/octo-server/src/api/mod.rs` | Task 6 | 导出 agents 模块 |
| `crates/octo-engine/src/context/budget.rs` | Task 7 | budget 统一，对齐 ContextInjector |

---

## Task 1：AgentRegistry 核心结构

**目标**：AgentEntry + AgentManifest + AgentRegistry（DashMap 三索引）。

### Step 1: 创建 `entry.rs`

```rust
//! AgentEntry and AgentManifest - core registry types

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
    fn default() -> Self { Self::new() }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent specification - provided at creation time, defines identity and behavior.
///
/// System prompt priority:
///   system_prompt > role/goal/backstory > SOUL.md > CORE_INSTRUCTIONS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,

    // Identity (three-part, CrewAI pattern)
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub backstory: Option<String>,

    // Full system prompt override (highest priority, skips role/goal/backstory)
    #[serde(default)]
    pub system_prompt: Option<String>,

    // Runtime overrides
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tool_filter: Vec<String>,  // empty = all tools available
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
    fn default() -> Self { Self::Created }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Running => write!(f, "running"),
            Self::Paused  => write!(f, "paused"),
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

### Step 2: 创建 `registry/mod.rs`

```rust
//! Agent Registry - concurrent multi-index store
mod entry;
pub mod lifecycle;

pub use entry::{AgentEntry, AgentId, AgentManifest, AgentStatus};
pub use lifecycle::AgentError;

use dashmap::DashMap;
use crate::agent::CancellationToken;

pub(crate) struct AgentRuntimeHandle {
    pub cancel_token: CancellationToken,
}

pub struct AgentRegistry {
    by_id:   DashMap<AgentId, (AgentEntry, Option<AgentRuntimeHandle>)>,
    by_name: DashMap<String, AgentId>,
    by_tag:  DashMap<String, Vec<AgentId>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            by_id:   DashMap::new(),
            by_name: DashMap::new(),
            by_tag:  DashMap::new(),
        }
    }

    pub fn register(&self, manifest: AgentManifest) -> AgentId {
        let entry = AgentEntry::new(manifest);
        let id = entry.id.clone();
        self.by_name.insert(entry.manifest.name.clone(), id.clone());
        for tag in &entry.manifest.tags {
            self.by_tag.entry(tag.clone()).or_default().push(id.clone());
        }
        self.by_id.insert(id.clone(), (entry, None));
        id
    }

    pub fn get(&self, id: &AgentId) -> Option<AgentEntry> {
        self.by_id.get(id).map(|r| r.value().0.clone())
    }

    pub fn get_by_name(&self, name: &str) -> Option<AgentEntry> {
        self.by_name.get(name).and_then(|id| self.get(id.value()))
    }

    pub fn get_by_tag(&self, tag: &str) -> Vec<AgentEntry> {
        self.by_tag.get(tag)
            .map(|ids| ids.value().iter().filter_map(|id| self.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn list_all(&self) -> Vec<AgentEntry> {
        self.by_id.iter().map(|r| r.value().0.clone()).collect()
    }

    pub fn unregister(&self, id: &AgentId) -> Option<AgentEntry> {
        self.by_id.remove(id).map(|(_, (entry, handle))| {
            if let Some(h) = handle { h.cancel_token.cancel(); }
            self.by_name.remove(&entry.manifest.name);
            for tag in &entry.manifest.tags {
                if let Some(mut ids) = self.by_tag.get_mut(tag) {
                    ids.retain(|i| i != id);
                }
            }
            entry
        })
    }

    pub(crate) fn set_state(&self, id: &AgentId, state: AgentStatus) -> bool {
        self.by_id.get_mut(id).map(|mut r| { r.value_mut().0.state = state; true }).unwrap_or(false)
    }

    pub(crate) fn set_handle(&self, id: &AgentId, handle: Option<AgentRuntimeHandle>) -> bool {
        self.by_id.get_mut(id).map(|mut r| { r.value_mut().1 = handle; true }).unwrap_or(false)
    }

    pub fn state(&self, id: &AgentId) -> Option<AgentStatus> {
        self.by_id.get(id).map(|r| r.value().0.state.clone())
    }

    pub fn len(&self) -> usize { self.by_id.len() }
    pub fn is_empty(&self) -> bool { self.by_id.is_empty() }
}

impl Default for AgentRegistry {
    fn default() -> Self { Self::new() }
}
```

### Step 3: 创建 `lifecycle.rs`

```rust
//! Lifecycle state machine: start / stop / pause / resume

use super::{AgentId, AgentRegistry, AgentRuntimeHandle, AgentStatus};
use crate::agent::CancellationToken;

#[derive(Debug)]
pub enum AgentError {
    NotFound(AgentId),
    InvalidTransition { from: AgentStatus, action: &'static str },
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "agent not found: {id}"),
            Self::InvalidTransition { from, action } =>
                write!(f, "cannot {action} agent in state {from}"),
        }
    }
}

impl std::error::Error for AgentError {}

impl AgentRegistry {
    /// Mark agent as Running. Actual AgentLoop spawned by AgentRunner.
    pub fn mark_running(&self, id: &AgentId, cancel_token: CancellationToken) -> Result<(), AgentError> {
        let state = self.state(id).ok_or_else(|| AgentError::NotFound(id.clone()))?;
        match state {
            AgentStatus::Created | AgentStatus::Paused => {
                self.set_handle(id, Some(AgentRuntimeHandle { cancel_token }));
                self.set_state(id, AgentStatus::Running);
                Ok(())
            }
            other => Err(AgentError::InvalidTransition { from: other, action: "start" }),
        }
    }

    pub fn mark_stopped(&self, id: &AgentId) -> Result<(), AgentError> {
        let state = self.state(id).ok_or_else(|| AgentError::NotFound(id.clone()))?;
        if state == AgentStatus::Stopped {
            return Err(AgentError::InvalidTransition { from: state, action: "stop" });
        }
        if let Some(mut r) = self.by_id.get_mut(id) {
            if let Some(h) = r.value_mut().1.take() { h.cancel_token.cancel(); }
        }
        self.set_state(id, AgentStatus::Stopped);
        Ok(())
    }

    pub fn mark_paused(&self, id: &AgentId) -> Result<(), AgentError> {
        let state = self.state(id).ok_or_else(|| AgentError::NotFound(id.clone()))?;
        if state != AgentStatus::Running {
            return Err(AgentError::InvalidTransition { from: state, action: "pause" });
        }
        if let Some(mut r) = self.by_id.get_mut(id) {
            if let Some(h) = r.value_mut().1.take() { h.cancel_token.cancel(); }
        }
        self.set_state(id, AgentStatus::Paused);
        Ok(())
    }

    pub fn mark_resumed(&self, id: &AgentId, cancel_token: CancellationToken) -> Result<(), AgentError> {
        let state = self.state(id).ok_or_else(|| AgentError::NotFound(id.clone()))?;
        if state != AgentStatus::Paused {
            return Err(AgentError::InvalidTransition { from: state, action: "resume" });
        }
        self.set_handle(id, Some(AgentRuntimeHandle { cancel_token }));
        self.set_state(id, AgentStatus::Running);
        Ok(())
    }
}
```

### Step 4: 更新 `agent/mod.rs` 和 `lib.rs`

`agent/mod.rs` 添加：
```rust
pub mod registry;
pub mod runner;
pub use registry::{AgentEntry, AgentError, AgentId, AgentManifest, AgentRegistry, AgentStatus};
```

`lib.rs` 添加导出：
```rust
pub use agent::{AgentEntry, AgentError, AgentEvent, AgentId, AgentLoop,
                AgentManifest, AgentRegistry, AgentRunner, AgentStatus};
```

### Step 5: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 6: Commit

```bash
git add crates/octo-engine/src/agent/registry/
git add crates/octo-engine/src/agent/mod.rs crates/octo-engine/src/lib.rs
git commit -m "feat(agent): add AgentRegistry with AgentEntry/AgentManifest and three-index support"
```

---

## Task 2：AgentRegistry SQLite 持久化

**目标**：AgentEntry 持久化到 SQLite，重启不丢失。参考 `McpStorage` 模式。

### Step 1: 查看 McpStorage 实现

```bash
cat crates/octo-engine/src/mcp/storage.rs | head -80
```

### Step 2: 创建 `registry/store.rs`

```rust
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
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS agents (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                manifest    TEXT NOT NULL,  -- JSON
                state       TEXT NOT NULL DEFAULT 'created',
                created_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);
        ")?;
        Ok(())
    }

    pub fn save(&self, entry: &AgentEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let manifest_json = serde_json::to_string(&entry.manifest)?;
        let state = entry.state.to_string();
        conn.execute(
            "INSERT OR REPLACE INTO agents (id, name, manifest, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                entry.id.0, entry.manifest.name, manifest_json,
                state, entry.created_at
            ],
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
            "SELECT id, manifest, state, created_at FROM agents ORDER BY created_at ASC"
        )?;
        let entries = stmt.query_map([], |row| {
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
```

### Step 3: AgentRegistry 集成 Store

在 `AgentRegistry` 添加可选 store：

```rust
pub struct AgentRegistry {
    by_id:   DashMap<AgentId, (AgentEntry, Option<AgentRuntimeHandle>)>,
    by_name: DashMap<String, AgentId>,
    by_tag:  DashMap<String, Vec<AgentId>>,
    store:   Option<Arc<AgentStore>>,
}

impl AgentRegistry {
    pub fn with_store(mut self, store: Arc<AgentStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Load persisted entries from store into memory indexes
    pub fn load_from_store(&self) -> anyhow::Result<usize> {
        if let Some(store) = &self.store {
            let entries = store.load_all()?;
            let count = entries.len();
            for entry in entries {
                let id = entry.id.clone();
                self.by_name.insert(entry.manifest.name.clone(), id.clone());
                for tag in &entry.manifest.tags {
                    self.by_tag.entry(tag.clone()).or_default().push(id.clone());
                }
                self.by_id.insert(id, (entry, None));
            }
            Ok(count)
        } else {
            Ok(0)
        }
    }
}
```

`register()` 和 `unregister()` 在写 DashMap 的同时写 store。

### Step 4: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 5: Commit

```bash
git add crates/octo-engine/src/agent/registry/store.rs
git add crates/octo-engine/src/agent/registry/mod.rs
git commit -m "feat(agent): add AgentRegistry SQLite persistence"
```

---

## Task 3：AgentRunner

**目标**：`AgentRunner` 持有启动依赖，构建 per-agent ToolRegistry，启动 AgentLoop。

### Step 1: 查看现有 AppState 结构

```bash
cat crates/octo-server/src/state.rs
```

### Step 2: 创建 `crates/octo-engine/src/agent/runner.rs`

```rust
//! AgentRunner - owns startup dependencies, builds per-agent ToolRegistry,
//! spawns and manages AgentLoop tasks.

use std::sync::Arc;
use anyhow::Result;

use crate::agent::{AgentConfig, AgentId, AgentRegistry, AgentError, CancellationToken};
use crate::event::EventBus;
use crate::memory::store_traits::MemoryStore;
use crate::memory::WorkingMemory;
use crate::providers::Provider;
use crate::tools::ToolRegistry;

pub struct AgentRunner {
    pub registry: Arc<AgentRegistry>,
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,         // global tool registry (source for filtering)
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    default_model: String,
    event_bus: Option<Arc<EventBus>>,
}

impl AgentRunner {
    pub fn new(
        registry: Arc<AgentRegistry>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        default_model: String,
    ) -> Self {
        Self {
            registry,
            provider,
            tools,
            memory,
            memory_store: None,
            default_model,
            event_bus: None,
        }
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Start agent: build per-agent ToolRegistry, spawn AgentLoop task.
    pub async fn start(&self, id: &AgentId) -> Result<(), AgentError> {
        let entry = self.registry.get(id).ok_or_else(|| AgentError::NotFound(id.clone()))?;

        // Build per-agent ToolRegistry from tool_filter
        let agent_tools = self.build_tool_registry(&entry.manifest.tool_filter);

        // Resolve model
        let model = entry.manifest.model.clone()
            .unwrap_or_else(|| self.default_model.clone());

        // Build system prompt from manifest (Zone A)
        let system_prompt = build_system_prompt(&entry.manifest);

        let cancel_token = CancellationToken::new();

        // Mark as running before spawn
        self.registry.mark_running(id, cancel_token.clone())?;

        // Spawn AgentLoop task
        let provider = self.provider.clone();
        let memory = self.memory.clone();
        let memory_store = self.memory_store.clone();
        let event_bus = self.event_bus.clone();
        let config = entry.manifest.config.clone();
        let registry = self.registry.clone();
        let agent_id = id.clone();

        tokio::spawn(async move {
            let mut loop_ = crate::agent::AgentLoop::new(provider, agent_tools, memory)
                .with_model(model)
                .with_config(config)
                .with_system_prompt(system_prompt);

            if let Some(store) = memory_store {
                loop_ = loop_.with_memory_store(store);
            }
            if let Some(bus) = event_bus {
                loop_ = loop_.with_event_bus(bus);
            }

            // AgentLoop runs until cancelled or done
            // On completion, mark agent stopped
            let _ = registry.mark_stopped(&agent_id);
        });

        Ok(())
    }

    /// Stop agent: cancel token, mark stopped.
    pub async fn stop(&self, id: &AgentId) -> Result<(), AgentError> {
        self.registry.mark_stopped(id)
    }

    /// Pause agent: cancel current loop, keep state resumable.
    pub async fn pause(&self, id: &AgentId) -> Result<(), AgentError> {
        self.registry.mark_paused(id)
    }

    /// Resume agent: spawn new AgentLoop task from paused state.
    pub async fn resume(&self, id: &AgentId) -> Result<(), AgentError> {
        let cancel_token = CancellationToken::new();
        self.registry.mark_resumed(id, cancel_token)
    }

    /// Build per-agent ToolRegistry from global tools filtered by whitelist.
    fn build_tool_registry(&self, tool_filter: &[String]) -> Arc<ToolRegistry> {
        if tool_filter.is_empty() {
            // No filter: use global registry as-is
            return self.tools.clone();
        }

        // Build filtered subset
        let mut filtered = ToolRegistry::new();
        for name in tool_filter {
            if let Some(tool) = self.tools.get(name) {
                filtered.register_arc(name.clone(), tool);
            }
        }
        Arc::new(filtered)
    }
}

/// Build Zone A system prompt from AgentManifest.
/// Priority: system_prompt > role/goal/backstory > CORE_INSTRUCTIONS
fn build_system_prompt(manifest: &crate::agent::AgentManifest) -> String {
    // Highest priority: explicit system_prompt
    if let Some(ref prompt) = manifest.system_prompt {
        return prompt.clone();
    }

    // Three-part identity
    if manifest.role.is_some() || manifest.goal.is_some() || manifest.backstory.is_some() {
        let mut parts = Vec::new();
        if let Some(ref role) = manifest.role {
            parts.push(format!("## Role\n{role}"));
        }
        if let Some(ref goal) = manifest.goal {
            parts.push(format!("## Goal\n{goal}"));
        }
        if let Some(ref backstory) = manifest.backstory {
            parts.push(format!("## Backstory\n{backstory}"));
        }
        return parts.join("\n\n");
    }

    // Fallback: use SystemPromptBuilder with SOUL.md / CORE_INSTRUCTIONS
    crate::context::builder::SystemPromptBuilder::new()
        .build_system_prompt()
}
```

### Step 3: `ToolRegistry` 添加 `register_arc` 方法

在 `crates/octo-engine/src/tools/mod.rs` 添加：

```rust
pub fn register_arc(&mut self, name: String, tool: Arc<dyn Tool>) {
    self.tools.insert(name, tool);
}
```

### Step 4: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 5: Commit

```bash
git add crates/octo-engine/src/agent/runner.rs
git add crates/octo-engine/src/tools/mod.rs
git commit -m "feat(agent): add AgentRunner with per-agent ToolRegistry and lifecycle management"
```

---

## Task 4：上下文工程重构（Zone A/B 分离）

**目标**：working memory 从 system prompt 移到首条 Human Message（Zone B）。清理 `AgentPersona`/`SandboxContext`。

### Step 1: 清理 `MemoryBlockKind`

在 `crates/octo-types/src/memory.rs` 修改：

```rust
pub enum MemoryBlockKind {
    // SandboxContext: moved to Zone A (SystemPromptBuilder), kept for migration compat
    #[deprecated(note = "Use SystemPromptBuilder capabilities section instead")]
    SandboxContext,
    // AgentPersona: replaced by AgentManifest role/goal/backstory
    #[deprecated(note = "Use AgentManifest for agent identity")]
    AgentPersona,
    UserProfile,    // Zone B ✓
    TaskContext,    // Zone B ✓
    AutoExtracted,  // Zone B ✓
    Custom,         // Zone B ✓
}
```

### Step 2: 清理 `InMemoryWorkingMemory` 默认 blocks

在 `crates/octo-engine/src/memory/working.rs`，默认 blocks 只保留：
```rust
MemoryBlock::new(MemoryBlockKind::UserProfile, "User Profile", ""),
MemoryBlock::new(MemoryBlockKind::TaskContext, "Task Context", ""),
```

移除 `AgentPersona` 和 `SandboxContext` 默认 block。

### Step 3: 修改 `ContextInjector` 输出格式

在 `crates/octo-engine/src/memory/injector.rs`：

```rust
pub struct ContextInjector;

impl ContextInjector {
    /// Build Zone B dynamic context message content.
    pub fn compile(blocks: &[MemoryBlock]) -> String {
        let datetime = chrono::Local::now()
            .format("%Y-%m-%d %H:%M %Z").to_string();

        let mut output = format!("<context>\n<datetime>{datetime}</datetime>\n");

        let mut sorted: Vec<&MemoryBlock> = blocks.iter()
            .filter(|b| !b.value.is_empty())
            .collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        let budget = 12_000usize;
        let mut used = output.len();

        for block in sorted {
            let tag = match &block.kind {
                MemoryBlockKind::UserProfile   => "user_profile",
                MemoryBlockKind::TaskContext   => "task_context",
                MemoryBlockKind::AutoExtracted => "memory",
                MemoryBlockKind::Custom        => "custom",
                _ => continue, // skip deprecated kinds
            };
            let entry = format!(
                "<{tag} priority=\"{}\">{}</{tag}>\n",
                block.priority, block.value
            );
            if used + entry.len() > budget { break; }
            used += entry.len();
            output.push_str(&entry);
        }

        output.push_str("</context>");
        output
    }
}
```

### Step 4: 修改 `AgentLoop.run()` — Zone A/B 分离

在 `crates/octo-engine/src/agent/loop_.rs`：

添加字段：
```rust
pub struct AgentLoop {
    // ...existing fields...
    system_prompt_override: Option<String>,  // from AgentRunner
}
```

添加 builder 方法：
```rust
pub fn with_system_prompt(mut self, prompt: String) -> Self {
    self.system_prompt_override = Some(prompt);
    self
}
```

修改 `run()` 开头的 system prompt 构建：
```rust
// Zone A: static system prompt (built once by AgentRunner, or fallback)
let system_prompt = self.system_prompt_override.clone()
    .unwrap_or_else(|| {
        SystemPromptBuilder::new().build_system_prompt()
    });

// Zone B: dynamic context injected as first human message
let memory_xml = self.memory.compile(user_id, sandbox_id).await.unwrap_or_default();
if !memory_xml.is_empty() {
    // Insert Zone B as first message (replace if already present)
    let zone_b = ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text { text: memory_xml }],
    };
    if messages.first().map(|m| m.role == MessageRole::User).unwrap_or(false) {
        // Check if it's already a context injection (starts with <context>)
        let is_context = messages.first()
            .and_then(|m| m.content.first())
            .map(|b| matches!(b, ContentBlock::Text { text } if text.starts_with("<context>")))
            .unwrap_or(false);
        if is_context {
            messages[0] = zone_b;
        } else {
            messages.insert(0, zone_b);
        }
    } else {
        messages.insert(0, zone_b);
    }
}
```

### Step 5: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 6: Commit

```bash
git add crates/octo-engine/src/memory/
git add crates/octo-engine/src/agent/loop_.rs
git add crates/octo-types/src/memory.rs
git commit -m "feat(context): Zone A/B separation - working memory injected as first human message"
```

---

## Task 5：Budget 统一

**目标**：消除 `ContextInjector::WORKING_MEMORY_BUDGET_CHARS` 与 `TokenBudget::memory` 不一致。

### Step 1: 查看现有 budget.rs

```bash
cat crates/octo-engine/src/context/budget.rs | head -60
```

### Step 2: 统一 budget 来源

在 `ContextInjector` 移除硬编码常量，改为接受参数：

```rust
impl ContextInjector {
    pub fn compile(blocks: &[MemoryBlock]) -> String {
        Self::compile_with_budget(blocks, 12_000)
    }

    pub fn compile_with_budget(blocks: &[MemoryBlock], char_budget: usize) -> String {
        // ... same logic, use char_budget instead of constant
    }
}
```

在 `AgentLoop` 调用时从 `TokenBudget` 派生：
```rust
let memory_budget_chars = (self.budget.snapshot(...).memory as usize) * 4;
let memory_xml = ContextInjector::compile_with_budget(&blocks, memory_budget_chars);
```

`TokenBudget::system_prompt` 从 `4_000` 调整为 `16_000`：
```rust
system_prompt: 16_000,  // accommodate identity + bootstrap + guidelines
```

### Step 3: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/context/
git add crates/octo-engine/src/memory/injector.rs
git commit -m "fix(context): unify memory budget between ContextInjector and TokenBudget"
```

---

## Task 6：AppState 集成 + REST API

**目标**：AppState 持有 `AgentRunner`，REST API CRUD + 生命周期端点。

### Step 1: 修改 `state.rs`

```rust
use octo_engine::{AgentRegistry, AgentRunner, AgentStore};

pub struct AppState {
    // ...existing fields...
    pub agent_runner: Arc<AgentRunner>,
}

// In AppState::new():
let agent_store = Arc::new(AgentStore::new(conn.clone())?);
let agent_registry = Arc::new(
    AgentRegistry::new().with_store(agent_store)
);
// Load persisted agents
let loaded = agent_registry.load_from_store().unwrap_or(0);
tracing::info!("Loaded {loaded} persisted agents");

let agent_runner = Arc::new(
    AgentRunner::new(
        agent_registry,
        provider.clone(),
        tools.clone(),
        memory.clone(),
        model.clone(),
    )
    .with_memory_store(memory_store.clone())
    .with_event_bus(/* event_bus if exists */)
);
```

### Step 2: 创建 `api/agents.rs`

```rust
//! Agent registry REST API
//!
//! POST   /api/v1/agents              register new agent (body: AgentManifest)
//! GET    /api/v1/agents              list all agents
//! GET    /api/v1/agents/:id          get agent by id
//! POST   /api/v1/agents/:id/start    start agent
//! POST   /api/v1/agents/:id/stop     stop agent
//! POST   /api/v1/agents/:id/pause    pause agent
//! POST   /api/v1/agents/:id/resume   resume agent
//! DELETE /api/v1/agents/:id          unregister agent

use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use octo_engine::{AgentEntry, AgentId, AgentManifest};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agents", get(list_agents).post(create_agent))
        .route("/agents/:id", get(get_agent).delete(delete_agent))
        .route("/agents/:id/start",  post(start_agent))
        .route("/agents/:id/stop",   post(stop_agent))
        .route("/agents/:id/pause",  post(pause_agent))
        .route("/agents/:id/resume", post(resume_agent))
}

async fn list_agents(State(s): State<Arc<AppState>>) -> Json<Vec<AgentEntry>> {
    Json(s.agent_runner.registry.list_all())
}

async fn create_agent(
    State(s): State<Arc<AppState>>,
    Json(manifest): Json<AgentManifest>,
) -> (StatusCode, Json<AgentEntry>) {
    let id = s.agent_runner.registry.register(manifest);
    let entry = s.agent_runner.registry.get(&id).unwrap();
    (StatusCode::CREATED, Json(entry))
}

async fn get_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    s.agent_runner.registry.get(&AgentId(id))
        .map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn start_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner.start(&agent_id).await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner.registry.get(&agent_id)
        .map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn stop_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner.stop(&agent_id).await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner.registry.get(&agent_id)
        .map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn pause_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner.pause(&agent_id).await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner.registry.get(&agent_id)
        .map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn resume_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AgentEntry>, StatusCode> {
    let agent_id = AgentId(id);
    s.agent_runner.resume(&agent_id).await
        .map_err(|_| StatusCode::CONFLICT)?;
    s.agent_runner.registry.get(&agent_id)
        .map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn delete_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    if s.agent_runner.registry.unregister(&AgentId(id)).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
```

### Step 3: 更新 `api/mod.rs` 和 `router.rs`

`api/mod.rs` 添加：
```rust
pub mod agents;
```

`router.rs` 添加路由：
```rust
.nest("/api/v1", crate::api::agents::router())
```

### Step 4: 验证编译

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

### Step 5: Commit

```bash
git add crates/octo-server/src/state.rs
git add crates/octo-server/src/api/agents.rs
git add crates/octo-server/src/api/mod.rs
git add crates/octo-server/src/router.rs
git commit -m "feat(api): add Agent CRUD + lifecycle REST API, integrate AgentRunner into AppState"
```

---

## Task 7：构建验证

### Step 1: 完整编译检查

```bash
cargo check --workspace 2>&1 | tail -5
```

### Step 2: 运行测试

```bash
cargo test -p octo-engine 2>&1 | tail -20
```

### Step 3: TypeScript 检查

```bash
cd web && npx tsc --noEmit 2>&1 | tail -10 && cd ..
```

### Step 4: 更新文档

更新 `docs/dev/NEXT_SESSION_GUIDE.md` Phase 2.11 状态为 ✅

更新 `docs/dev/MEMORY_INDEX.md`：
```
- HH:MM | Phase 2.11 完成: AgentRegistry + AgentRunner + Zone A/B 上下文重构 + SQLite 持久化
```

### Step 5: Commit

```bash
git add docs/dev/
git commit -m "docs: Phase 2.11 complete - AgentRegistry + context engineering refactor"
```

---

## 完成标准

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| AgentManifest | role/goal/backstory/system_prompt/tool_filter 字段完整 |
| AgentRegistry | DashMap 三索引 + SQLite 持久化，重启加载 |
| AgentRunner | per-agent ToolRegistry 过滤，start/stop/pause/resume |
| Zone A/B | system prompt 静态，working memory 注入首条 Human Message |
| MemoryBlockKind | AgentPersona/SandboxContext 标记 deprecated，默认 blocks 清理 |
| Budget | ContextInjector 与 TokenBudget 对齐，system_prompt budget = 16,000 |
| REST API | 8 个端点，body=AgentManifest，响应=AgentEntry |

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。每次开始新 Task 前先检查此列表。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D1 | SkillRegistry 热重载后同步 per-agent ToolRegistry：新增/删除 skill 时已运行 Agent 的工具集自动更新 | per-agent ToolRegistry（Task 3）完成后 | ✅ 已补 |
| D2 | SOUL.md/AGENTS.md 项目文件加载接入 AgentLoop（SystemPromptBuilder.with_bootstrap_dir()） | Task 4 Zone A 重构完成后 | ✅ 已补 |
| D3 | AgentLoop 实际运行与 AgentRunner 的 session/messages 管理（AgentRunner 目前 spawn 空任务） | WebSocket/session 层与 AgentRunner 集成设计后 | ⏳ |

---

## 提交历史预期

```
feat(agent): add AgentRegistry with AgentEntry/AgentManifest and three-index support
feat(agent): add AgentRegistry SQLite persistence
feat(agent): add AgentRunner with per-agent ToolRegistry and lifecycle management
feat(context): Zone A/B separation - working memory injected as first human message
fix(context): unify memory budget between ContextInjector and TokenBudget
feat(api): add Agent CRUD + lifecycle REST API, integrate AgentRunner into AppState
docs: Phase 2.11 complete - AgentRegistry + context engineering refactor
```
