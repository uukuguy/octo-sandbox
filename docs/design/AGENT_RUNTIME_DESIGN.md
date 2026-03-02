# AgentRuntime 设计文档

**版本**: v1.0
**创建日期**: 2026-03-03
**状态**: 正式规范（D3 实现依据）

---

## 一、背景与问题

### 当前实现的缺陷

octo-sandbox 在 Phase 2.11 之前，AgentLoop 的调用方式如下：

```
WebSocket 收到消息
  → AgentLoop::new(provider, tools, memory)   ← 每次新建
  → agent_loop.run(session_id, messages, ...)  ← 从 DB 重建历史
  → AgentLoop 销毁
```

这种"无状态、按需创建"的模式存在以下问题：

1. **无持久化 Agent 实体**：每次请求从 DB 重建对话历史，无法维护 Agent 内部状态
2. **AgentRunner 是空壳**：`start()` 只修改状态机，从未 spawn 真正的持久化任务
3. **Session 与 Agent 无绑定**：`SessionData` 无 `agent_id` 字段
4. **多连接无法共享上下文**：多个 WebSocket 连接到同一 Session 无法看到统一的 Agent 行为

### 正确的 Agent Harness 模型

真正的自主智能体（Agent Harness）具备以下特征：

- **持续运行**：Server 启动后 Agent 以持久化 tokio task 形式存在
- **LLM 为头脑**：负责规划与任务委派、工具约束、上下文管理
- **持久化记忆**：对话历史在内存中积累，不依赖每次从 DB 重建
- **观察与审计**：通过广播机制向所有 Channel 播报执行过程
- **可被外部信号中断**：支持 Cancel 消息打断当前 round

---

## 二、架构设计

### 2.1 整体结构

```
多通道输入（可扩展）              Agent 本体（与通道无关）
────────────────────             ──────────────────────────────────────
WebSocket conn A  ──┐            │                                      │
WebSocket conn B  ──┼──→ mpsc →  │  AgentRuntime（持久化 tokio task）   │
Telegram（未来）  ──┤   tx/rx    │    while let Some(msg) = rx.recv()  │ ──→ broadcast
Cron/Timer（未来）──┘            │      history.push(user_msg)          │     ↓
                                 │      AgentLoop.run(&mut history)     │   所有订阅者
                                 │      history 原地更新（含 assistant）  │   看到完整过程
                                 │      SessionStore.set_messages()     │
                                 └──────────────────────────────────────┘
```

### 2.2 核心组件

#### AgentRuntime（Agent 本体）

```rust
pub struct AgentRuntime {
    // 身份
    session_id: SessionId,
    user_id: UserId,
    sandbox_id: SandboxId,

    // 通道（与 Channel 层解耦）
    rx: mpsc::Receiver<AgentMessage>,
    broadcast_tx: broadcast::Sender<AgentEvent>,

    // Harness 核心（跨 round 持久化）
    history: Vec<ChatMessage>,           // 完整对话历史
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<dyn WorkingMemory>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    session_store: Option<Arc<dyn SessionStore>>,
    model: Option<String>,

    // 生命周期控制
    cancel_flag: Arc<AtomicBool>,
}
```

**主循环**：
```rust
pub async fn run(mut self) {
    while let Some(msg) = self.rx.recv().await {
        match msg {
            AgentMessage::UserMessage { content, .. } => {
                self.history.push(ChatMessage::user(&content));
                AgentLoop::new(...).run(&mut self.history, ...).await;
                self.session_store.set_messages(&self.session_id, self.history.clone()).await;
            }
            AgentMessage::Cancel => {
                self.cancel_flag.store(true, Ordering::Relaxed);
            }
        }
    }
    // rx 关闭时自动退出（Session 销毁时 tx drop）
}
```

#### AgentMessage（Channel → Runtime 协议）

```rust
pub enum AgentMessage {
    UserMessage {
        content: String,
        channel_id: String,   // "websocket" / "telegram" / "cron"
    },
    Cancel,
}
```

#### AgentRuntimeHandle（对外句柄，可 clone）

```rust
pub struct AgentRuntimeHandle {
    pub tx: mpsc::Sender<AgentMessage>,
    pub broadcast_tx: broadcast::Sender<AgentEvent>,
    pub session_id: SessionId,
}

impl AgentRuntimeHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> { ... }
    pub async fn send(&self, msg: AgentMessage) -> Result<...> { ... }
}
```

#### AgentRuntimeRegistry（Session → Handle 映射）

```rust
pub struct AgentRuntimeRegistry {
    handles: DashMap<SessionId, AgentRuntimeHandle>,
}

impl AgentRuntimeRegistry {
    pub fn get(&self, session_id: &SessionId) -> Option<AgentRuntimeHandle>;
    pub fn get_or_spawn(&self, session_id, user_id, sandbox_id, initial_history, ...) -> AgentRuntimeHandle;
    pub fn remove(&self, session_id: &SessionId);
}
```

### 2.3 单 Agent 多 Channel 广播

octo-workbench 当前为单智能体架构（一个 AgentRuntime），支持多 WebSocket 连接（PC 端、移动端等）。

```
AgentRuntime
  broadcast_tx: broadcast::Sender<AgentEvent>（容量 256）
       │
       ├── WebSocket conn A：handle.subscribe() → Receiver A
       ├── WebSocket conn B：handle.subscribe() → Receiver B
       └── （未来）Telegram：handle.subscribe() → Receiver C
```

所有订阅者看到相同的事件流，包括：
- 其他 Channel 发来的用户消息（通过 `AgentEvent::UserMessage` 广播）
- Agent 的思考过程（ThinkingDelta）
- 工具调用（ToolStart / ToolResult）
- 最终回复（TextDelta / TextComplete / Done）

### 2.4 AgentRuntime 与 AgentRunner 的职责分工

| 组件 | 职责 |
|------|------|
| `AgentRunner` | 依赖注入容器：提供 `provider()` / `memory()` / `build_tool_registry()` |
| `AgentRuntimeRegistry` | 生命周期管理：spawn / get / remove AgentRuntime |
| `AgentRuntime` | 自主智能体本体：持久化主循环、Harness 执行、历史积累 |
| `AgentLoop` | 无状态执行器：单 round LLM 调用 + Tool 调用链 |
| `ws.rs` (WebSocket) | 纯 Channel 层：路由消息到 AgentRuntime，订阅广播事件 |

---

## 三、数据流

### 3.1 首次消息（Session 新建）

```
1. 用户发 WebSocket 消息（无 session_id）
2. ws.rs 创建新 Session → SessionStore
3. runtime_registry.get_or_spawn(session_id, ...)
   → 从 SessionStore 加载 initial_history（空）
   → spawn AgentRuntime tokio task（持久化运行）
   → 返回 AgentRuntimeHandle
4. ws.rs 订阅广播：handle.subscribe() → rx
5. ws.rs 发消息：handle.send(AgentMessage::UserMessage{content})
6. AgentRuntime.rx.recv() 收到消息，执行 round
7. AgentRuntime 广播 AgentEvent（TextDelta / Done 等）
8. ws.rs 的广播 rx 收到事件，转发给 WebSocket 客户端
```

### 3.2 续接消息（Session 已有 AgentRuntime）

```
1. 用户发 WebSocket 消息（带 session_id）
2. ws.rs 验证 Session → 已存在
3. runtime_registry.get(session_id) → 已有 Handle（复用，不重建）
4. 订阅广播、发消息（同上 4-8）
   ← history 由 AgentRuntime 内存持有，无 DB 读取开销
```

### 3.3 Server 重启恢复

```
1. Server 重启后 AgentRuntime 全部消失
2. 用户带旧 session_id 重连
3. ws.rs 从 SessionStore 读取历史（runtime 每 round 写回 DB）
4. runtime_registry.get_or_spawn(initial_history=DB历史)
5. 新的 AgentRuntime 从历史记录继续
```

---

## 四、生命周期管理

### AgentRuntime 启动条件
- Session 创建后第一条用户消息到达时，由 `AgentRuntimeRegistry.get_or_spawn()` spawn

### AgentRuntime 停止条件
- 所有持有该 Session Handle 的 `tx` 都被 drop（通常是 WebSocket 连接断开）
- `rx.recv()` 返回 `None`，主循环自然退出

### AgentRuntime 优雅取消
- Channel 发送 `AgentMessage::Cancel`
- AgentRuntime 设置 `cancel_flag = true`
- AgentLoop 在下一个 check point 检测到 cancel_flag 后停止当前 round

---

## 五、未来扩展方向

| 扩展 | 实现方式 |
|------|---------|
| Telegram Channel | 新增 Telegram 适配器，持有 `AgentRuntimeHandle.tx`，向同一 AgentRuntime 发消息 |
| 多 Agent | AgentRuntimeRegistry 按 AgentId 而非 SessionId 索引；Session 持有 agent_id 字段 |
| 自主定时触发 | Scheduler 持有 Handle.tx，定时发送 `AgentMessage::UserMessage{content: "[TICK]"}` |
| Agent 迁移 | AgentRuntime 序列化 history + state，反序列化到新节点 |

---

## 六、实现文件索引

| 文件 | 内容 |
|------|------|
| `crates/octo-engine/src/agent/runtime.rs` | AgentMessage、AgentRuntimeHandle、AgentRuntime |
| `crates/octo-engine/src/agent/runtime_registry.rs` | AgentRuntimeRegistry |
| `crates/octo-engine/src/agent/mod.rs` | 导出 |
| `crates/octo-engine/src/lib.rs` | 公开导出给 octo-server |
| `crates/octo-server/src/state.rs` | AppState 新增 runtime_registry 字段 |
| `crates/octo-server/src/ws.rs` | 改造为纯 Channel 层 |

**实现计划**：`docs/plans/2026-03-03-d3-agent-runtime.md`
