# P1-4 Agent Pool + WebSocket 集成实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现每用户 WebSocket + AgentRuntime 懒加载 + Agent 池，支持热实例复用和多种隔离策略

**Architecture:** 在 UserRuntime 中引入 AgentPool 管理共享热实例。用户消息通过 WebSocket 路由到 AgentPool 获取实例，实例归还时持久化状态并清空 workspace 保证隔离。隔离策略可配置：Memory（默认）/ Process / Session

**Tech Stack:** Rust, tokio, dashmap, AgentRuntime (octo-engine)

---

## Task 1: 定义 AgentPool 数据结构

**Files:**
- Create: `crates/octo-platform-server/src/agent_pool.rs`

**Step 1: 创建基础数据结构**

```rust
use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent 实例状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceState {
    Idle,      // 空闲，可分配
    Busy,      // 工作中
    Releasing, // 归还中
}

/// 隔离策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IsolationStrategy {
    #[default]
    Memory,    // 内存级隔离（默认）
    Process,   // 进程级隔离
    Session,   // 会话级隔离
}

/// Agent 池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// 软上限，正常运行时保持
    pub soft_max_total: u32,
    /// 硬上限，不可超过
    pub hard_max_total: u32,
    /// 最小空闲实例数（预热）
    pub min_idle: u32,
    /// 最大空闲实例数（回收阈值）
    pub max_idle: u32,
    /// 空闲超时回收
    pub idle_timeout: Duration,
    /// 隔离策略
    pub strategy: IsolationStrategy,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            soft_max_total: 5,
            hard_max_total: 10,
            min_idle: 0,
            max_idle: 5,
            idle_timeout: Duration::from_secs(300), // 5分钟
            strategy: IsolationStrategy::Memory,
        }
    }
}

/// Agent 实例 ID
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceId(pub String);

impl InstanceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: 添加 Workspace 结构（暂时为空结构，后续 Task 完善）**

```rust
/// 用户工作空间 - 隔离的记忆/会话上下文
#[derive(Debug, Clone)]
pub struct Workspace {
    pub user_id: String,
    pub session_ids: Vec<String>,
}

/// Agent 实例
#[derive(Debug)]
pub struct AgentInstance {
    pub id: InstanceId,
    pub runtime: AgentRuntime,          // 来自 octo-engine
    pub workspace: Option<Workspace>,     // 当前占用的工作空间
    pub state: InstanceState,
    pub last_used: DateTime<Utc>,
}
```

**Step 3: 创建 AgentPool 核心结构**

```rust
/// Agent 池
pub struct AgentPool {
    config: PoolConfig,
    instances: DashMap<InstanceId, AgentInstance>,
    idle_instances: std::sync::Arc<tokio::sync::Mutex<Vec<InstanceId>>>,
}
```

**Step 4: 验证编译**

Run: `cargo check -p octo-platform-server`
Expected: PASS

---

## Task 2: 实现 AgentPool 基本方法

**Files:**
- Modify: `crates/octo-platform-server/src/agent_pool.rs`

**Step 1: 添加 AgentPool 构造方法**

```rust
impl AgentPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            instances: DashMap::new(),
            idle_instances: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// 当前实例总数
    pub fn total_count(&self) -> usize {
        self.instances.len()
    }

    /// 当前空闲实例数
    pub async fn idle_count(&self) -> usize {
        self.idle_instances.lock().await.len()
    }
}
```

**Step 2: 实现 get_instance 方法（获取实例）**

```rust
impl AgentPool {
    pub async fn get_instance(&self, user_id: &str) -> Result<AgentInstance, PoolError> {
        // 1. 尝试从空闲池获取
        let idle_instances = self.idle_instances.lock().await;
        if let Some(instance_id) = idle_instances.pop() {
            drop(idle_instances);

            // 获取实例并标记为忙碌
            if let Some(mut instance) = self.instances.get_mut(&instance_id) {
                instance.state = InstanceState::Busy;
                instance.workspace = Some(Workspace::new(user_id.to_string()));
                instance.last_used = Utc::now();
                return Ok(instance.clone());
            }
        }
        drop(idle_instances);

        // 2. 空闲池没有，检查是否可创建新实例
        let current_total = self.instances.len() as u32;
        if current_total >= self.config.hard_max_total {
            return Err(PoolError::Exhausted {
                current: current_total,
                max: self.config.hard_max_total,
            });
        }

        // 3. 创建新实例（实际创建 AgentRuntime）
        // 这部分在后续 Task 实现
        let instance = self.create_instance(user_id).await?;
        Ok(instance)
    }

    async fn create_instance(&self, user_id: &str) -> Result<AgentInstance, PoolError> {
        // TODO: 创建 AgentRuntime
        // 临时返回错误，让后续 Task 实现
        Err(PoolError::NotImplemented("AgentRuntime creation not yet implemented"))
    }
}

/// 池错误
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("Pool exhausted: {current}/{max}")]
    Exhausted { current: u32, max: u32 },

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Instance not found: {0}")]
    NotFound(InstanceId),

    #[error("Instance busy: {0}")]
    Busy(InstanceId),
}
```

**Step 3: 实现 release_instance 方法（归还实例）**

```rust
impl AgentPool {
    pub async fn release_instance(&self, instance_id: InstanceId) -> Result<(), PoolError> {
        // 1. 获取实例
        let mut instance = self.instances.get_mut(&instance_id)
            .ok_or(PoolError::NotFound(instance_id.clone()))?;

        // 2. 状态检查
        if instance.state != InstanceState::Busy {
            return Err(PoolError::Busy(instance_id.clone()));
        }

        // 3. 持久化用户状态（TODO：后续 Task 实现）
        // 4. 清空 workspace（隔离保证）
        instance.workspace = None;
        instance.state = InstanceState::Idle;
        instance.last_used = Utc::now();

        // 5. 加入空闲池
        let instance_id_clone = instance_id.clone();
        drop(instance);

        let mut idle_instances = self.idle_instances.lock().await;
        idle_instances.push(instance_id_clone);

        Ok(())
    }
}
```

**Step 4: 验证编译**

Run: `cargo check -p octo-platform-server`
Expected: PASS

---

## Task 3: 集成 octo-engine AgentRuntime

**Files:**
- Modify: `crates/octo-platform-server/src/agent_pool.rs`

**Step 1: 添加 AgentRuntime 依赖导入**

```rust
use octo_engine::agent::AgentRuntime;
```

**Step 2: 修改 create_instance 方法，创建真实 AgentRuntime**

```rust
async fn create_instance(&self, user_id: &str) -> Result<AgentInstance, PoolError> {
    // 使用 octo-engine 创建 AgentRuntime
    // 这里需要根据实际情况配置
    let config = octo_engine::agent::AgentRuntimeConfig::default();

    let runtime = AgentRuntime::new(config)
        .await
        .map_err(|e| PoolError::RuntimeError(e.to_string()))?;

    let instance = AgentInstance {
        id: InstanceId::new(),
        runtime,
        workspace: Some(Workspace::new(user_id.to_string())),
        state: InstanceState::Busy,
        last_used: Utc::now(),
    };

    self.instances.insert(instance.id.clone(), instance.clone());

    Ok(instance)
}
```

**Step 3: 添加 RuntimeError 变体**

```rust
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    // ... existing variants

    #[error("Runtime error: {0}")]
    RuntimeError(String),
}
```

**Step 4: 验证编译**

Run: `cargo check -p octo-platform-server`
Expected: 可能需要调整 AgentRuntimeConfig 参数

---

## Task 4: 实现 Workspace 和状态持久化

**Files:**
- Modify: `crates/octo-platform-server/src/agent_pool.rs`

**Step 1: 扩展 Workspace 结构**

```rust
/// 用户工作空间 - 隔离的记忆/会话上下文
#[derive(Debug, Clone)]
pub struct Workspace {
    pub user_id: String,
    pub session_ids: Vec<String>,
    pub context: Option<ContextSnapshot>,  // Agent 上下文快照
}

impl Workspace {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            session_ids: Vec::new(),
            context: None,
        }
    }

    /// 添加会话
    pub fn add_session(&mut self, session_id: String) {
        if !self.session_ids.contains(&session_id) {
            self.session_ids.push(session_id);
        }
    }

    /// 清空（归还池时调用）
    pub fn clear(&mut self) {
        self.session_ids.clear();
        self.context = None;
    }
}

/// Agent 上下文快照（用于恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub working_memory: Vec<MemoryBlock>,
    // ... 其他需要持久化的状态
}

/// 简化的 MemoryBlock（实际使用 octo-engine 的类型）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBlock {
    pub id: String,
    pub content: String,
    pub kind: String,
}
```

**Step 2: 实现 release_instance 中的持久化逻辑**

```rust
pub async fn release_instance(&self, instance_id: InstanceId) -> Result<(), PoolError> {
    let mut instance = self.instances.get_mut(&instance_id)
        .ok_or(PoolError::NotFound(instance_id.clone()))?;

    if instance.state != InstanceState::Busy {
        return Err(PoolError::Busy(instance_id.clone()));
    }

    // 持久化用户状态
    if let Some(ref workspace) = instance.workspace {
        self.persist_workspace(workspace).await;
    }

    // 清空 workspace（隔离保证）
    instance.workspace = None;
    instance.state = InstanceState::Idle;
    instance.last_used = Utc::now();

    // 加入空闲池
    let instance_id_clone = instance_id.clone();
    drop(instance);

    let mut idle_instances = self.idle_instances.lock().await;
    idle_instances.push(instance_id_clone);

    Ok(())
}

async fn persist_workspace(&self, workspace: &Workspace) {
    // TODO: 实现持久化到 Session Store
    // - WorkingMemory -> Session Store
    // - ContextSnapshot -> 持久化存储
    tracing::debug!("Persisting workspace for user: {}", workspace.user_id);
}
```

**Step 3: 验证编译**

Run: `cargo check -p octo-platform-server`
Expected: PASS

---

## Task 5: 实现空闲实例回收

**Files:**
- Modify: `crates/octo-platform-server/src/agent_pool.rs`

**Step 1: 添加回收方法**

```rust
impl AgentPool {
    /// 定期回收检查（应定时调用）
    pub async fn cleanup(&self) {
        let mut idle_instances = self.idle_instances.lock().await;
        let now = Utc::now();
        let timeout = self.config.idle_timeout;

        // 找出超时的实例
        let to_remove: Vec<InstanceId> = idle_instances
            .iter()
            .filter(|id| {
                if let Some(instance) = self.instances.get(*id) {
                    (now - instance.last_used) > timeout
                } else {
                    true // 实例已不存在
                }
            })
            .cloned()
            .collect();

        // 移除超时的实例
        for id in &to_remove {
            idle_instances.retain(|i| i != id);
            self.instances.remove(id);
            tracing::info!("Recycled idle agent instance: {}", id.0);
        }

        // 保持 min_idle
        // 如果空闲实例少于 min_idle，不需要额外操作（会在 get_instance 时创建）
    }

    /// 获取池统计信息
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            total: self.instances.len(),
            idle: self.idle_instances.lock().unwrap().len(),
            busy: self.instances.len() - self.idle_instances.lock().unwrap().len(),
        }
    }
}

/// 池统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total: usize,
    pub idle: usize,
    pub busy: usize,
}
```

**Step 2: 实现自动回收后台任务**

```rust
impl AgentPool {
    /// 启动后台回收任务
    pub fn spawn_cleanup_task(self: &Arc<Self>) {
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // 每分钟检查
            loop {
                interval.tick().await;
                pool.cleanup().await;
            }
        });
    }
}
```

**Step 3: 验证编译**

Run: `cargo check -p octo-platform-server`
Expected: PASS

---

## Task 6: WebSocket 与 AgentPool 集成

**Files:**
- Modify: `crates/octo-platform-server/src/ws.rs`

**Step 1: 修改 ws_handler 使用 AgentPool**

```rust
use crate::agent_pool::{AgentPool, InstanceId};

pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthExtractor,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // 获取 AgentPool
    let pool = state.agent_pool();

    // 从池中获取实例
    let instance = match pool.get_instance(&auth.user_id).await {
        Ok(i) => i,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: format!("Agent pool exhausted: {}", e),
                }),
            )
                .into_response();
        }
    };

    // 保存 instance_id 用于后续归还
    let instance_id = instance.id.clone();

    // ... 后续处理
    ws.on_upgrade(move |socket| handle_socket(session_id, socket, instance_id))
}
```

**Step 2: 修改 handle_socket 签名**

```rust
async fn handle_socket(
    session_id: String,
    socket: WebSocket,
    instance_id: InstanceId,
) {
    // ... 现有逻辑

    // WebSocket 关闭时归还实例
    send_task.abort();
    tracing::info!("WebSocket closed for session: {}, returning instance to pool", session_id);

    // 归还实例到池（忽略错误）
    let pool = state.agent_pool();
    let _ = pool.release_instance(instance_id).await;
}
```

**Step 3: 验证编译**

Run: `cargo check -p octo-platform-server`
Expected: PASS

---

## Task 7: 集成测试：Agent 池基本功能

**Files:**
- Create: `crates/octo-platform-server/tests/test_agent_pool.rs`

**Step 1: 编写基本测试**

```rust
use octo_platform_server::agent_pool::{
    AgentPool, PoolConfig, IsolationStrategy, InstanceState
};
use std::time::Duration;

#[tokio::test]
async fn test_pool_creation() {
    let config = PoolConfig {
        soft_max_total: 3,
        hard_max_total: 5,
        min_idle: 0,
        max_idle: 2,
        idle_timeout: Duration::from_secs(60),
        strategy: IsolationStrategy::Memory,
    };

    let pool = AgentPool::new(config);
    assert_eq!(pool.total_count(), 0);
}

#[tokio::test]
async fn test_pool_stats() {
    let config = PoolConfig::default();
    let pool = AgentPool::new(config);

    let stats = pool.stats();
    assert_eq!(stats.total, 0);
    assert_eq!(stats.idle, 0);
    assert_eq!(stats.busy, 0);
}
```

**Step 2: 运行测试**

Run: `cargo test -p octo-platform-server test_pool`
Expected: PASS

---

## Task 8: 集成测试：多用户并发

**Files:**
- Modify: `crates/octo-platform-server/tests/test_agent_pool.rs`

**Step 1: 编写并发测试**

```rust
#[tokio::test]
async fn test_concurrent_users() {
    let config = PoolConfig {
        soft_max_total: 2,
        hard_max_total: 3,
        ..Default::default()
    };

    let pool = AgentPool::new(config);

    // 模拟多个用户获取实例
    let mut handles = vec![];
    for i in 0..3 {
        let pool = Arc::new(pool.clone());
        let handle = tokio::spawn(async move {
            pool.get_instance(&format!("user_{}", i)).await
        });
        handles.push(handle);
    }

    // 等待所有结果
    let results = futures_util::future::join_all(handles).await;

    // 验证结果
    let mut success_count = 0;
    for result in results {
        if result.is_ok() {
            success_count += 1;
        }
    }

    // 硬上限是 3，应该全部成功
    assert_eq!(success_count, 3);
}
```

**Step 2: 运行测试**

Run: `cargo test -p octo-platform-server test_concurrent`
Expected: PASS (或根据实际 AgentRuntime 创建情况调整)

---

## 实施顺序

1. Task 1: 定义 AgentPool 数据结构
2. Task 2: 实现 AgentPool 基本方法
3. Task 3: 集成 octo-engine AgentRuntime
4. Task 4: 实现 Workspace 和状态持久化
5. Task 5: 实现空闲实例回收
6. Task 6: WebSocket 与 AgentPool 集成
7. Task 7: 集成测试：基本功能
8. Task 8: 集成测试：多用户并发
