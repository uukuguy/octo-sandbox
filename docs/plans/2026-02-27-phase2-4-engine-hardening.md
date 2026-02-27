# Phase 2.4 Engine Hardening 实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**：将 OpenFang 验证的核心 Agent 安全机制移植到 octo-workbench，同时修复 Batch 3 遗留问题，完成 v1.0 全面加固

**架构**：本阶段聚焦引擎内部，不改变 API 接口；共 5 个后端模块（Loop Guard / LLM 错误分类 / Context Overflow 4+1 / EventBus / Tool 执行安全）+ Batch 3 遗留 5 个 bugfix。前端仅 EventBus 新增 Debug 信号展示小改动。

**技术栈**：Rust async/tokio、SQLite (rusqlite)、OpenFang 参考源码（loop_guard.rs ~100 LOC / retry.rs ~770 LOC / bus.rs ~149 LOC / overflow.rs ~120 LOC）、rmcp 0.16

---

## 架构调整背景

本计划是根据 `docs/design/ARCHITECTURE_DESIGN.md` v1.1 和 `docs/design/CONTEXT_ENGINEERING_DESIGN.md` v1.1 的更新制定的：

| 设计变更 | 来源文档 | 本计划对应任务 |
|---------|---------|-------------|
| §3.2.1 Loop Guard / Circuit Breaker | ARCH §3.2.1 | Task 1 |
| §3.7.1 Context Overflow 4+1 阶段（70%/90% 双阈值） | ARCH §3.7.1, CTX §4.1 | Task 2 |
| §E-07 LLM 错误分类（8 类，可重试 vs 不可重试） | ARCH §E-07 | Task 3 |
| EventBus（广播通道，解耦组件通信） | ARCH §Phase 2.4 | Task 4 |
| §5.5 工具执行安全（ExecSecurityMode / env_clear / WASM Fuel+Epoch / SSRF） | ARCH §5.5 | Task 5 |
| Batch 3 遗留 bugfix（5 项） | CHECKPOINT_PLAN §已知限制 | Task 6 |

---

## 文件索引

### 新增文件
| 文件 | 任务 | 说明 |
|------|------|------|
| `crates/octo-engine/src/agent/loop_guard.rs` | Task 1 | Loop Guard / Circuit Breaker |
| `crates/octo-engine/src/provider/retry.rs` | Task 3 | LLM 错误分类 + 指数退避重试 |
| `crates/octo-engine/src/event/mod.rs` | Task 4 | EventBus 模块入口 |
| `crates/octo-engine/src/event/bus.rs` | Task 4 | EventBus 实现 |

### 修改文件
| 文件 | 任务 | 说明 |
|------|------|------|
| `crates/octo-engine/src/context/budget.rs` | Task 2 | DegradationLevel 4→6 变体，阈值 70%/90% |
| `crates/octo-engine/src/context/pruner.rs` | Task 2 | 4+1 阶段执行逻辑 |
| `crates/octo-engine/src/agent/loop_.rs` | Task 1/2/3/4 | 集成 Loop Guard / Context Overflow / Retry / EventBus |
| `crates/octo-engine/src/tools/bash.rs` | Task 5 | ExecSecurityMode + env_clear + 路径遍历保护 |
| `crates/octo-engine/src/tools/mod.rs` | Task 5 | ExecPolicy 注入 |
| `crates/octo-engine/src/lib.rs` | Task 1/4 | 新模块注册 |
| `crates/octo-server/src/main.rs` | Task 6 | Recorder 共享 DB 连接 |
| `crates/octo-server/src/ws.rs` | Task 6 | TokenBudgetUpdate 发射 |
| `crates/octo-server/src/api/sessions.rs` | Task 6 | list_sessions 修复 |
| `crates/octo-server/src/api/memories.rs` | Task 6 | get_working_memory 修复 |

---

## Task 1：Loop Guard / Circuit Breaker

**目标**：防止 Agent 陷入死循环（重复调用 + 乒乓 + 全局过热）

**参考**：`ARCHITECTURE_DESIGN.md §3.2.1`，OpenFang `openfang-kernel/src/agent/loop_guard.rs`

**文件**：
- 新增：`crates/octo-engine/src/agent/loop_guard.rs`
- 修改：`crates/octo-engine/src/agent/loop_.rs`
- 修改：`crates/octo-engine/src/lib.rs`

### Step 1: 创建 loop_guard.rs

```rust
// crates/octo-engine/src/agent/loop_guard.rs
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Loop Guard 触发原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopGuardViolation {
    /// 同一工具调用（name+params）重复 ≥ 5 次
    RepetitiveCall { tool_name: String, count: u32 },
    /// 乒乓模式：A-B-A 或 A-B-A-B 检测到 ≥ 3 次循环
    PingPong { pattern: String },
    /// 全局断路器：总调用次数 ≥ 30
    CircuitBreaker { total_calls: u64 },
}

impl std::fmt::Display for LoopGuardViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RepetitiveCall { tool_name, count } =>
                write!(f, "repetitive tool call '{}' ({} times)", tool_name, count),
            Self::PingPong { pattern } =>
                write!(f, "ping-pong loop detected: {}", pattern),
            Self::CircuitBreaker { total_calls } =>
                write!(f, "circuit breaker triggered after {} total calls", total_calls),
        }
    }
}

pub struct LoopGuard {
    /// tool_name + params_hash → count
    call_counts: HashMap<u64, (String, u32)>,
    /// 最近 6 次工具调用名称（滑动窗口）
    recent_calls: VecDeque<String>,
    /// 全局调用计数器（原子，无锁）
    total_calls: Arc<AtomicU64>,
    /// 重复阈值（默认 5）
    repetition_threshold: u32,
    /// 全局断路器阈值（默认 30）
    circuit_breaker_threshold: u64,
}

impl LoopGuard {
    pub fn new() -> Self {
        Self {
            call_counts: HashMap::new(),
            recent_calls: VecDeque::with_capacity(6),
            total_calls: Arc::new(AtomicU64::new(0)),
            repetition_threshold: 5,
            circuit_breaker_threshold: 30,
        }
    }

    /// 记录一次工具调用，返回是否触发违规
    pub fn record_call(&mut self, tool_name: &str, params_json: &str) -> Option<LoopGuardViolation> {
        let total = self.total_calls.fetch_add(1, Ordering::Relaxed) + 1;

        // 1. 全局断路器检查
        if total >= self.circuit_breaker_threshold {
            return Some(LoopGuardViolation::CircuitBreaker { total_calls: total });
        }

        // 2. 重复调用检测
        let key = Self::hash_call(tool_name, params_json);
        let entry = self.call_counts.entry(key).or_insert((tool_name.to_string(), 0));
        entry.1 += 1;
        if entry.1 >= self.repetition_threshold {
            return Some(LoopGuardViolation::RepetitiveCall {
                tool_name: tool_name.to_string(),
                count: entry.1,
            });
        }

        // 3. 乒乓检测（滑动窗口 6 次）
        self.recent_calls.push_back(tool_name.to_string());
        if self.recent_calls.len() > 6 {
            self.recent_calls.pop_front();
        }
        if let Some(violation) = self.detect_ping_pong() {
            return Some(violation);
        }

        None
    }

    fn hash_call(tool_name: &str, params_json: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        tool_name.hash(&mut hasher);
        params_json.hash(&mut hasher);
        hasher.finish()
    }

    /// 检测最近调用窗口中的乒乓模式（A-B-A 重复 ≥ 3 次）
    fn detect_ping_pong(&self) -> Option<LoopGuardViolation> {
        let calls: Vec<&str> = self.recent_calls.iter().map(|s| s.as_str()).collect();
        let len = calls.len();
        if len < 4 {
            return None;
        }
        // 检测 A-B-A-B 模式（长度 4）
        if len >= 4 && calls[len-4] == calls[len-2] && calls[len-3] == calls[len-1]
            && calls[len-4] != calls[len-3] {
            let pattern = format!("{}-{}-{}-{}", calls[len-4], calls[len-3], calls[len-2], calls[len-1]);
            return Some(LoopGuardViolation::PingPong { pattern });
        }
        // 检测 A-B-A 模式（长度 3，出现 2 次）
        if len >= 6 && calls[len-6] == calls[len-4] && calls[len-4] == calls[len-2]
            && calls[len-5] == calls[len-3] {
            let pattern = format!("{}-{}-{} (x2)", calls[len-4], calls[len-3], calls[len-2]);
            return Some(LoopGuardViolation::PingPong { pattern });
        }
        None
    }

    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(Ordering::Relaxed)
    }
}

impl Default for LoopGuard {
    fn default() -> Self {
        Self::new()
    }
}
```

### Step 2: 验证 loop_guard.rs 编译

```bash
cargo check -p octo-engine 2>&1 | head -20
```

预期：如果 loop_guard.rs 还没被 mod 引入，会有 "file not found" 类的提示或无错误（因为未引用）。

### Step 3: 注册 loop_guard 模块到 lib.rs

在 `crates/octo-engine/src/lib.rs` 中，在 `pub mod agent;` 附近找到 `agent/mod.rs`（或直接在 agent 模块内注册）。

实际上 loop_guard.rs 放在 `agent/` 子目录下，需要在 `crates/octo-engine/src/agent/mod.rs` 添加：

```rust
// 在 crates/octo-engine/src/agent/mod.rs 末尾添加
pub mod loop_guard;
```

若 `agent` 目录没有 `mod.rs` 而是直接引用文件，则在 `crates/octo-engine/src/lib.rs` 中找到 agent 模块相关声明，添加 `loop_guard` 子模块声明。

### Step 4: 集成到 AgentLoop

在 `crates/octo-engine/src/agent/loop_.rs` 中：

1. 在 `AgentLoop` struct 中添加字段：
```rust
loop_guard: crate::agent::loop_guard::LoopGuard,
```

2. 在 `AgentLoop::new()` 中初始化：
```rust
loop_guard: crate::agent::loop_guard::LoopGuard::new(),
```

3. 在工具调用循环中（每次执行工具 **之前**），添加检查：
```rust
// 在 execute_tool() 调用前
let params_json = serde_json::to_string(&tool_use.input).unwrap_or_default();
if let Some(violation) = self.loop_guard.record_call(&tool_use.name, &params_json) {
    tracing::warn!("Loop Guard triggered: {}", violation);
    return Err(OctoError::Agent(format!("Loop Guard: {}", violation)));
}
```

### Step 5: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep -E "^error" | head -20
```

预期：0 errors

### Step 6: Commit

```bash
git add crates/octo-engine/src/agent/loop_guard.rs \
        crates/octo-engine/src/agent/loop_.rs \
        crates/octo-engine/src/lib.rs
git commit -m "feat(engine): add Loop Guard with repetitive/ping-pong/circuit-breaker detection"
```

---

## Task 2：Context Overflow 4+1 阶段（budget.rs + pruner.rs 重构）

**目标**：将 `DegradationLevel` 从 4 变体扩展到 6 变体；将 pruner.rs 的降级执行逻辑从三阶段改为 4+1 阶段；阈值从 60%/80%/90% 改为 60%/70%/90%

**参考**：`CONTEXT_ENGINEERING_DESIGN.md §4.1, §7.1, §7.4`

**文件**：
- 修改：`crates/octo-engine/src/context/budget.rs`
- 修改：`crates/octo-engine/src/context/pruner.rs`
- 修改：`crates/octo-engine/src/agent/loop_.rs`（集成新 DegradationLevel）

### Step 1: 读取当前 budget.rs

先读取当前实现了解现有结构：

```bash
# 查看 DegradationLevel 和相关函数
grep -n "DegradationLevel\|degradation\|usage_ratio\|compute_degradation" \
    crates/octo-engine/src/context/budget.rs | head -30
```

### Step 2: 更新 DegradationLevel（budget.rs）

找到当前的 `DegradationLevel` enum，替换为 6 变体版本：

```rust
/// 上下文降级级别（4+1 阶段，参考 CONTEXT_ENGINEERING_DESIGN.md §7.1）
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradationLevel {
    /// 使用率 ≤ 70%：无需降级（SoftTrim 在 60-70% 之间作为预警性干预）
    None,
    /// 使用率 60%-70%：工具结果头尾裁剪（前1500 + 后500 chars）
    SoftTrim,
    /// 使用率 70%-90%：保留最近 10 条消息，其余替换为占位符
    AutoCompaction,
    /// 使用率 > 90%：保留最近 4 条消息，触发 Memory Flush + 结构化摘要
    OverflowCompaction,
    /// 压缩后仍超限：截断当前工具结果至 2K tokens
    ToolResultTruncation,
    /// 全部手段失效：返回结构化错误，终止 Agent Loop
    FinalError,
}
```

更新 `compute_degradation_level` 方法，将阈值改为 60%/70%/90%：

```rust
pub fn compute_degradation_level(&self, messages: &[ChatMessage]) -> DegradationLevel {
    let ratio = self.usage_ratio(messages);
    // 4+1 阶段溢出恢复（参考 CONTEXT_ENGINEERING_DESIGN.md §7.4）
    // SoftTrim 在 60%-70% 之间作为预警性轻度干预
    match ratio {
        r if r < 0.60 => DegradationLevel::None,
        r if r < 0.70 => DegradationLevel::SoftTrim,
        r if r < 0.90 => DegradationLevel::AutoCompaction,
        _ => DegradationLevel::OverflowCompaction,
        // ToolResultTruncation 和 FinalError 由 ContextPruner 按需升级触发
    }
}
```

### Step 3: 验证 budget.rs 编译

```bash
cargo check -p octo-engine 2>&1 | grep -E "^error" | head -20
```

如果有编译错误（因为 pruner.rs 或 loop_.rs 引用了旧的枚举变体），记录错误并在下一步修复。

### Step 4: 更新 pruner.rs 实现 4+1 阶段

读取当前 pruner.rs（先 grep 了解结构）：

```bash
grep -n "fn prune\|DegradationLevel::\|match level\|hard_clear\|soft_trim\|compact" \
    crates/octo-engine/src/context/pruner.rs | head -30
```

在 `prune()` 或等效函数中，将 match 分支从三阶段改为 4+1 阶段：

```rust
pub fn prune(&self, messages: &mut Vec<ChatMessage>, level: &DegradationLevel) {
    match level {
        DegradationLevel::None => { /* 无操作 */ }

        DegradationLevel::SoftTrim => {
            // 对 ≥2 轮前的工具结果做头尾截断
            // 保留前 1500 chars + 后 500 chars，中间替换为省略标记
            self.soft_trim_old_tool_results(messages, 2);
        }

        DegradationLevel::AutoCompaction => {
            // 保留最近 10 条消息完整历史
            // 更早的工具结果替换为占位符，但保留 name + input 摘要
            self.compact_old_tool_results(messages, 10);
        }

        DegradationLevel::OverflowCompaction => {
            // 1. 触发 Memory Flush（保存关键事实到 Working Memory）
            // 2. 仅保留最近 4 条消息完整历史
            // 3. 替换旧历史为结构化摘要
            // 注：Memory Flush 由 AgentLoop 在调用 prune 前执行
            self.overflow_compaction(messages, 4);
        }

        DegradationLevel::ToolResultTruncation => {
            // 截断当前轮次工具结果至 2K tokens（约 8K chars）
            self.truncate_current_tool_results(messages, 8_000);
        }

        DegradationLevel::FinalError => {
            // 不修改 messages，由调用方处理为错误返回
        }
    }
}

/// 对 offset 轮之前的工具结果做头尾截断
fn soft_trim_old_tool_results(&self, messages: &mut Vec<ChatMessage>, offset: usize) {
    const HEAD: usize = 1500;
    const TAIL: usize = 500;
    let cutoff = messages.len().saturating_sub(offset * 2);
    for msg in messages.iter_mut().take(cutoff) {
        for block in msg.content_mut() {
            if let ContentBlock::ToolResult { content, .. } = block {
                if let Some(text) = content.as_text_mut() {
                    if text.len() > HEAD + TAIL {
                        let omitted = text.len() - HEAD - TAIL;
                        let new_text = format!(
                            "{}\n\n[... 已省略 {} chars ...]\n\n{}",
                            &text[..HEAD],
                            omitted,
                            &text[text.len() - TAIL..]
                        );
                        *text = new_text;
                    }
                }
            }
        }
    }
}

/// 保留最近 keep_recent 条消息，其余工具结果替换为占位符
fn compact_old_tool_results(&self, messages: &mut Vec<ChatMessage>, keep_recent: usize) {
    let cutoff = messages.len().saturating_sub(keep_recent);
    for msg in messages.iter_mut().take(cutoff) {
        for block in msg.content_mut() {
            if let ContentBlock::ToolResult { tool_use_id, content, .. } = block {
                *content = ContentBlock::text(format!(
                    "[工具 {} 已执行，结果已省略]",
                    tool_use_id
                )).into();
            }
        }
    }
}

/// OverflowCompaction：仅保留最近 keep_recent 条消息，其余截断
fn overflow_compaction(&self, messages: &mut Vec<ChatMessage>, keep_recent: usize) {
    let total = messages.len();
    if total > keep_recent {
        // 移除旧消息（最前面的）
        messages.drain(0..total - keep_recent);
    }
}

/// 截断当前轮次工具结果至 max_chars
fn truncate_current_tool_results(&self, messages: &mut Vec<ChatMessage>, max_chars: usize) {
    // 只处理最后一条工具结果消息
    if let Some(msg) = messages.last_mut() {
        for block in msg.content_mut() {
            if let ContentBlock::ToolResult { content, .. } = block {
                if let Some(text) = content.as_text_mut() {
                    if text.len() > max_chars {
                        let truncated = format!(
                            "{}\n\n[... 工具结果已截断，原始大小 {} chars ...]",
                            &text[..max_chars],
                            text.len()
                        );
                        *text = truncated;
                    }
                }
            }
        }
    }
}
```

**注意**：上述代码假设 `ContentBlock` 有 `content_mut()` 方法。读取实际代码后按实际 API 调整。

### Step 5: 修复 loop_.rs 中对旧 DegradationLevel 变体的引用

```bash
grep -n "DegradationLevel::\|HardClear\|Compact\b" \
    crates/octo-engine/src/agent/loop_.rs | head -20
```

将引用到旧变体（`HardClear`、`Compact` 等）的代码更新为新变体名称。同时在 `OverflowCompaction` 分支前添加 Memory Flush 调用（如果 `MemoryFlusher` 已存在）：

```rust
DegradationLevel::OverflowCompaction => {
    // 先执行 Memory Flush，再压缩
    if let Some(flusher) = &self.memory_flusher {
        flusher.flush(&messages_to_flush).await;
    }
    self.context_pruner.prune(&mut messages, &level);
}
DegradationLevel::FinalError => {
    return Err(OctoError::Agent(
        "Context overflow: all recovery strategies exhausted".to_string()
    ));
}
```

### Step 6: 验证编译

```bash
cargo check --workspace 2>&1 | grep -E "^error" | head -30
```

预期：0 errors

### Step 7: Commit

```bash
git add crates/octo-engine/src/context/budget.rs \
        crates/octo-engine/src/context/pruner.rs \
        crates/octo-engine/src/agent/loop_.rs
git commit -m "feat(context): update to 4+1 stage overflow recovery with 70%/90% dual thresholds"
```

---

## Task 3：LLM 错误分类 + 指数退避重试（provider/retry.rs）

**目标**：将现有的简单 5xx 重试机制升级为 8 类错误分类 + 可重试/不可重试区分 + 指数退避

**参考**：`ARCHITECTURE_DESIGN.md §E-07`，OpenFang `openfang-kernel/src/provider/retry.rs`

**文件**：
- 新增：`crates/octo-engine/src/provider/retry.rs`
- 修改：`crates/octo-engine/src/agent/loop_.rs`（替换原重试逻辑）
- 修改：`crates/octo-engine/src/lib.rs`（注册模块）

### Step 1: 创建 provider/retry.rs

```rust
// crates/octo-engine/src/provider/retry.rs
use std::time::Duration;
use octo_types::OctoError;

/// LLM 错误 8 分类（参考 ARCHITECTURE_DESIGN.md §E-07）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmErrorKind {
    // === 可重试 ===
    /// HTTP 429 - 速率限制，需等待 Retry-After
    RateLimit,
    /// HTTP 529 / "overloaded" - 服务过载，短暂等待后重试
    Overloaded,
    /// 网络超时或连接断开
    Timeout,
    /// HTTP 500/502/503 - 瞬时服务错误
    ServiceError,

    // === 不可重试 ===
    /// HTTP 402 / "credit_balance_too_low" - 账户余额不足
    BillingError,
    /// HTTP 401/403 - 认证/授权失败
    AuthError,
    /// 上下文窗口超限（即使已压缩）
    ContextOverflow,
    /// 其他未分类错误（保守地视为不可重试）
    Unknown,
}

impl LlmErrorKind {
    /// 判断是否应该重试
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit | Self::Overloaded | Self::Timeout | Self::ServiceError)
    }

    /// 从错误信息中分类
    pub fn classify(error: &OctoError) -> Self {
        let msg = error.to_string().to_lowercase();
        if msg.contains("429") || msg.contains("rate_limit") || msg.contains("rate limit") {
            Self::RateLimit
        } else if msg.contains("529") || msg.contains("overloaded") {
            Self::Overloaded
        } else if msg.contains("timeout") || msg.contains("timed out") || msg.contains("connection") {
            Self::Timeout
        } else if msg.contains("500") || msg.contains("502") || msg.contains("503") {
            Self::ServiceError
        } else if msg.contains("402") || msg.contains("credit_balance") || msg.contains("billing") {
            Self::BillingError
        } else if msg.contains("401") || msg.contains("403") || msg.contains("auth") || msg.contains("api_key") {
            Self::AuthError
        } else if msg.contains("context_length") || msg.contains("context overflow") || msg.contains("too long") {
            Self::ContextOverflow
        } else {
            Self::Unknown
        }
    }
}

/// 重试策略配置
pub struct RetryPolicy {
    /// 最大重试次数（默认 3）
    pub max_retries: u32,
    /// 基础等待时间（默认 1s）
    pub base_delay: Duration,
    /// 最大等待时间上限（默认 60s）
    pub max_delay: Duration,
    /// 指数退避系数（默认 2.0）
    pub backoff_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
        }
    }
}

impl RetryPolicy {
    /// 计算第 attempt 次重试的等待时间（指数退避）
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let delay_secs = self.base_delay.as_secs_f64()
            * self.backoff_factor.powi(attempt as i32);
        let clamped = delay_secs.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(clamped)
    }

    /// 判断给定错误和重试次数是否应该继续重试
    pub fn should_retry(&self, error: &OctoError, attempt: u32) -> bool {
        if attempt >= self.max_retries {
            return false;
        }
        LlmErrorKind::classify(error).is_retryable()
    }
}
```

### Step 2: 注册模块

在 `crates/octo-engine/src/provider/mod.rs`（或等效位置）添加：

```rust
pub mod retry;
pub use retry::{LlmErrorKind, RetryPolicy};
```

### Step 3: 替换 loop_.rs 中的重试逻辑

读取当前 loop_.rs 的重试代码：

```bash
grep -n "retry\|5xx\|retries\|attempt\|backoff" \
    crates/octo-engine/src/agent/loop_.rs | head -20
```

找到现有的重试 loop，替换为使用 `RetryPolicy`：

```rust
// 使用新的 RetryPolicy 替换原始的 for _ in 0..3 重试逻辑
let retry_policy = RetryPolicy::default();
let mut attempt = 0u32;
let stream = loop {
    match self.provider.stream(request.clone()).await {
        Ok(s) => break s,
        Err(e) => {
            if retry_policy.should_retry(&e, attempt) {
                let delay = retry_policy.delay_for(attempt);
                tracing::warn!(
                    "LLM call failed (attempt {}/{}): {} - retrying in {:?}",
                    attempt + 1, retry_policy.max_retries, e, delay
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            } else {
                let kind = LlmErrorKind::classify(&e);
                tracing::error!("LLM call failed (non-retryable, kind={:?}): {}", kind, e);
                return Err(e);
            }
        }
    }
};
```

### Step 4: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep -E "^error" | head -20
```

预期：0 errors

### Step 5: Commit

```bash
git add crates/octo-engine/src/provider/retry.rs \
        crates/octo-engine/src/provider/mod.rs \
        crates/octo-engine/src/agent/loop_.rs
git commit -m "feat(provider): add LLM error classification (8 types) + exponential backoff retry"
```

---

## Task 4：EventBus（内部事件广播）

**目标**：实现轻量级内部事件总线，解耦 AgentLoop 与调试/监控组件的通信

**参考**：`ARCHITECTURE_DESIGN.md §Phase 2.4`，OpenFang `openfang-kernel/src/event/bus.rs`（~149 LOC）

**文件**：
- 新增：`crates/octo-engine/src/event/mod.rs`
- 新增：`crates/octo-engine/src/event/bus.rs`
- 修改：`crates/octo-engine/src/lib.rs`
- 修改：`crates/octo-engine/src/agent/loop_.rs`（可选：发布 AgentLoop 事件）

### Step 1: 创建事件类型和 EventBus

```rust
// crates/octo-engine/src/event/bus.rs
use std::sync::Arc;
use std::collections::VecDeque;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

/// octo-engine 内部事件（参考 ARCHITECTURE_DESIGN.md §Phase 2.4）
#[derive(Debug, Clone)]
pub enum OctoEvent {
    /// Agent Loop 开始新一轮
    LoopTurnStarted { session_id: String, turn: u32 },
    /// 工具调用开始
    ToolCallStarted { session_id: String, tool_name: String },
    /// 工具调用完成
    ToolCallCompleted { session_id: String, tool_name: String, duration_ms: u64 },
    /// 上下文降级触发
    ContextDegraded { session_id: String, level: String },
    /// Loop Guard 触发
    LoopGuardTriggered { session_id: String, reason: String },
    /// Token 预算快照
    TokenBudgetUpdated { session_id: String, used: u64, total: u64, ratio: f64 },
}

/// 内部事件广播总线
///
/// 设计：broadcast::Sender（1000 容量）+ 环形缓冲区历史（最近 1000 条）
/// 参考：OpenFang openfang-kernel/src/event/bus.rs ~149 LOC
pub struct EventBus {
    sender: broadcast::Sender<OctoEvent>,
    history: Arc<RwLock<VecDeque<OctoEvent>>>,
    history_capacity: usize,
}

impl EventBus {
    pub fn new(channel_capacity: usize, history_capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(channel_capacity);
        Self {
            sender,
            history: Arc::new(RwLock::new(VecDeque::with_capacity(history_capacity))),
            history_capacity,
        }
    }

    /// 发布事件（fire-and-forget，不阻塞发送方）
    pub async fn publish(&self, event: OctoEvent) {
        // 存入历史环形缓冲区
        {
            let mut history = self.history.write().await;
            if history.len() >= self.history_capacity {
                history.pop_front();
            }
            history.push_back(event.clone());
        }
        // 广播给订阅者（忽略无订阅者的错误）
        let _ = self.sender.send(event);
    }

    /// 订阅事件流（每个订阅者独立接收）
    pub fn subscribe(&self) -> broadcast::Receiver<OctoEvent> {
        self.sender.subscribe()
    }

    /// 获取最近 N 条历史事件
    pub async fn recent_events(&self, n: usize) -> Vec<OctoEvent> {
        let history = self.history.read().await;
        history.iter().rev().take(n).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1000, 1000)
    }
}
```

```rust
// crates/octo-engine/src/event/mod.rs
pub mod bus;
pub use bus::{EventBus, OctoEvent};
```

### Step 2: 注册 event 模块到 lib.rs

在 `crates/octo-engine/src/lib.rs` 中添加：

```rust
pub mod event;
pub use event::{EventBus, OctoEvent};
```

### Step 3: 将 EventBus 集成到 AgentLoop（可选但推荐）

在 `AgentLoop` struct 中添加可选字段：

```rust
event_bus: Option<Arc<EventBus>>,
```

在工具调用前后发布事件：

```rust
// 工具调用开始
if let Some(bus) = &self.event_bus {
    bus.publish(OctoEvent::ToolCallStarted {
        session_id: self.session_id.to_string(),
        tool_name: tool_use.name.clone(),
    }).await;
}

// 工具调用完成
if let Some(bus) = &self.event_bus {
    bus.publish(OctoEvent::ToolCallCompleted {
        session_id: self.session_id.to_string(),
        tool_name: tool_use.name.clone(),
        duration_ms,
    }).await;
}
```

### Step 4: 验证编译

```bash
cargo check --workspace 2>&1 | grep -E "^error" | head -20
```

预期：0 errors

### Step 5: Commit

```bash
git add crates/octo-engine/src/event/mod.rs \
        crates/octo-engine/src/event/bus.rs \
        crates/octo-engine/src/lib.rs \
        crates/octo-engine/src/agent/loop_.rs
git commit -m "feat(engine): add EventBus for internal event broadcasting (broadcast + ring buffer)"
```

---

## Task 5：工具执行安全（ExecSecurityMode + SSRF + 路径遍历）

**目标**：为 BashTool 添加企业级安全策略；参考 `ARCHITECTURE_DESIGN.md §5.5.1`

**文件**：
- 修改：`crates/octo-engine/src/tools/bash.rs`
- 修改：`crates/octo-engine/src/tools/mod.rs`（ExecPolicy 注入）

### Step 1: 读取当前 bash.rs 结构

```bash
grep -n "pub struct\|pub fn\|impl\|env::\|Command\|process" \
    crates/octo-engine/src/tools/bash.rs | head -30
```

### Step 2: 添加 ExecSecurityMode 和 ExecPolicy

在 `bash.rs` 顶部添加安全策略类型（参考 `ARCHITECTURE_DESIGN.md §5.5.1`）：

```rust
/// Shell 命令执行安全模式（参考 ARCHITECTURE_DESIGN.md §5.5.1）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecSecurityMode {
    /// 禁止所有 shell 执行
    Deny,
    /// 仅允许白名单命令（默认）
    Allowlist,
    /// 允许所有命令（开发模式，仅本地/可信环境）
    Full,
}

/// 工具执行安全策略
#[derive(Debug, Clone)]
pub struct ExecPolicy {
    pub mode: ExecSecurityMode,
    /// 内置安全命令集
    pub safe_bins: Vec<String>,
    /// 用户扩展白名单
    pub allowed_commands: Vec<String>,
}

impl Default for ExecPolicy {
    fn default() -> Self {
        Self {
            mode: ExecSecurityMode::Allowlist,
            safe_bins: vec![
                "ls", "cat", "head", "tail", "grep", "find", "echo", "pwd",
                "wc", "sort", "uniq", "cut", "awk", "sed", "tr", "diff",
                "git", "cargo", "npm", "python3", "python", "node",
            ].into_iter().map(String::from).collect(),
            allowed_commands: vec![],
        }
    }
}

impl ExecPolicy {
    /// 检查命令是否被允许执行
    pub fn is_allowed(&self, command: &str) -> bool {
        match self.mode {
            ExecSecurityMode::Deny => false,
            ExecSecurityMode::Full => true,
            ExecSecurityMode::Allowlist => {
                // 提取命令名（取第一个词）
                let cmd = command.split_whitespace().next().unwrap_or("");
                // 去掉路径前缀（如 /usr/bin/ls → ls）
                let cmd_name = cmd.rsplit('/').next().unwrap_or(cmd);
                self.safe_bins.iter().any(|b| b == cmd_name)
                    || self.allowed_commands.iter().any(|b| b == cmd_name)
            }
        }
    }
}
```

### Step 3: 修改 BashTool 使用安全的环境变量清理

在 BashTool 的 `execute()` 方法中，替换环境变量处理为白名单模式：

```rust
// 白名单保留的安全环境变量（参考 ARCHITECTURE_DESIGN.md §5.5.1）
const SAFE_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "TERM", "USER", "SHELL",
];

// 清理环境：使用 env_clear() 后只注入白名单变量
let safe_env: Vec<(String, String)> = std::env::vars()
    .filter(|(k, _)| SAFE_ENV_VARS.contains(&k.as_str()))
    .collect();

let mut cmd = tokio::process::Command::new("sh");
cmd.arg("-c").arg(&command)
    .env_clear()
    .envs(safe_env)  // 只注入白名单变量
    .current_dir(&working_dir)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());
```

### Step 4: 添加安全检查（模式检查 + 路径遍历）

在 `execute()` 方法开头添加：

```rust
// 1. ExecPolicy 模式检查
if let Some(policy) = &self.exec_policy {
    if !policy.is_allowed(command) {
        return Ok(ToolResult::error(format!(
            "Command '{}' is not allowed by ExecPolicy (mode: {:?}). \
             Configure ExecSecurityMode::Full for unrestricted access.",
            command.split_whitespace().next().unwrap_or(""),
            policy.mode
        )));
    }
}

// 2. 路径遍历基础检查：拒绝包含 "../" 的路径参数
if command.contains("../") || command.contains("..\\") {
    return Ok(ToolResult::error(
        "Path traversal detected: '..' components are not allowed in commands".to_string()
    ));
}
```

### Step 5: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep -E "^error" | head -20
```

### Step 6: Commit

```bash
git add crates/octo-engine/src/tools/bash.rs \
        crates/octo-engine/src/tools/mod.rs
git commit -m "feat(tools): add ExecSecurityMode + env_clear + path traversal protection to BashTool"
```

---

## Task 6：Batch 3 遗留 Bugfix（5 项）

**参考**：`CHECKPOINT_PLAN.md §已知限制`

**文件**：
- `crates/octo-server/src/ws.rs`（TokenBudgetUpdate 发射）
- `crates/octo-engine/src/context/budget.rs`（snapshot() 修复）
- `crates/octo-server/src/main.rs`（Recorder 共享连接）
- `crates/octo-server/src/api/sessions.rs`（list_sessions）
- `crates/octo-server/src/api/memories.rs`（get_working_memory）

### Fix 1: TokenBudgetUpdate 事件发射

**问题**：AgentLoop 在 MessageStop 后没有发射 TokenBudgetUpdate 事件，导致前端 Token 预算栏不更新。

在 `loop_.rs` 中，找到处理 `MessageStop` 的代码块，在其后添加：

```rust
// 在 MessageStop 处理后，发射 TokenBudgetUpdate 事件
let snapshot = self.budget_manager.snapshot(&self.messages);
let _ = self.event_sender.send(AgentEvent::TokenBudgetUpdate {
    used_tokens: snapshot.used_tokens,
    total_tokens: snapshot.total_tokens,
    degradation_level: format!("{:?}", snapshot.degradation_level),
});
```

**如何找到正确位置**：
```bash
grep -n "MessageStop\|StopReason\|finish_reason" \
    crates/octo-engine/src/agent/loop_.rs | head -20
```

### Fix 2: snapshot() 填充 dynamic_context

**问题**：`snapshot()` 返回的 `dynamic_context=0`，应使用实际的工具 token 估算值。

在 `budget.rs` 的 `snapshot()` 函数中，找到 `dynamic_context: 0` 并修改：

```rust
// 用最近一轮工具结果的 tokens 近似代替 dynamic_context
let tool_tokens = self.estimate_tool_tokens(messages);
TokenBudgetSnapshot {
    // ...
    dynamic_context: tool_tokens,
    degradation_level: self.compute_degradation_level(messages),
    // ...
}
```

### Fix 3: Recorder 共享 Database 连接

**问题**：`ToolExecutionRecorder` 使用独立的 `Database` 连接，可能导致 `SQLITE_BUSY`。

在 `crates/octo-server/src/main.rs` 中，找到 Recorder 的初始化：

```rust
// 修改前（独立连接）：
let recorder = ToolExecutionRecorder::new(db_path.clone()).await?;

// 修改后（共享连接）：
let recorder = ToolExecutionRecorder::from_db(db.clone());
```

需要在 `ToolExecutionRecorder` 中添加 `from_db(db: Arc<Database>)` 构造器（如果还没有的话）。

### Fix 4: list_sessions 返回实际数据

**问题**：`GET /api/sessions` 返回空列表，因为 `SessionStore` trait 未实现 `list_all()`。

在 `crates/octo-server/src/api/sessions.rs` 中的 `list_sessions` handler：

```bash
grep -n "list_sessions\|SessionStore" \
    crates/octo-server/src/api/sessions.rs | head -10
```

为 `InMemorySessionStore` 添加 `list_all()` 实现（返回 DashMap 中所有 session_id），并在 API 中使用。

### Fix 5: get_working_memory 使用正确的 SandboxId

**问题**：`GET /api/memories/working` 每次创建新的 `SandboxId`，导致总是返回空的 working memory。

在 `crates/octo-server/src/api/memories.rs` 中：

```bash
grep -n "SandboxId\|working_memory\|get_working" \
    crates/octo-server/src/api/memories.rs | head -10
```

修改为从 `AppState` 中获取活跃会话的 `SandboxId`，而不是每次创建新的。

### Step 6.5: 验证所有 Fix

```bash
cargo check --workspace 2>&1 | grep -E "^error" | head -20
```

预期：0 errors

### Step 6.6: Commit

```bash
git add crates/octo-server/src/ws.rs \
        crates/octo-engine/src/context/budget.rs \
        crates/octo-server/src/main.rs \
        crates/octo-server/src/api/sessions.rs \
        crates/octo-server/src/api/memories.rs \
        crates/octo-engine/src/agent/loop_.rs
git commit -m "fix: batch3 known issues - TokenBudgetUpdate event, snapshot(), recorder DB, sessions list, working memory"
```

---

## Task 7：全量构建验证 + 文档更新

### Step 1: 完整构建检查

```bash
cargo check --workspace 2>&1 | tail -5
```

预期：`warning: X warnings emitted` 或类似（0 errors）

### Step 2: TypeScript 类型检查

```bash
cd web && npx tsc --noEmit 2>&1 | tail -10
cd ..
```

预期：0 errors

### Step 3: 前端构建

```bash
cd web && npx vite build 2>&1 | tail -5
cd ..
```

预期：`dist/assets/*.js` 构建成功

### Step 4: 更新 CHECKPOINT_PLAN.md

在 `docs/main/CHECKPOINT_PLAN.md` 中：
1. 将总体计划表中 `octo-workbench Phase 2.4 | ⏳ 待开始` 改为 `✅ 完成`
2. 更新 "已知限制" 列表（Batch 3 的 5 项修复标记为已解决）
3. 更新 OpenFang 架构整合里程碑中 Phase 2.4 的 3 个模块状态为 ✅

### Step 5: 更新 WORK_LOG.md

在 `docs/main/WORK_LOG.md` 追加 Phase 2.4 完成记录：

```markdown
## Phase 2.4: Engine Hardening（2026-02-27）

### 变更内容

**任务 1: Loop Guard / Circuit Breaker**
- 新增 `crates/octo-engine/src/agent/loop_guard.rs`
- 三层保护：重复调用检测（≥5次阻断）/ 乒乓检测（A-B-A ≥3次） / 全局断路器（≥30次终止）
- 集成到 AgentLoop，每次工具调用前检查

**任务 2: Context Overflow 4+1 阶段**
- `context/budget.rs`：DegradationLevel 4→6 变体，阈值改为 60%/70%/90%
- `context/pruner.rs`：实现 SoftTrim / AutoCompaction / OverflowCompaction / ToolResultTruncation / FinalError 五个降级执行函数

**任务 3: LLM 错误分类**
- 新增 `provider/retry.rs`：8 类错误分类 + RetryPolicy 指数退避
- 替换 AgentLoop 原始的 3 次简单重试

**任务 4: EventBus**
- 新增 `event/bus.rs`：broadcast::Sender + 环形缓冲区历史（1000 条）
- 集成到 AgentLoop：ToolCallStarted/Completed/ContextDegraded/LoopGuardTriggered 事件

**任务 5: 工具执行安全**
- BashTool：env_clear() + 白名单 8 个安全变量 + ExecSecurityMode + 路径遍历检测

**任务 6: Batch 3 Bugfix**
- TokenBudgetUpdate 事件在 MessageStop 后发射
- snapshot() dynamic_context 填充实际工具 tokens
- Recorder 共享 Database 连接（避免 SQLITE_BUSY）
- list_sessions 返回实际数据
- get_working_memory 使用正确 SandboxId

### 验证结果
- `cargo check --workspace` ✅
- `tsc --noEmit` ✅
- `vite build` ✅
```

### Step 6: 最终 Commit

```bash
git add docs/main/CHECKPOINT_PLAN.md docs/main/WORK_LOG.md
git commit -m "docs: Phase 2.4 Engine Hardening complete - Loop Guard + Context 4+1 + LLM Retry + EventBus + Tool Security"
```

---

## 完成标准

| 模块 | 验收标准 |
|------|---------|
| Loop Guard | `cargo check` 通过；`LoopGuard::record_call()` 在重复/乒乓/超限时返回 `Some(violation)` |
| Context 4+1 | `DegradationLevel` 有 6 变体；`compute_degradation_level()` 在 65% 返回 `SoftTrim`，75% 返回 `AutoCompaction`，95% 返回 `OverflowCompaction` |
| LLM 错误分类 | `LlmErrorKind::classify()` 对 "429" 返回 `RateLimit`（可重试），对 "401" 返回 `AuthError`（不可重试） |
| EventBus | `EventBus::publish()` + `subscribe()` + `recent_events()` 编译通过；AgentLoop 可选集成 |
| 工具执行安全 | `ExecPolicy::is_allowed("rm")` 返回 `false`（Allowlist 模式）；`execute()` 对 `"cat ../../../etc/passwd"` 返回错误 |
| Batch 3 Bugfix | `cargo check --workspace` 0 errors；API `/api/sessions` 返回非空 |

---

## 提交历史预期

```
feat(engine): add Loop Guard with repetitive/ping-pong/circuit-breaker detection
feat(context): update to 4+1 stage overflow recovery with 70%/90% dual thresholds
feat(provider): add LLM error classification (8 types) + exponential backoff retry
feat(engine): add EventBus for internal event broadcasting (broadcast + ring buffer)
feat(tools): add ExecSecurityMode + env_clear + path traversal protection to BashTool
fix: batch3 known issues - TokenBudgetUpdate event, snapshot(), recorder DB, sessions list, working memory
docs: Phase 2.4 Engine Hardening complete - Loop Guard + Context 4+1 + LLM Retry + EventBus + Tool Security
```
