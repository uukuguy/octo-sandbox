# Agent Harness 实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 `AgentLoop::run()` 从 949 行 monolithic 方法重构为 `run_agent_loop()` 纯函数式设计，返回 `BoxStream<AgentEvent>`，并集成 pre-harness-refactor 阶段创建的 18+ 基础模块。

**Architecture:** 按 P0/P1/P2/P3 四阶段渐进式实施。P0 完成核心接口切换（最高风险）；P1 集成已有模块；P2 适配消费者；P3 端到端验证。

**Tech Stack:** Rust 1.75+, Tokio 1.42, futures-util (BoxStream)

**前提条件:** pre-harness-refactor 42/42 + 5 Deferred 完成，857 tests passing

**设计来源:** `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` §3.2

---

## 阶段概览

| 阶段 | 名称 | 任务数 | 核心目标 | 风险等级 |
|------|------|--------|---------|---------|
| **P0** | 核心接口切换 | 8 | AgentEvent 统一、run_agent_loop() 纯函数、Stream 返回 | 高 |
| **P1** | 模块集成 | 8 | 集成 Continuation/ObservationMasker/Interceptor/DeferredAction/TurnGate | 中 |
| **P2** | 消费者适配 | 6 | AgentExecutor/WS handler/Scheduler/Runtime 适配新接口 | 中 |
| **P3** | 端到端验证与清理 | 6 | 全量测试、文档更新、废弃代码清理 | 低 |
| **合计** | | **28** | | |

---

## P0: 核心接口切换

> 最高风险阶段：改变 AgentLoop 的核心签名和事件定义。每个任务后必须 `cargo check --workspace`。

### P0-1: 统一 AgentEvent 到 events.rs

**来源:** 设计文档 §3.2 AgentEvent Stream

**目标:** 将 `loop_.rs` 中的 `AgentEvent` enum 迁移到 `events.rs`，合并已有的 `AgentLoopResult` 和 `NormalizedStopReason`。

**Files:**
- Modify: `crates/octo-engine/src/agent/events.rs` — 承接完整 AgentEvent 定义
- Modify: `crates/octo-engine/src/agent/loop_.rs` — 删除 AgentEvent 定义，改为 `use super::events::AgentEvent`
- Modify: `crates/octo-engine/src/agent/mod.rs` — 更新 re-export
- Test: `crates/octo-engine/tests/agent_events.rs`

**Step 1:** 将 `loop_.rs` 中 AgentEvent 的完整定义（包括 ContextDegraded, MemoryFlushed, ApprovalRequired, SecurityBlocked, IterationStart, IterationEnd, Completed 等）复制到 `events.rs`。

**Step 2:** 在 `loop_.rs` 中删除 AgentEvent 定义，改为 `use super::events::AgentEvent;`

**Step 3:** 确保 `mod.rs` 的 `pub use` 从 `events` 导出 AgentEvent（而非 `loop_`）

**Step 4:** 写测试验证所有 variant 可构建

**Verify:** `cargo check --workspace && cargo test -p octo-engine -- --test-threads=1`

---

### P0-2: 扩展 AgentLoopConfig 为完整依赖注入容器

**来源:** 设计文档 §3.2 AgentLoopConfig

**目标:** 将 `AgentLoop` struct 的所有字段迁移到 `AgentLoopConfig`，使其成为 `run_agent_loop()` 的唯一入参。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_config.rs`
- Test: `crates/octo-engine/tests/agent_loop_config.rs` (已有，需扩展)

**Step 1:** 扩展 `AgentLoopConfig`，增加以下字段：

```rust
pub struct AgentLoopConfig {
    // === 控制参数（已有） ===
    pub max_iterations: u32,
    pub max_concurrent_tools: usize,
    pub tool_timeout_secs: u64,
    pub force_text_at_last: bool,
    pub max_tokens_continuation: u32,

    // === 依赖注入（新增） ===
    pub provider: Arc<dyn Provider>,
    pub tools: Arc<ToolRegistry>,
    pub memory: Arc<dyn WorkingMemory>,
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    pub model: String,
    pub max_tokens: u32,
    pub budget: ContextBudgetManager,
    pub pruner: ContextPruner,
    pub loop_guard: LoopGuard,

    // === 可选组件 ===
    pub recorder: Option<Arc<ToolExecutionRecorder>>,
    pub event_bus: Option<Arc<EventBus>>,
    pub hook_registry: Option<Arc<HookRegistry>>,
    pub defence: Option<Arc<AiDefence>>,
    pub manifest: Option<AgentManifest>,

    // === 会话上下文（由调用方传入） ===
    pub session_id: SessionId,
    pub user_id: UserId,
    pub sandbox_id: SandboxId,
    pub tool_ctx: ToolContext,
    pub cancel_token: CancellationToken,

    // === Agent 行为配置 ===
    pub agent_config: AgentConfig,
}
```

**Step 2:** 为 `AgentLoopConfig` 实现 builder pattern（扩展已有 builder）

**Step 3:** 验证编译通过

**Verify:** `cargo check --workspace`

**注意:** 此步骤暂不修改 `AgentLoop`，只是扩展配置结构体。

---

### P0-3: 实现 run_agent_loop() 骨架

**来源:** 设计文档 §3.2 纯函数式入口

**目标:** 创建 `run_agent_loop(config, messages) -> BoxStream<AgentEvent>` 纯函数骨架。

**Files:**
- Create: `crates/octo-engine/src/agent/harness.rs` — 新的纯函数式 harness
- Modify: `crates/octo-engine/src/agent/mod.rs` — 添加 `pub mod harness`
- Test: `crates/octo-engine/tests/harness_basic.rs`

**Step 1:** 创建 `harness.rs`，实现基本骨架：

```rust
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::events::AgentEvent;
use super::loop_config::AgentLoopConfig;

/// Pure-function agent loop entry point.
/// All dependencies injected via config; returns a stream of events.
pub fn run_agent_loop(
    config: AgentLoopConfig,
    messages: Vec<ChatMessage>,
) -> BoxStream<'static, AgentEvent> {
    let (tx, rx) = mpsc::channel(256);

    tokio::spawn(async move {
        run_agent_loop_inner(config, messages, tx).await;
    });

    tokio_stream::wrappers::ReceiverStream::new(rx).boxed()
}

async fn run_agent_loop_inner(
    config: AgentLoopConfig,
    mut messages: Vec<ChatMessage>,
    tx: mpsc::Sender<AgentEvent>,
) {
    // Skeleton: emit Done immediately
    let _ = tx.send(AgentEvent::Done).await;
}
```

**Step 2:** 写测试验证 Stream 可被消费

```rust
#[tokio::test]
async fn test_harness_returns_done() {
    let config = /* minimal config */;
    let messages = vec![];
    let mut stream = run_agent_loop(config, messages);
    let event = stream.next().await.unwrap();
    assert!(matches!(event, AgentEvent::Done));
}
```

**Verify:** `cargo test -p octo-engine --test harness_basic -- --test-threads=1`

---

### P0-4: 迁移 Zone A/B 构建逻辑到 harness

**来源:** loop_.rs lines 231-280

**目标:** 将 system prompt 构建和 Zone B 注入逻辑提取到 `harness.rs` 的 `run_agent_loop_inner()`。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`
- Modify: `crates/octo-engine/src/agent/loop_steps.rs` — 复用已有 `inject_zone_b()`

**Step 1:** 在 `run_agent_loop_inner` 开头实现 Zone A 构建（system prompt）

**Step 2:** 调用 `loop_steps::inject_zone_b()` 实现 Zone B 注入

**Step 3:** 发射 `AgentEvent::IterationStart { round: 0 }`

**Verify:** `cargo check --workspace`

---

### P0-5: 迁移 Provider 调用 + Stream 处理逻辑到 harness

**来源:** loop_.rs lines 426-626

**目标:** 将 CompletionRequest 构建、retry 逻辑、stream 消费逻辑迁移到 harness。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 实现 `call_provider_with_retry()` step function

```rust
async fn call_provider_with_retry(
    provider: &dyn Provider,
    request: CompletionRequest,
    retry_policy: &RetryPolicy,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<BoxStream<'static, Result<StreamEvent>>> { ... }
```

**Step 2:** 实现 `consume_stream()` step function — 处理所有 StreamEvent variant，累积 text/thinking/tool_uses

```rust
async fn consume_stream(
    stream: &mut BoxStream<'_, Result<StreamEvent>>,
    tx: &mpsc::Sender<AgentEvent>,
    config: &AgentConfig,
) -> Result<StreamResult> { ... }

struct StreamResult {
    full_text: String,
    full_thinking: String,
    tool_uses: Vec<PendingToolUse>,
    stop_reason: StopReason,
    usage: Usage,
}
```

**Step 3:** 在 `run_agent_loop_inner` 的 for loop 中调用这两个函数

**Verify:** `cargo check --workspace`

---

### P0-6: 迁移 Tool 执行逻辑到 harness

**来源:** loop_.rs lines 628-870

**目标:** 将 tool 执行（包括 LoopGuard 检查、parallel/sequential 执行、PostToolUse hook）迁移到 harness。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 实现 `execute_tools_step()` step function

```rust
async fn execute_tools_step(
    tool_uses: &[PendingToolUse],
    tools: &ToolRegistry,
    loop_guard: &mut LoopGuard,
    config: &AgentLoopConfig,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<Vec<ContentBlock>> { ... }
```

**Step 2:** 复用 `loop_steps::check_loop_guard_verdict()` 和 `loop_steps::should_execute_parallel()`

**Step 3:** 集成 parallel::execute_parallel 和 sequential 分支

**Verify:** `cargo check --workspace`

---

### P0-7: 迁移 Context 管理 + AIDefence 逻辑到 harness

**来源:** loop_.rs lines 359-424

**目标:** 将 context budget/pruning/AIDefence 逻辑迁移到 harness。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 实现 `manage_context_step()`

```rust
async fn manage_context_step(
    messages: &mut Vec<ChatMessage>,
    system_prompt: &str,
    tool_specs: &[ToolSpec],
    budget: &ContextBudgetManager,
    pruner: &ContextPruner,
    provider: &dyn Provider,
    memory: &dyn WorkingMemory,
    memory_store: Option<&dyn MemoryStore>,
    model: &str,
    user_id: &str,
    hook_registry: Option<&HookRegistry>,
    session_id: &str,
    round: u32,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<()> { ... }
```

**Step 2:** 实现 `check_ai_defence_input()` 和 `check_ai_defence_output()`

**Verify:** `cargo check --workspace`

---

### P0-8: 完成 harness 主循环 + Hook 生命周期

**来源:** loop_.rs lines 291-350, 600-610, 870-901

**目标:** 在 `run_agent_loop_inner` 中完成完整的 for loop + hook 生命周期。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`
- Test: `crates/octo-engine/tests/harness_loop.rs`

**Step 1:** 在 for loop 中按序组合所有 step functions：
1. SessionStart hook (round 0)
2. PreTask hook (round 0)
3. IterationStart event
4. manage_context_step()
5. AIDefence input check (round 0)
6. call_provider_with_retry()
7. consume_stream()
8. 判断 stop_reason → 如果是 EndTurn/MaxIterations，发送 TextComplete + Done，return
9. execute_tools_step()
10. LoopTurnEnd hook
11. next round

**Step 2:** max_rounds 超限处理

**Step 3:** 发射 `AgentEvent::Completed(AgentLoopResult { ... })` 替代简单 Done

**Step 4:** 写集成测试（使用 mock provider）

**Verify:** `cargo test -p octo-engine --test harness_loop -- --test-threads=1`

---

## P1: 模块集成

> 将 pre-harness 创建的独立模块逐个集成到 harness 中。

### P1-1: 集成 ContinuationTracker（max-tokens 续写）

**来源:** `agent/continuation.rs`

**目标:** 在 harness 的 `consume_stream()` 返回 MaxTokens 时，自动注入 continuation prompt 并重新调用 provider。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`
- Test: `crates/octo-engine/tests/harness_continuation.rs`

**Step 1:** 在 run_agent_loop_inner 创建 `ContinuationTracker`

**Step 2:** 在 stream result 判断分支中，当 `stop_reason == MaxTokens` 时调用 `tracker.should_continue()`，若 true 则注入 continuation prompt 并 continue loop

**Verify:** `cargo test -p octo-engine --test harness_continuation -- --test-threads=1`

---

### P1-2: 集成 ObservationMasker

**来源:** `context/observation_masker.rs`

**目标:** 在 context 管理步骤中，对传给 LLM 的 messages 应用 observation masking。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`
- Test: `crates/octo-engine/tests/harness_masking.rs`

**Step 1:** 在 `manage_context_step()` 中创建 `ObservationMasker::with_defaults()`

**Step 2:** 在构建 CompletionRequest 时，使用 `masker.mask(&messages)` 而非原始 messages

**Step 3:** 注意：masking 只影响发给 LLM 的消息副本，原始 messages 保持完整

**Verify:** `cargo test -p octo-engine --test harness_masking -- --test-threads=1`

---

### P1-3: 集成 ToolCallInterceptor

**来源:** `tools/interceptor.rs`

**目标:** 在 tool 执行前，通过 interceptor 检查每个 tool call 是否被 skill 约束阻止。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 在 `AgentLoopConfig` 中增加 `interceptor: Option<ToolCallInterceptor>` 字段

**Step 2:** 在 `execute_tools_step()` 中，对每个 tool_use 调用 `interceptor.check_permission()`

**Step 3:** 被拦截的 tool 返回 ToolResult::error("Tool blocked by skill constraint")

**Verify:** `cargo check --workspace`

---

### P1-4: 集成 DeferredActionDetector

**来源:** `agent/deferred_action.rs`

**目标:** 在 LLM 返回纯文本响应时，检测是否包含 deferred action 模式，发出警告。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 在 run_agent_loop_inner 创建 `DeferredActionDetector::new()`

**Step 2:** 在 TextComplete 之前调用 `detector.detect(&full_text)`

**Step 3:** 如果检测到 deferred actions，发射 `AgentEvent::Error { message: "Deferred action detected: ..." }`（作为 warning，不中断）

**Verify:** `cargo check --workspace`

---

### P1-5: 集成 NormalizedStopReason

**来源:** `agent/events.rs`

**目标:** 在 harness 中使用 NormalizedStopReason 替代原始 StopReason 进行流程控制。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 在 `consume_stream()` 返回的 `StreamResult` 中使用 `NormalizedStopReason`

**Step 2:** 主循环中基于 `NormalizedStopReason::is_terminal()` 决定是否继续

**Step 3:** 在 `AgentEvent::Completed` 中携带 `NormalizedStopReason`

**Verify:** `cargo check --workspace`

---

### P1-6: 集成 TurnGate 到 AgentExecutor

**来源:** `agent/turn_gate.rs`

**目标:** 在 AgentExecutor 中使用 TurnGate 防止并发 turn。

**Files:**
- Modify: `crates/octo-engine/src/agent/executor.rs`
- Test: `crates/octo-engine/tests/turn_gate.rs` (已有)

**Step 1:** 在 `AgentExecutor` 中添加 `turn_gate: TurnGate` 字段

**Step 2:** 在 `run()` 的 UserMessage 处理分支中，`let _guard = self.turn_gate.acquire().await;`

**Verify:** `cargo check --workspace`

---

### P1-7: 集成 IterationStart/End 事件

**来源:** 设计文档 §3.2 AgentEvent

**目标:** 在 harness 的每轮迭代开始/结束时发射 IterationStart/IterationEnd 事件。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 在 for loop 开头发射 `AgentEvent::IterationStart { round }`

**Step 2:** 在 for loop 结尾发射 `AgentEvent::IterationEnd { round }`

**Verify:** `cargo check --workspace`

---

### P1-8: 集成 ContextDegraded/MemoryFlushed 事件

**来源:** 设计文档 §3.2 AgentEvent

**目标:** 在 context 管理步骤中发射详细的降级和刷写事件。

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1:** 在 `manage_context_step()` 触发降级时发射 `AgentEvent::ContextDegraded { level, usage_pct }`

**Step 2:** 在 MemoryFlusher 刷写后发射 `AgentEvent::MemoryFlushed { facts_count }`

**Verify:** `cargo check --workspace`

---

## P2: 消费者适配

> 使所有 AgentLoop 消费者切换到新的 harness 接口。

### P2-1: AgentExecutor 适配 run_agent_loop()

**来源:** executor.rs lines 161-213

**目标:** AgentExecutor 从构建 `AgentLoop` + `run()` 切换到构建 `AgentLoopConfig` + `run_agent_loop()`。

**Files:**
- Modify: `crates/octo-engine/src/agent/executor.rs`
- Test: `crates/octo-engine/tests/executor.rs` (已有)

**Step 1:** 在 UserMessage 处理分支中：
1. 构建 `AgentLoopConfig` 而非 `AgentLoop`
2. 调用 `harness::run_agent_loop(config, messages.clone())`
3. 消费 Stream，转发到 broadcast_tx
4. 从 Stream 中收集最终 messages（或通过回调返回）

**Step 2:** 处理 messages 更新问题 — Stream 模式下 messages 不再通过 `&mut Vec` 返回。方案：
- 在 AgentEvent 中添加 `MessagesUpdated(Vec<ChatMessage>)` variant
- 或在 `Completed` variant 中携带 updated messages

**Verify:** `cargo test -p octo-engine -- --test-threads=1`

---

### P2-2: WS Handler 适配新 AgentEvent variants

**来源:** `crates/octo-server/src/ws.rs`

**目标:** WS handler 处理所有新增 AgentEvent variants。

**Files:**
- Modify: `crates/octo-server/src/ws.rs`

**Step 1:** 在 match 分支中添加：
- `AgentEvent::ContextDegraded` → `ServerMessage::ContextDegraded`
- `AgentEvent::MemoryFlushed` → `ServerMessage::MemoryFlushed`
- `AgentEvent::IterationStart` / `IterationEnd` → 可选转发或忽略
- `AgentEvent::ApprovalRequired` → `ServerMessage::ApprovalRequired`
- `AgentEvent::SecurityBlocked` → `ServerMessage::SecurityBlocked`

**Step 2:** 确保 catch-all `_` 分支处理未知 variants

**Verify:** `cargo check --workspace`

---

### P2-3: Scheduler 适配

**来源:** `crates/octo-engine/src/scheduler/mod.rs`

**目标:** 检查 scheduler 是否使用 AgentLoop，如需要则适配。

**Files:**
- Modify: `crates/octo-engine/src/scheduler/mod.rs` (如需要)

**Step 1:** 检查 scheduler 中 AgentLoop 的使用方式

**Step 2:** 如果直接使用 AgentLoop，切换到 run_agent_loop()

**Verify:** `cargo check --workspace`

---

### P2-4: AgentRuntime 适配

**来源:** `crates/octo-engine/src/agent/runtime.rs`

**目标:** 检查 AgentRuntime 中 AgentLoop 的引用，确保兼容。

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs` (如需要)

**Verify:** `cargo check --workspace`

---

### P2-5: 保留向后兼容 AgentLoop wrapper

**目标:** 保留 `AgentLoop` struct 作为 thin wrapper，内部委托给 `run_agent_loop()`。确保现有使用 `AgentLoop::run()` 的代码不会立即 break。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1:** 将 `AgentLoop::run()` 改为：
1. 从 self 字段构建 `AgentLoopConfig`
2. 调用 `run_agent_loop(config, messages.clone())`
3. 消费 Stream，转发到 broadcast::Sender<AgentEvent>
4. 从 Stream 中收集 messages 更新写回 `&mut Vec<ChatMessage>`

**Step 2:** 标记 `AgentLoop::run()` 为 `#[deprecated(note = "Use run_agent_loop() directly")]`

**Verify:** `cargo test --workspace -- --test-threads=1`

---

### P2-6: lib.rs re-export 更新

**目标:** 在 `octo-engine/src/lib.rs` 中导出 `run_agent_loop` 和 `AgentLoopConfig`。

**Files:**
- Modify: `crates/octo-engine/src/lib.rs`

**Verify:** `cargo check --workspace`

---

## P3: 端到端验证与清理

### P3-1: 全量测试回归验证

**目标:** 运行完整 857 测试 + 新增 harness 测试，确保零失败。

**Command:** `cargo test --workspace -- --test-threads=1`

**Acceptance:** 全部通过，零 failure。

---

### P3-2: AgentEvent 序列化测试

**目标:** 确保所有 AgentEvent variants 可 `serde_json::to_string()` 序列化（WS 需要）。

**Files:**
- Test: `crates/octo-engine/tests/agent_events_serde.rs`

---

### P3-3: Harness 集成测试 — Mock Provider 完整流程

**目标:** 使用 mock provider 模拟完整流程：用户消息 → LLM 响应 → tool call → tool result → LLM 最终响应。

**Files:**
- Test: `crates/octo-engine/tests/harness_integration.rs`

---

### P3-4: 清理 loop_.rs 废弃代码

**目标:** 移除 `loop_.rs` 中不再需要的私有函数和 struct（PendingToolUse, maybe_trim_tool_result 等），保留 AgentLoop wrapper。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Verify:** `cargo check --workspace`

---

### P3-5: 更新设计文档

**目标:** 更新 `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md`，标注已实现项。

**Files:**
- Modify: `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md`

---

### P3-6: Clippy + fmt 清理

**目标:** 确保新代码通过 clippy 和 fmt 检查。

**Command:** `cargo clippy --workspace -- -D warnings && cargo fmt --all -- --check`

---

## Deferred Items

> 条件未满足或复杂度过高，不在本阶段实施。

| ID | 名称 | 前提条件 | 备注 |
|----|------|---------|------|
| D1 | messages 所有权迁移（Arc<Mutex<Vec>>） | P2-1 完成后评估 | 当前用 clone + MessagesUpdated 事件 |
| D2 | ApprovalManager 交互式审批 | 需要前端 WS 双向通信 | oneshot::Sender 在 Stream 中需要特殊处理 |
| D3 | SmartRouting（简单查询→廉价模型） | Provider 基础设施就绪 | 需要 query complexity 分类器 |
| D4 | run_agent_loop 支持 SubAgent 递归调用 | P0 完成后 | SubAgent 可复用 run_agent_loop |
| D5 | 删除旧 AgentLoop struct | 所有消费者迁移完成 | 暂保留向后兼容 |
| D6 | Event recording/replay | 设计中 | 基于 AgentEvent Stream 天然支持 |

---

## 执行策略

### 推荐执行顺序

```
P0-1 → P0-2 → P0-3 → P0-4 → P0-5 → P0-6 → P0-7 → P0-8 (严格顺序)
↓
P1-1 ─┬─ P1-2 ─┬─ P1-3 ─┬─ P1-4 (可并行)
P1-5 ─┘  P1-7 ─┘  P1-8 ─┘
P1-6 (独立)
↓
P2-5 → P2-1 → P2-2 (严格顺序：先保留兼容层，再切换 executor，再适配 WS)
P2-3, P2-4 (独立检查)
P2-6
↓
P3-1 → P3-2, P3-3 (并行) → P3-4 → P3-5, P3-6 (并行)
```

### 风险控制

1. **P0 每步后必须 `cargo check --workspace`** — 编译失败立即修复
2. **P0-8 完成后必须全量测试** — 确保 857 测试不回退
3. **P2-5（向后兼容层）必须在 P2-1 之前** — 避免消费者 break
4. **每个 P 阶段完成后提交** — `git commit` 保存检查点

### 工作量估计

| 阶段 | 预估 LOC | 核心难度 |
|------|---------|---------|
| P0 | ~800 | 高 — 核心接口变更 |
| P1 | ~400 | 中 — 模块集成 |
| P2 | ~300 | 中 — 适配消费者 |
| P3 | ~200 | 低 — 测试和清理 |
| **合计** | **~1700** | |
