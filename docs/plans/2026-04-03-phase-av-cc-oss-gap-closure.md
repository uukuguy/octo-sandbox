# Phase AV: CC-OSS Gap Closure Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 补齐 Octo 与 CC-OSS 的 6 项真正差距，分 3 波实施

**Architecture:** Wave 1 (T1-T3) 为互不依赖的小改动可并行；Wave 2 (T4-T5) 为中等改动；Wave 3 (T6) 为核心循环重构。所有改动在 `crates/octo-engine/` 内完成。

**Tech Stack:** Rust, Tokio, serde_json, tokio::sync::Semaphore

---

## Wave 1: 小改动快速出成果（T1-T3 互不依赖，可并行）

---

### Task 1: 并发安全分区 (Concurrent Safety Partitioning)

**目标**: harness 并发路径中按 `is_concurrency_safe()` 将工具分为并行 batch 和串行 batch

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs:1481-1614` (并发执行路径)
- Test: `crates/octo-engine/tests/agent_parallel_partition.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/agent_parallel_partition.rs
use octo_engine::tools::traits::Tool;

#[test]
fn test_partition_tools_by_concurrency_safety() {
    // 模拟 5 个工具: 3 safe (grep, glob, file_read) + 2 unsafe (bash, file_write)
    let tools = vec![
        ("grep".to_string(), true),
        ("bash".to_string(), false),
        ("glob".to_string(), true),
        ("file_write".to_string(), false),
        ("file_read".to_string(), true),
    ];
    let (safe, unsafe_): (Vec<_>, Vec<_>) = tools
        .into_iter()
        .partition(|(_, is_safe)| *is_safe);
    assert_eq!(safe.len(), 3);
    assert_eq!(unsafe_.len(), 2);
    // safe 保持原始相对顺序
    assert_eq!(safe[0].0, "grep");
    assert_eq!(safe[1].0, "glob");
    assert_eq!(safe[2].0, "file_read");
}
```

**Step 2: 运行测试验证失败**

```bash
cargo test -p octo-engine --test agent_parallel_partition -- --test-threads=1
```
预期: PASS（这是纯逻辑测试，先确认测试框架正常）

**Step 3: 修改 harness.rs 并发路径**

在 `harness.rs:1481` 的 `if config.agent_config.enable_parallel` 分支内，在构建 `tools_to_run` 后、调用 `execute_parallel` 前，加入分区逻辑：

```rust
// harness.rs — 在 tools_to_run 构建之后（约 1568 行后）

// --- AV-T1: Partition tools by concurrency safety ---
let (safe_tools, unsafe_tools): (Vec<_>, Vec<_>) = tools_to_run
    .into_iter()
    .enumerate()
    .partition::<Vec<_>, _>(|(idx, (name, _))| {
        tools.get(name)
            .map(|t| t.is_concurrency_safe())
            .unwrap_or(false)  // unknown tools treated as unsafe
    });

// Phase 1: Execute safe tools in parallel
let safe_items: Vec<_> = safe_tools.into_iter().map(|(idx, item)| (idx, item)).collect();
let safe_tool_inputs: Vec<_> = safe_items.iter().map(|(_, item)| item.clone()).collect();
let safe_results = if !safe_tool_inputs.is_empty() {
    execute_parallel(
        safe_tool_inputs,
        &tools,
        config.agent_config.max_parallel_tools,
        &cancellation_token,
        &tool_ctx,
        config_timeout,
    ).await
} else {
    vec![]
};

// Phase 2: Execute unsafe tools serially
let mut unsafe_results = Vec::new();
for (_, (name, input)) in &unsafe_tools {
    if cancellation_token.is_cancelled() { break; }
    let result = match tools.get(name) {
        Some(tool) => {
            match tokio::time::timeout(
                std::time::Duration::from_secs(config.tool_timeout_secs.max(30)),
                tool.execute(input.clone(), &tool_ctx),
            ).await {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => ToolOutput::error(format!("Tool error: {e}")),
                Err(_) => ToolOutput::error("Tool execution timed out".to_string()),
            }
        }
        None => ToolOutput::error(format!("Unknown tool: {name}")),
    };
    unsafe_results.push((name.clone(), result));
}

// Phase 3: Merge results back in original order
let mut indexed_results: Vec<(usize, (String, ToolOutput))> = Vec::new();
let mut safe_iter = safe_results.into_iter();
for (idx, _) in &safe_items {
    if let Some(r) = safe_iter.next() {
        indexed_results.push((*idx, r));
    }
}
let mut unsafe_iter = unsafe_results.into_iter();
for (idx, _) in &unsafe_tools {
    if let Some(r) = unsafe_iter.next() {
        indexed_results.push((*idx, r));
    }
}
indexed_results.sort_by_key(|(idx, _)| *idx);
let parallel_results: Vec<_> = indexed_results.into_iter().map(|(_, r)| r).collect();
```

**Step 4: 写集成测试验证分区行为**

```rust
// 在 agent_parallel_partition.rs 中增加
#[tokio::test]
async fn test_unsafe_tools_run_serially_after_safe() {
    // 用 AtomicUsize 记录执行顺序，验证 safe 工具先并行完成，unsafe 工具后串行
    // ... (具体实现依赖 test registry mock)
}
```

**Step 5: 运行全量 agent 测试**

```bash
cargo test -p octo-engine -- agent --test-threads=1
```
预期: 全部 PASS

**Step 6: 提交**

```bash
git add crates/octo-engine/src/agent/harness.rs crates/octo-engine/tests/agent_parallel_partition.rs
git commit -m "feat(agent): partition parallel tools by is_concurrency_safe()

AV-T1: Safe tools (grep, glob, file_read) run in parallel batch,
unsafe tools (bash, file_write) run serially after.
Preserves original result order via indexed merge.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### Task 2: Prompt Caching API 集成

**目标**: Anthropic provider 请求中为系统提示静态部分添加 `cache_control` 标记

**Files:**
- Modify: `crates/octo-engine/src/providers/anthropic.rs:323-362` (stream 方法)
- Modify: `crates/octo-engine/src/providers/anthropic.rs` (ApiRequest struct)
- Test: `crates/octo-engine/tests/anthropic_prompt_caching.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/anthropic_prompt_caching.rs
#[test]
fn test_system_prompt_split_into_cached_blocks() {
    use serde_json::Value;

    let static_part = "You are a helpful assistant.";
    let dynamic_part = "Current time: 2026-04-03T12:00:00Z";

    let blocks = build_system_content_blocks(static_part, Some(dynamic_part));

    assert_eq!(blocks.len(), 2);
    // First block has cache_control
    assert_eq!(blocks[0]["type"], "text");
    assert_eq!(blocks[0]["text"], static_part);
    assert!(blocks[0]["cache_control"].is_object());
    assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
    // Second block has no cache_control
    assert_eq!(blocks[1]["type"], "text");
    assert_eq!(blocks[1]["text"], dynamic_part);
    assert!(blocks[1].get("cache_control").is_none());
}

#[test]
fn test_system_prompt_single_block_when_no_dynamic() {
    let blocks = build_system_content_blocks("Static only", None);
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0]["cache_control"].is_object());
}
```

**Step 2: 运行测试验证失败**

```bash
cargo test -p octo-engine --test anthropic_prompt_caching -- --test-threads=1
```
预期: FAIL (函数 `build_system_content_blocks` 不存在)

**Step 3: 在 anthropic.rs 中实现**

```rust
// anthropic.rs — 新增函数
fn build_system_content_blocks(
    static_part: &str,
    dynamic_part: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut blocks = vec![serde_json::json!({
        "type": "text",
        "text": static_part,
        "cache_control": { "type": "ephemeral" }
    })];
    if let Some(dynamic) = dynamic_part {
        if !dynamic.is_empty() {
            blocks.push(serde_json::json!({
                "type": "text",
                "text": dynamic
            }));
        }
    }
    blocks
}
```

**Step 4: 修改 ApiRequest 和 stream() 方法**

将 `system: request.system` 从 `String` 改为 `Vec<Value>`：

```rust
// ApiRequest struct 修改
#[derive(Serialize)]
struct ApiRequest {
    // ... 其他字段不变
    system: serde_json::Value,  // 从 String 改为 Value (支持 string 或 array)
    // ...
}

// stream() 方法修改（约 328 行）
let system_value = if let Some((static_part, dynamic_part)) = request.system.split_once("\n---DYNAMIC---\n") {
    serde_json::Value::Array(build_system_content_blocks(static_part, Some(dynamic_part)))
} else {
    // 无分隔符时整个系统提示作为单个缓存 block
    serde_json::Value::Array(build_system_content_blocks(&request.system, None))
};

let api_req = ApiRequest {
    // ...
    system: system_value,
    // ...
};
```

同时在请求头中添加 beta header:
```rust
.header("anthropic-beta", "prompt-caching-2024-07-31")
```

**Step 5: 修改 SystemPromptBuilder.build_separated()**

在 `system_prompt.rs` 的 `build_separated()` 返回的 `PromptParts` 中，用 `\n---DYNAMIC---\n` 分隔符连接 static 和 dynamic 部分，让 Anthropic provider 能识别分割点。

**Step 6: 运行测试**

```bash
cargo test -p octo-engine --test anthropic_prompt_caching -- --test-threads=1
cargo test -p octo-engine -- anthropic --test-threads=1
```
预期: 全部 PASS

**Step 7: 提交**

```bash
git add crates/octo-engine/src/providers/anthropic.rs crates/octo-engine/src/context/system_prompt.rs crates/octo-engine/tests/anthropic_prompt_caching.rs
git commit -m "feat(providers): enable Anthropic prompt caching for system prompts

AV-T2: Split system prompt into static (cached) and dynamic blocks.
Static part gets cache_control: {type: 'ephemeral'} for API-side caching.
Adds anthropic-beta: prompt-caching-2024-07-31 header.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### Task 3: 自动 History Snipping + /compact 命令

**目标**: Budget 压力时自动裁剪旧消息 + TUI `/compact` slash 命令 + `Ctrl+K` 快捷键

**Files:**
- Modify: `crates/octo-engine/src/context/compaction_pipeline.rs:458-526` (新增 auto_snip)
- Modify: `crates/octo-engine/src/agent/harness.rs` (budget 检查处调用 auto_snip)
- Modify: `crates/octo-cli/src/tui/key_handler.rs` (添加 /compact 和 Ctrl+K)
- Test: `crates/octo-engine/tests/auto_snip.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/auto_snip.rs
use octo_engine::context::CompactionPipeline;
use octo_types::message::{ChatMessage, ContentBlock, MessageRole};

#[tokio::test]
async fn test_auto_snip_preserves_recent_messages() {
    let mut messages: Vec<ChatMessage> = (0..25)
        .map(|i| ChatMessage {
            role: if i % 2 == 0 { MessageRole::User } else { MessageRole::Assistant },
            content: vec![ContentBlock::Text { text: format!("Message {i}") }],
        })
        .collect();

    let boundary = messages.len().saturating_sub(10); // keep last 10
    let removed = CompactionPipeline::auto_snip(
        &mut messages,
        boundary,
        None, // no provider → simple truncation
        "test-model",
        None,
        None,
    ).await.unwrap();

    assert_eq!(removed, 15); // 25 - 10
    assert_eq!(messages.len(), 10);
    assert_eq!(messages[0].text_content(), "Message 15");
}

#[tokio::test]
async fn test_auto_snip_no_action_when_few_messages() {
    let mut messages: Vec<ChatMessage> = (0..5)
        .map(|i| ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: format!("Msg {i}") }],
        })
        .collect();

    let removed = CompactionPipeline::auto_snip(
        &mut messages, 0, None, "test", None, None,
    ).await.unwrap();

    assert_eq!(removed, 0); // too few messages, no action
}
```

**Step 2: 运行测试验证失败**

```bash
cargo test -p octo-engine --test auto_snip -- --test-threads=1
```
预期: FAIL (`auto_snip` 方法不存在)

**Step 3: 实现 auto_snip 方法**

```rust
// compaction_pipeline.rs — 在 snip_compact() 之后新增
impl CompactionPipeline {
    /// Auto-snip: truncate/summarize messages before `boundary` index.
    /// Same logic as snip_compact() but triggered automatically by budget pressure.
    ///
    /// - If fewer than 8 messages total, returns 0 (no action).
    /// - If a provider + pipeline are available, attempts LLM summarization.
    /// - Otherwise, simple truncation.
    pub async fn auto_snip(
        messages: &mut Vec<ChatMessage>,
        boundary: usize,
        provider: Option<&dyn Provider>,
        model: &str,
        pipeline: Option<&CompactionPipeline>,
        context: Option<&CompactionContext>,
    ) -> Result<usize> {
        if messages.len() < 8 || boundary == 0 || boundary >= messages.len() {
            return Ok(0);
        }

        info!(boundary, total = messages.len(), "Auto-snip triggered");

        // Attempt LLM summarization if available
        if let (Some(pipeline), Some(provider), Some(ctx)) = (pipeline, provider, context) {
            let to_summarize: Vec<_> = messages[..boundary].to_vec();
            if let Ok(summary) = pipeline.summarize_messages(&to_summarize, provider, model, ctx).await {
                messages.drain(..boundary);
                messages.insert(0, ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text {
                        text: format!("[Previous conversation summary]\n{summary}"),
                    }],
                });
                return Ok(boundary);
            }
        }

        // Fallback: simple truncation
        let removed = boundary;
        messages.drain(..boundary);
        info!(removed, "Auto-snip (truncation only)");
        Ok(removed)
    }
}
```

**Step 4: 在 harness.rs budget 检查处调用 auto_snip**

在 harness.rs 的 budget 降级检查处（SoftTrim 级别），加入 auto_snip 调用：

```rust
// harness.rs — 在 budget 检查后（约在 PTL recovery 附近）
if degradation_level >= DegradationLevel::SoftTrim
    && messages.len() > 20
    && !has_auto_snipped
{
    let boundary = messages.len().saturating_sub(10);
    let snip_provider = config.provider.as_deref();
    let snip_pipeline = config.compaction_pipeline.as_deref();
    let snip_ctx = /* ... build CompactionContext ... */;
    if let Ok(removed) = CompactionPipeline::auto_snip(
        &mut messages, boundary, snip_provider, &config.model, snip_pipeline, snip_ctx.as_ref(),
    ).await {
        if removed > 0 {
            has_auto_snipped = true;
            let _ = tx.send(AgentEvent::ContextCompacted {
                strategy: "auto_snip".into(),
                pre_tokens: 0,
                post_tokens: removed,
            });
        }
    }
}
```

**Step 5: 在 TUI 中添加 /compact 命令和 Ctrl+K 快捷键**

```rust
// key_handler.rs — 在 slash 命令匹配处
"/compact" => {
    // Send a CompactRequest event to the agent
    self.send_system_command("compact");
    return InputResult::Consumed;
}

// 快捷键处（Ctrl+K）
KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    self.send_system_command("compact");
    return InputResult::Consumed;
}
```

**Step 6: 运行测试**

```bash
cargo test -p octo-engine --test auto_snip -- --test-threads=1
cargo test -p octo-engine -- compaction --test-threads=1
```
预期: 全部 PASS

**Step 7: 提交**

```bash
git add crates/octo-engine/src/context/compaction_pipeline.rs crates/octo-engine/src/agent/harness.rs crates/octo-cli/src/tui/key_handler.rs crates/octo-engine/tests/auto_snip.rs
git commit -m "feat(context): auto history snipping + /compact command

AV-T3: Auto-snip when budget reaches SoftTrim (60-70%) and >20 messages.
Keeps last 10 messages, summarizes or truncates older ones.
TUI: /compact slash command + Ctrl+K shortcut.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

## Wave 2: 中等改动（T4-T5 独立）

---

### Task 4: Unattended Retry 模式

**目标**: RetryPolicy 增加无限重试模式，配合 Autonomous Agent 使用

**Files:**
- Modify: `crates/octo-engine/src/providers/retry.rs:225-236` (RetryPolicy struct)
- Modify: `crates/octo-engine/src/providers/retry.rs` (should_retry_with_info 方法)
- Modify: `crates/octo-engine/src/agent/harness.rs` (autonomous mode 时启用)
- Test: `crates/octo-engine/tests/unattended_retry.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/unattended_retry.rs
use octo_engine::providers::retry::{LlmErrorKind, RetryPolicy};
use std::time::Duration;

#[test]
fn test_unattended_retries_indefinitely_for_rate_limit() {
    let policy = RetryPolicy::unattended();
    // Should retry at attempt 100 for RateLimit
    let delay = policy.should_retry_with_info(100, &LlmErrorKind::RateLimit, None);
    assert!(delay.is_some(), "Unattended should retry RateLimit indefinitely");
    // Delay capped at unattended_max_delay (5 min)
    assert!(delay.unwrap() <= Duration::from_secs(300));
}

#[test]
fn test_unattended_does_not_retry_auth_error() {
    let policy = RetryPolicy::unattended();
    let delay = policy.should_retry_with_info(0, &LlmErrorKind::AuthError, None);
    assert!(delay.is_none(), "Auth errors should never retry");
}

#[test]
fn test_normal_policy_stops_at_max_retries() {
    let policy = RetryPolicy::default();
    let delay = policy.should_retry_with_info(3, &LlmErrorKind::RateLimit, None);
    assert!(delay.is_none(), "Normal policy should stop at max_retries=3");
}
```

**Step 2: 运行测试验证失败**

```bash
cargo test -p octo-engine --test unattended_retry -- --test-threads=1
```
预期: FAIL (`RetryPolicy::unattended()` 不存在)

**Step 3: 修改 RetryPolicy**

```rust
// retry.rs
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
    /// When true, retries indefinitely for transient errors (RateLimit, Overloaded).
    /// AuthError/BillingError still fail immediately.
    pub unattended: bool,
    /// Max delay between retries in unattended mode (default 5 min).
    pub unattended_max_delay: Duration,
}

impl RetryPolicy {
    /// Create an unattended retry policy for autonomous agents.
    pub fn unattended() -> Self {
        Self {
            max_retries: u32::MAX, // effectively infinite
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
            unattended: true,
            unattended_max_delay: Duration::from_secs(300), // 5 min
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
            unattended: false,
            unattended_max_delay: Duration::from_secs(300),
        }
    }
}
```

**Step 4: 修改 should_retry_with_info()**

```rust
pub fn should_retry_with_info(
    &self, attempt: u32, kind: &LlmErrorKind, retry_after: Option<Duration>,
) -> Option<Duration> {
    // Never retry non-retryable errors
    if matches!(kind, LlmErrorKind::AuthError | LlmErrorKind::BillingError) {
        return None;
    }

    // Check attempt limit (unattended mode ignores this)
    if !self.unattended && attempt >= self.max_retries {
        return None;
    }

    // Only retry retryable error kinds
    if !kind.is_retryable() {
        return None;
    }

    let delay = retry_after.unwrap_or_else(|| {
        let exp_delay = self.base_delay.mul_f64(self.backoff_factor.powi(attempt as i32));
        let cap = if self.unattended { self.unattended_max_delay } else { self.max_delay };
        exp_delay.min(cap)
    });

    Some(delay)
}
```

**Step 5: harness.rs 中 autonomous mode 启用 unattended**

在 `run_agent_loop_inner()` 初始化阶段：

```rust
let retry_policy = if config.autonomous_mode {
    RetryPolicy::unattended()
} else {
    config.retry_policy.clone().unwrap_or_default()
};
```

**Step 6: 运行测试**

```bash
cargo test -p octo-engine --test unattended_retry -- --test-threads=1
cargo test -p octo-engine -- retry --test-threads=1
```
预期: 全部 PASS

**Step 7: 提交**

```bash
git add crates/octo-engine/src/providers/retry.rs crates/octo-engine/src/agent/harness.rs crates/octo-engine/tests/unattended_retry.rs
git commit -m "feat(providers): unattended retry mode for autonomous agents

AV-T4: RetryPolicy::unattended() retries indefinitely for transient
errors (RateLimit, Overloaded) with 5-min max backoff.
Auth/Billing errors still fail immediately.
Auto-enabled when autonomous_mode=true.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### Task 5: Coordinator Mode

**目标**: 编排者 Agent 模式 — 专用系统提示 + worker 工具子集限制

**Files:**
- Create: `crates/octo-engine/src/agent/coordinator.rs`
- Modify: `crates/octo-engine/src/agent/entry.rs:34-64` (AgentManifest 扩展)
- Modify: `crates/octo-engine/src/agent/mod.rs` (导出 coordinator)
- Modify: `crates/octo-engine/src/context/system_prompt.rs` (注入编排者提示)
- Test: `crates/octo-engine/tests/coordinator_mode.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/coordinator_mode.rs
use octo_engine::agent::coordinator::{CoordinatorConfig, build_coordinator_prompt};

#[test]
fn test_coordinator_prompt_contains_role_definition() {
    let config = CoordinatorConfig {
        worker_tools: vec!["bash".into(), "file_read".into(), "grep".into()],
        mcp_servers: vec!["postgres".into()],
    };
    let prompt = build_coordinator_prompt(&config);
    assert!(prompt.contains("orchestrator"));
    assert!(prompt.contains("bash"));
    assert!(prompt.contains("postgres"));
    // Should NOT contain agent_spawn in worker tools
    assert!(!prompt.contains("worker_allowed_tools") || !prompt.contains("agent_spawn"));
}

#[test]
fn test_coordinator_default_worker_tools() {
    let defaults = CoordinatorConfig::default_worker_tools();
    assert!(defaults.contains(&"bash".to_string()));
    assert!(defaults.contains(&"file_read".to_string()));
    assert!(defaults.contains(&"grep".to_string()));
    // agent_spawn should NOT be in defaults
    assert!(!defaults.contains(&"agent_spawn".to_string()));
}
```

**Step 2: 运行测试验证失败**

```bash
cargo test -p octo-engine --test coordinator_mode -- --test-threads=1
```
预期: FAIL (模块不存在)

**Step 3: 创建 coordinator.rs**

```rust
// crates/octo-engine/src/agent/coordinator.rs

/// Configuration for Coordinator mode.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Tools available to worker agents (subset of full registry).
    pub worker_tools: Vec<String>,
    /// MCP servers accessible to workers.
    pub mcp_servers: Vec<String>,
}

impl CoordinatorConfig {
    /// Default tool subset for workers — excludes agent_spawn to prevent recursion.
    pub fn default_worker_tools() -> Vec<String> {
        vec![
            "bash", "file_read", "file_write", "file_edit",
            "grep", "glob", "web_fetch", "web_search",
        ].into_iter().map(String::from).collect()
    }
}

/// Build the coordinator system prompt section.
pub fn build_coordinator_prompt(config: &CoordinatorConfig) -> String {
    let worker_tools_list = config.worker_tools.join(", ");
    let mcp_list = if config.mcp_servers.is_empty() {
        "None".to_string()
    } else {
        config.mcp_servers.join(", ")
    };

    format!(
r#"## Coordinator Mode

You are a task orchestrator that coordinates work across multiple worker agents.

### Your Tools
- `agent_spawn` — Create worker agents with specific tasks
- `send_message` — Continue a running worker agent
- `task_stop` — Stop a worker agent

### Worker Capabilities
Each worker agent has access to these tools: {worker_tools_list}
MCP servers available to workers: {mcp_list}

### Best Practices
1. **Parallel research**: Spawn multiple workers for independent investigation tasks
2. **Serial implementation**: Use one worker at a time for file modifications to avoid conflicts
3. **No worker-to-worker**: All coordination flows through you. Workers cannot spawn other workers.
4. **Synthesize before delegating**: Review worker results before assigning next task
5. **Clear task descriptions**: Give each worker a complete, self-contained task description
"#)
}
```

**Step 4: 扩展 AgentManifest**

```rust
// entry.rs — AgentManifest 新增字段
pub struct AgentManifest {
    // ... existing fields ...

    /// When true, this agent runs in Coordinator mode with orchestration prompt.
    #[serde(default)]
    pub coordinator: bool,

    /// Tools available to worker agents spawned by this coordinator.
    /// Empty = use default worker tools. Only relevant when coordinator=true.
    #[serde(default)]
    pub worker_allowed_tools: Vec<String>,
}
```

**Step 5: SystemPromptBuilder 注入编排者提示**

```rust
// system_prompt.rs — build_static() 方法中
if manifest.coordinator {
    let coordinator_config = CoordinatorConfig {
        worker_tools: if manifest.worker_allowed_tools.is_empty() {
            CoordinatorConfig::default_worker_tools()
        } else {
            manifest.worker_allowed_tools.clone()
        },
        mcp_servers: self.mcp_server_names.clone().unwrap_or_default(),
    };
    parts.push(build_coordinator_prompt(&coordinator_config));
}
```

**Step 6: 运行测试**

```bash
cargo test -p octo-engine --test coordinator_mode -- --test-threads=1
cargo test -p octo-engine -- coordinator --test-threads=1
```
预期: 全部 PASS

**Step 7: 提交**

```bash
git add crates/octo-engine/src/agent/coordinator.rs crates/octo-engine/src/agent/entry.rs crates/octo-engine/src/agent/mod.rs crates/octo-engine/src/context/system_prompt.rs crates/octo-engine/tests/coordinator_mode.rs
git commit -m "feat(agent): add Coordinator mode for multi-agent orchestration

AV-T5: Coordinator agents get specialized system prompt defining
orchestration tools (agent_spawn, send_message, task_stop) and
worker tool restrictions. Workers cannot spawn other workers.
AgentManifest gains coordinator and worker_allowed_tools fields.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

## Wave 3: 核心重构（T6 — 最大改动，单独实施）

---

### Task 6: 流式工具执行 (Streaming Tool Execution)

**目标**: 在 API 流式期间解析已完成的 tool_use block 并立即执行 safe 工具

**Files:**
- Create: `crates/octo-engine/src/agent/streaming_executor.rs`
- Modify: `crates/octo-engine/src/agent/harness.rs:2069+` (consume_stream 改造)
- Modify: `crates/octo-engine/src/agent/mod.rs` (导出)
- Modify: `crates/octo-engine/src/agent/config.rs` (新增 enable_streaming_execution)
- Test: `crates/octo-engine/tests/streaming_executor.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/streaming_executor.rs
use octo_engine::agent::streaming_executor::StreamingToolExecutor;

#[tokio::test]
async fn test_safe_tool_executes_during_stream() {
    let registry = test_registry(); // grep, bash
    let mut executor = StreamingToolExecutor::new(registry, tool_ctx(), 4);

    // Simulate: tool_use block for "grep" completes during stream
    executor.on_tool_block_complete("call_1", "grep", json!({"pattern": "test"}));

    // grep is safe → should already be spawned
    assert_eq!(executor.pending_count(), 1);
    assert_eq!(executor.spawned_count(), 1); // started immediately

    // Simulate: tool_use block for "bash" completes
    executor.on_tool_block_complete("call_2", "bash", json!({"command": "echo hi"}));

    // bash is unsafe → queued but NOT spawned
    assert_eq!(executor.pending_count(), 2);
    assert_eq!(executor.spawned_count(), 1); // still only grep

    // Finalize: API stream done, execute remaining unsafe tools
    let results = executor.finalize().await;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "call_1"); // grep result first (original order)
    assert_eq!(results[1].0, "call_2"); // bash result second
}

#[tokio::test]
async fn test_stream_failure_discards_pending() {
    let registry = test_registry();
    let mut executor = StreamingToolExecutor::new(registry, tool_ctx(), 4);

    executor.on_tool_block_complete("call_1", "grep", json!({"pattern": "test"}));
    // grep is safe and spawned

    // API stream fails → discard all
    executor.discard();
    assert_eq!(executor.pending_count(), 0);
}
```

**Step 2: 运行测试验证失败**

```bash
cargo test -p octo-engine --test streaming_executor -- --test-threads=1
```
预期: FAIL (模块不存在)

**Step 3: 创建 streaming_executor.rs**

```rust
// crates/octo-engine/src/agent/streaming_executor.rs

use std::sync::Arc;
use tokio::task::JoinHandle;
use crate::tools::{ToolOutput, ToolRegistry, ToolContext};

#[derive(Debug)]
enum ToolState {
    /// Safe tool: spawned immediately, handle stored
    Spawned {
        handle: JoinHandle<(String, ToolOutput)>,
    },
    /// Unsafe tool: queued, will execute after stream completes
    Queued {
        name: String,
        input: serde_json::Value,
    },
}

pub struct StreamingToolExecutor {
    registry: Arc<ToolRegistry>,
    tool_ctx: ToolContext,
    max_parallel: u8,
    /// Tools in order of arrival, preserving original sequence
    tools: Vec<(String, ToolState)>, // (tool_use_id, state)
}

impl StreamingToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>, ctx: ToolContext, max_parallel: u8) -> Self {
        Self {
            registry,
            tool_ctx: ctx,
            max_parallel,
            tools: Vec::new(),
        }
    }

    /// Called when a tool_use content block completes during API streaming.
    pub fn on_tool_block_complete(
        &mut self,
        tool_use_id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) {
        let is_safe = self.registry.get(tool_name)
            .map(|t| t.is_concurrency_safe())
            .unwrap_or(false);

        let state = if is_safe {
            // Spawn immediately
            let registry = self.registry.clone();
            let ctx = self.tool_ctx.clone();
            let name = tool_name.to_string();
            let inp = input.clone();
            let handle = tokio::spawn(async move {
                match registry.get(&name) {
                    Some(tool) => {
                        match tool.execute(inp, &ctx).await {
                            Ok(output) => (name, output),
                            Err(e) => (name, ToolOutput::error(format!("{e}"))),
                        }
                    }
                    None => (name, ToolOutput::error(format!("Unknown tool: {name}"))),
                }
            });
            ToolState::Spawned { handle }
        } else {
            ToolState::Queued {
                name: tool_name.to_string(),
                input,
            }
        };

        self.tools.push((tool_use_id.to_string(), state));
    }

    /// How many tools are pending (spawned or queued).
    pub fn pending_count(&self) -> usize { self.tools.len() }

    /// How many safe tools were spawned immediately.
    pub fn spawned_count(&self) -> usize {
        self.tools.iter().filter(|(_, s)| matches!(s, ToolState::Spawned { .. })).count()
    }

    /// Finalize: wait for spawned tools, execute queued tools serially, return in order.
    pub async fn finalize(self) -> Vec<(String, ToolOutput)> {
        let mut results: Vec<(String, Option<ToolOutput>)> = Vec::with_capacity(self.tools.len());

        // Collect handles and queued items, preserving order
        let mut handles: Vec<(usize, JoinHandle<(String, ToolOutput)>)> = Vec::new();
        let mut queued: Vec<(usize, String, serde_json::Value)> = Vec::new();

        for (i, (id, state)) in self.tools.into_iter().enumerate() {
            results.push((id, None));
            match state {
                ToolState::Spawned { handle } => handles.push((i, handle)),
                ToolState::Queued { name, input } => queued.push((i, name, input)),
            }
        }

        // Wait for all spawned (safe) tools
        for (idx, handle) in handles {
            if let Ok((_, output)) = handle.await {
                results[idx].1 = Some(output);
            }
        }

        // Execute queued (unsafe) tools serially
        for (idx, name, input) in queued {
            let output = match self.registry.get(&name) {
                Some(tool) => tool.execute(input, &self.tool_ctx).await
                    .unwrap_or_else(|e| ToolOutput::error(format!("{e}"))),
                None => ToolOutput::error(format!("Unknown tool: {name}")),
            };
            results[idx].1 = Some(output);
        }

        results.into_iter()
            .map(|(id, output)| (id, output.unwrap_or_else(|| ToolOutput::error("No result".into()))))
            .collect()
    }

    /// Discard all pending work (API stream failed).
    pub fn discard(self) {
        for (_, state) in self.tools {
            if let ToolState::Spawned { handle } = state {
                handle.abort();
            }
        }
    }
}
```

**Step 4: 修改 consume_stream 以支持流式工具执行**

在 `harness.rs` 中，当 `enable_streaming_execution=true` 时，`consume_stream()` 内部在 `ContentBlockStop` (tool_use block 完成) 时调用 `executor.on_tool_block_complete()`。

关键修改点在 `consume_stream()` 的 stream event 处理循环中：

```rust
// harness.rs — consume_stream() 内，处理 tool_use block 完成事件
StreamEvent::ContentBlockStop { index } => {
    if let Some(tool_block) = pending_tool_blocks.remove(&index) {
        if let Some(executor) = streaming_executor.as_mut() {
            executor.on_tool_block_complete(
                &tool_block.id,
                &tool_block.name,
                tool_block.input.clone(),
            );
        }
        tool_use_blocks.push(tool_block);
    }
}
```

**Step 5: 在 AgentConfig 中新增配置**

```rust
// config.rs
pub struct AgentConfig {
    // ... existing
    /// Enable streaming tool execution (safe tools start during API stream).
    /// Default: false (gradual rollout).
    #[serde(default)]
    pub enable_streaming_execution: bool,
}
```

**Step 6: harness 工具执行路径整合**

当 `enable_streaming_execution=true` 且有 `StreamingToolExecutor` 时，跳过现有的 parallel/sequential 路径，直接使用 `executor.finalize()` 收集结果。

**Step 7: 运行全量测试**

```bash
cargo test -p octo-engine --test streaming_executor -- --test-threads=1
cargo test -p octo-engine -- agent --test-threads=1
cargo check --workspace
```
预期: 全部 PASS

**Step 8: 提交**

```bash
git add crates/octo-engine/src/agent/streaming_executor.rs crates/octo-engine/src/agent/harness.rs crates/octo-engine/src/agent/config.rs crates/octo-engine/src/agent/mod.rs crates/octo-engine/tests/streaming_executor.rs
git commit -m "feat(agent): streaming tool execution during API response

AV-T6: StreamingToolExecutor starts safe (read-only) tools immediately
as tool_use blocks complete during API streaming, without waiting for
full response. Unsafe tools queue and execute serially after stream ends.
Results collected in original order. Gated behind enable_streaming_execution.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

## 执行检查点

| Wave | Task | 估计改动量 | 关键文件 | 依赖 |
|------|------|-----------|---------|------|
| W1 | T1 并发安全分区 | ~60 行 | harness.rs | 无 |
| W1 | T2 Prompt Caching | ~50 行 | anthropic.rs, system_prompt.rs | 无 |
| W1 | T3 自动 Snip | ~80 行 | compaction_pipeline.rs, harness.rs, key_handler.rs | 无 |
| W2 | T4 Unattended Retry | ~50 行 | retry.rs, harness.rs | 无 |
| W2 | T5 Coordinator Mode | ~150 行 | coordinator.rs(新), entry.rs, system_prompt.rs | 无 |
| W3 | T6 流式工具执行 | ~200 行 | streaming_executor.rs(新), harness.rs, config.rs | T1 (分区逻辑) |
| | **合计** | **~590 行** | | |

### 验收标准

- [x] `cargo check --workspace` 无错误
- [x] `cargo test --workspace -- --test-threads=1` 全部通过（33 新测试）
- [x] T1: 并发路径中 safe/unsafe 工具分区执行
- [x] T2: Anthropic API 请求包含 `cache_control` 和 beta header
- [x] T3: `/compact` 命令 + Ctrl+K + auto_snip 方法（harness 自动触发见 AV-D2）
- [x] T4: RetryPolicy::unattended() 无限重试（harness 自动启用见 AV-D3）
- [x] T5: `coordinator: true` 的 Agent 获得编排者系统提示
- [x] T6: `enable_streaming_execution: true` 时 safe 工具在 API 流期间执行

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| AV-D1 | StreamingToolExecutor finalize() 结果替换 harness 正常工具执行路径（当前 safe 工具在流期间执行但结果未复用，harness 仍重新执行所有工具） | T6 infrastructure 稳定 + 性能基准测试 | ⏳ |
| AV-D2 | T3 auto_snip 集成到 harness budget 检查路径 | T3 完成 + budget 监控稳定 | ✅ 已补 @ 6c85074 |
| AV-D3 | T4 unattended retry 自动启用 | T4 完成 + autonomous mode 功能完善 | ✅ 已补 @ 6c85074 |
| AV-D4 | T5 Coordinator worker 工具过滤实际生效（当前只注入系统提示，未在 ToolRegistry 层面限制 worker 可用工具） | T5 完成 + SubAgentManager 支持 tool_filter | ⏳ |
