# Phase AQ — 自主能力 + 智能交互 ✅ COMPLETE (6/6)

> 目标：补齐 Agent 自主运行、智能交互和工具发现能力，解锁 5 个 AP deferred 项。
> 日期：2026-04-02
> 依据：Phase AP 全部 18/18 任务完成后的能力缺口分析
> 执行策略：全包分 Wave，每 Wave 独立 commit/验证
> **完成时间**：2026-04-02 06:30
> **Commits**: dcb10f1 (W1), 6288dc2 (W2-4)
> **新增测试**: 29 (15 W1 + 14 W2-4)
> **新增文件**: 5 (interaction.rs, ask_user.rs, tool_search.rs, blob_store.rs, autonomous_audit.rs)
> **新增代码**: ~1220 行

---

## 一、设计决策

| 决策点 | 选项 | 决定 | 理由 |
|--------|------|------|------|
| Session 文件架构 | A:混合 / B:文件为主 / **C:Blob-only** | C | 最小改动解锁 AP-D1，不破坏 SessionStore trait |
| 自主模式集成 | **A:Harness 内部 tick** / B:外部 tick | A | 连续上下文，CC-OSS 风格 |
| ask_user 交互 | A:复用 ApprovalGate / **B:InteractionGate** | B | 独立通道支持多种交互类型（问答/选择/确认） |
| tool_search 算法 | 子串匹配 + 简单评分 | — | 工具数 <200，语义搜索 YAGNI |

---

## 二、依赖图

```
T1 InteractionGate + AskUserTool ──┐
                                    ├── 零依赖，可并行
T2 ToolSearchTool ─────────────────┘
        │
T3 BlobStore ──────────────────────── 零依赖，与 T1/T2 可并行
        │
T4 Harness Tick Loop ──────────────── 依赖 T14(已完成) AutonomousConfig
        │
T5 暂停/恢复 API + Cron 触发 ──────── 依赖 T4
        │
T6 用户感知 + 审计日志 ────────────── 依赖 T4+T5
```

---

## 三、Wave 执行顺序

```
时间 →
─────────────────────────────────────────────────
Wave 1  │ T1(ask_user) + T2(tool_search)   │ 零依赖，可并行
────────┤                                    ├────
Wave 2  │ T3(BlobStore)                     │ 零依赖
────────┤                                    ├────
Wave 3  │ T4(tick loop) + T5(pause/resume)  │ T4→T5 顺序
────────┤                                    ├────
Wave 4  │ T6(用户感知 + 审计)               │ 依赖 T4+T5
─────────────────────────────────────────────────
```

---

## 四、任务详细清单

### Wave 1：智能交互工具（零依赖）

#### T1 — InteractionGate + AskUserTool (~150 行)

- **Deferred**: AP-D14
- **依赖**: 无
- **新增文件**:
  - `crates/octo-engine/src/tools/interaction.rs` (~100 行)
  - `crates/octo-engine/src/tools/ask_user.rs` (~50 行)
- **修改文件**:
  - `tools/mod.rs` (注册)
  - `agent/loop_config.rs` (+interaction_gate 字段)
  - `agent/harness.rs` (传递 gate)
  - `event/mod.rs` (+InteractionRequested 事件)

**InteractionGate 设计**:

```rust
/// 交互请求类型
pub enum InteractionRequest {
    /// 自由文本提问
    Question { prompt: String, default: Option<String> },
    /// 单选
    Select { prompt: String, options: Vec<String> },
    /// 确认 (y/n)
    Confirm { prompt: String },
}

/// 用户回复
pub enum InteractionResponse {
    Text(String),
    Selected(usize, String),
    Confirmed(bool),
    Timeout,
}

/// 异步交互通道
pub struct InteractionGate {
    pending: DashMap<String, oneshot::Sender<InteractionResponse>>,
}

impl InteractionGate {
    /// 发起交互请求，返回等待通道
    pub fn request(&self, id: &str, req: InteractionRequest)
        -> oneshot::Receiver<InteractionResponse>;

    /// 前端/TUI 回复交互
    pub fn respond(&self, id: &str, resp: InteractionResponse);
}
```

**AskUserTool**:
- 参数: `question: String`, `options: Option<Vec<String>>`, `default: Option<String>`
- 当 `options` 非空 → `InteractionRequest::Select`
- 当 `options` 为空且无 default → `InteractionRequest::Question`
- 超时 60 秒，返回 `InteractionResponse::Timeout` → ToolOutput 错误
- 描述手册 (~30 行): 何时用/何时不用/示例

**事件**: `AgentEvent::InteractionRequested { tool_id: String, request: InteractionRequest }`

**TUI 集成要点** (不在本 Task 实现，仅定义接口):
- 收到 `InteractionRequested` → 显示问题弹窗（类似 approval dialog）
- 用户输入后 → `gate.respond(tool_id, response)`

**测试** (~6 tests):
- InteractionGate request/respond 往返
- 超时返回 Timeout
- Question/Select/Confirm 三种类型
- AskUserTool 参数解析
- AskUserTool execute 集成（mock gate）
- 并发多请求

---

#### T2 — ToolSearchTool (~100 行)

- **Deferred**: AP-D13
- **依赖**: 无
- **新增文件**: `crates/octo-engine/src/tools/tool_search.rs` (~60 行)
- **修改文件**:
  - `tools/mod.rs` (注册 + ToolRegistry::search 方法)

**ToolRegistry 扩展**:

```rust
impl ToolRegistry {
    /// 模糊搜索已注册工具
    pub fn search(&self, query: &str, limit: usize) -> Vec<ToolSearchResult> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<ToolSearchResult> = self.tools.iter()
            .map(|(name, tool)| {
                let spec = tool.spec();
                let name_lower = name.to_lowercase();
                let desc_lower = spec.description.to_lowercase();

                let score = if name_lower == query_lower { 100 }
                    else if name_lower.contains(&query_lower) { 80 }
                    else if desc_lower.contains(&query_lower) { 40 }
                    else { 0 };

                ToolSearchResult { name: name.clone(), description: spec.description.clone(), score }
            })
            .filter(|r| r.score > 0)
            .collect();

        results.sort_by(|a, b| b.score.cmp(&a.score));
        results.truncate(limit);
        results
    }
}

pub struct ToolSearchResult {
    pub name: String,
    pub description: String,
    pub score: u32,
}
```

**ToolSearchTool**:
- 参数: `query: String`, `limit: Option<usize>` (default 10)
- 调用 `registry.search(query, limit)`
- 返回 JSON 数组: `[{"name": "bash", "description": "...", "score": 80}]`
- 描述手册 (~20 行): 何时用（>50 个工具时搜索比列表更有效）

**测试** (~4 tests):
- 精确匹配 score=100
- 名称包含 score=80
- 描述包含 score=40
- limit 截断 + 排序

---

### Wave 2：工具结果外部持久化

#### T3 — BlobStore + Harness 集成 (~180 行)

- **Deferred**: AP-D1
- **依赖**: 无
- **新增文件**: `crates/octo-engine/src/storage/blob_store.rs` (~120 行)
- **新增文件**: `crates/octo-engine/src/storage/mod.rs` (~5 行)
- **修改文件**:
  - `agent/loop_config.rs` (+blob_store 字段)
  - `agent/harness.rs` (~40 行, 工具结果 blob 化)
  - `root/mod.rs` (+blobs_dir 方法)

**BlobStore 设计**:

```rust
pub struct BlobStore {
    base_dir: PathBuf,  // ~/.octo/blobs/
}

/// Blob 引用格式
pub const BLOB_PREFIX: &str = "[blob:sha256:";
pub const BLOB_SUFFIX: &str = "]";

/// 超过此大小的工具输出自动 blob 化
pub const BLOB_THRESHOLD_BYTES: usize = 4096;

impl BlobStore {
    pub fn new(base_dir: PathBuf) -> Self;

    /// 存储内容，返回 SHA-256 hex hash
    pub fn store(&self, content: &[u8]) -> Result<String>;

    /// 按 hash 加载内容
    pub fn load(&self, hash: &str) -> Result<Vec<u8>>;

    /// 检查 hash 是否存在
    pub fn exists(&self, hash: &str) -> bool;

    /// 判断文本是否为 blob 引用
    pub fn is_blob_ref(text: &str) -> Option<&str>; // 返回 hash

    /// 解析 blob 引用，加载原始内容
    pub fn resolve(&self, text: &str) -> Result<String>;
}
```

**存储路径**: `base_dir/<hash[0..2]>/<hash[2..]>` — 两级目录避免单目录过大

**Harness 集成逻辑**:

```rust
// 工具执行后，存入 session 前：
if let Some(ref blob_store) = config.blob_store {
    if result_content.len() > BLOB_THRESHOLD_BYTES {
        let hash = blob_store.store(result_content.as_bytes())?;
        // 替换 ToolResult content 为引用
        result_content = format!("{BLOB_PREFIX}{hash}{BLOB_SUFFIX}");
        // 注意：当前轮的 LLM 消息仍使用完整原文
        // blob 引用仅在存入 session 后的重新加载时生效
    }
}
```

**关键语义**:
- LLM 在**当前轮**看到完整工具输出（不受 blob 影响）
- blob 引用在**下次加载 session 消息**时生效（节省 token）
- 这与 CC-OSS 的 `recordContentReplacement` 语义一致

**OctoRoot 扩展**:
```rust
impl OctoRoot {
    pub fn blobs_dir(&self) -> PathBuf {
        self.global_root().join("blobs")
    }
}
```

**测试** (~5 tests):
- store/load 往返
- SHA-256 hash 正确性
- 两级目录结构
- is_blob_ref 解析
- 阈值判断（<4KB 不 blob 化）

---

### Wave 3：自主模式 Phase 2

#### T4 — Harness Tick Loop (~200 行)

- **Deferred**: AP-D9 (部分)
- **依赖**: T14 (已完成)
- **修改文件**:
  - `agent/harness.rs` (~120 行)
  - `agent/autonomous.rs` (~30 行, record_tick 方法)
  - `agent/mod.rs` (~10 行, AgentMessage 新变体)
  - `event/mod.rs` (~20 行, AgentEvent 新变体)

**新增 AgentMessage 变体**:
```rust
pub enum AgentMessage {
    // ... existing ...
    Pause,
    Resume,
    UserPresence(bool),  // 预留给 T6
}
```

**新增 AgentEvent 变体**:
```rust
pub enum AgentEvent {
    // ... existing ...
    AutonomousSleeping { secs: u64 },
    AutonomousTick { round: u32 },
    AutonomousPaused,
    AutonomousResumed,
    AutonomousExhausted { reason: String },
}
```

**Harness Tick Loop 伪代码**:

```rust
// 在主循环的 Done/Completed 返回点之后
'autonomous: loop {
    // 仅当自主模式启用时执行
    let auto_config = match &config.autonomous_config {
        Some(c) if c.enabled => c,
        _ => break 'autonomous,
    };

    // 1. 预算检查
    if let Some(reason) = auto_state.check_budget() {
        yield AgentEvent::AutonomousExhausted { reason };
        break 'autonomous;
    }

    // 2. 检查暂停/取消信号 (非阻塞)
    while let Ok(msg) = message_rx.try_recv() {
        match msg {
            AgentMessage::Pause => {
                yield AgentEvent::AutonomousPaused;
                // 阻塞等待 Resume 或 Cancel
                loop {
                    match message_rx.recv().await {
                        Some(AgentMessage::Resume) => {
                            yield AgentEvent::AutonomousResumed;
                            break;
                        }
                        Some(AgentMessage::Cancel) => break 'autonomous,
                        _ => continue,
                    }
                }
            }
            AgentMessage::Cancel => break 'autonomous,
            AgentMessage::UserPresence(online) => {
                auto_state.user_online = online;
            }
            _ => {}
        }
    }

    // 3. Sleep (可被 cancel/pause 中断)
    let sleep_dur = auto_state.sleep_duration();
    yield AgentEvent::AutonomousSleeping { secs: sleep_dur };

    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(sleep_dur)) => {}
        msg = message_rx.recv() => {
            match msg {
                Some(AgentMessage::Cancel) => break 'autonomous,
                Some(AgentMessage::Pause) => {
                    yield AgentEvent::AutonomousPaused;
                    // wait for resume...
                    continue 'autonomous;
                }
                _ => {}
            }
        }
    }

    // 4. 注入 tick 消息
    auto_state.record_tick();
    yield AgentEvent::AutonomousTick { round: auto_state.rounds_completed };
    messages.push(ChatMessage::system(
        "<tick> Autonomous check-in. Review progress and continue or sleep."
    ));

    // 5. 重新进入主循环 (LLM call → tool execution → ...)
    // 这里复用主循环的单轮逻辑
    continue 'main_loop;  // 跳回主循环头部
}
```

**AutonomousState 扩展**:
```rust
impl AutonomousState {
    pub fn record_tick(&mut self) {
        self.rounds_completed += 1;
        self.last_tick_at = Some(Instant::now());
    }
}
```

**测试** (~4 tests):
- 预算耗尽退出 tick loop
- Pause → Resume 恢复
- Cancel 立即退出
- tick 消息注入验证

---

#### T5 — 暂停/恢复 API + Cron 触发 (~150 行)

- **Deferred**: AP-D9 (完成)
- **依赖**: T4
- **修改文件**:
  - `agent/executor.rs` (~30 行)
  - `agent/runtime.rs` (~40 行)
  - `scheduler/mod.rs` (~50 行)
  - `crates/octo-server/src/api/` (~30 行, REST endpoints)

**AgentExecutorHandle 扩展**:
```rust
impl AgentExecutorHandle {
    pub async fn pause(&self) -> Result<()> {
        self.tx.send(AgentMessage::Pause).await?;
        Ok(())
    }

    pub async fn resume(&self) -> Result<()> {
        self.tx.send(AgentMessage::Resume).await?;
        Ok(())
    }

    pub async fn set_user_presence(&self, online: bool) -> Result<()> {
        self.tx.send(AgentMessage::UserPresence(online)).await?;
        Ok(())
    }
}
```

**AgentRuntime 扩展**:
```rust
impl AgentRuntime {
    /// 启动自主模式 agent，返回可控制的 handle
    pub async fn start_autonomous(
        &self,
        session_id: SessionId,
        config: AutonomousConfig,
    ) -> Result<AgentExecutorHandle>;
}
```

**Scheduler Cron 触发**:
- `ScheduledTask` 新增字段: `autonomous: bool`
- 当 `autonomous: true` 时：
  - Scheduler tick 检测到 cron 到期
  - 调用 `runtime.start_autonomous()` 代替普通 `execute_task()`
  - 保持 handle 引用，不等待完成
  - 如果已有运行中 autonomous → 跳过（不重复启动）

**Server API** (仅在 octo-server 需要时):
```
POST /api/v1/autonomous/start   → start_autonomous()
POST /api/v1/autonomous/pause   → handle.pause()
POST /api/v1/autonomous/resume  → handle.resume()
GET  /api/v1/autonomous/status  → AutonomousState JSON
```

**测试** (~4 tests):
- pause()/resume() 信号传递
- start_autonomous 创建并返回 handle
- Cron 触发自主模式启动
- 重复启动跳过

---

### Wave 4：自主模式 Phase 3

#### T6 — 用户感知 + 自主审计日志 (~200 行)

- **Deferred**: AP-D10
- **依赖**: T4+T5
- **新增文件**: `crates/octo-engine/src/agent/autonomous_audit.rs` (~90 行)
- **修改文件**:
  - `agent/harness.rs` (~40 行, 用户感知 + 审计写入)
  - `agent/autonomous.rs` (~20 行, user_online 状态)
  - `audit/mod.rs` (~20 行, autonomous 审计类别)
  - `crates/octo-server/src/api/` (~30 行, 审计查询端点)

**用户感知逻辑**:

```rust
// harness tick loop 中的 sleep 计算
let sleep_dur = if auto_config.user_presence_aware && auto_state.user_online {
    auto_config.active_sleep_secs  // 短间隔 (5s)
} else {
    auto_state.sleep_duration()    // idle_sleep 或工具请求
};

// tick 消息根据用户状态调整
let tick_msg = if auto_state.user_online {
    "<tick> Autonomous check-in. User is online. Summarize progress briefly."
} else {
    "<tick> Autonomous check-in. Continue working quietly."
};
```

**TUI 用户状态转发**:
```rust
// event_handler.rs: 终端焦点变化时
AppEvent::FocusGained => {
    state.has_focus = true;
    let _ = state.handle.tx.try_send(AgentMessage::UserPresence(true));
}
AppEvent::FocusLost => {
    state.has_focus = false;
    let _ = state.handle.tx.try_send(AgentMessage::UserPresence(false));
}
```

**自主审计日志**:

```rust
pub struct AutonomousAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub event_type: AutonomousAuditEvent,
    pub details: serde_json::Value,
}

pub enum AutonomousAuditEvent {
    Started { config_summary: String },
    TickCompleted { round: u32, tokens_used: u64, cost_usd: f64 },
    Paused { reason: String },
    Resumed,
    BudgetExhausted { limit: String, value: String },
    UserPresenceChanged { online: bool },
    Completed { total_rounds: u32, total_tokens: u64, total_cost_usd: f64 },
    Failed { error: String },
}

impl AutonomousAuditEntry {
    /// 转换为通用 AuditRecord 写入现有审计表
    pub fn to_audit_record(&self) -> AuditRecord {
        AuditRecord {
            event_type: format!("autonomous.{}", self.event_type.name()),
            details: self.details.clone(),
            timestamp: self.timestamp,
            // ...
        }
    }
}
```

**Server 审计查询**:
```
GET /api/v1/autonomous/audit?session_id=xxx&limit=50
```

**测试** (~5 tests):
- 用户在线时 active_sleep_secs 生效
- 用户离线时 idle_sleep_secs 生效
- UserPresence 消息正确传递
- 审计记录写入和查询
- AutonomousAuditEvent 序列化

---

## 五、工作量总结

| Wave | 任务 | 新增/修改代码 | 累计 |
|------|------|-------------|------|
| W1 | T1(ask_user) + T2(tool_search) | ~250 行 | 250 |
| W2 | T3(BlobStore) | ~180 行 | 430 |
| W3 | T4(tick loop) + T5(pause/resume) | ~350 行 | 780 |
| W4 | T6(用户感知+审计) | ~200 行 | 980 |
| **总计** | **6 任务** | **~980 行** | |

---

## 六、Deferred 状态变更

| ID | 内容 | Phase AQ 后状态 |
|----|------|----------------|
| AP-D1 | 工具结果外部持久化 | ✅ BlobStore (T3) |
| AP-D9 | 自主模式 Phase 2 | ✅ Tick Loop + Pause/Resume (T4+T5) |
| AP-D10 | 自主模式 Phase 3 | ✅ 用户感知 + 审计 (T6) |
| AP-D13 | tool_search 工具 | ✅ ToolSearchTool (T2) |
| AP-D14 | ask_user 工具 | ✅ InteractionGate + AskUserTool (T1) |
| AP-D2 | max_output_tokens 自动升级 | ⏳ 保持 (ContinuationTracker 重构) |
| AP-D3 | Fallback 剥离 thinking | ⏳ 保持 (ProviderChain 重构) |
| AP-D4 | Teleport branch 名 | ⏳ 保持 (低优先) |
| AP-D6 | 会话抄本 | ⏳ 保持 (可在 BlobStore 基础上追加) |
| AP-D7 | 会话 fork/rewind | ⏳ 保持 (需 session 快照) |
| AP-D8 | MCP OAuth | ⏳ 保持 (MCP SDK 升级) |

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| AQ-D1 | InteractionGate TUI 弹窗集成 | T1 完成 + TUI overlay 扩展 | ⏳ |
| AQ-D2 | BlobStore GC（清理未引用 blob） | T3 完成 + session 级 blob 引用追踪 | ⏳ |
| AQ-D3 | tool_search 语义搜索（embedding） | 向量存储集成 | ⏳ |
| AQ-D4 | 自主模式 Webhook 触发 | HTTP server endpoint + 认证 | ⏳ |
| AQ-D5 | 自主模式 MessageQueue 触发 | 消息队列集成 (Redis/NATS) | ⏳ |
