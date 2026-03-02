# AgentRegistry 多代理架构实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**：在 octo-sandbox 中引入 AgentRegistry，支持多代理注册、生命周期管理、多索引查询，为 octo-platform 打基础。

**架构**：采用 DashMap 实现并发存储，支持多索引（ID、Name、Tag），支持 Agent 生命周期管理（create/start/stop/pause/resume）。

**技术栈**：Rust async/tokio、DashMap、Arc、RwLock

---

## 背景：当前代码状态

- `crates/octo-engine/src/agent/loop_.rs` — 单一 AgentLoop 实现
- `crates/octo-engine/src/session/mod.rs` — SessionData 结构，包含 session_id/user_id/sandbox_id
- `crates/octo-engine/src/agent/config.rs` — AgentConfig 配置

**当前局限**：仅支持单一 Agent 实例，无注册表、无多代理管理。

---

## 文件索引

### 新增文件
| 文件 | 任务 | 说明 |
|------|------|------|
| `crates/octo-engine/src/agent/registry.rs` | Task 1-3 | AgentRegistry 核心实现 |
| `crates/octo-engine/src/agent/registry/mod.rs` | Task 1 | 模块入口 |
| `crates/octo-engine/src/agent/instance.rs` | Task 2 | AgentInstance 状态管理 |

### 修改文件
| 文件 | 任务 | 说明 |
|------|------|------|
| `crates/octo-engine/src/agent/mod.rs` | Task 1 | 导出 AgentRegistry |
| `crates/octo-engine/src/lib.rs` | Task 1 | 注册 agent::registry 模块 |
| `crates/octo-server/src/state.rs` | Task 4 | AppState 添加 AgentRegistry |
| `crates/octo-server/src/router.rs` | Task 5 | 注册 Agent REST API |

---

## Task 1：创建 AgentRegistry 核心结构

**目标**：定义 AgentRegistry、DashMap 存储、多索引结构。

**文件**：
- 新增：`crates/octo-engine/src/agent/registry/mod.rs`
- 新增：`crates/octo-engine/src/agent/registry.rs`（模块入口）

### Step 1: 创建 registry/mod.rs

```rust
//! Agent Registry - Multi-agent registration and lifecycle management

mod instance;

pub use instance::AgentInstance;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unique agent identifier
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

/// Agent metadata for registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub id: AgentId,
    pub name: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub status: AgentStatus,
}

/// Agent runtime status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// Agent registry with multi-index support
pub struct AgentRegistry {
    /// Primary index: AgentId -> AgentInstance
    by_id: DashMap<AgentId, Arc<RwLock<AgentInstance>>>,
    /// Secondary index: Name -> AgentId(s)
    by_name: DashMap<String, Vec<AgentId>>,
    /// Tertiary index: Tag -> AgentId(s)
    by_tag: DashMap<String, Vec<AgentId>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            by_id: DashMap::new(),
            by_name: DashMap::new(),
            by_tag: DashMap::new(),
        }
    }

    /// Register a new agent
    pub fn register(&self, metadata: AgentMetadata, instance: AgentInstance) -> AgentId {
        let id = metadata.id.clone();

        // Primary index
        self.by_id.insert(id.clone(), Arc::new(RwLock::new(instance)));

        // Secondary index: name
        self.by_name
            .entry(metadata.name.clone())
            .or_default()
            .push(id.clone());

        // Tertiary index: tags
        for tag in &metadata.tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .push(id.clone());
        }

        id
    }

    /// Get agent by ID
    pub fn get(&self, id: &AgentId) -> Option<Arc<RwLock<AgentInstance>>> {
        self.by_id.get(id).map(|r| r.value().clone())
    }

    /// Get agent by name
    pub fn get_by_name(&self, name: &str) -> Option<Arc<RwLock<AgentInstance>>> {
        self.by_name
            .get(name)
            .and_then(|ids| ids.value().first())
            .and_then(|id| self.get(id))
    }

    /// Get all agents by tag
    pub fn get_by_tag(&self, tag: &str) -> Vec<Arc<RwLock<AgentInstance>>> {
        self.by_tag
            .get(tag)
            .map(|ids| {
                ids.value()
                    .iter()
                    .filter_map(|id| self.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all agent IDs
    pub fn list_all(&self) -> Vec<AgentId> {
        self.by_id.iter().map(|r| r.key().clone()).collect()
    }

    /// Unregister agent
    pub fn unregister(&self, id: &AgentId) -> Option<AgentMetadata> {
        if let Some(instance) = self.by_id.remove(id) {
            let metadata = instance.blocking_read().metadata.clone();

            // Remove from name index
            let name = &metadata.name;
            if let Some(mut ids) = self.by_name.remove(name) {
                ids.retain(|i| i != id);
                if !ids.is_empty() {
                    self.by_name.insert(name.clone(), ids);
                }
            }

            // Remove from tag indices
            for tag in &metadata.tags {
                if let Some(mut ids) = self.by_tag.remove(tag) {
                    ids.retain(|i| i != id);
                    if !ids.is_empty() {
                        self.by_tag.insert(tag.clone(), ids);
                    }
                }
            }

            Some(metadata)
        } else {
            None
        }
    }

    /// Get count
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

### Step 2: 创建 instance.rs

```rust
//! Agent instance - runtime state management

use super::{AgentId, AgentMetadata, AgentStatus};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Runtime agent instance
pub struct AgentInstance {
    pub metadata: AgentMetadata,
    pub state: AgentState,
}

/// Mutable agent state
#[derive(Debug)]
pub enum AgentState {
    /// Agent created, not started
    Created,
    /// Agent running (loop active)
    Running(AgentRuntimeHandle),
    /// Agent paused (loop suspended)
    Paused,
    /// Agent stopped (resources cleaned)
    Stopped,
    /// Agent error
    Error(String),
}

/// Handle to running agent
#[derive(Clone)]
pub struct AgentRuntimeHandle {
    pub cancel_token: tokio_util::sync::CancellationToken,
}

impl AgentInstance {
    pub fn new(metadata: AgentMetadata) -> Self {
        Self {
            metadata,
            state: AgentState::Created,
        }
    }

    pub fn status(&self) -> &AgentStatus {
        &self.metadata.status
    }

    pub fn set_status(&mut self, status: AgentStatus) {
        self.metadata.status = status;
    }
}
```

### Step 3: 更新 agent/mod.rs

在 `crates/octo-engine/src/agent/mod.rs` 添加：

```rust
pub mod registry;
pub use registry::{AgentId, AgentMetadata, AgentRegistry, AgentStatus};
```

### Step 4: 更新 lib.rs

在 `crates/octo-engine/src/lib.rs` 添加：

```rust
pub mod agent;
```

### Step 5: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

预期：0 errors（可能需要添加 dashmap 依赖）。

### Step 6: Commit

```bash
git add crates/octo-engine/src/agent/registry/
git commit -m "feat(agent): add AgentRegistry with multi-index support (id/name/tag)"
```

---

## Task 2：集成 AgentConfig 与 AgentRegistry

**目标**：将现有 AgentConfig 接入 Registry，支持通过配置创建 Agent。

**文件**：
- 修改：`crates/octo-engine/src/agent/config.rs`
- 修改：`crates/octo-engine/src/agent/registry/mod.rs`

### Step 1: 添加 ConfigSource

在 registry/mod.rs 添加配置源：

```rust
/// Agent configuration source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentConfigSource {
    /// Inline configuration
    Inline(AgentConfig),
    /// Reference to saved config
    Named(String),
}
```

### Step 2: 实现 from_config 构造

在 AgentInstance 添加：

```rust
impl AgentInstance {
    pub fn from_config(config: AgentConfig) -> Self {
        let metadata = AgentMetadata {
            id: AgentId::new(),
            name: config.name.clone(),
            tags: config.tags.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            status: AgentStatus::Created,
        };

        Self::new(metadata)
    }
}
```

### Step 3: 实现 create 方法

在 AgentRegistry 添加便捷方法：

```rust
impl AgentRegistry {
    /// Create agent from config
    pub fn create_from_config(&self, config: AgentConfig) -> AgentId {
        let instance = AgentInstance::from_config(config);
        let metadata = instance.metadata.clone();
        self.register(metadata, instance)
    }
}
```

### Step 4: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 5: Commit

```bash
git add crates/octo-engine/src/agent/
git commit -m "feat(agent): integrate AgentConfig with AgentRegistry"
```

---

## Task 3：Agent 生命周期管理

**目标**：实现 Agent 的 start/stop/pause/resume 生命周期方法。

**文件**：
- 修改：`crates/octo-engine/src/agent/registry/mod.rs`

### Step 1: 添加生命周期 trait

```rust
/// Agent lifecycle operations
#[async_trait]
pub trait AgentLifecycle: Send + Sync {
    async fn start(&self) -> Result<(), AgentError>;
    async fn stop(&self) -> Result<(), AgentError>;
    async fn pause(&self) -> Result<(), AgentError>;
    async fn resume(&self) -> Result<(), AgentError>;
}

#[derive(Debug)]
pub enum AgentError {
    NotFound(AgentId),
    AlreadyRunning(AgentId),
    AlreadyStopped(AgentId),
    RuntimeError(String),
}
```

### Step 2: 实现生命周期方法

在 AgentRegistry 添加：

```rust
impl AgentRegistry {
    /// Start agent by ID
    pub async fn start(&self, id: &AgentId) -> Result<(), AgentError> {
        let instance = self.get(id).ok_or(AgentError::NotFound(id.clone()))?;
        let mut guard = instance.write().await;

        match &guard.state {
            AgentState::Created | AgentState::Paused => {
                // Initialize runtime
                let cancel_token = CancellationToken::new();
                guard.state = AgentState::Running(AgentRuntimeHandle { cancel_token });
                guard.set_status(AgentStatus::Running);
                Ok(())
            }
            AgentState::Running(_) => Err(AgentError::AlreadyRunning(id.clone())),
            AgentState::Stopped => Err(AgentError::AlreadyStopped(id.clone())),
            AgentState::Error(e) => Err(AgentError::RuntimeError(e.clone())),
        }
    }

    /// Stop agent by ID
    pub async fn stop(&self, id: &AgentId) -> Result<(), AgentError> {
        let instance = self.get(id).ok_or(AgentError::NotFound(id.clone()))?;
        let mut guard = instance.write().await;

        // Cancel runtime if running
        if let AgentState::Running(handle) = &guard.state {
            handle.cancel_token.cancel();
        }

        guard.state = AgentState::Stopped;
        guard.set_status(AgentStatus::Stopped);
        Ok(())
    }

    /// Pause agent
    pub async fn pause(&self, id: &AgentId) -> Result<(), AgentError> {
        let instance = self.get(id).ok_or(AgentError::NotFound(id.clone()))?;
        let mut guard = instance.write().await;

        if let AgentState::Running(handle) = &guard.state {
            handle.cancel_token.cancel();
            guard.state = AgentState::Paused;
            guard.set_status(AgentStatus::Paused);
            Ok(())
        } else {
            Err(AgentError::RuntimeError("Agent not running".to_string()))
        }
    }

    /// Resume agent
    pub async fn resume(&self, id: &AgentId) -> Result<(), AgentError> {
        let instance = self.get(id).ok_or(AgentError::NotFound(id.clone()))?;
        let mut guard = instance.write().await;

        if let AgentState::Paused = &guard.state {
            let cancel_token = CancellationToken::new();
            guard.state = AgentState::Running(AgentRuntimeHandle { cancel_token });
            guard.set_status(AgentStatus::Running);
            Ok(())
        } else {
            Err(AgentError::RuntimeError("Agent not paused".to_string()))
        }
    }
}
```

### Step 3: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/agent/
git commit -m "feat(agent): add lifecycle management (start/stop/pause/resume)"
```

---

## Task 4：AppState 集成

**目标**：将 AgentRegistry 集成到 AppState。

**文件**：
- 修改：`crates/octo-server/src/state.rs`

### Step 1: 检查现有 state.rs

```bash
head -50 crates/octo-server/src/state.rs
```

### Step 2: 添加 AgentRegistry

```rust
use octo_engine::agent::AgentRegistry;

// In AppState struct:
pub agent_registry: Arc<AgentRegistry>,
```

### Step 3: 初始化

```rust
impl AppState {
    pub async fn new(/* ... */) -> Self {
        Self {
            // ... existing fields
            agent_registry: Arc::new(AgentRegistry::new()),
        }
    }
}
```

### Step 4: 验证编译

```bash
cargo check -p octo-server 2>&1 | grep "^error" | head -20
```

### Step 5: Commit

```bash
git add crates/octo-server/src/state.rs
git commit -m "feat(state): integrate AgentRegistry into AppState"
```

---

## Task 5：REST API 端点

**目标**：创建 Agent CRUD REST API。

**文件**：
- 新增：`crates/octo-server/src/api/agents.rs`
- 修改：`crates/octo-server/src/router.rs`

### Step 1: 创建 agents.rs

```rust
use axum::{
    extract::State,
    routing::{get, post, delete},
    Json, Router,
};
use octo_engine::agent::{AgentConfig, AgentId, AgentMetadata, AgentRegistry, AgentStatus};

pub fn router(registry: Arc<AgentRegistry>) -> Router {
    Router::new()
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents", post(create_agent))
        .route("/api/v1/agents/:id", get(get_agent))
        .route("/api/v1/agents/:id/start", post(start_agent))
        .route("/api/v1/agents/:id/stop", post(stop_agent))
        .route("/api/v1/agents/:id", delete(delete_agent))
        .with_state(registry)
}

#[derive(serde::Serialize)]
struct AgentResponse {
    id: String,
    name: String,
    tags: Vec<String>,
    status: String,
    created_at: i64,
}

async fn list_agents(State(state): State<Arc<AgentRegistry>>) -> Json<Vec<AgentResponse>> {
    let agents = state.list_all()
        .into_iter()
        .filter_map(|id| state.get(&id))
        .map(|instance| {
            let inst = instance.blocking_read();
            AgentResponse {
                id: inst.metadata.id.0.clone(),
                name: inst.metadata.name.clone(),
                tags: inst.metadata.tags.clone(),
                status: format!("{:?}", inst.metadata.status),
                created_at: inst.metadata.created_at,
            }
        })
        .collect();
    Json(agents)
}

async fn create_agent(
    State(state): State<Arc<AgentRegistry>>,
    Json(config): Json<AgentConfig>,
) -> Json<AgentResponse> {
    let id = state.create_from_config(config.clone());
    let instance = state.get(&id).unwrap();
    let inst = instance.blocking_read();
    Json(AgentResponse {
        id: inst.metadata.id.0.clone(),
        name: inst.metadata.name.clone(),
        tags: inst.metadata.tags.clone(),
        status: format!("{:?}", inst.metadata.status),
        created_at: inst.metadata.created_at,
    })
}

async fn get_agent(
    State(state): State<Arc<AgentRegistry>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<AgentResponse>, axum::http::StatusCode> {
    let agent_id = AgentId(id);
    let instance = state.get(&agent_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    let inst = instance.blocking_read();
    Ok(Json(AgentResponse {
        id: inst.metadata.id.0.clone(),
        name: inst.metadata.name.clone(),
        tags: inst.metadata.tags.clone(),
        status: format!("{:?}", inst.metadata.status),
        created_at: inst.metadata.created_at,
    }))
}

async fn start_agent(
    State(state): State<Arc<AgentRegistry>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<AgentResponse>, axum::http::StatusCode> {
    let agent_id = AgentId(id);
    state.start(&agent_id).await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let instance = state.get(&agent_id).unwrap();
    let inst = instance.blocking_read();
    Ok(Json(AgentResponse {
        id: inst.metadata.id.0.clone(),
        name: inst.metadata.name.clone(),
        tags: inst.metadata.tags.clone(),
        status: format!("{:?}", inst.metadata.status),
        created_at: inst.metadata.created_at,
    }))
}

async fn stop_agent(
    State(state): State<Arc<AgentRegistry>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<AgentResponse>, axum::http::StatusCode> {
    let agent_id = AgentId(id);
    state.stop(&agent_id).await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let instance = state.get(&agent_id).unwrap();
    let inst = instance.blocking_read();
    Ok(Json(AgentResponse {
        id: inst.metadata.id.0.clone(),
        name: inst.metadata.name.clone(),
        tags: inst.metadata.tags.clone(),
        status: format!("{:?}", inst.metadata.status),
        created_at: inst.metadata.created_at,
    }))
}

async fn delete_agent(
    State(state): State<Arc<AgentRegistry>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::http::StatusCode, axum::http::StatusCode> {
    let agent_id = AgentId(id);
    state.unregister(&agent_id)
        .map(|_| axum::http::StatusCode::NO_CONTENT)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}
```

### Step 2: 注册路由

在 `router.rs` 添加：

```rust
mod agents;
// ...
let agent_router = agents::router(state.agent_registry.clone());
router = router.nest("/api/v1", agent_router);
```

### Step 3: 验证编译

```bash
cargo check --workspace 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-server/src/api/agents.rs crates/octo-server/src/router.rs
git commit -m "feat(api): add Agent CRUD REST API endpoints"
```

---

## Task 6：构建验证

### Step 1: 完整编译检查

```bash
cargo check --workspace 2>&1 | tail -5
```

### Step 2: TypeScript 检查

```bash
cd web && npx tsc --noEmit 2>&1 | tail -10 && cd ..
```

### Step 3: 更新文档

在 `docs/dev/NEXT_SESSION_GUIDE.md` 添加：

```
| AgentRegistry | P2 | ✅ 已实施 |
```

在 `docs/dev/MEMORY_INDEX.md` 追加：

```
- {时间} | AgentRegistry 完成: 多代理注册 + 生命周期 + REST API
```

### Step 4: Commit

```bash
git add docs/dev/
git commit -m "docs: AgentRegistry complete - multi-agent support"
```

---

## 完成标准

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| AgentRegistry | DashMap 多索引 (id/name/tag) |
| 生命周期 | start/stop/pause/resume 方法正确实现 |
| REST API | CRUD 端点正确响应 |
| AppState | AgentRegistry 已集成 |

---

## 提交历史预期

```
feat(agent): add AgentRegistry with multi-index support (id/name/tag)
feat(agent): integrate AgentConfig with AgentRegistry
feat(agent): add lifecycle management (start/stop/pause/resume)
feat(state): integrate AgentRegistry into AppState
feat(api): add Agent CRUD REST API endpoints
docs: AgentRegistry complete - multi-agent support
```
