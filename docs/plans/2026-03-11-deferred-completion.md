# Deferred 项完成方案 — Wave 1 + Wave 2 (方案 B)

> **目标**: 完成 17 个 Deferred 项中的 10 个高价值核心项，覆盖安全加固、可观测性、实时交互、平台补全、智能路由。
>
> **基线**: 1343 tests passing @ commit `49bd94f`
>
> **预估工期**: ~8 天（并行可压缩到 5 天）

---

## 总览

| Wave | 主题 | Tasks | 预估 |
|------|------|-------|------|
| **Wave 1 (P0)** | 安全加固 + 可观测性 | T1-T5 | 1.5 天 |
| **Wave 2 (P1)** | 实时交互 + 平台补全 + 智能路由 | T6-T10 | 5-7 天 |

### 依赖关系

```
T2 (Symlink) ──────────(独立)
T5 (TTL 清理) ─────────(独立)
T3 (Observability) ──→ T8 (Dashboard 实时) ──→ T9 (协作面板)
T4 (EventStore) ──────→ T8
T1 (Canary) ──────────→ T7 (ApprovalGate，共享 SafetyPipeline 模式)
T7 (ApprovalGate) ────→ T6 (Platform WS)
T10 (SmartRouting) ───(独立)
```

### 执行波次

```
Wave 1 并行:
  Agent-A: T2 (Symlink) + T5 (TTL 清理)      ← 独立，0.5 天
  Agent-B: T3 (Observability) + T1 (Canary)   ← 同文件(harness.rs)，串行 1 天
  Agent-C: T4 (EventStore + REST)              ← 独立，1 天

Wave 2 并行:
  Agent-D: T6 (Platform WS) + T7 (ApprovalGate)  ← 串行依赖，3 天
  Agent-E: T8 (Dashboard 实时) + T9 (协作面板)    ← 串行依赖，2-3 天
  Agent-F: T10 (SmartRouting)                      ← 独立，3-4 天
```

---

## Wave 1: 安全加固 + 可观测性

### T1: Canary Token 接入 AgentLoop

**目标**: 在系统提示词中注入 canary token，LLM 输出如包含 canary 则触发 SecurityBlocked。

**现状分析**:
- `CanaryGuardLayer` 已实现 (`security/pipeline.rs:266-324`)，含 `canary()` getter
- `SafetyPipeline` 已在 harness.rs 的 3 个检查点调用（input:322, output:532, tool_result:959）
- `AgentLoopConfig` 已有 `safety_pipeline: Option<Arc<SafetyPipeline>>`
- **缺失**: canary 从未注入系统提示词，且 `SafetyPipeline` 创建时未包含 `CanaryGuardLayer`

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/octo-engine/src/agent/loop_config.rs` | 新增 `canary_token: Option<String>` 字段 + builder 方法 |
| `crates/octo-engine/src/agent/harness.rs:114-126` | 系统提示词构建后追加 canary: `\n\n<!-- CANARY: {token} -->` |
| `crates/octo-engine/src/agent/runtime.rs:202-210` | 创建默认 `SafetyPipeline` 含 `CanaryGuardLayer::with_default_canary()` |
| `crates/octo-engine/src/agent/executor.rs:198-219` | 传递 `safety_pipeline` 和 `canary_token` 到 `AgentLoopConfig` |

**集成要点**:
- Canary 注入必须在 `SystemPromptBuilder::build()` **之后**（避免 manifest override 绕过）
- 添加辅助函数 `fn inject_canary(prompt: &str, canary: &str) -> String`
- 输出校验已由 SafetyPipeline 自动处理，无需额外代码

**测试**:
- 单元测试: `inject_canary()` 正确追加 token
- 集成测试: mock agent loop，输出含 canary → 触发 SecurityBlocked
- 回归: `canary_token: None` 时现有测试不受影响

**风险**: manifest 的 `system_prompt` override (harness.rs:185-189) 会绕过 builder，canary 必须在 build() 之后追加。

---

### T2: Symlink 防护

**目标**: 文件工具操作前检测并拒绝符号链接，防止沙箱逃逸。

**现状分析**:
- `file_read.rs:80`, `file_write.rs:81`, `file_edit.rs:91` 均直接使用 `tokio::fs`
- `PathValidator` trait 存在 (`octo-types/src/tool.rs:180`) 但仅检查路径包含关系
- **无符号链接检测**

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/octo-engine/src/tools/path_safety.rs` | **新建**: 共享辅助函数 `reject_symlink(path) -> Option<ToolOutput>` |
| `crates/octo-engine/src/tools/mod.rs` | 添加 `pub mod path_safety;` |
| `crates/octo-engine/src/tools/file_read.rs:76` | path validation 后插入 `reject_symlink()` 调用 |
| `crates/octo-engine/src/tools/file_write.rs:69` | 同上 |
| `crates/octo-engine/src/tools/file_edit.rs:82` | 同上 |

**实现**:
```rust
// crates/octo-engine/src/tools/path_safety.rs
pub fn reject_symlink(path: &std::path::Path) -> Option<ToolOutput> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Some(ToolOutput::error(format!(
                "Refusing to follow symlink: {}", path.display()
            )))
        }
        _ => None,
    }
}
```

**测试**:
- 临时目录创建 symlink → 调用 file_read/write/edit → 验证返回 error
- 不存在的路径（file_write 新建）→ `symlink_metadata` 返回 Err → 正常通过

**风险**: TOCTOU 竞争条件在 native 模式下存在，Docker/WASM 沙箱自带隔离。

---

### T3: Observability publish() 接入 Harness

**目标**: 在 agent loop 关键路径补齐 TelemetryBus 事件发布。

**现状分析**:
- harness.rs 已有 3 个 publish 点: `LoopTurnStarted` (line 225), `ToolCallStarted` (line 696), `ToolCallCompleted` (line 909)
- **缺失**: `ContextDegraded`, `LoopGuardTriggered`, `TokenBudgetUpdated` 事件

**修改文件**:

| 文件 | 位置 | 新增事件 |
|------|------|---------|
| `crates/octo-engine/src/agent/harness.rs:243` | `DegradationLevel != None` 分支后 | `ContextDegraded { session_id, level }` |
| `crates/octo-engine/src/agent/harness.rs` | budget 计算后 | `TokenBudgetUpdated { session_id, used, total, ratio }` |
| `crates/octo-engine/src/agent/harness.rs` | `LoopGuardVerdict::Abort` 分支 | `LoopGuardTriggered { session_id, reason }` |

**集成要点**:
- 使用已有的 `config.event_bus: Option<Arc<TelemetryBus>>` 字段
- `publish()` 为 fire-and-forget，不阻塞 loop
- 可能需要 `ContextBudgetManager::snapshot()` 方法获取 (used, total, ratio)

**测试**:
- 订阅 TelemetryBus → 运行 test loop → 断言收到正确事件类型
- 小 budget 触发 degradation → 验证 `ContextDegraded` 事件

---

### T4: EventStore 初始化 + REST 端点

**目标**: 将事件持久化到 SQLite，并通过 REST API 暴露查询。

**现状分析**:
- `EventStore` 已完整实现 (`event/store.rs`) — SQLite append-only + sequence numbers
- `StateReconstructor` 已实现 (`event/reconstructor.rs`) — 事件重放
- `TelemetryBus.with_event_store()` 方法已存在 (bus.rs:73-81)
- **缺失**: AgentRuntime 未创建 EventStore，无 REST 端点

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/octo-engine/src/agent/runtime.rs:202-210` | 创建 `EventStore::new(conn.clone())` 并 attach 到 TelemetryBus |
| `crates/octo-engine/src/agent/runtime.rs` | 新增字段 `event_store: Option<Arc<EventStore>>` + getter |
| `crates/octo-server/src/api/events.rs` | **新建**: 3 个端点 |
| `crates/octo-server/src/api/mod.rs` | 添加 `pub mod events;` + 路由注册 |

**REST 端点设计**:
```
GET /api/events?after_sequence=N&limit=100
    → event_store.read_after(sequence, limit)
    → 返回 Vec<StoredEvent>

GET /api/events/session/{session_id}?limit=100
    → event_store.read_by_session(session_id, limit)
    → 返回 Vec<StoredEvent>

GET /api/events/stats
    → { count, latest_sequence }
```

**测试**:
- 单元测试: publish → EventStore → read_after 验证持久化
- API 测试: 遵循现有 memories.rs / audit.rs 测试模式

**风险**: 每次 `publish()` 触发 SQLite INSERT，高吞吐场景可能需要批量写入（P2 优化）。

---

### T5: Memory TTL 清理定时任务

**目标**: 定时删除过期记忆条目（`created_at + ttl < now`）。

**现状分析**:
- `ttl: Option<i64>` 字段已存在于 memories 表 (sqlite_store.rs:49)
- `time_decay()` 仅影响搜索排序，不删除记录
- Scheduler 存在 (`scheduler/mod.rs`) 支持 cron 定时任务
- **缺失**: `delete_expired()` 方法

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/octo-engine/src/memory/store_traits.rs` | trait 新增 `async fn delete_expired(&self) -> Result<usize>` (默认返回 0) |
| `crates/octo-engine/src/memory/sqlite_store.rs` | 实现 `delete_expired()`: `DELETE FROM memories WHERE ttl IS NOT NULL AND (created_at + ttl) < ?1` |
| `crates/octo-engine/src/agent/runtime.rs` | 新增 `cleanup_expired_memories()` 便捷方法 |
| `crates/octo-server/src/main.rs` | spawn 定时清理任务 (每小时) |

**实现要点**:
- SQL: `DELETE FROM memories WHERE ttl IS NOT NULL AND (created_at + ttl) < ?1` (now timestamp)
- 建议添加索引: `CREATE INDEX IF NOT EXISTS idx_memories_ttl ON memories(ttl, created_at)`
- 使用 `tokio::time::interval(Duration::from_secs(3600))` 或现有 Scheduler

**测试**:
- 插入短 TTL 记忆 → 等待 → `delete_expired()` → 验证删除
- 插入无 TTL 记忆 → `delete_expired()` → 验证存活
- 插入长 TTL 记忆 → `delete_expired()` → 验证存活

---

## Wave 2: 实时交互 + 平台补全 + 智能路由

### T6: Platform WS AgentRuntime 集成

**目标**: 替换 `octo-platform-server/src/ws.rs:159` 的 STUB，接入真实 AgentRuntime。

**现状分析**:
- WebSocket handler 基础结构完整（JWT 认证、session 校验、AgentPool 管理）
- Line 158-172 STUB: 仅 echo 消息 `"[Stub] Received: ..."`
- `AgentInstance.runtime: Option<Arc<AgentRuntime>>` 已存在但未使用
- `octo-server/src/ws.rs` 使用 `AgentExecutorHandle` 模式 (subscribe + send)
- **关键发现**: `handle_socket` 仅接收 `InstanceId`，未传入 `AgentInstance` 本身

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/octo-platform-server/src/ws.rs` | 重写 `handle_socket`，mirror octo-server 模式 |
| `crates/octo-platform-server/src/ws.rs` | 扩展 `ClientMessage`: 新增 `Cancel`, `ApprovalResponse` |
| `crates/octo-platform-server/src/ws.rs` | 扩展 `ServerMessage`: 新增 TextDelta, TextComplete, ThinkingDelta, ToolStart, ToolResult, ToolExecution, TokenBudgetUpdate, ContextDegraded, MemoryFlushed, ApprovalRequired, SecurityBlocked, Typing, Done |
| `crates/octo-platform-server/src/ws.rs` | 修改 `handle_socket` 签名: 接受 `AgentInstance` (非 `InstanceId`) |
| `crates/octo-platform-server/src/agent_pool.rs` | 新增 `AgentInstance::get_or_create_handle()` → 创建 `AgentExecutorHandle` |
| `crates/octo-engine/src/agent/runtime.rs` | 若缺少 `spawn_executor_for_session()` 方法则新增 |

**集成模式** (参考 octo-server/src/ws.rs):
1. `ClientMessage::Chat { content }` → `AgentMessage::UserMessage`
2. 通过 `AgentExecutorHandle.send()` 发送到 executor
3. 通过 `handle.subscribe()` 获取 `AgentEvent` broadcast receiver
4. Loop: `AgentEvent` → `ServerMessage` 映射 → WebSocket sender
5. `Cancel` → `AgentMessage::Cancel`
6. `ApprovalResponse` → `ApprovalGate.resolve()` (依赖 T7)
7. Disconnect → 释放 instance 回 pool

**测试**:
- Mock AgentRuntime → WebSocket 集成测试
- 多租户隔离: 不同 tenant 的 session 互不影响
- `ClientMessage` / `ServerMessage` serde 往返

**风险**:
- `AgentRuntime` 可能未暴露 clean `spawn_executor` API → 需添加辅助方法
- Session 重连需复用 executor handle → AgentPool 需缓存 per-session handles
- `create_instance` 传 `false` 给 event_bus → 需改为 `true`

---

### T7: ApprovalGate 交互式审批

**目标**: 接通已实现但未 wire 的审批流程，使工具执行可等待用户批准。

**现状分析** (关键发现):
- `ApprovalGate` **已完整实现** (`tools/approval.rs`) — 含 `register()`, `wait_for_approval()`, `respond()` 方法
- `AgentLoopConfig` 已有 `approval_gate: Option<ApprovalGate>` 字段 (loop_config.rs:147)
- `AppState` 已有 `approval_gate: Option<ApprovalGate>` 字段 (state.rs:55)
- Harness 已调用 `request_approval()` 并检查 gate (harness.rs:1230-1235)
- WS handler 已处理 `ClientMessage::ApprovalResponse` 并调用 `gate.respond()`
- **核心问题**: 两端均 default 为 `None`，从未创建共享的 `ApprovalGate` 实例！
- 当 `gate == None` 时，harness auto-rejects → 工具执行被阻止但无法被用户批准

**修改文件** (仅需 wiring，无需新建 struct):

| 文件 | 变更 |
|------|------|
| `crates/octo-server/src/main.rs` | 创建 `ApprovalGate::new()` 共享实例 |
| `crates/octo-server/src/state.rs` | 构造函数接受 `approval_gate` 参数（或 setter） |
| `crates/octo-engine/src/agent/executor.rs:198-219` | 新增 `approval_gate` 字段，传入 `AgentLoopConfig` |
| `crates/octo-engine/src/agent/runtime.rs` | `AgentRuntimeConfig` 或 spawn 方法注入 `approval_gate` |
| `crates/octo-server/src/main.rs` | 配置 `ApprovalManager` 使用 `SmartApprove` 策略（非 AlwaysApprove） |

**共享模式** (ApprovalGate 是 `Clone`，内部 `Arc<Mutex<HashMap>>`):
```
main.rs:
  let gate = ApprovalGate::new();
  AppState { approval_gate: Some(gate.clone()), ... }
  AgentRuntime/Executor { approval_gate: Some(gate.clone()), ... }

WS handler: state.approval_gate.respond(tool_id, approved)
Harness:    config.approval_gate.wait_for_approval(tool_id)
```

**同时需要 wire `ApprovalManager`**: 即使 gate 接通，若 `ApprovalManager` 缺失或使用 `AlwaysApprove`，harness 永远不会触发 approval 检查。需设置为 `SmartApprove` 策略。

**测试**:
- 集成测试: 构建 executor + gate → mock provider 返回 "bash" tool call → 验证 `ApprovalRequired` 事件
- E2E WS 测试: 连接 WS → 发送 chat → 收到 `approval_required` → 发送 `approval_response` → 工具继续执行
- 超时测试: 无响应 → 30s 后 auto-reject

**风险**:
- 超时默认 30s 可能对交互式使用太短 → 需配置化
- 需确保 gate 能 thread 穿过 `AgentRuntime → AgentExecutor → AgentLoopConfig` 完整链路

---

### T8: Dashboard WebSocket 实时事件推送 (D11)

**目标**: 前端实时展示 agent 事件流（工具调用、token 预算、上下文降级）。

**现状分析**:
- 后端 WS handler 已流式发送 `AgentEvent`
- `ServerMessage` 已支持 TextDelta, ToolStart, ToolResult, TokenBudgetUpdate, ContextDegraded 等
- 前端 `web/src/ws/` 有 WebSocket 连接管理器
- `web/src/pages/Debug.tsx` 有 TokenBudgetBar 组件
- **缺失**: 专门的实时事件面板

**依赖**: T3 (Observability) 必须先完成，确保事件被 publish

**现状补充** (来自代码分析):
- `ws/events.ts` 的 `handleWsEvent()` 当前仅处理: `session_created`, `text_delta/complete`, `thinking_delta/complete`, `tool_start/result`, `tool_execution`, `token_budget_update`, `error`, `done`
- **未处理**: `context_degraded`, `memory_flushed`, `approval_required`, `security_blocked`, `typing`
- `ws/types.ts` 的 `ServerMessage` 类型缺少上述变体

**修改文件**:

| 文件 | 变更 |
|------|------|
| `web/src/ws/types.ts` | 新增 5 个 `ServerMessage` 变体 |
| `web/src/ws/events.ts` | `handleWsEvent` 新增 5 个事件处理分支 |
| `web/src/atoms/debug.ts` | 新增 `liveEventsAtom`, `contextStatusAtom` atoms |
| `web/src/pages/Debug.tsx` | 新增 EventStream + ContextStatus 面板 |
| `web/src/components/debug/EventStream.tsx` | **新建**: 实时事件流组件 |
| `web/src/components/debug/ContextStatus.tsx` | **新建**: 上下文降级指示器 |

**新增 ServerMessage 类型** (`ws/types.ts`):
```typescript
| { type: "context_degraded"; session_id: string; level: string; usage_pct: number }
| { type: "memory_flushed"; session_id: string; facts_count: number }
| { type: "approval_required"; session_id: string; tool_name: string; tool_id: string; risk_level: string }
| { type: "security_blocked"; session_id: string; reason: string }
| { type: "typing"; session_id: string; state: boolean }
```

**新增 Atoms** (`atoms/debug.ts`):
```typescript
interface LiveEvent { id: string; timestamp: number; type: string; summary: string; data?: unknown; }
export const liveEventsAtom = atom<LiveEvent[]>([]);  // FIFO cap at 500
export const contextStatusAtom = atom<{ level: string; usage_pct: number } | null>(null);
```

**前端组件设计**:
```
Debug Page
├── TokenBudgetBar (existing)
├── ContextStatus (new) — 降级等级 L0-L3 + 使用率条
├── EventStream (new)
│   ├── Filter: [All] [ToolCalls] [Context] [Budget] [Security]
│   ├── Event Card: timestamp + type icon + summary (color-coded)
│   ├── Auto-scroll with pause button
│   └── Clear button
└── EventBus Viewer (existing, enhance)
```

**颜色编码**: green=tool_start, blue=text, yellow=context_degraded, red=error/security_blocked, orange=approval_required

**测试**:
- 前端: 模拟 WS 消息 → 验证 EventStream 渲染
- 验证 `handleWsEvent` 正确 dispatch 所有新事件类型
- 集成: 启动 agent → 观察事件流出现在 Dashboard

**风险**:
- `liveEventsAtom` 无限增长 → FIFO 限制 500 条
- 快速事件导致 render thrashing → `requestAnimationFrame` 或 debounce

---

### T9: Agent 协作 Dashboard 面板 (D13)

**目标**: 可视化多 Agent 协作状态（agent 列表、提案投票、共享上下文）。

**现状分析**:
- 后端: `agent/collaboration/` 8 个模块完整（manager, protocol, context, channel, handle, injection）
- 有 `Proposal`, `Vote`, `ProposalStatus` 类型
- `CollaborationProtocol` 支持 `propose_action()`, `vote()`, `accept/reject_proposal()`
- **缺失**: REST API 端点 + 前端页面

**依赖**: T8 (Dashboard 实时) 最好先完成，共享 EventTimeline 组件

**后端 API 层面** (来自代码分析):
- `CollaborationManager` 暴露: `status()` → `CollaborationStatus { id, agent_count, active_agent, pending_proposals, event_count, state_keys }`
- `CollaborationContext` 暴露: `events()` → `Vec<CollaborationEvent>`, `proposals()` → `Vec<Proposal>`, `state_keys()`
- `CollaborationEvent` 类型: AgentJoined, AgentLeft, MessageSent, TaskDelegated, StateUpdated
- `AppState` 需新增 `collaboration_manager: Option<Arc<tokio::sync::Mutex<CollaborationManager>>>`

**修改文件 (后端)**:

| 文件 | 变更 |
|------|------|
| `crates/octo-server/src/api/collaboration.rs` | **新建**: 5 个 REST handler |
| `crates/octo-server/src/api/mod.rs` | 添加 `pub mod collaboration;` |
| `crates/octo-server/src/router.rs` | 注册 collaboration 路由 |
| `crates/octo-server/src/state.rs` | 新增 `collaboration_manager` 字段 |

**修改文件 (前端)**:

| 文件 | 变更 |
|------|------|
| `web/src/atoms/ui.ts` | `TabId` 类型新增 `"collaboration"` |
| `web/src/App.tsx` | 导入 + Tab 路由注册 |
| `web/src/pages/Collaboration.tsx` | **新建**: 协作主页面 |
| `web/src/components/collaboration/AgentList.tsx` | **新建**: Agent 卡片列表 |
| `web/src/components/collaboration/EventLog.tsx` | **新建**: 协作事件时间线 |
| `web/src/components/collaboration/ProposalList.tsx` | **新建**: 提案 + 投票 UI |
| `web/src/components/collaboration/SharedState.tsx` | **新建**: 共享状态 JSON viewer |
| `web/src/atoms/collaboration.ts` | **新建**: Jotai atoms |

**REST API 设计**:
```
GET  /api/collaboration/status        → CollaborationStatus
GET  /api/collaboration/agents        → Vec<AgentInfo>
GET  /api/collaboration/events        → Vec<CollaborationEvent>
GET  /api/collaboration/proposals     → Vec<Proposal>
POST /api/collaboration/proposals     → 创建提案
POST /api/collaboration/proposals/:id/vote → 投票
```

**前端组件设计**:
```
Collaboration Page
├── StatusBar (collaboration ID, agent count, active agent)
├── AgentList Panel
│   ├── Agent Card: id + status + role + capabilities
│   └── Agent count badge
├── EventLog Panel
│   └── 按时间排序的 CollaborationEvent 列表
├── Proposals Panel
│   ├── Proposal Card: content + from_agent + status + vote count
│   └── Approve/Reject 按钮
└── Shared State Panel
    └── Key-value 表格 (collapsible JSON tree)
```

**测试**:
- 后端: 各 handler 单元测试 (mock CollaborationManager)
- 前端: 组件测试 (mock collaboration data)
- 集成: 启动两个 agent 协作 → GET /api/collaboration/status → 验证 JSON 结构

**风险**:
- 单 agent 模式下无协作 → 端点返回空/404，前端需 graceful handling
- `CollaborationManager` 内部用 `HashMap` (非 `DashMap`) → `Mutex` 包装正确但可能竞争
- `std::sync::RwLock` 在 `CollaborationContext` 中 → 包在 `tokio::sync::Mutex` 中需确保 `Send`

---

### T10: SmartRouting — 查询复杂度分类 + 模型路由

**目标**: 根据输入复杂度自动选择最优 LLM 模型（Simple→Haiku, Medium→Sonnet, Complex→Opus）。

**现状分析**:
- `Provider` trait + `ProviderChain` (failover) + `ProviderPipeline` (decorator) 已完善
- `CompletionRequest` 包含 messages, tools, system, model, max_tokens
- harness.rs:368 设置 `request.model` 来自 `AgentLoopConfig.model`
- **缺失**: 复杂度分类器、路由策略、SmartRouterProvider

**新建文件**:

| 文件 | 内容 |
|------|------|
| `crates/octo-engine/src/providers/smart_router.rs` | QueryAnalyzer + QueryComplexity + SmartRouterProvider |

**修改文件**:

| 文件 | 变更 |
|------|------|
| `crates/octo-engine/src/providers/mod.rs` | 添加 `pub mod smart_router;` |
| `crates/octo-engine/src/providers/pipeline.rs` | 添加 `with_smart_routing()` 到 PipelineBuilder |
| `crates/octo-server/src/config.rs` | 添加 `smart_routing: Option<SmartRoutingConfig>` |
| `crates/octo-engine/src/agent/runtime.rs` | 可选 wrap provider with SmartRouter |

**QueryAnalyzer 分类规则** (纯 CPU 启发式，< 1us):

| 信号 | 来源 | 权重 |
|------|------|------|
| 输入文本长度 | `messages[].content` 总长 | <500 chars=0, 500-3000=1, >3000=2 |
| 对话轮次 | `messages.len()` | 1-2=0, 3-8=1, >8=2 |
| 工具数量 | `tools.len()` | 0=0, 1-5=1, >5=2 |
| 系统提示词长度 | `system.len()` | <2000=0, >2000=1, >5000=2 |
| 关键词信号 | 最后一条 user message | "architect/design/refactor"=+2, "hello/thanks"=-1 |
| max_tokens | `max_tokens` | >8192=+1 |

**评分映射**: 总分 <=1=Simple, 2-4=Medium, >=5=Complex

**SmartRouterProvider (V1)** — 单 Provider + model override:
```rust
struct SmartRouterProvider {
    inner: Box<dyn Provider>,       // 底层 provider (如 Anthropic)
    analyzer: QueryAnalyzer,
    tier_models: HashMap<QueryComplexity, String>,  // Simple→haiku, Medium→sonnet, Complex→opus
}
```

**Pipeline 集成顺序**: `Raw Provider → SmartRouter → Retry → CircuitBreaker → CostGuard`

**配置**:
```yaml
smart_routing:
  enabled: true
  default_tier: "medium"
  tiers:
    simple:
      model: "claude-3-5-haiku-20241022"
    medium:
      model: "claude-sonnet-4-20250514"
    complex:
      model: "claude-opus-4-20250514"
  thresholds:
    text_length_medium: 500
    text_length_complex: 3000
    tool_count_complex: 5
```

**测试** (10+ 测试用例):
- `test_simple_query_classification`: 短文本无工具 → Simple
- `test_complex_query_classification`: 长文本多工具 + "architect" → Complex
- `test_model_override_applied`: 验证 inner provider 收到正确的 model 名称
- `test_fallback_on_missing_tier`: 未配置的 tier → 使用默认 model
- `test_pipeline_integration`: SmartRouter + Retry + CostGuard 链式调用
- `test_config_deserialization`: YAML 配置反序列化

**风险与缓解**:
| 风险 | 缓解 |
|------|------|
| 误分类（复杂查询发到 Haiku） | 保守默认偏向 Medium；支持 per-agent `min_tier` override |
| 模型名称不匹配 | 配置加载时校验模型名 |
| 跨 Provider 路由 | V1 仅支持同 Provider 路由（model override），V2 再做跨 Provider |

---

## Deferred（暂缓项）

> 本计划已知但暂未实现的功能点。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D1 | Byzantine 共识 (PBFT lite) | T9 协作面板 + 投票 UI | ⏳ |
| D2 | Extension + Hook 系统合并 | 重构评估 | ⏳ |
| D3 | ContentBlock 扩展 (Image/Audio) | 多模态 Provider 支持 | ⏳ |
| D4 | Let's Encrypt ACME 自动证书 | 公网域名 + 平台生产部署 | ⏳ |
| D5 | Tauri 自动更新 (tauri-plugin-updater) | 发布流程 + artifact 托管 | ⏳ |
| D6 | 离线模式 SQLite 同步 | CRDT/冲突解决设计 | ⏳ |
| D7 | SmartRouting V2 跨 Provider 路由 | T10 V1 完成 + 多 Provider 场景 | ⏳ |

---

## 验收标准

### Wave 1 完成标准
- [ ] `cargo check --workspace` 无错误
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过（>= 1343 + 新增测试）
- [ ] Canary token: mock output 含 canary → SecurityBlocked
- [ ] Symlink: 文件工具拒绝 symlink 并返回错误
- [ ] Observability: TelemetryBus 发布 ContextDegraded/TokenBudgetUpdated/LoopGuardTriggered
- [ ] EventStore: `/api/events` 返回持久化事件
- [ ] TTL: 过期记忆被定时清理

### Wave 2 完成标准
- [ ] Platform WS: 真实 AgentRuntime 响应（非 echo）
- [ ] ApprovalGate: 工具执行阻塞等待用户批准，超时 30s
- [ ] Dashboard 实时: EventTimeline 显示实时事件流
- [ ] 协作面板: `/api/sessions/{id}/collaboration/status` 返回正确数据
- [ ] SmartRouting: 短查询路由到 Haiku，复杂查询路由到 Opus

---

## 提交策略

```
Wave 1:
  commit 1: "feat(security): T1+T2 — Canary token integration + symlink defense"
  commit 2: "feat(observability): T3+T4 — Event publishing + EventStore REST API"
  commit 3: "feat(memory): T5 — TTL cleanup scheduled task"
  checkpoint: "checkpoint: Wave 1 COMPLETE — 5/10 tasks, N tests"

Wave 2:
  commit 4: "feat(platform): T6+T7 — Platform WS integration + ApprovalGate"
  commit 5: "feat(dashboard): T8+T9 — Realtime events + Collaboration panel"
  commit 6: "feat(providers): T10 — SmartRouting query complexity classifier"
  checkpoint: "checkpoint: Wave 2 COMPLETE — 10/10 tasks, N tests"
```
