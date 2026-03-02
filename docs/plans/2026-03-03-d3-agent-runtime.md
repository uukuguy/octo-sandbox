# D3: AgentRuntime — 持久化 Agent 主循环实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 octo-sandbox 从"每条消息创建一个临时 AgentLoop"改造为"每个 Session 绑定一个持久运行的 AgentRuntime 自主智能体"，WebSocket 作为消息 Channel 向 AgentRuntime 发送消息并订阅广播结果。

**Architecture:**
- `AgentRuntime`：新增结构体，持有完整 Harness（history + provider + tools + memory），运行 `while rx.recv()` 主循环，通过 `broadcast::Sender<AgentEvent>` 向所有订阅的 Channel 广播结果。
- `AgentRuntimeRegistry`：管理 Session → AgentRuntime 的映射（`DashMap<SessionId, AgentRuntimeHandle>`），负责 spawn 和 cleanup。
- WebSocket（ws.rs）：降级为纯 Channel 层——收到用户消息后通过 `mpsc::Sender` 发给对应 AgentRuntime，订阅 `broadcast::Receiver` 接收结果流。

**Tech Stack:** Rust, Tokio (`mpsc`, `broadcast`, `CancellationToken`), DashMap, 现有 `AgentLoop` / `AgentRunner` / `SessionStore`

---

## 背景：当前代码问题

**ws.rs 当前做法（需要改掉）：**
```rust
// 每次 SendMessage 都 new 一个 AgentLoop，处理完即丢弃
let mut agent_loop = octo_engine::AgentLoop::new(provider, tools, memory);
agent_loop.run(&session_id, ..., &mut messages, tx, ...).await;
// 处理完后 messages 写回 SessionStore，下次再从 DB 重建历史
```

**问题：**
1. 无持久化 Agent 实体，每次 cold start 从 DB 重建历史
2. `AgentRunner.start()` 是空壳，从未 spawn 真正的任务
3. `SessionData` 无 `agent_id` 字段，Session 与 Agent 无绑定
4. 多 WebSocket 连接无法共享同一个 Agent 的上下文

---

## Task 1: 定义 AgentMessage 协议和 AgentRuntimeHandle

**目标：** 定义 Channel → AgentRuntime 的消息结构和 AgentRuntime 的对外句柄。

**Files:**
- Create: `crates/octo-engine/src/agent/runtime.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs`

**Step 1: 创建 runtime.rs，定义消息协议和 Handle**

```rust
// crates/octo-engine/src/agent/runtime.rs

use tokio::sync::{broadcast, mpsc};
use octo_types::{SessionId, SandboxId, UserId};
use crate::agent::AgentEvent;

/// Channel → AgentRuntime 的消息
#[derive(Debug, Clone)]
pub enum AgentMessage {
    /// 用户发来的文本消息
    UserMessage {
        content: String,
        /// 消息来源 channel 标识（用于广播给其他 channel）
        channel_id: String,
    },
    /// 外部请求取消当前正在运行的 round
    Cancel,
}

/// AgentRuntime 的对外句柄（可 clone，廉价）
#[derive(Clone)]
pub struct AgentRuntimeHandle {
    /// 向 AgentRuntime 发送消息
    pub tx: mpsc::Sender<AgentMessage>,
    /// 订阅 AgentRuntime 的广播事件
    pub broadcast_tx: broadcast::Sender<AgentEvent>,
    /// 关联的 session_id
    pub session_id: SessionId,
}

impl AgentRuntimeHandle {
    /// 创建一个新的广播订阅者
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.broadcast_tx.subscribe()
    }

    /// 发送用户消息到 AgentRuntime
    pub async fn send(&self, msg: AgentMessage) -> Result<(), mpsc::error::SendError<AgentMessage>> {
        self.tx.send(msg).await
    }
}
```

**Step 2: 在 mod.rs 中导出**

在 `crates/octo-engine/src/agent/mod.rs` 末尾添加：
```rust
pub mod runtime;
pub use runtime::{AgentMessage, AgentRuntimeHandle};
```

**Step 3: 编译验证**
```bash
cd /path/to/octo-sandbox
cargo check -p octo-engine 2>&1 | head -30
```
预期：无错误

**Step 4: Commit**
```bash
git add crates/octo-engine/src/agent/runtime.rs crates/octo-engine/src/agent/mod.rs
git commit -m "feat(agent): define AgentMessage protocol and AgentRuntimeHandle"
```

---

## Task 2: 实现 AgentRuntime 主循环

**目标：** 实现持久化 Agent 主循环——持有完整 Harness，`while rx.recv()` 等待消息，调用 `AgentLoop` 处理，广播结果。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: 在 runtime.rs 中添加 AgentRuntime 结构体**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, warn};

use octo_types::{ChatMessage, SessionId, SandboxId, UserId, ToolContext};
use crate::agent::{AgentEvent, AgentLoop};
use crate::memory::{WorkingMemory, store_traits::MemoryStore};
use crate::providers::Provider;
use crate::tools::ToolRegistry;

/// 持久化运行的 Agent 自主智能体本体
pub struct AgentRuntime {
    // 身份
    session_id: SessionId,
    user_id: UserId,
    sandbox_id: SandboxId,

    // 通道
    rx: mpsc::Receiver<AgentMessage>,
    broadcast_tx: broadcast::Sender<AgentEvent>,

    // Harness 核心（所有字段跨 round 持久化）
    history: Vec<ChatMessage>,
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    model: Option<String>,

    // 生命周期
    cancel_flag: Arc<AtomicBool>,
}

impl AgentRuntime {
    pub fn new(
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        rx: mpsc::Receiver<AgentMessage>,
        broadcast_tx: broadcast::Sender<AgentEvent>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        memory_store: Option<Arc<dyn MemoryStore>>,
        model: Option<String>,
    ) -> Self {
        Self {
            session_id,
            user_id,
            sandbox_id,
            rx,
            broadcast_tx,
            history: initial_history,
            provider,
            tools,
            memory,
            memory_store,
            model,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Agent 主循环入口 — 持续等待消息，处理，广播结果
    pub async fn run(mut self) {
        info!(session_id = %self.session_id.as_str(), "AgentRuntime started");

        while let Some(msg) = self.rx.recv().await {
            match msg {
                AgentMessage::UserMessage { content, .. } => {
                    // 重置取消标志
                    self.cancel_flag.store(false, Ordering::Relaxed);

                    // 追加用户消息到持久化历史
                    self.history.push(ChatMessage::user(&content));

                    // 构建 AgentLoop（每 round 新建，但 history 由 AgentRuntime 持有）
                    let mut agent_loop = AgentLoop::new(
                        self.provider.clone(),
                        self.tools.clone(),
                        self.memory.clone(),
                    );
                    if let Some(ref ms) = self.memory_store {
                        agent_loop = agent_loop.with_memory_store(ms.clone());
                    }
                    if let Some(ref m) = self.model {
                        agent_loop = agent_loop.with_model(m.clone());
                    }

                    let tool_ctx = ToolContext {
                        sandbox_id: self.sandbox_id.clone(),
                        working_dir: PathBuf::from("/tmp/octo-sandbox"),
                    };
                    let _ = tokio::fs::create_dir_all(&tool_ctx.working_dir).await;

                    // 运行一个 round，agent_loop.run() 会修改 messages 并广播 AgentEvent
                    if let Err(e) = agent_loop
                        .run(
                            &self.session_id,
                            &self.user_id,
                            &self.sandbox_id,
                            &mut self.history,
                            self.broadcast_tx.clone(),
                            tool_ctx,
                            Some(self.cancel_flag.clone()),
                        )
                        .await
                    {
                        warn!("AgentRuntime round error: {e}");
                        let _ = self.broadcast_tx.send(AgentEvent::Error {
                            message: e.to_string(),
                        });
                    }
                    // history 已由 agent_loop.run() 原地更新（追加了 assistant 消息）
                }
                AgentMessage::Cancel => {
                    self.cancel_flag.store(true, Ordering::Relaxed);
                    info!(session_id = %self.session_id.as_str(), "AgentRuntime: cancel requested");
                }
            }
        }

        info!(session_id = %self.session_id.as_str(), "AgentRuntime stopped (channel closed)");
    }

    /// 返回当前对话历史（用于 session 持久化）
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }
}
```

**Step 2: 编译验证**
```bash
cargo check -p octo-engine 2>&1 | head -40
```
预期：无错误（注意 `AgentLoop::run` 的签名，如有类型不匹配需对照 `loop_.rs` 调整）

**Step 3: Commit**
```bash
git add crates/octo-engine/src/agent/runtime.rs
git commit -m "feat(agent): implement AgentRuntime persistent main loop"
```

---

## Task 3: 实现 AgentRuntimeRegistry

**目标：** 管理 `SessionId → AgentRuntimeHandle` 的映射，负责 spawn AgentRuntime 和 cleanup。

**Files:**
- Create: `crates/octo-engine/src/agent/runtime_registry.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs`
- Modify: `crates/octo-engine/src/lib.rs`（导出）

**Step 1: 创建 runtime_registry.rs**

```rust
// crates/octo-engine/src/agent/runtime_registry.rs

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc};
use tracing::info;

use octo_types::{ChatMessage, SandboxId, SessionId, UserId};

use crate::agent::{AgentEvent, AgentMessage, AgentRuntime, AgentRuntimeHandle};
use crate::memory::{store_traits::MemoryStore, WorkingMemory};
use crate::providers::Provider;
use crate::tools::ToolRegistry;

const MPSC_CAPACITY: usize = 32;
const BROADCAST_CAPACITY: usize = 256;

/// Session → AgentRuntimeHandle 的注册表
pub struct AgentRuntimeRegistry {
    handles: DashMap<SessionId, AgentRuntimeHandle>,
}

impl AgentRuntimeRegistry {
    pub fn new() -> Self {
        Self {
            handles: DashMap::new(),
        }
    }

    /// 获取已有的 AgentRuntimeHandle（如果存在）
    pub fn get(&self, session_id: &SessionId) -> Option<AgentRuntimeHandle> {
        self.handles.get(session_id).map(|h| h.clone())
    }

    /// Spawn 一个新的 AgentRuntime 并注册其 Handle。
    /// 如果该 session 已有运行中的 runtime，直接返回已有 handle。
    pub fn get_or_spawn(
        &self,
        session_id: SessionId,
        user_id: UserId,
        sandbox_id: SandboxId,
        initial_history: Vec<ChatMessage>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        memory: Arc<dyn WorkingMemory>,
        memory_store: Option<Arc<dyn MemoryStore>>,
        model: Option<String>,
    ) -> AgentRuntimeHandle {
        // 已有 handle 则直接复用
        if let Some(handle) = self.get(&session_id) {
            return handle;
        }

        let (tx, rx) = mpsc::channel::<AgentMessage>(MPSC_CAPACITY);
        let (broadcast_tx, _) = broadcast::channel::<AgentEvent>(BROADCAST_CAPACITY);

        let handle = AgentRuntimeHandle {
            tx,
            broadcast_tx: broadcast_tx.clone(),
            session_id: session_id.clone(),
        };

        let runtime = AgentRuntime::new(
            session_id.clone(),
            user_id,
            sandbox_id,
            initial_history,
            rx,
            broadcast_tx,
            provider,
            tools,
            memory,
            memory_store,
            model,
        );

        // Spawn 持久化主循环
        tokio::spawn(async move {
            runtime.run().await;
        });

        info!(session_id = %session_id.as_str(), "AgentRuntime spawned");

        self.handles.insert(session_id, handle.clone());
        handle
    }

    /// 移除 session 对应的 handle（当 session 销毁时调用）
    /// AgentRuntime 主循环会在 mpsc::Sender 全部 drop 后自动退出
    pub fn remove(&self, session_id: &SessionId) {
        self.handles.remove(session_id);
        info!(session_id = %session_id.as_str(), "AgentRuntime handle removed");
    }

    /// 当前注册的 runtime 数量
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }
}

impl Default for AgentRuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 2: 在 mod.rs 中添加导出**

```rust
// 在 crates/octo-engine/src/agent/mod.rs 添加：
pub mod runtime_registry;
pub use runtime_registry::AgentRuntimeRegistry;
// 同时补上 AgentRuntime 的导出：
pub use runtime::AgentRuntime;
```

**Step 3: 在 lib.rs 中导出（让 octo-server 能用）**

找到 `crates/octo-engine/src/lib.rs` 中 agent 相关的 pub use，添加：
```rust
pub use agent::{AgentRuntimeRegistry, AgentRuntimeHandle, AgentMessage};
```

**Step 4: 编译验证**
```bash
cargo check -p octo-engine 2>&1 | head -40
```

**Step 5: Commit**
```bash
git add crates/octo-engine/src/agent/runtime_registry.rs \
        crates/octo-engine/src/agent/mod.rs \
        crates/octo-engine/src/lib.rs
git commit -m "feat(agent): implement AgentRuntimeRegistry for session-bound runtime management"
```

---

## Task 4: 将 AgentRuntimeRegistry 集成到 AppState

**目标：** `AppState` 持有 `AgentRuntimeRegistry`，替代 ws.rs 中散落的依赖注入。

**Files:**
- Modify: `crates/octo-server/src/state.rs`
- Modify: `crates/octo-server/src/main.rs`（初始化）

**Step 1: 在 AppState 中添加 runtime_registry 字段**

在 `crates/octo-server/src/state.rs` 中：

```rust
// 在 use 段添加：
use octo_engine::AgentRuntimeRegistry;

// 在 AppState struct 中添加字段：
pub struct AppState {
    // ... 现有字段 ...
    /// Session 绑定的 AgentRuntime 注册表
    pub runtime_registry: Arc<AgentRuntimeRegistry>,
}
```

在 `AppState::new()` 参数列表和 `Self { ... }` 初始化中同步添加 `runtime_registry` 字段。

**Step 2: 在 main.rs 中初始化**

找到 `AppState::new(...)` 调用处，添加：
```rust
let runtime_registry = Arc::new(AgentRuntimeRegistry::new());
// 传入 AppState::new(...)
```

**Step 3: 编译验证**
```bash
cargo check -p octo-server 2>&1 | head -40
```

**Step 4: Commit**
```bash
git add crates/octo-server/src/state.rs crates/octo-server/src/main.rs
git commit -m "feat(server): add AgentRuntimeRegistry to AppState"
```

---

## Task 5: 改造 ws.rs — 变为纯 Channel 层

**目标：** ws.rs 不再直接创建 AgentLoop，改为：Session 首次消息时通过 `runtime_registry.get_or_spawn()` 获取 Handle，后续通过 Handle 发消息、订阅广播。

**Files:**
- Modify: `crates/octo-server/src/ws.rs`

**Step 1: 修改 `SendMessage` 处理逻辑**

将 ws.rs 中 `ClientMessage::SendMessage` 分支的 agent 执行部分替换：

**删除这段（约第 246-278 行）：**
```rust
// Create a fresh AgentLoop per invocation...
let provider = state.provider.clone();
// ... 整个 AgentLoop::new() + tokio::spawn 块
```

**替换为：**
```rust
// 获取或 spawn 对应 Session 的 AgentRuntime
let initial_history = state
    .sessions
    .get_messages(&session.session_id)
    .await
    .unwrap_or_default();

let handle = state.runtime_registry.get_or_spawn(
    session.session_id.clone(),
    session.user_id.clone(),
    session.sandbox_id.clone(),
    initial_history,
    state.agent_runner.provider(),
    state.agent_runner.build_tool_registry(&[]),  // 空 filter = 全部工具
    state.agent_runner.memory(),
    Some(state.memory_store.clone()),
    state.model.clone(),
);

// 订阅广播（在 send 之前订阅，避免错过事件）
let mut rx = handle.subscribe();

// 将用户消息推送给 AgentRuntime
let _ = handle
    .send(AgentMessage::UserMessage {
        content: content.clone(),
        channel_id: format!("websocket"),
    })
    .await;
```

**Step 2: 修改事件转发循环**

原来的广播接收循环逻辑不变（`loop { match rx.recv().await { ... } }`），只是 `rx` 来自 `handle.subscribe()` 而不是本地 `broadcast::channel`。

**Step 3: 删除 session messages 写回逻辑**

删除原来的：
```rust
// Get updated messages from agent loop
if let Ok(updated_messages) = agent_handle.await {
    state.sessions.set_messages(&session.session_id, updated_messages).await;
}
```

因为现在 AgentRuntime 持有 history，history 持久化由 AgentRuntime 自己管理（见 Task 6）。

**Step 4: 修改 Cancel 处理**

```rust
ClientMessage::Cancel { session_id } => {
    let sid = SessionId::from_string(&session_id);
    if let Some(handle) = state.runtime_registry.get(&sid) {
        let _ = handle.send(AgentMessage::Cancel).await;
    }
    info!("Agent cancellation requested for session {session_id}");
}
```

**Step 5: 编译验证**
```bash
cargo check -p octo-server 2>&1 | head -50
```

**Step 6: Commit**
```bash
git add crates/octo-server/src/ws.rs
git commit -m "feat(ws): refactor to pure channel layer using AgentRuntimeRegistry"
```

---

## Task 6: AgentRuntime 持久化 history 到 SessionStore

**目标：** 每次 round 完成后，AgentRuntime 将最新 history 写回 `SessionStore`，保证 Server 重启后能从 DB 恢复历史。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: 给 AgentRuntime 注入 SessionStore 引用**

在 `AgentRuntime` struct 中添加：
```rust
session_store: Option<Arc<dyn crate::session::SessionStore>>,
```

在 `AgentRuntime::new()` 中添加对应参数和赋值。

**Step 2: 在 run() 主循环 round 结束后持久化**

在 `AgentMessage::UserMessage` 分支末尾，`agent_loop.run()` 调用之后添加：
```rust
// 持久化 history 到 SessionStore
if let Some(ref store) = self.session_store {
    store.set_messages(&self.session_id, self.history.clone()).await;
}
```

**Step 3: 更新 AgentRuntimeRegistry::get_or_spawn 签名**

在 `runtime_registry.rs` 中添加 `session_store` 参数，透传给 `AgentRuntime::new()`。

**Step 4: 更新 ws.rs 中的调用**

在 `get_or_spawn()` 调用中传入 `Some(state.sessions.clone())`。

**Step 5: 编译验证**
```bash
cargo build --workspace 2>&1 | grep -E "^error" | head -20
```

**Step 6: Commit**
```bash
git add crates/octo-engine/src/agent/runtime.rs \
        crates/octo-engine/src/agent/runtime_registry.rs \
        crates/octo-server/src/ws.rs
git commit -m "feat(agent): persist history to SessionStore after each runtime round"
```

---

## Task 7: 集成测试验证

**目标：** 验证端到端功能正常，多连接共享 Agent 上下文。

**Step 1: 编译完整构建**
```bash
cargo build --workspace 2>&1 | tail -5
```
预期：`Finished` 无 error

**Step 2: 运行单元测试**
```bash
cargo test -p octo-engine agent::runtime 2>&1
```

**Step 3: 启动服务验证（提供给用户执行）**

```bash
# Terminal 1: 启动后端
make server

# Terminal 2: 连接 WebSocket，发送消息
# 使用 wscat 或浏览器 DevTools
wscat -c ws://localhost:3001/ws
# 发送：
{"type":"send_message","content":"Hello, list files in /tmp"}
# 预期：收到 session_created，然后收到 text_delta 流，最终 done
```

**Step 4: 验证 AgentRuntime 复用**

```bash
# 第二次发消息时带上 session_id（从第一次的 session_created 中获取）
{"type":"send_message","session_id":"<session_id>","content":"Now list only .log files"}
# 预期：Agent 能记住上下文（因为 history 由 AgentRuntime 持久持有）
```

**Step 5: Final Commit**
```bash
git add -A
git commit -m "feat(agent): D3 complete - AgentRuntime persistent main loop integrated"
```

---

## 关键设计决策备忘

| 决策 | 选择 | 理由 |
|------|------|------|
| AgentRuntime 生命周期 | 与 Session 绑定 | Session 销毁时 handle drop，rx 关闭，runtime 自然退出 |
| history 持有方 | AgentRuntime（内存） + SessionStore（持久化） | 运行时零 DB 读取，重启后从 DB 恢复 |
| 广播通道 | `broadcast::Sender` | 多 WebSocket 连接订阅同一 Agent |
| AgentLoop 复用 | 每 round 新建 AgentLoop，history 由 Runtime 持有 | AgentLoop 设计为无状态执行器，不改动现有代码 |
| AgentRunner 角色 | 保留，提供 `provider()`/`memory()`/`build_tool_registry()` | AgentRunner 作为依赖提供者，AgentRuntime 作为执行实体 |
