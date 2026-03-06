# P1-4 Agent 池 + WebSocket 集成设计

> 日期：2026-03-04
> 状态：设计完成
> 依赖：P1-3 PlatformState + Per-User AgentRuntime

---

## 1. 架构总览

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Platform Server                                  │
│                                                                       │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    UserRuntime                                  │  │
│  │                                                                │  │
│  │   ┌─────────────────┐    ┌─────────────────────────────────┐ │  │
│  │   │  Agent Pool     │    │  Isolation Strategy             │ │  │
│  │   │                 │    │  ┌─────────────────────────────┐ │ │  │
│  │   │  [Agent 1] idle │    │  │ 1. Process (高安全)        │ │ │  │
│  │   │  [Agent 2] busy │    │  │ 2. Memory (默认✓)          │ │ │  │
│  │   │  [Agent 3] idle │    │  │ 3. Session (高并发)        │ │ │  │
│  │   │  ...            │    │  └─────────────────────────────┘ │ │  │
│  │   │                 │    │                                   │ │  │
│  │   │  soft_max: 5    │    │  可配置，按场景切换            │ │  │
│  │   │  hard_max: 10   │    │                                   │ │  │
│  │   └─────────────────┘    └─────────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                       │
│  WebSocket: /ws/{session_id}                                         │
└─────────────────────────────────────────────────────────────────────┘
```

**核心设计：**
- Agent 池是**共享热实例**，不属于特定用户
- 用户登出 → 持久化状态 → 清空上下文 → 归还池
- 下用户登录 → 从池获取热实例 → 恢复/初始化状态

---

## 2. 核心组件

| 组件 | 职责 |
|------|------|
| `AgentPool` | 管理 Agent 实例生命周期：创建、回收、复用 |
| `AgentInstance` | 单个 Agent 运行实例，含状态、引用计数 |
| `Workspace` | 用户级工作空间，隔离的记忆/会话上下文 |
| `IsolationStrategy` | 隔离策略：Process / Memory (默认) / Session |

```rust
pub struct AgentPool {
    soft_max_total: u32,
    hard_max_total: u32,
    min_idle: u32,
    max_idle: u32,
    idle_timeout: Duration,
    strategy: IsolationStrategy,
    instances: Vec<AgentInstance>,
}

pub enum IsolationStrategy {
    Process,   // 进程级隔离
    Memory,    // 内存级隔离（默认）
    Session,   // 会话级隔离
}

pub struct AgentInstance {
    id: InstanceId,
    runtime: AgentRuntime,           // octo-engine 的 AgentRuntime
    workspace: Option<Workspace>,      // 当前占用的工作空间
    state: InstanceState,
    last_used: DateTime<Utc>,
}

pub struct Workspace {
    user_id: UserId,
    session_ids: Vec<SessionId>,
    working_memory: WorkingMemory,     // 来自 octo-engine
    context: ContextSnapshot,
}
```

---

## 3. 数据流

```
用户发消息
    │
    ▼
WebSocket Handler
    │
    ▼
获取/创建 AgentInstance ◄── 池中空闲？→ 创建新实例
    │
    ▼
检查工作空间
    │
    ├─ 有 → 恢复上下文（Memory/Session）
    │         │
    │         ▼
    │      处理消息 → AgentRuntime
    │
    └─ 无 → 创建新工作空间
              │
              ▼
           初始化 → AgentRuntime
              │
              ▼
           处理消息
    │
    ▼
返回流式响应
    │
    ▼
用户登出 / 超时 → 持久化状态 → 清空 workspace → 归还池
```

---

## 4. 隔离策略详解

### 4.1 Memory 级隔离（默认）

```
┌─────────────────────────────────────────────────────┐
│              AgentPool (单进程)                       │
│                                                      │
│  ┌───────────────────────────────────────────────┐  │
│  │            AgentInstance                      │  │
│  │                                                │  │
│  │   workspace: Option<Workspace>                 │  │
│  │                                                │  │
│  │   ┌─────────────────────────────────────────┐│  │
│  │   │           Workspace                      ││  │
│  │   │  user_id: UserId                       ││  │
│  │   │  session_ids: Vec<SessionId>          ││  │
│  │   │  working_memory: WorkingMemory        ││  │
│  │   │  context: ContextSnapshot             ││  │
│  │   └─────────────────────────────────────────┘│  │
│  └───────────────────────────────────────────────┘  │
│                                                      │
│  切换用户: workspace = None → 清空 → 新建            │
└─────────────────────────────────────────────────────┘
```

| 操作 | 行为 |
|------|------|
| 用户 A 占用 | `workspace = Some(Workspace::new(A))` |
| 用户 A 登出 | 持久化 → `workspace = None` |
| 用户 B 占用 | 检查空闲 → `workspace = Some(Workspace::new(B))` |

### 4.2 Process 级隔离

```
┌──────────┐   ┌──────────┐   ┌──────────┐
│ Process  │   │ Process  │   │ Process  │
│  Agent 1 │   │ Agent 2  │   │ Agent 3  │
│ (User A) │   │ (User B) │   │ (User C) │
└──────────┘   └──────────┘   └──────────┘
```

### 4.3 Session 级隔离

```
┌─────────────────────────────────────┐
│       AgentInstance (单 Agent)       │
│                                      │
│  session_map: HashMap<SessionId,     │
│                     SessionContext>  │
│                                      │
│  Session 1 ──┐                      │
│  Session 2 ──┼──→ 并行处理          │
│  Session 3 ──┘                      │
└─────────────────────────────────────┘
```

---

## 5. Agent 池生命周期

### 5.1 获取实例

```
get_instance(user_id) → AgentInstance
│
├─ 检查池中空闲实例
│   │
│   └─ 有 → 分配 → 恢复/创建 workspace → 返回
│
├─ 池已满？
│   │
│   ├─ 未达 hard_max_total → 创建新实例 → 返回
│   └─已达 hard_max_total → 等待 / 返回错误
│
└─ 定期检查 min_idle，保持预热实例
```

### 5.2 归还实例

```
release_instance(instance_id)
│
├─ 持久化用户状态
│   └─ WorkingMemory → Session Store
│
├─ 清空 workspace（隔离保证）
│   └─ workspace = None
│
├─ 标记为空闲
│
└─ 检查池大小
    │
    └─ 超过 min_idle + 空闲超时 → 销毁
```

---

## 6. WebSocket 消息协议

### Client → Server

```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "chat")]
    Chat { content: String },

    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "create_session")]
    CreateSession { name: Option<String> },

    #[serde(rename = "resume_session")]
    ResumeSession { session_id: String },

    #[serde(rename = "pause")]
    Pause,      // 暂停 Agent，保留状态

    #[serde(rename = "stop")]
    Stop,       // 停止 Agent，清空状态
}
```

### Server → Client

```rust
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "response")]
    Response { content: String, done: bool },

    #[serde(rename = "error")]
    Error { code: String, message: String },

    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "session_created")]
    SessionCreated { session_id: String },

    #[serde(rename = "session_resumed")]
    SessionResumed { session_id: String },

    #[serde(rename = "agent_state")]
    AgentState { state: AgentState },  // Running/Paused/Stopped
}
```

---

## 7. 错误处理

| 错误码 | 说明 | HTTP 状态码 |
|--------|------|-------------|
| `SESSION_NOT_FOUND` | 会话不存在 | 404 |
| `SESSION_ACCESS_DENIED` | 无权访问会话 | 403 |
| `POOL_EXHAUSTED` | 池资源耗尽 | 503 |
| `AGENT_ERROR` | Agent 运行错误 | 500 |
| `INVALID_MESSAGE` | 消息格式错误 | 400 |

---

## 8. 设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| Agent 池模式 | 共享热实例 | 低延迟，复用预热 |
| 隔离策略 | Memory 级（默认）+ Process/Session 可选 | 平衡隔离与性能 |
| 配额控制 | 软配额 + 硬配额 | 保体验 + 控制资源 |
| 用户登出 | 持久化 → 清空 workspace → 归还池 | 保证隔离 + 热复用 |
