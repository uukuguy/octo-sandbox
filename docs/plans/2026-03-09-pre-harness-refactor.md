# Pre-Harness Refactor 完整实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 基于 4 份设计文档（Opus Harness + Opus Skills + Sonnet 4.6 Harness + Sonnet 4.6 Skills）的全部设计项，对 octo-engine 进行预重构，为后续 Agent Harness 架构做好基础准备。

**Architecture:** 按 P0/P1/P2/P3 四阶段渐进式重构。P0 解决核心断联和基础设施；P1 完善安全、工具和上下文管理；P2 增强 Provider 和 Skills 高级特性；P3 实现生态扩展和高级功能。

**Tech Stack:** Rust 1.75+, Tokio 1.42, octo-engine (21 modules), octo-types

**设计文档来源:**
- `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` (Opus)
- `docs/design/AGENT_SKILLS_BEST_IMPLEMENTATION_DESIGN.md` (Opus)
- `docs/found-by-sonnet4.6/AGENT_HARNESS_DESIGN.md` (Sonnet 4.6)
- `docs/found-by-sonnet4.6/AGENT_SKILLS_DESIGN.md` (Sonnet 4.6)

---

## 阶段概览

| 阶段 | 名称 | 任务数 | 核心目标 | 预估 LOC |
|------|------|--------|---------|---------|
| **P0** | 核心重构 + 断联修复 | 10 | AgentLoop 纯函数化、SkillTool 断联修复、TrustManager、基础类型增强 | ~1200 |
| **P1** | 安全 + 工具 + 上下文 | 12 | Tool trait 增强、SafetyPipeline、ContextManager 统一、Hook 升级、错误处理 | ~1000 |
| **P2** | Provider + Skills 高级 | 10 | Provider 装饰器链、SkillSelector 4阶段管线、多运行时、Skills↔Context 集成 | ~800 |
| **P3** | 生态 + 高级功能 | 10 | SubAgent、远程 Registry、Observation Masking、MCP 增强 | ~600 |
| **合计** | | **42** | | **~3600** |

---

## P0: 核心重构 + 断联修复

> 解决最基础的架构问题：AgentLoop 拆分、SkillTool 断联、信任模型、并发控制。

### P0-1: AgentLoopConfig 结构体

**来源:** Opus Harness §3.2, Sonnet 4.6 Harness §4.2.1 (ironclaw AgentDeps)

**目标:** 将 AgentLoop::run() 的 17+ 参数打包为单一配置结构体。

**Files:**
- Create: `crates/octo-engine/src/agent/loop_config.rs`
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs`
- Test: `crates/octo-engine/tests/agent_loop_config.rs`

**Step 1: Write the failing test**

```rust
// crates/octo-engine/tests/agent_loop_config.rs
use octo_engine::agent::AgentLoopConfig;

#[test]
fn test_agent_loop_config_builder() {
    let config = AgentLoopConfig::builder()
        .max_iterations(30)
        .max_concurrent_tools(8)
        .tool_timeout_secs(120)
        .build();

    assert_eq!(config.max_iterations, 30);
    assert_eq!(config.max_concurrent_tools, 8);
    assert_eq!(config.tool_timeout_secs, 120);
}

#[test]
fn test_agent_loop_config_defaults() {
    let config = AgentLoopConfig::default();
    assert_eq!(config.max_iterations, 30);
    assert_eq!(config.max_concurrent_tools, 8);
    assert_eq!(config.tool_timeout_secs, 120);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p octo-engine --test agent_loop_config -- --test-threads=1`
Expected: FAIL — `AgentLoopConfig` not found

**Step 3: Implement AgentLoopConfig**

```rust
// crates/octo-engine/src/agent/loop_config.rs

/// Agent Loop 配置 — 替代 run() 的 17+ 参数
/// 来源: IronClaw AgentDeps + ZeroClaw run_agent_loop()
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    // 控制参数
    pub max_iterations: u32,
    pub max_concurrent_tools: usize,
    pub tool_timeout_secs: u64,
    pub force_text_at_last: bool,      // IronClaw: 最后一次迭代强制 text-only
    pub max_tokens_continuation: u32,  // ZeroClaw: max-tokens 续写次数上限
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 30,
            max_concurrent_tools: 8,
            tool_timeout_secs: 120,
            force_text_at_last: true,
            max_tokens_continuation: 3,
        }
    }
}

impl AgentLoopConfig {
    pub fn builder() -> AgentLoopConfigBuilder {
        AgentLoopConfigBuilder::default()
    }
}

#[derive(Default)]
pub struct AgentLoopConfigBuilder {
    config: AgentLoopConfig,
}

impl AgentLoopConfigBuilder {
    pub fn max_iterations(mut self, v: u32) -> Self { self.config.max_iterations = v; self }
    pub fn max_concurrent_tools(mut self, v: usize) -> Self { self.config.max_concurrent_tools = v; self }
    pub fn tool_timeout_secs(mut self, v: u64) -> Self { self.config.tool_timeout_secs = v; self }
    pub fn force_text_at_last(mut self, v: bool) -> Self { self.config.force_text_at_last = v; self }
    pub fn max_tokens_continuation(mut self, v: u32) -> Self { self.config.max_tokens_continuation = v; self }
    pub fn build(self) -> AgentLoopConfig { self.config }
}
```

**Step 4: Export from mod.rs, run test to verify pass**

Run: `cargo test -p octo-engine --test agent_loop_config -- --test-threads=1`
Expected: PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(engine): add AgentLoopConfig struct replacing 17+ run() parameters

Introduces AgentLoopConfig with builder pattern, consolidating all AgentLoop
control parameters into a single struct. Includes force_text_at (IronClaw)
and max_tokens_continuation (ZeroClaw) fields.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-2: AgentLoop Step Function 提取

**来源:** Opus Harness §3.2 (7 step functions), Sonnet 4.6 Harness §4.2.4

**目标:** 将 AgentLoop::run() 909 行拆分为独立可测试的步骤函数。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Create: `crates/octo-engine/src/agent/loop_steps.rs`
- Test: `crates/octo-engine/tests/agent_loop_steps.rs`

**Step 1: Write tests for step functions**

```rust
// crates/octo-engine/tests/agent_loop_steps.rs
use octo_engine::agent::loop_steps;

#[test]
fn test_check_loop_guard_normal() {
    // LoopGuard 正常情况返回 Continue
}

#[test]
fn test_check_loop_guard_max_iterations() {
    // 达到 max_iterations 时返回 Stop
}

#[test]
fn test_should_execute_parallel() {
    // 多工具且无需审批时返回 true
}

#[test]
fn test_should_execute_sequential_single_tool() {
    // 单工具时返回 false
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p octo-engine --test agent_loop_steps -- --test-threads=1`

**Step 3: Extract step functions from loop_.rs**

从 `loop_.rs` 的 `run()` 方法中提取以下独立函数到 `loop_steps.rs`:
1. `build_context()` — Zone A + Zone B 系统提示构建 (lines 195-268)
2. `manage_context_budget()` — 上下文预算检查和降级 (lines 271-320)
3. `call_provider_with_retry()` — LLM 调用含重试逻辑 (lines 401-459)
4. `parse_stream_response()` — 流式响应解析 (lines 461-643)
5. `execute_tools()` — 工具执行轮次 (lines 645-858)
6. `check_loop_guard()` — 循环检测
7. `should_execute_parallel()` — 并行/串行决策（ZeroClaw 模式）

注意: 此步骤仅提取函数，不改变 run() 的行为。run() 调用这些新函数。

**Step 4: Run all tests**

Run: `cargo test --workspace -- --test-threads=1`
Expected: ALL PASS (行为不变)

**Step 5: Commit**

```bash
git add -A && git commit -m "refactor(engine): extract AgentLoop step functions from 909-line run()

Extracts 7 independent step functions from AgentLoop::run() into loop_steps.rs:
build_context, manage_context_budget, call_provider_with_retry,
parse_stream_response, execute_tools, check_loop_guard,
should_execute_parallel. No behavioral changes.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-3: AgentEvent 增强枚举

**来源:** Opus Harness §3.2 (AgentEvent ~20 variants), Sonnet 4.6 Harness §4.2.3 (ironclaw 23 Submission)

**目标:** 扩展现有 AgentEvent 枚举，增加 Tool 执行、上下文管理、安全事件。

**Files:**
- Modify: `crates/octo-engine/src/event/mod.rs` (或现有 AgentEvent 位置)
- Create: `crates/octo-engine/src/agent/events.rs` (如果 AgentEvent 需要新文件)
- Test: `crates/octo-engine/tests/agent_events.rs`

**Step 1: Write test**

```rust
#[test]
fn test_agent_event_variants() {
    let event = AgentEvent::ToolStart {
        id: "tc_1".into(),
        name: "bash".into(),
    };
    assert!(matches!(event, AgentEvent::ToolStart { .. }));

    let event = AgentEvent::Done(AgentResult {
        rounds: 3,
        tool_calls: 5,
        stop_reason: StopReason::EndTurn,
        ..Default::default()
    });
    assert!(matches!(event, AgentEvent::Done(_)));
}
```

**Step 2: Implement enhanced AgentEvent + AgentResult + StopReason**

```rust
/// 增强的 Agent 事件（覆盖 Opus §3.2 全部事件类型）
pub enum AgentEvent {
    // 现有事件保留...
    TextDelta(String),
    ThinkingDelta(String),

    // 新增: Tool 执行事件
    ToolStart { id: String, name: String },
    ToolResult { id: String, is_error: bool },

    // 新增: 上下文事件
    ContextDegraded { level: String, usage_pct: f32 },
    MemoryFlushed { facts_count: usize },

    // 新增: 安全事件
    ApprovalRequired { tool_name: String },
    SecurityBlocked { reason: String },

    // 新增: 元信息
    IterationStart { round: u32 },
    IterationEnd { round: u32 },
    Done(AgentResult),
    Error(String),
}

/// 结构化返回结果 (Opus §3.2)
#[derive(Debug, Default)]
pub struct AgentResult {
    pub rounds: u32,
    pub tool_calls: u32,
    pub stop_reason: StopReason,
}

/// 统一停止原因 (ZeroClaw NormalizedStopReason)
#[derive(Debug, Default, Clone, PartialEq)]
pub enum StopReason {
    #[default]
    EndTurn,
    ToolCall,
    MaxTokens,
    MaxIterations,
    ContextOverflow,
    SafetyBlocked,
    Cancelled,
    Error,
}
```

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat(engine): add enhanced AgentEvent enum with AgentResult and StopReason

Adds ToolStart/ToolResult/ContextDegraded/SecurityBlocked/Done events,
AgentResult structured return, and NormalizedStopReason (ZeroClaw pattern).

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-4: SkillDefinition 增强字段

**来源:** Opus Skills §5.3, Sonnet 4.6 Skills §4.1

**目标:** 添加 model, context_fork, always, trust_level, triggers, dependencies, tags, denied_tools, source_type 字段。

**Files:**
- Modify: `crates/octo-types/src/skill.rs`
- Test: `crates/octo-engine/tests/skill_definition.rs`

**Step 1: Write test**

```rust
#[test]
fn test_skill_definition_enhanced_fields() {
    let yaml = r#"
name: test-skill
description: Test
version: "1.0.0"
model: claude-sonnet-4-6
context-fork: true
always: true
trust-level: trusted
denied-tools:
  - http_request
tags:
  - devops
"#;
    let skill: SkillDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(skill.model, Some("claude-sonnet-4-6".into()));
    assert!(skill.context_fork);
    assert!(skill.always);
    assert_eq!(skill.trust_level, TrustLevel::Trusted);
    assert_eq!(skill.denied_tools, Some(vec!["http_request".into()]));
}
```

**Step 2: Add fields to SkillDefinition**

在 `crates/octo-types/src/skill.rs` 添加:
- `model: Option<String>` — 模型覆盖
- `context_fork: bool` — 独立上下文
- `always: bool` — compact 不裁剪
- `trust_level: TrustLevel` — 信任等级枚举
- `triggers: Vec<SkillTrigger>` — 自动触发条件
- `dependencies: Vec<String>` — 依赖的其他 Skill
- `tags: Vec<String>` — 分类标签
- `denied_tools: Option<Vec<String>>` — 显式拒绝工具
- `source_type: SkillSourceType` — 来源类型枚举

同时添加 `TrustLevel`, `SkillTrigger`, `SkillSourceType` 枚举定义。

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat(types): enhance SkillDefinition with model/trust/triggers/denied_tools fields

Adds 9 new fields to SkillDefinition: model, context_fork, always,
trust_level (Trusted/Installed/Unknown), triggers (FilePattern/Command/Keyword),
dependencies, tags, denied_tools, source_type (ProjectLocal/UserLocal/PluginBundled/Registry).

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-5: SkillTool ↔ SkillRuntimeBridge 断联修复

**来源:** Opus Skills §5.4, Sonnet 4.6 Skills §6.2-6.3

**目标:** SkillTool.execute() 支持 activate/run_script/list_scripts 三种 action，连通 SkillRuntimeBridge。

**Files:**
- Modify: `crates/octo-engine/src/skills/tool.rs`
- Test: `crates/octo-engine/tests/skill_tool.rs`

**Step 1: Write tests**

```rust
#[tokio::test]
async fn test_skill_tool_activate_returns_body() {
    let tool = create_test_skill_tool();
    let params = json!({"action": "activate"});
    let result = tool.execute(params, &ctx).await.unwrap();
    assert!(result.content.contains("test skill body"));
}

#[tokio::test]
async fn test_skill_tool_list_scripts_empty() {
    let tool = create_test_skill_tool_no_scripts();
    let params = json!({"action": "list_scripts"});
    let result = tool.execute(params, &ctx).await.unwrap();
    assert!(result.content.contains("No scripts"));
}
```

**Step 2: Modify SkillTool to support action dispatch**

将现有 `SkillTool::execute()` 从 `Ok(ToolResult::success(&self.skill.body))` 改为:
- `action: "activate"` → 返回 body (现有行为)
- `action: "run_script"` → 通过 SkillRuntimeBridge 执行脚本
- `action: "list_scripts"` → 列出 scripts/ 目录内容

SkillTool 需要新增 `runtime_bridge: Arc<SkillRuntimeBridge>` 字段。

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "fix(engine): connect SkillTool to SkillRuntimeBridge with action dispatch

SkillTool.execute() now supports three actions: activate (return body),
run_script (execute via SkillRuntimeBridge), list_scripts (enumerate scripts/).
Fixes the core disconnection between SkillTool and SkillRuntimeBridge.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-6: TrustManager 信任等级管理

**来源:** Opus Skills §5.5, Sonnet 4.6 Skills §5.4 (Attenuator)

**目标:** 实现三级信任管理：Trusted/Installed/Unknown，根据 source_type 和声明取较低值。

**Files:**
- Create: `crates/octo-engine/src/skills/trust.rs`
- Modify: `crates/octo-engine/src/skills/mod.rs`
- Test: `crates/octo-engine/tests/skill_trust.rs`

**Step 1: Write tests**

```rust
#[test]
fn test_effective_trust_project_local_is_trusted() {
    let tm = TrustManager::new(vec![]);
    let mut skill = test_skill();
    skill.source_type = SkillSourceType::ProjectLocal;
    skill.trust_level = TrustLevel::Trusted;
    assert_eq!(tm.effective_trust_level(&skill), TrustLevel::Trusted);
}

#[test]
fn test_effective_trust_registry_capped_at_unknown() {
    let tm = TrustManager::new(vec![]);
    let mut skill = test_skill();
    skill.source_type = SkillSourceType::Registry;
    skill.trust_level = TrustLevel::Trusted; // 声明的高信任
    // 实际应被来源降到 Unknown
    assert_eq!(tm.effective_trust_level(&skill), TrustLevel::Unknown);
}

#[test]
fn test_check_tool_permission_unknown_only_readonly() {
    let tm = TrustManager::new(vec![]);
    let mut skill = test_skill();
    skill.trust_level = TrustLevel::Unknown;
    skill.source_type = SkillSourceType::Registry;
    assert!(tm.check_tool_permission(&skill, "read").is_ok());
    assert!(tm.check_tool_permission(&skill, "bash").is_err());
}
```

**Step 2: Implement TrustManager**

实现 Opus Skills §5.5 的完整设计:
- `effective_trust_level()` — 取声明和来源推断的较低值
- `check_tool_permission()` — 根据信任等级检查工具权限
- `check_script_permission()` — 检查脚本执行权限
- 只读工具白名单: read, glob, grep, list_directory

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat(engine): implement TrustManager with 3-level trust attenuation

Implements IronClaw-style Trust Attenuation: Trusted (all tools),
Installed (allowed-tools only), Unknown (read-only only). effective_trust_level()
takes min(declared, source-inferred). check_tool_permission() enforces at runtime.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-7: ToolCallInterceptor — allowed-tools 运行时强制

**来源:** Opus Skills §5.6, Sonnet 4.6 Skills §6.2-6.3

**目标:** 在 AgentLoop 的工具调用路径中拦截违规工具。

**Files:**
- Create: `crates/octo-engine/src/tools/interceptor.rs`
- Modify: `crates/octo-engine/src/tools/mod.rs`
- Modify: `crates/octo-engine/src/agent/loop_.rs` (集成点)
- Test: `crates/octo-engine/tests/tool_interceptor.rs`

**Step 1: Write tests**

```rust
#[test]
fn test_interceptor_no_skill_allows_all() {
    let interceptor = ToolCallInterceptor::new(trust_manager, None);
    assert!(interceptor.check_permission("bash").is_ok());
}

#[test]
fn test_interceptor_installed_skill_blocks_unlisted() {
    let skill = skill_with_allowed_tools(vec!["read".into()]);
    let interceptor = ToolCallInterceptor::new(trust_manager, Some(skill));
    assert!(interceptor.check_permission("read").is_ok());
    assert!(interceptor.check_permission("bash").is_err());
}

#[test]
fn test_interceptor_denied_tools_override() {
    let mut skill = skill_with_allowed_tools(vec!["*".into()]);
    skill.denied_tools = Some(vec!["http_request".into()]);
    let interceptor = ToolCallInterceptor::new(trust_manager, Some(skill));
    assert!(interceptor.check_permission("bash").is_ok());
    assert!(interceptor.check_permission("http_request").is_err());
}
```

**Step 2: Implement ToolCallInterceptor**

包含:
- `check_permission(tool_name)` — 检查单个工具是否允许
- `filter_available_tools(all_tools)` — 过滤 LLM 可见的工具列表
- denied_tools 优先级高于 allowed_tools

**Step 3: 在 AgentLoop 中集成** (loop_.rs 工具执行前插入检查)

**Step 4: Run tests, commit**

```bash
git add -A && git commit -m "feat(engine): add ToolCallInterceptor for runtime allowed-tools enforcement

Intercepts tool calls in AgentLoop based on active skill's allowed_tools
and denied_tools. denied_tools takes priority. filter_available_tools()
removes non-permitted tools from LLM tool parameters.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-8: TurnGate 并发控制

**来源:** Sonnet 4.6 Harness §2.6 (localgpt), Opus Harness §3.2

**目标:** Arc<Semaphore>(1) 防止 HTTP 请求和心跳并发触发 agent turn。

**Files:**
- Create: `crates/octo-engine/src/agent/turn_gate.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs`
- Test: `crates/octo-engine/tests/turn_gate.rs`

**Step 1: Write tests**

```rust
#[tokio::test]
async fn test_turn_gate_mutual_exclusion() {
    let gate = TurnGate::new();
    let guard1 = gate.acquire().await;
    // 第二次 acquire 应该阻塞，用 try_acquire 测试
    assert!(gate.try_acquire().is_none());
    drop(guard1);
    assert!(gate.try_acquire().is_some());
}
```

**Step 2: Implement TurnGate**

```rust
pub struct TurnGate {
    semaphore: Arc<Semaphore>,
}

impl TurnGate {
    pub fn new() -> Self {
        Self { semaphore: Arc::new(Semaphore::new(1)) }
    }
    pub async fn acquire(&self) -> TurnGateGuard { ... }
    pub fn try_acquire(&self) -> Option<TurnGateGuard> { ... }
}
```

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat(engine): add TurnGate per-session concurrency control

Implements localgpt's TurnGate pattern: Arc<Semaphore>(1) ensures only one
agent turn runs per session at a time. Prevents TOCTOU race when HTTP requests
and heartbeat runners compete for the same session.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-9: 错误响应不持久化

**来源:** Sonnet 4.6 Harness §2.10 (nanobot), Opus Harness §3.2

**目标:** LLM API 错误不写入对话历史，防止污染后续上下文。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Test: `crates/octo-engine/tests/error_not_persisted.rs`

**Step 1: Write test**

```rust
#[test]
fn test_provider_error_not_added_to_messages() {
    let mut messages = vec![user_message("hello")];
    let original_len = messages.len();
    // Simulate provider error handling
    handle_provider_error(&mut messages, "503 Service Unavailable");
    // messages 长度不应增加
    assert_eq!(messages.len(), original_len);
}
```

**Step 2: Modify loop_.rs error handling**

在 `call_provider_with_retry()` 的 `Err` 分支中，确保不调用 `messages.push()` 添加错误消息到历史。错误通过 `AgentEvent::Error` 事件发送给调用方，但不进入 `messages` 向量。

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "fix(engine): do not persist LLM API errors to conversation history

Implements nanobot principle: provider errors are infrastructure failures,
not conversation events. Error responses are sent via AgentEvent::Error
but never appended to the messages vector, preventing context pollution.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

### P0-10: ProviderErrorKind 语义路由

**来源:** Sonnet 4.6 Harness §2.7 (moltis), Opus Harness §2.3

**目标:** 增强 LlmErrorKind，添加 should_failover() 和 routing_strategy() 方法。

**Files:**
- Modify: `crates/octo-engine/src/providers/retry.rs`
- Test: `crates/octo-engine/tests/provider_error_routing.rs`

**Step 1: Write tests**

```rust
#[test]
fn test_rate_limit_should_failover() {
    assert!(LlmErrorKind::RateLimit.should_failover());
}

#[test]
fn test_auth_error_should_not_failover() {
    assert!(!LlmErrorKind::AuthError.should_failover());
}

#[test]
fn test_context_overflow_triggers_compact() {
    assert_eq!(
        LlmErrorKind::ContextOverflow.routing_strategy(),
        ErrorStrategy::CompactAndRetry
    );
}

#[test]
fn test_billing_error_fails_immediately() {
    assert_eq!(
        LlmErrorKind::BillingError.routing_strategy(),
        ErrorStrategy::Fail
    );
}
```

**Step 2: Add methods to LlmErrorKind**

```rust
pub enum ErrorStrategy {
    Retry,
    Failover,
    CompactAndRetry,
    Fail,
}

impl LlmErrorKind {
    pub fn should_failover(&self) -> bool {
        matches!(self, Self::RateLimit | Self::ServiceError | Self::Overloaded | Self::Unknown)
    }

    pub fn routing_strategy(&self) -> ErrorStrategy {
        match self {
            Self::RateLimit | Self::Overloaded => ErrorStrategy::Retry,
            Self::ServiceError | Self::Timeout => ErrorStrategy::Failover,
            Self::ContextOverflow => ErrorStrategy::CompactAndRetry,
            Self::AuthError | Self::BillingError => ErrorStrategy::Fail,
            Self::Unknown => ErrorStrategy::Retry,
        }
    }
}
```

**Step 3: Run tests, commit**

```bash
git add -A && git commit -m "feat(engine): add semantic error routing to LlmErrorKind

Adds should_failover() and routing_strategy() to LlmErrorKind (moltis pattern).
AuthError/BillingError fail immediately (no failover). ContextOverflow triggers
CompactAndRetry. RateLimit/Overloaded retry with backoff.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>"
```

---

## P1: 安全 + 工具 + 上下文

> 增强 Tool trait、实现 SafetyPipeline、统一上下文管理、升级 Hook 系统。

### P1-1: Tool Trait 增强 — risk_level() + approval()

**来源:** Opus Harness §3.3, Sonnet 4.6 Harness §2.2 (IronClaw)

**目标:** Tool trait 新增 risk_level() 和 approval() 方法（带默认实现），添加 RiskLevel/ApprovalRequirement 枚举。

**Files:**
- Modify: `crates/octo-engine/src/tools/traits.rs`
- Modify: `crates/octo-types/src/tool.rs` (枚举定义)
- Modify: 所有内置工具实现（Bash, FileRead, FileWrite 等，更新 risk_level 返回值）
- Test: `crates/octo-engine/tests/tool_risk_level.rs`

**实现要点:**
- `RiskLevel`: ReadOnly / LowRisk / HighRisk / Destructive (对齐 MCP Tool Annotations)
- `ApprovalRequirement`: Never / AutoApprovable / Always
- 默认实现: `risk_level() -> LowRisk`, `approval() -> Never`
- Bash → Destructive/Always, FileRead → ReadOnly/Never, FileWrite → HighRisk/AutoApprovable

---

### P1-2: ToolOutput 结构化

**来源:** Opus Harness §3.3 (artifacts + metadata), pi_agent_rust 输出截断

**目标:** 替换简单的 String 返回，支持 artifacts、metadata、truncation 标记。

**Files:**
- Modify: `crates/octo-types/src/tool.rs`
- Modify: `crates/octo-engine/src/tools/traits.rs`
- Test: `crates/octo-engine/tests/tool_output.rs`

**实现要点:**
```rust
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    pub artifacts: Vec<Artifact>,
    pub metadata: Option<serde_json::Value>,
    pub truncated: bool,
    pub duration_ms: u64,
}
```

---

### P1-3: 工具输出截断统一处理

**来源:** Opus Harness §3.3 (pi_agent_rust 50KB/2000行), Opus Harness §2.2

**目标:** 在 harness 层统一处理工具输出截断。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs` (maybe_trim_tool_result 增强)
- Test: `crates/octo-engine/tests/tool_truncation.rs`

**实现要点:**
- `ToolExecutionConfig`: max_output_bytes (50KB), max_output_lines (2000)
- `TruncationStrategy`: Head67Tail27 (保留头部 67% + 尾部 27% + 中间省略提示) | HeadOnly | TailOnly
- 在现有 `maybe_trim_tool_result()` 基础上增强

---

### P1-4: HookFailureMode — FailOpen/FailClosed

**来源:** Sonnet 4.6 Harness §4.2.5, Opus Harness §2.7

**目标:** HookHandler trait 添加 failure_mode() 方法，HookRegistry 执行时根据 mode 决定错误行为。

**Files:**
- Modify: `crates/octo-engine/src/hooks/handler.rs`
- Modify: `crates/octo-engine/src/hooks/registry.rs`
- Test: `crates/octo-engine/tests/hook_failure_mode.rs`

**实现要点:**
```rust
pub enum HookFailureMode {
    FailOpen,   // Hook 出错时继续（默认）
    FailClosed, // Hook 出错时中止（安全关键 Hook）
}

// HookHandler trait 新增:
fn failure_mode(&self) -> HookFailureMode { HookFailureMode::FailOpen }
```

在 `HookRegistry::execute()` line 96-104 的 `Err` 分支，检查 `failure_mode()`:
- FailOpen → 记录日志，继续
- FailClosed → 返回 HookAction::Abort

---

### P1-5: max-tokens 自动续写

**来源:** Opus Harness P1-7 (ZeroClaw), Sonnet 4.6 Harness §2.2

**目标:** 检测 MaxTokens 停止原因后自动续写（最多 3 次，配置在 AgentLoopConfig）。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs` (stream 处理逻辑)
- Test: `crates/octo-engine/tests/max_tokens_continuation.rs`

**实现要点:**
- 检测 `stop_reason == "max_tokens"` 或 provider 返回 MaxTokens
- 自动追加 "Please continue where you left off" 消息
- 最多续写 `config.max_tokens_continuation` 次（默认 3）
- 累计输出字符数不超过 120K（ZeroClaw MAX_TOKENS_CONTINUATION_MAX_OUTPUT_CHARS）

---

### P1-6: force_text_at 最后迭代

**来源:** Opus Harness P0-5 (IronClaw)

**目标:** 当达到 max_iterations - 1 时，强制 text-only 响应（不传 tools 参数给 LLM）。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Test: `crates/octo-engine/tests/force_text_at.rs`

**实现要点:**
- 在 `call_provider_with_retry()` 中检查 `round >= config.max_iterations - 1 && config.force_text_at_last`
- 如果是最后一次，不传 `tools` 参数给 LLM
- 确保 LLM 生成总结性文本而非继续工具调用

---

### P1-7: SkillManager 统一管理入口

**来源:** Opus Skills §5.7

**目标:** 整合 SkillLoader + SkillRegistry + SkillRuntimeBridge + TrustManager。

**Files:**
- Create: `crates/octo-engine/src/skills/manager.rs`
- Modify: `crates/octo-engine/src/skills/mod.rs`
- Test: `crates/octo-engine/tests/skill_manager.rs`

**实现要点:**
- `build_index()` → L1 轻量索引
- `activate_skill()` → L2 按需加载
- `execute_script()` → L3 脚本执行
- `create_skill_tools()` → 生成 SkillTool 列表
- `prompt_section_l1()` / `prompt_section_l2()` → 生成 system prompt 段

---

### P1-8: SafetyPipeline 安全管线

**来源:** Opus Harness §3.7 (IronClaw SafetyLayer)

**目标:** 实现可组合的安全检查管线。

**Files:**
- Create: `crates/octo-engine/src/security/pipeline.rs`
- Modify: `crates/octo-engine/src/security/mod.rs`
- Test: `crates/octo-engine/tests/safety_pipeline.rs`

**实现要点:**
```rust
pub struct SafetyPipeline {
    layers: Vec<Box<dyn SafetyLayer>>,
}

#[async_trait]
pub trait SafetyLayer: Send + Sync {
    async fn check_input(&self, message: &str) -> SafetyDecision;
    async fn check_output(&self, response: &str) -> SafetyDecision;
    async fn check_tool_result(&self, tool: &str, result: &str) -> SafetyDecision;
}

pub enum SafetyDecision {
    Allow,
    Sanitize(String),
    Block(String),
    Warn(String),
}
```

初始层: CredentialScrubber (凭证清理), OutputValidator (输出验证)
集成现有 AIDefence 到 SafetyLayer trait 实现。

---

### P1-9: ContextManager 统一

**来源:** Opus Harness §3.5

**目标:** 合并 SystemPromptBuilder 和 ContextBudgetManager + ContextPruner 为统一 ContextManager。

**Files:**
- Create: `crates/octo-engine/src/context/manager.rs`
- Modify: `crates/octo-engine/src/context/mod.rs`
- Test: `crates/octo-engine/tests/context_manager.rs`

**实现要点:**
- 合并 `builder.rs` 和 `system_prompt.rs` 两套并存
- `TokenCounter` trait: `count(text) -> usize`, `count_messages(messages) -> usize`
- `EstimateCounter`: 英文 chars/4, 中文 chars/1.5 (替换现有 chars/4)

⏳ **Deferred**: 精确 tiktoken-rs 计数 (依赖 P2-6 添加 tiktoken-rs crate)

---

### P1-10: Tool Approval 系统

**来源:** Opus Harness §3.7, Opus Harness P1-1 (IronClaw 三级审批)

**目标:** 实现基于 RiskLevel 的 Tool 审批流程。

**Files:**
- Create: `crates/octo-engine/src/tools/approval.rs`
- Modify: `crates/octo-engine/src/agent/loop_.rs` (集成)
- Test: `crates/octo-engine/tests/tool_approval.rs`

**实现要点:**
```rust
pub struct ApprovalManager {
    policy: ApprovalPolicy,
}

pub enum ApprovalPolicy {
    AlwaysApprove,                    // 开发模式
    SmartApprove(SmartApproveRules),  // 基于规则自动审批
    AlwaysAsk,                        // 生产模式
}
```

在 AgentLoop 工具执行前检查: 如果 `tool.approval() == Always`，发送 `AgentEvent::ApprovalRequired` 等待用户确认。

---

### P1-11: 参数自动类型转换

**来源:** Opus Harness P1-5 (nanobot cast_params)

**目标:** 自动处理 LLM 返回的参数类型不匹配（如字符串 "42" → 整数 42）。

**Files:**
- Create: `crates/octo-engine/src/tools/cast_params.rs`
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Test: `crates/octo-engine/tests/cast_params.rs`

---

### P1-12: 错误后引导提示

**来源:** Opus Harness P1-6 (nanobot)

**目标:** 工具执行失败后，追加引导提示引导 LLM 换策略。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Test: `crates/octo-engine/tests/error_hint.rs`

**实现要点:**
工具返回 error 时，在 tool result 后追加提示:
```
"The tool call failed. Consider alternative approaches:
1. Try a different tool
2. Modify the parameters
3. Ask the user for clarification"
```

---

## P2: Provider 增强 + Skills 高级

> Provider 装饰器链、SkillSelector 4阶段管线、多运行时、Skills↔Context 集成。

### P2-1: Provider 装饰器链 (ProviderPipelineBuilder)

**来源:** Opus Harness §3.4 (IronClaw 6层装饰器链)

**目标:** 实现 Raw → Retry → Failover → CircuitBreaker → CostGuard → Recording 管线。

**Files:**
- Create: `crates/octo-engine/src/providers/pipeline.rs`
- Create: `crates/octo-engine/src/providers/circuit_breaker.rs`
- Create: `crates/octo-engine/src/providers/cost_guard.rs`
- Modify: `crates/octo-engine/src/providers/mod.rs`
- Test: `crates/octo-engine/tests/provider_pipeline.rs`

**实现要点:**
```rust
let provider = ProviderPipelineBuilder::new(AnthropicProvider::new(config))
    .with_retry(RetryPolicy::exponential(3, Duration::from_secs(1)))
    .with_failover(vec![Box::new(OpenAIProvider::new(fallback_config))])
    .with_circuit_breaker(CircuitBreakerConfig::default())
    .with_cost_guard(CostBudget::daily(10.0))
    .build();
```

---

### P2-2: NormalizedStopReason 统一

**来源:** Opus Harness P2-2 (ZeroClaw)

**目标:** 统一各 Provider 的停止原因为 StopReason 枚举。

**Files:**
- Modify: `crates/octo-engine/src/providers/anthropic.rs`
- Modify: `crates/octo-engine/src/providers/openai.rs`
- Test: `crates/octo-engine/tests/stop_reason.rs`

**实现要点:**
- Anthropic `stop_reason: "end_turn"` → `StopReason::EndTurn`
- Anthropic `stop_reason: "tool_use"` → `StopReason::ToolCall`
- Anthropic `stop_reason: "max_tokens"` → `StopReason::MaxTokens`
- OpenAI `finish_reason: "stop"` → `StopReason::EndTurn`
- OpenAI `finish_reason: "tool_calls"` → `StopReason::ToolCall`
- OpenAI `finish_reason: "length"` → `StopReason::MaxTokens`

---

### P2-3: SkillSelector 4阶段选择管线

**来源:** Sonnet 4.6 Skills §5 (ironclaw Gate→Score→Budget→Attenuate)

**目标:** 实现确定性的 Skill 选择管线。

**Files:**
- Create: `crates/octo-engine/src/skills/selector.rs`
- Test: `crates/octo-engine/tests/skill_selector.rs`

**实现要点:**
- Phase 1 **Gate**: 检查 bins/env 前置条件 (SkillGate)
- Phase 2 **Score**: always(1000) > slash_command(900) > regex(+20) > keyword(+10) (SkillScorer)
- Phase 3 **Budget**: 按分数降序选取，累计不超过 token 预算 (SkillBudget)
- Phase 4 **Attenuate**: 按信任等级裁剪工具访问 (SkillAttenuator，复用 TrustManager)

统一入口:
```rust
pub struct SkillSelector {
    gate: SkillGate,
    scorer: SkillScorer,
    budget: SkillBudget,
    attenuator: SkillAttenuator,
}
```

---

### P2-4: NodeJS Runtime 实现

**来源:** Opus Skills §5.10, Sonnet 4.6 Skills §7.2

**目标:** 实现 NodeJS 脚本运行时。

**Files:**
- Create: `crates/octo-engine/src/skill_runtime/nodejs.rs`
- Modify: `crates/octo-engine/src/skills/runtime_bridge.rs`
- Test: `crates/octo-engine/tests/nodejs_runtime.rs`

**实现要点:**
- `which::which("node")` 检测可用性
- 环境变量注入: `SKILL_NAME`, `SKILL_ARGS` (JSON), `SKILL_BASE_DIR`
- `tokio::process::Command::new("node")` 执行
- 超时控制: `tokio::time::timeout()`

---

### P2-5: Shell Runtime 实现

**来源:** Opus Skills §5.10

**目标:** 实现 Shell/Bash 脚本运行时。

**Files:**
- Create: `crates/octo-engine/src/skill_runtime/shell.rs`
- Modify: `crates/octo-engine/src/skills/runtime_bridge.rs`
- Test: `crates/octo-engine/tests/shell_runtime.rs`

---

### P2-6: 精确 Token 计数 (TokenCounter trait)

**来源:** Opus Harness P2-6 (tiktoken-rs)

**目标:** 替换 chars/4 估算，至少区分中英文。

**Files:**
- Create: `crates/octo-engine/src/context/token_counter.rs`
- Modify: `crates/octo-engine/src/context/manager.rs` (使用 TokenCounter)
- Modify: `Cargo.toml` (可选依赖 tiktoken-rs)
- Test: `crates/octo-engine/tests/token_counter.rs`

**实现要点:**
```rust
pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
    fn count_messages(&self, messages: &[Message]) -> usize;
}

// 默认实现（无外部依赖）
pub struct EstimateCounter;
impl TokenCounter for EstimateCounter {
    fn count(&self, text: &str) -> usize {
        // 英文: chars/4, 中文: chars/1.5
        text.chars().map(|c| if c.is_ascii() { 1 } else { 3 }).sum::<usize>() / 4
    }
}
```

---

### P2-7: Skills ↔ Context 集成

**来源:** Opus Skills §5.8, Sonnet 4.6 Skills §3.2

**目标:** L1 prompt section 集成到 SystemPromptBuilder, always 标记的 Skill 在 compact 时保留。

**Files:**
- Modify: `crates/octo-engine/src/context/builder.rs` (或 system_prompt.rs)
- Modify: `crates/octo-engine/src/context/pruner.rs`
- Test: `crates/octo-engine/tests/skill_context_integration.rs`

**实现要点:**
- SystemPromptBuilder 调用 `skill_manager.prompt_section_l1()` 添加 `<available-skills>` 列表
- ContextPruner 检查 `skill.always` 标记，不裁剪标记为 always 的 skill 内容
- 激活 skill 时调用 `prompt_section_l2()` 注入完整 body 到上下文

---

### P2-8: ToolConstraintEnforcer (Sonnet 4.6 设计)

**来源:** Sonnet 4.6 Skills §6

**目标:** 从激活的 skills 构建约束合集，支持 glob 通配 (mcp:server:*)。

**Files:**
- Create: `crates/octo-engine/src/skills/constraint.rs`
- Test: `crates/octo-engine/tests/tool_constraint.rs`

**实现要点:**
- `ToolConstraintEnforcer::from_active_skills()` — 合并多个 skill 的 allowed/denied
- `check(tool_name)` → `ConstraintResult::Allowed | Denied(reason)`
- 支持 `mcp:server-name:*` glob 通配格式
- 空 skill 时不限制（向后兼容）

---

### P2-9: Skill 依赖图

**来源:** Opus Skills §5.9, Sonnet 4.6 Skills §8

**目标:** 实现 depends_on 字段的 DAG 依赖解析。

**Files:**
- Create: `crates/octo-engine/src/skills/dependency.rs`
- Test: `crates/octo-engine/tests/skill_dependency.rs`

**实现要点:**
- `SkillDependencyGraph::build()` — 从 skills 构建 DAG，检测循环依赖 (Kahn 算法)
- `resolve_with_deps()` — 拓扑排序返回包含依赖的激活列表
- 循环依赖返回 `SkillError::CyclicDependency`

---

### P2-10: Skill 斜杠命令路由

**来源:** Sonnet 4.6 Skills §11 (localgpt slash command)

**目标:** 支持 `/skill-name` 快速激活语法。

**Files:**
- Create: `crates/octo-engine/src/skills/slash_router.rs`
- Test: `crates/octo-engine/tests/skill_slash_router.rs`

**实现要点:**
- `SkillSlashRouter::build(skills)` — 扫描所有 skill 的 `activation.slash_command`
- `route(message)` → `Option<skill_name>` — 检查消息是否是斜杠命令
- 支持 `/skill-name arg1 arg2` 格式

---

## P3: 生态 + 高级功能

> SubAgent、远程 Registry、Observation Masking、高级 MCP 集成。

### P3-1: SubAgent 支持

**来源:** Opus Harness §3.6 (Goose/Moltis), Sonnet 4.6 Harness §2.6 (localgpt spawn)

**目标:** 支持独立上下文中执行子任务。

**Files:**
- Create: `crates/octo-engine/src/agent/subagent.rs`
- Test: `crates/octo-engine/tests/subagent.rs`

**实现要点:**
```rust
pub struct SubAgentManager {
    runtime: Arc<AgentRuntime>,
    active_agents: DashMap<String, SubAgentHandle>,
    max_concurrent: usize,
    max_depth: usize,  // 防止无限递归 (localgpt/nanobot)
}

pub struct SubAgentTask {
    pub description: String,
    pub context: Vec<ChatMessage>,
    pub tools: Option<Vec<String>>,  // 工具白名单
    pub max_iterations: u32,
}
```

---

### P3-2: Observation Masking

**来源:** Opus Harness P3-2 (JetBrains Research)

**目标:** 选择性遮蔽旧轮次的 tool output，保留 action/reasoning，减少 token 消耗。

**Files:**
- Create: `crates/octo-engine/src/context/observation_masker.rs`
- Test: `crates/octo-engine/tests/observation_masking.rs`

**实现要点:**
- 保留最近 N 轮完整输出
- 旧轮次: 保留 tool 名称和参数（action），遮蔽 tool 结果（observation）
- 替换为 `[output hidden - N chars]` 占位符

---

### P3-3: context-fork 实现

**来源:** Opus Skills §5.3, Opus Skills P2-13

**目标:** Skill 的 context_fork=true 时在独立上下文中执行。

**Files:**
- Modify: `crates/octo-engine/src/agent/executor.rs`
- Test: `crates/octo-engine/tests/context_fork.rs`

---

### P3-4: Skill model 覆盖

**来源:** Opus Skills P2-14

**目标:** Skill 可以通过 model 字段指定使用的 Provider/Model。

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`
- Test: `crates/octo-engine/tests/skill_model_override.rs`

---

### P3-5: WASM Skill Runtime

**来源:** Opus Skills P3-21, Sonnet 4.6 Skills §7.3

**目标:** 利用已有 Wasmtime 基础设施实现 Skill WASM 运行时。

**Files:**
- Create: `crates/octo-engine/src/skill_runtime/wasm.rs`
- Modify: `crates/octo-engine/src/skills/runtime_bridge.rs`
- Test: `crates/octo-engine/tests/wasm_skill_runtime.rs`

---

### P3-6: Skill REST API

**来源:** Opus Skills P1-12, Sonnet 4.6 Skills §13.4

**目标:** `/api/v1/skills` CRUD 端点。

**Files:**
- Create: `crates/octo-server/src/api/skills.rs`
- Modify: `crates/octo-server/src/router.rs`
- Test: 手动验证（Runtime Verification）

---

### P3-7: SkillCatalog — 远程 Registry

**来源:** Sonnet 4.6 Skills §12 (ClawHub), Opus Skills P3-19

**目标:** 支持远程 Skill 搜索和安装。

**Files:**
- Create: `crates/octo-engine/src/skills/catalog.rs`
- Test: `crates/octo-engine/tests/skill_catalog.rs`

---

### P3-8: MCP Tool Annotations 解析

**来源:** Opus Harness P1-2 (MCP 2025 规范)

**目标:** 解析 MCP 工具的 read-only/destructive 标注，映射到 RiskLevel。

**Files:**
- Modify: `crates/octo-engine/src/mcp/tool_bridge.rs`
- Test: `crates/octo-engine/tests/mcp_annotations.rs`

---

### P3-9: Skill 向量语义索引

**来源:** Sonnet 4.6 Skills §9 (HybridQueryEngine)

**目标:** 利用 HybridQueryEngine 为 skills 构建语义搜索。

**Files:**
- Create: `crates/octo-engine/src/skills/semantic_index.rs`
- Test: `crates/octo-engine/tests/skill_semantic_index.rs`

---

### P3-10: Skill 生命周期 Hooks

**来源:** Sonnet 4.6 Skills §10

**目标:** 在 HookPoint 中添加 Skill 特定事件。

**Files:**
- Modify: `crates/octo-engine/src/hooks/mod.rs`
- Test: `crates/octo-engine/tests/skill_hooks.rs`

**实现要点:**
新增 HookContext 中的 skill 事件:
- `SkillsActivated { skills, query }`
- `SkillDeactivated { skill_name }`
- `SkillScriptStarted { skill_name, script, runtime }`
- `ToolConstraintViolated { tool_name, skill_name, reason }`

---

## 设计文档覆盖追踪

### Opus Harness (AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md)

| 编号 | 设计项 | 计划任务 | 状态 |
|------|--------|---------|------|
| P0-1 | AgentLoop 纯函数式拆分 | P0-2 | 覆盖 |
| P0-2 | AgentEvent Stream 返回值 | P0-3 | 覆盖 |
| P0-3 | AgentResult 结构化返回 | P0-3 | 覆盖 |
| P0-4 | tool_timeout_secs 启用 | P0-1 (AgentLoopConfig) | 覆盖 |
| P0-5 | 并行 Tool 执行时调用 Hooks | P0-2 (step functions) | 覆盖 |
| P0-6 | 合并 Extension 和 Hook 系统 | Deferred (P3+) | 低优先 |
| P1-1 | Tool Approval 三级审批 | P1-10 | 覆盖 |
| P1-2 | Tool RiskLevel + Annotations | P1-1 | 覆盖 |
| P1-3 | ToolOutput 结构化 | P1-2 | 覆盖 |
| P1-4 | 工具输出截断统一 | P1-3 | 覆盖 |
| P1-5 | 参数自动类型转换 | P1-11 | 覆盖 |
| P1-6 | 错误后引导提示 | P1-12 | 覆盖 |
| P1-7 | max-tokens 自动续写 | P1-5 | 覆盖 |
| P1-8 | Canary Token | Deferred (P3+) | 低优先 |
| P2-1 | Provider 装饰器链 | P2-1 | 覆盖 |
| P2-2 | NormalizedStopReason | P2-2 | 覆盖 |
| P2-3 | SmartRouting | Deferred (P2-1 后可选) | 可选 |
| P2-4 | CostGuard 预算 | P2-1 (含在 Pipeline) | 覆盖 |
| P2-5 | stream() failover | Deferred | 中期 |
| P2-6 | 精确 Token 计数 | P2-6 | 覆盖 |
| P3-1 | SubAgent 支持 | P3-1 | 覆盖 |
| P3-2 | Observation Masking | P3-2 | 覆盖 |
| P3-3 | Auto-Compact with LLM Summary | Deferred | 中期 |
| P3-4 | Memory Decay | Deferred | 低优先 |
| P3-9 | Deferred Action 检测 | Deferred | 低优先 |
| P3-10 | ToolProgress 实时进度 | Deferred | 低优先 |

### Opus Skills (AGENT_SKILLS_BEST_IMPLEMENTATION_DESIGN.md)

| 编号 | 设计项 | 计划任务 | 状态 |
|------|--------|---------|------|
| P0-1 | 修复 SkillTool 断联 | P0-5 | 覆盖 |
| P0-2 | SkillDefinition 增强 | P0-4 | 覆盖 |
| P0-3 | TrustManager | P0-6 | 覆盖 |
| P0-4 | allowed-tools 运行时强制 | P0-7 | 覆盖 |
| P0-5 | SkillManager | P1-7 | 覆盖 |
| P1-6 | NodeJS Runtime | P2-4 | 覆盖 |
| P1-7 | Shell Runtime | P2-5 | 覆盖 |
| P1-8 | Context 集成 (always + compact) | P2-7 | 覆盖 |
| P1-9 | 跨组件调用 | Deferred (P3+) | 复杂 |
| P1-10 | 脚本超时 | P2-4/P2-5 内含 | 覆盖 |
| P1-11 | Symlink 防护 | Deferred | 安全增强 |
| P1-12 | REST API | P3-6 | 覆盖 |
| P2-13 | context-fork | P3-3 | 覆盖 |
| P2-14 | model 覆盖 | P3-4 | 覆盖 |
| P2-15 | 触发器系统 | P0-4 (triggers 字段) + P2-3 (Selector) | 覆盖 |
| P2-18 | 依赖管理 | P2-9 | 覆盖 |
| P3-19 | 远程 Registry | P3-7 | 覆盖 |
| P3-21 | WASM Runtime | P3-5 | 覆盖 |

### Sonnet 4.6 Harness (AGENT_HARNESS_DESIGN.md)

| 设计项 | 计划任务 | 状态 |
|--------|---------|------|
| ironclaw AgentDeps | P0-1 (AgentLoopConfig) | 覆盖 |
| TurnGate (localgpt) | P0-8 | 覆盖 |
| 错误不持久化 (nanobot) | P0-9 | 覆盖 |
| ProviderErrorKind 路由 (moltis) | P0-10 | 覆盖 |
| HookFailureMode (autoagents) | P1-4 | 覆盖 |
| FailoverProvider 链 (localgpt) | P2-1 (Pipeline) | 覆盖 |
| CircuitBreaker (moltis) | P2-1 (Pipeline) | 覆盖 |
| 强类型 ChatMessage (moltis) | Deferred (P3+) | 大规模重构 |
| 9-hook + Abort (autoagents) | 现有 10 HookPoint 已覆盖 | 已有 |
| 三级上下文压缩 (ironclaw) | 现有 4+1 阶段已优于此 | 已有 |
| 循环检测 (zeroclaw) | 现有 LoopGuard 已覆盖 | 已有 |
| CostGuard (ironclaw) | P2-1 (Pipeline) | 覆盖 |
| Hands 能力包 (openfang) | Deferred | 低优先 |
| Merkle 审计链 (openfang) | Deferred | 低优先 |

### Sonnet 4.6 Skills (AGENT_SKILLS_DESIGN.md)

| 设计项 | 计划任务 | 状态 |
|--------|---------|------|
| SkillSelector 4阶段管线 | P2-3 | 覆盖 |
| ToolConstraintEnforcer | P2-8 | 覆盖 |
| 多运行时 (Node/WASM/Shell) | P2-4, P2-5, P3-5 | 覆盖 |
| Skill 依赖图 | P2-9 | 覆盖 |
| Skill 斜杠命令路由 | P2-10 | 覆盖 |
| SkillCatalog 远程注册表 | P3-7 | 覆盖 |
| Skill 向量语义索引 | P3-9 | 覆盖 |
| Skill 生命周期 Hooks | P3-10 | 覆盖 |
| skill_invoke 跨 skill 工具 | Deferred (P3+) | 复杂 |
| HybridQueryEngine 集成 | P3-9 | 覆盖 |

---

## Deferred Items 汇总

| 项目 | 前置条件 | 优先级 |
|------|---------|--------|
| 合并 Extension 和 Hook 系统 | P1-4 完成后评估 | 低 |
| Canary Token (prompt exfiltration) | P1-8 SafetyPipeline | 低 |
| SmartRouting (简单查询到廉价模型) | P2-1 Provider Pipeline | 可选 |
| stream() failover 支持 | P2-1 Provider Pipeline | 中 |
| Auto-Compact with LLM Summary | P1-9 ContextManager | 中 |
| Memory Decay 机制 | P1-9 完成后 | 低 |
| Deferred Action 检测 | P0-2 step functions | 低 |
| ToolProgress 实时进度 | P1-2 ToolOutput | 低 |
| 强类型 ChatMessage 枚举 | 大规模重构，全量迁移 | 低 |
| Hands 能力包 (openfang) | AgentHarness 完成后 | 低 |
| Merkle 审计链 (openfang) | 审计系统增强时 | 低 |
| 跨组件调用 (ToolInvoker/SkillInvoker) | P1-7 SkillManager | 复杂 |
| Symlink 防护增强 | 安全审计时 | 安全 |
| skill_invoke 跨 skill 工具 | P2-9 依赖图 | 复杂 |
| 精确 tiktoken-rs 计数 | P2-6 添加依赖后 | 可选 |

---

## 执行建议

### 推荐执行顺序

```
P0 (10 tasks) --- 基础必须 ---
  P0-1  AgentLoopConfig
  P0-4  SkillDefinition 增强字段
  P0-2  AgentLoop Step Functions 提取
  P0-3  AgentEvent 增强枚举
  P0-5  SkillTool 断联修复
  P0-6  TrustManager
  P0-7  ToolCallInterceptor
  P0-8  TurnGate
  P0-9  错误不持久化
  P0-10 ProviderErrorKind 路由

P1 (12 tasks) --- 安全+工具 ---
  P1-1  Tool trait risk_level + approval
  P1-2  ToolOutput 结构化
  P1-3  工具输出截断
  P1-4  HookFailureMode
  P1-5  max-tokens 续写
  P1-6  force_text_at
  P1-7  SkillManager
  P1-8  SafetyPipeline
  P1-9  ContextManager 统一
  P1-10 Tool Approval 系统
  P1-11 cast_params
  P1-12 错误引导提示

P2 (10 tasks) --- Provider+Skills ---
  P2-1  Provider 装饰器链
  P2-2  NormalizedStopReason
  P2-3  SkillSelector 4阶段
  P2-4  NodeJS Runtime
  P2-5  Shell Runtime
  P2-6  TokenCounter trait
  P2-7  Skills<>Context 集成
  P2-8  ToolConstraintEnforcer
  P2-9  Skill 依赖图
  P2-10 Skill 斜杠命令路由

P3 (10 tasks) --- 生态+高级 ---
  P3-1 ~ P3-10
```

### 关键依赖关系

```
P0-4 --> P0-5 --> P0-6 --> P0-7
         (SkillDef -> SkillTool -> TrustManager -> Interceptor)

P0-1 --> P0-2
         (Config -> Step Functions)

P0-3 --> P1-5, P1-6
         (AgentEvent/StopReason -> max-tokens/force_text)

P1-1 --> P1-2 --> P1-3
         (RiskLevel -> ToolOutput -> Truncation)

P1-1 --> P1-10
         (RiskLevel/Approval -> ApprovalManager)

P0-10 --> P2-1
          (ErrorKind -> Provider Pipeline)

P0-6 --> P2-3
         (TrustManager -> SkillSelector Attenuate)
```

### 每阶段验证命令

```bash
# 每个 task 完成后
cargo check --workspace
cargo test --workspace -- --test-threads=1

# 每个阶段完成后
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```
