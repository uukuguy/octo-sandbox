# Phase AH — Hook 系统增强：三层混合架构 实施方案

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 octo 的 hook 系统从"空转框架"升级为三层混合架构（编程式 + 策略引擎 + 声明式），支持多语言扩展和完整环境上下文传递，同时满足企业平台和个人客户端的差异化需求。

**Architecture:** 三层 Hook 执行栈——Layer 1 编程式（Rust trait，平台核心）→ Layer 2 策略引擎（规则配置，零代码）→ Layer 3 声明式（hooks.yaml，prompt/command/webhook 三类型），任一层 Deny 则终止。通过统一的 `HookExecutionContext` 结构体在所有层间传递完整环境上下文，声明式 hook 通过 env vars + stdin JSON + 模板变量三通道暴露上下文。

**Tech Stack:** Rust (octo-engine), serde_yaml, serde_json, tokio::process (command 类型), reqwest (webhook 类型), regex (matcher)

---

## 设计背景

### 问题现状

1. `HookRegistry` 框架完备（14 个 HookPoint），但**生产代码中无任何 handler 注册**，所有 hook 调用点"空转"
2. Hook 仅支持 Rust trait 编程式，**企业管理员和终端用户无法自行扩展**
3. `HookContext` 缺少沙箱模式、安全策略、工作目录等**关键环境上下文**
4. 无声明式配置能力，与业界主流（CC hooks、K8s Admission Webhooks、NVIDIA OpenShell）差距明显

### 业界调研结论

| 框架 | Hook 模型 | 核心特点 |
|------|----------|---------|
| **Claude Code** | 声明式 JSON (`settings.json`) | `prompt`（LLM 自评估）+ `command`（shell 脚本），6 个事件 |
| **NVIDIA OpenShell** | 策略驱动 guardrails | 运行时强制执行安全策略，policy-based |
| **K8s Admission Webhooks** | 声明式 + HTTP 回调 | Mutating + Validating 双阶段，webhook 模式 |
| **LangGraph** | middleware 链 | stateful 上下文传递，可组合 |
| **CrewAI** | 装饰器 + 回调 | 事件驱动，@listen/@router |

### 设计原则

- **不同技能的人在不同层操作**：管理员→策略配置，运维→脚本/webhook，终端用户→prompt，平台开发者→Rust
- **渐进式增强**：每个阶段独立可用，不依赖后续阶段
- **上下文完备性**：任何类型的 hook 都能获取完整环境信息
- **安全优先**：任一层 Deny 即终止，FailOpen/FailClosed 可配

---

## 三层架构总览

```
┌───────────────────────────────────────────────────────────────┐
│                     Layer 3: 声明式 Hook                       │
│           （hooks.yaml，面向管理员/运维/终端用户）                 │
│                                                               │
│   ┌─ prompt 类型 ── LLM 自评估（自然语言描述安全规则）            │
│   ├─ command 类型 ── 执行外部脚本（Python/JS/Go/Shell 任何语言）  │
│   └─ webhook 类型 ── HTTP 回调（企业审计/合规微服务）             │
├───────────────────────────────────────────────────────────────┤
│                     Layer 2: 策略引擎                          │
│             （policies.yaml，面向平台运维，零代码）                │
│                                                               │
│   ┌─ 内置规则 ── 路径黑名单、命令风险分级、频率限制               │
│   └─ 条件匹配 ── JSONPath 风格表达式匹配上下文                   │
├───────────────────────────────────────────────────────────────┤
│                     Layer 1: 编程式 Hook                       │
│           （Rust HookHandler trait，面向平台开发者）              │
│                                                               │
│   └─ 内置 handler ── SecurityPolicyHandler, AuditLogHandler    │
│   └─ 桥接 handler ── DeclarativeHookBridge, PolicyEngineBridge │
└───────────────────────────────────────────────────────────────┘

执行顺序: Layer 1 → Layer 2 → Layer 3，任一层 Deny 则终止
```

---

## Hook 事件分类

| 分组 | 事件 | 支持 Layer | 说明 |
|------|------|-----------|------|
| **工具生命周期** | PreToolUse, PostToolUse | L1+L2+L3 | 安全关键，最常用 |
| **任务生命周期** | PreTask, PostTask, LoopTurnStart, LoopTurnEnd | L1+L2+L3 | 流程控制 |
| **会话生命周期** | SessionStart, SessionEnd | L1+L2+L3 | 审计、初始化 |
| **智能体路由** | AgentRoute | L1+L2 | 需编程灵活性 |
| **Skill 管理** | SkillsActivated, SkillDeactivated, SkillScriptStarted, ToolConstraintViolated | L1 | 内部机制 |
| **上下文管理** | ContextDegraded | L1+L2 | 可观测性 |

---

## 环境上下文传递协议

### 统一上下文结构（`HookExecutionContext`）

所有层共享此结构，由 harness.rs 在每次 hook 调用前构建：

```rust
pub struct HookExecutionContext {
    // === 事件信息 ===
    pub event: HookPoint,
    pub session_id: String,
    pub user_id: String,
    pub agent_id: Option<String>,
    pub turn: u32,

    // === 工具信息（PreToolUse/PostToolUse 时填充）===
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_result: Option<ToolResultContext>,  // PostToolUse 时

    // === 运行环境 ===
    pub working_dir: String,
    pub sandbox_mode: String,          // "host" | "docker" | "wasm"
    pub sandbox_profile: String,       // "development" | "staging" | "production"
    pub model: String,
    pub autonomy_level: String,        // "full" | "supervised" | "restricted"

    // === Skill 信息 ===
    pub active_skill: Option<String>,

    // === 历史信息 ===
    pub total_tool_calls: u32,
    pub current_round: u32,
    pub recent_tools: Vec<String>,     // 最近 N 次工具调用名

    // === 扩展 ===
    pub metadata: HashMap<String, serde_json::Value>,
}

pub struct ToolResultContext {
    pub success: bool,
    pub output: String,          // 截断到前 1KB
    pub duration_ms: u64,
}
```

### 各类型 Hook 的上下文暴露方式

| 类型 | 传递方式 | 说明 |
|------|---------|------|
| **Rust trait** | 直接访问 `HookExecutionContext` | 完整结构体引用 |
| **command** | 环境变量（快速判断）+ stdin JSON（完整数据） | 双通道 |
| **prompt** | 模板变量 `{{tool_name}}` + 自动附加 JSON | LLM 友好 |
| **webhook** | HTTP POST body JSON | RESTful |
| **策略规则** | 条件表达式 `context.sandbox_profile == "production"` | 声明式 |

### Command 类型环境变量

```bash
OCTO_HOOK_EVENT=PreToolUse
OCTO_TOOL_NAME=bash
OCTO_SESSION_ID=sess_abc123
OCTO_USER_ID=user_001
OCTO_AGENT_ID=agent_default
OCTO_TURN=3
OCTO_WORKING_DIR=/home/user/proj
OCTO_SANDBOX_MODE=host
OCTO_SANDBOX_PROFILE=development
OCTO_AUTONOMY_LEVEL=supervised
```

### Command 类型 stdin JSON

```json
{
  "event": "PreToolUse",
  "session_id": "sess_abc123",
  "user_id": "user_001",
  "turn": 3,
  "tool": {
    "name": "bash",
    "input": { "command": "rm -rf /tmp/old_data", "timeout": 30 }
  },
  "context": {
    "working_dir": "/home/user/project",
    "sandbox_mode": "host",
    "sandbox_profile": "development",
    "model": "claude-sonnet-4-20250514",
    "active_skill": "code-review",
    "autonomy_level": "supervised"
  },
  "history": {
    "total_tool_calls": 12,
    "current_round": 3,
    "recent_tools": ["file_read", "bash", "file_write"]
  }
}
```

### Command 类型 stdout 返回协议

```json
{
  "decision": "allow|deny|ask",
  "reason": "Explanation for the decision",
  "updatedInput": { "command": "modified command" },
  "systemMessage": "Message injected into agent context"
}
```

- `decision` (必填): `allow` 放行, `deny` 拒绝, `ask` 请求人工确认
- `reason` (可选): 决策原因
- `updatedInput` (可选): 修改后的工具输入（Mutating hook）
- `systemMessage` (可选): 注入到 agent 上下文的消息
- exit code 0 = 使用 stdout JSON; exit code 2 = 错误反馈; 其他非零 = 根据 failure_mode 处理

### Prompt 类型模板变量

```yaml
prompt: |
  Evaluate this {{event}} for tool "{{tool_name}}":
  Command: {{tool_input.command}}
  Working Dir: {{context.working_dir}}
  Sandbox: {{context.sandbox_profile}}
  Autonomy: {{context.autonomy_level}}
  Recent tools: {{history.recent_tools}}
  Return JSON: {"decision": "allow|deny", "reason": "..."}
```

未使用模板变量时，框架自动将完整上下文 JSON 附加到 prompt 末尾。

---

## 声明式配置格式（hooks.yaml）

### 文件位置与加载优先级

1. 平台内置默认（编译期嵌入）— 最低
2. `~/.octo/hooks.yaml` — 全局
3. `$PROJECT/.octo/hooks.yaml` — 项目级
4. 环境变量 `OCTO_HOOKS_FILE` — 覆盖

**合并规则**: 同一 HookPoint 下的 matcher 列表合并（项目级追加到全局之后），同一 matcher 下的 actions 按声明顺序执行。

### 配置示例

```yaml
version: 1

hooks:
  PreToolUse:
    - matcher: "bash|shell_execute"
      actions:
        - type: prompt
          prompt: |
            Evaluate bash command safety: destructive operations,
            privilege escalation, network exfiltration.
            Return JSON: {"decision": "allow|deny", "reason": "..."}
          timeout: 10
        - type: command
          command: "python3 ~/.octo/hooks/validate_bash.py"
          timeout: 5
          failure_mode: fail_closed

    - matcher: "file_write|file_edit"
      actions:
        - type: prompt
          prompt: "Check file path safety: no system paths, no credentials, no path traversal."

  PostToolUse:
    - matcher: "bash"
      actions:
        - type: command
          command: "bash ~/.octo/hooks/audit_log.sh"
          failure_mode: fail_open

    - matcher: "*"
      actions:
        - type: webhook
          url: "https://audit.corp.example.com/api/tool-execution"
          method: POST
          timeout: 5
          failure_mode: fail_open

  Stop:
    - matcher: "*"
      actions:
        - type: prompt
          prompt: "Verify all requested tasks are completed. If code was modified, confirm tests were run."

  SessionStart:
    - matcher: "*"
      actions:
        - type: command
          command: "bash ~/.octo/hooks/session_init.sh"
          failure_mode: fail_open
```

### 策略配置格式（policies.yaml）

```yaml
version: 1

policies:
  - name: path_safety
    enabled: true
    hooks: [PreToolUse]
    matcher: "file_write|file_edit|file_read"
    rules:
      - deny_paths: ["/etc", "/sys", "/proc", "~/.ssh", "~/.gnupg"]
      - deny_patterns: ["**/credentials*", "**/.env*", "**/*.pem", "**/*.key"]

  - name: command_safety
    enabled: true
    hooks: [PreToolUse]
    matcher: "bash|shell_execute"
    rules:
      - deny_commands: ["rm -rf /", "mkfs", "dd if=/dev/zero", "> /dev/sda"]
      - require_approval: ["sudo *", "docker run *", "curl * | bash", "chmod 777 *"]

  - name: rate_limits
    enabled: true
    hooks: [PreToolUse]
    matcher: "*"
    rules:
      - tool: "bash"
        max_per_minute: 30
      - tool: "file_write"
        max_per_minute: 60
      - tool: "*"
        max_per_minute: 120

  - name: production_lockdown
    enabled: true
    hooks: [PreToolUse]
    condition: "context.sandbox_profile == 'production'"
    rules:
      - deny_tools: ["file_write", "file_edit", "bash"]
        message: "Production sandbox: write operations blocked"
```

---

## 企业平台 vs 个人客户端差异

| 维度 | 企业平台 (octo-platform-server) | 个人客户端 (octo-cli) |
|------|------|------|
| **配置来源** | 平台策略(不可覆盖) > 租户策略 > 用户配置 | 全局 > 项目级 |
| **webhook 类型** | 完整支持（审计合规必需） | 可选（需用户配置 URL） |
| **prompt 类型** | 支持（有 token 预算控制） | 支持 |
| **策略强制性** | 管理员策略**不可覆盖** | 用户完全自主 |
| **审计日志** | PostToolUse 审计**默认启用** | 可选 |
| **默认策略** | path_safety + command_safety + rate_limits 默认开启 | 仅 path_safety 默认开启 |

---

## 实施计划

### 分组概述

| 分组 | 任务数 | 核心目标 |
|------|-------|---------|
| **G1: HookContext 增强** | 3 个任务 | 扩展上下文字段 + 构建 HookExecutionContext |
| **G2: 内置 Handler 注册 (P0)** | 3 个任务 | SecurityPolicyHandler + AuditLogHandler + 注册接线 |
| **G3: 声明式加载与 Command 执行 (P1)** | 4 个任务 | hooks.yaml 解析 + command 执行器 + 桥接 handler |
| **G4: Prompt 类型 LLM 评估 (P2)** | 2 个任务 | 模板渲染 + LLM 调用 + 决策解析 |
| **G5: 策略引擎 (P3)** | 3 个任务 | policies.yaml 解析 + 规则匹配 + PolicyEngineBridge |

**Deferred (不在本阶段):**
- AH-D1: Webhook 类型实现 (P4)
- AH-D2: WASM 插件 hook (P5/未来)
- AH-D3: 平台租户策略合并逻辑 (需 octo-platform-server 推进)
- AH-D4: TUI hook 状态面板（显示已注册 hooks、执行统计）
- AH-D5: Stop / SubagentStop 事件支持

---

### G1: HookContext 增强（3 个任务）

#### Task 1: 扩展 HookContext 字段

**Files:**
- Modify: `crates/octo-engine/src/hooks/context.rs`
- Test: `crates/octo-engine/src/hooks/context.rs` (内联 tests)

**Step 1: 添加环境上下文字段**

在 `HookContext` 中添加：

```rust
// --- 运行环境（新增）---
/// Working directory for the current session.
pub working_dir: Option<String>,
/// Sandbox mode: "host", "docker", "wasm".
pub sandbox_mode: Option<String>,
/// Sandbox profile: "development", "staging", "production", "custom".
pub sandbox_profile: Option<String>,
/// LLM model name.
pub model: Option<String>,
/// Autonomy level: "full", "supervised", "restricted".
pub autonomy_level: Option<String>,

// --- 历史信息（新增）---
/// Total tool calls so far in this session turn.
pub total_tool_calls: Option<u32>,
/// Current round number.
pub current_round: Option<u32>,
/// Recent tool names (last N calls).
pub recent_tools: Option<Vec<String>>,

// --- 用户输入（新增）---
/// The user's original query for this turn.
pub user_query: Option<String>,
```

**Step 2: 添加 builder 方法**

```rust
pub fn with_environment(
    mut self,
    working_dir: &str,
    sandbox_mode: &str,
    sandbox_profile: &str,
    model: &str,
    autonomy_level: &str,
) -> Self {
    self.working_dir = Some(working_dir.to_string());
    self.sandbox_mode = Some(sandbox_mode.to_string());
    self.sandbox_profile = Some(sandbox_profile.to_string());
    self.model = Some(model.to_string());
    self.autonomy_level = Some(autonomy_level.to_string());
    self
}

pub fn with_history(mut self, total_calls: u32, round: u32, recent: Vec<String>) -> Self {
    self.total_tool_calls = Some(total_calls);
    self.current_round = Some(round);
    self.recent_tools = Some(recent);
    self
}

pub fn with_user_query(mut self, query: impl Into<String>) -> Self {
    self.user_query = Some(query.into());
    self
}
```

**Step 3: 写测试**

```rust
#[test]
fn test_hook_context_environment() {
    let ctx = HookContext::new()
        .with_session("s1")
        .with_environment("host", "/tmp/proj", "development", "claude-sonnet", "supervised");
    assert_eq!(ctx.sandbox_mode.as_deref(), Some("host"));
    assert_eq!(ctx.working_dir.as_deref(), Some("/tmp/proj"));
}

#[test]
fn test_hook_context_history() {
    let ctx = HookContext::new()
        .with_history(12, 3, vec!["bash".into(), "file_read".into()]);
    assert_eq!(ctx.total_tool_calls, Some(12));
    assert_eq!(ctx.recent_tools.as_ref().unwrap().len(), 2);
}
```

**Step 4: 运行测试**

Run: `cargo test -p octo-engine -- hooks::context --test-threads=1`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/octo-engine/src/hooks/context.rs
git commit -m "feat(hooks): extend HookContext with environment, history, and query fields"
```

---

#### Task 2: HookContext 序列化为 JSON

**Files:**
- Modify: `crates/octo-engine/src/hooks/context.rs`

**Step 1: 添加 Serialize 派生和 to_json 方法**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct HookContext {
    // ... 所有字段
}

impl HookContext {
    /// Serialize the context to a JSON Value for external hooks.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// Serialize to a flat map of environment variables (OCTO_ prefix).
    pub fn to_env_vars(&self) -> Vec<(String, String)> {
        let mut vars = Vec::new();
        if let Some(ref s) = self.session_id {
            vars.push(("OCTO_SESSION_ID".into(), s.clone()));
        }
        if let Some(ref t) = self.tool_name {
            vars.push(("OCTO_TOOL_NAME".into(), t.clone()));
        }
        if let Some(ref w) = self.working_dir {
            vars.push(("OCTO_WORKING_DIR".into(), w.clone()));
        }
        if let Some(ref m) = self.sandbox_mode {
            vars.push(("OCTO_SANDBOX_MODE".into(), m.clone()));
        }
        if let Some(ref p) = self.sandbox_profile {
            vars.push(("OCTO_SANDBOX_PROFILE".into(), p.clone()));
        }
        if let Some(ref a) = self.autonomy_level {
            vars.push(("OCTO_AUTONOMY_LEVEL".into(), a.clone()));
        }
        if let Some(turn) = self.turn {
            vars.push(("OCTO_TURN".into(), turn.to_string()));
        }
        if let Some(ref m) = self.model {
            vars.push(("OCTO_MODEL".into(), m.clone()));
        }
        vars
    }
}
```

**Step 2: 测试**

```rust
#[test]
fn test_hook_context_to_json() {
    let ctx = HookContext::new()
        .with_session("s1")
        .with_tool("bash", serde_json::json!({"command": "ls"}));
    let json = ctx.to_json();
    assert_eq!(json["session_id"], "s1");
    assert_eq!(json["tool_name"], "bash");
}

#[test]
fn test_hook_context_to_env_vars() {
    let ctx = HookContext::new()
        .with_session("s1")
        .with_environment("/tmp", "host", "dev", "model", "full");
    let vars = ctx.to_env_vars();
    assert!(vars.iter().any(|(k, v)| k == "OCTO_SESSION_ID" && v == "s1"));
    assert!(vars.iter().any(|(k, v)| k == "OCTO_SANDBOX_MODE" && v == "host"));
}
```

**Step 3: Commit**

```bash
git add crates/octo-engine/src/hooks/context.rs
git commit -m "feat(hooks): add HookContext JSON serialization and env var export"
```

---

#### Task 3: 在 harness.rs 中填充丰富的 HookContext

**Files:**
- Modify: `crates/octo-engine/src/agent/harness.rs`

**Step 1: 创建 helper 函数构建丰富上下文**

在 harness.rs 中添加：

```rust
/// Build a rich HookContext from the current agent loop state.
fn build_hook_context(
    config: &AgentLoopConfig,
    round: u32,
    total_tool_calls: u32,
    recent_tools: &[String],
) -> HookContext {
    let mut ctx = HookContext::new()
        .with_session(config.session_id.as_str())
        .with_turn(round)
        .with_history(total_tool_calls, round, recent_tools.to_vec());

    // Environment context from config
    if let Some(ref tool_ctx) = config.tool_ctx {
        ctx.working_dir = Some(tool_ctx.working_dir.display().to_string());
    }
    ctx.model = Some(config.model.clone());

    ctx
}
```

**Step 2: 在现有 hook 调用点使用丰富上下文**

将 harness.rs 中简单的 `HookContext::new().with_session(...)` 替换为 `build_hook_context(...)`。

**Step 3: 添加 recent_tools 追踪变量**

在 main loop 前添加：
```rust
let mut recent_tools: Vec<String> = Vec::new();
const MAX_RECENT_TOOLS: usize = 10;
```

在工具执行后记录：
```rust
recent_tools.push(tu.name.clone());
if recent_tools.len() > MAX_RECENT_TOOLS {
    recent_tools.remove(0);
}
```

**Step 4: 编译验证**

Run: `cargo check -p octo-engine`
Expected: 编译通过，无新 warning

**Step 5: Commit**

```bash
git add crates/octo-engine/src/agent/harness.rs
git commit -m "feat(hooks): populate rich HookContext with environment and history in agent loop"
```

---

### G2: 内置 Handler 注册 — P0（3 个任务）

#### Task 4: SecurityPolicyHandler（PreToolUse）

**Files:**
- Create: `crates/octo-engine/src/hooks/builtin/mod.rs`
- Create: `crates/octo-engine/src/hooks/builtin/security_policy.rs`
- Modify: `crates/octo-engine/src/hooks/mod.rs` (添加 `mod builtin;`)
- Test: 内联 tests

**目标:** 将现有 `SecurityPolicy` 的 `forbidden_paths`、`block_high_risk_commands` 等规则转化为 PreToolUse hook handler。

**Step 1: 写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_blocks_forbidden_path() {
        let policy = SecurityPolicy {
            forbidden_paths: vec!["/etc".into(), "/sys".into()],
            ..Default::default()
        };
        let handler = SecurityPolicyHandler::new(Arc::new(policy));
        let ctx = HookContext::new()
            .with_tool("file_write", serde_json::json!({"path": "/etc/passwd", "content": "x"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Block(_)));
    }

    #[tokio::test]
    async fn test_allows_safe_path() {
        let policy = SecurityPolicy {
            forbidden_paths: vec!["/etc".into()],
            working_dir: PathBuf::from("/home/user/project"),
            ..Default::default()
        };
        let handler = SecurityPolicyHandler::new(Arc::new(policy));
        let ctx = HookContext::new()
            .with_tool("file_write", serde_json::json!({"path": "/home/user/project/src/main.rs", "content": "x"}));
        let result = handler.execute(&ctx).await.unwrap();
        assert!(matches!(result, HookAction::Continue));
    }
}
```

**Step 2: 实现 SecurityPolicyHandler**

```rust
use crate::hooks::{HookAction, HookContext, HookHandler, HookFailureMode};
use crate::security::SecurityPolicy;
use async_trait::async_trait;
use std::sync::Arc;

pub struct SecurityPolicyHandler {
    policy: Arc<SecurityPolicy>,
}

impl SecurityPolicyHandler {
    pub fn new(policy: Arc<SecurityPolicy>) -> Self {
        Self { policy }
    }

    fn check_path(&self, path: &str) -> Option<String> {
        for forbidden in &self.policy.forbidden_paths {
            if path.starts_with(forbidden) {
                return Some(format!("Path '{}' is in forbidden zone '{}'", path, forbidden));
            }
        }
        None
    }
}

#[async_trait]
impl HookHandler for SecurityPolicyHandler {
    fn name(&self) -> &str { "security-policy" }
    fn priority(&self) -> u32 { 10 } // 高优先级
    fn failure_mode(&self) -> HookFailureMode { HookFailureMode::FailClosed }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        let Some(ref input) = ctx.tool_input else {
            return Ok(HookAction::Continue);
        };

        // Check file paths in tool input
        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            if let Some(reason) = self.check_path(path) {
                return Ok(HookAction::Block(reason));
            }
        }

        Ok(HookAction::Continue)
    }
}
```

**Step 3: 运行测试**

Run: `cargo test -p octo-engine -- hooks::builtin::security_policy --test-threads=1`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/octo-engine/src/hooks/builtin/
git commit -m "feat(hooks): add SecurityPolicyHandler for PreToolUse path safety"
```

---

#### Task 5: AuditLogHandler（PostToolUse）

**Files:**
- Create: `crates/octo-engine/src/hooks/builtin/audit_log.rs`
- Modify: `crates/octo-engine/src/hooks/builtin/mod.rs`

**目标:** PostToolUse 时记录工具执行摘要到 tracing 日志（结构化），为未来持久化审计做准备。

**Step 1: 实现**

```rust
pub struct AuditLogHandler;

#[async_trait]
impl HookHandler for AuditLogHandler {
    fn name(&self) -> &str { "audit-log" }
    fn priority(&self) -> u32 { 200 } // 低优先级，不阻断
    fn failure_mode(&self) -> HookFailureMode { HookFailureMode::FailOpen }

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        tracing::info!(
            hook = "audit",
            session_id = ctx.session_id.as_deref().unwrap_or("unknown"),
            tool = ctx.tool_name.as_deref().unwrap_or("unknown"),
            success = ctx.success.unwrap_or(true),
            duration_ms = ctx.duration_ms.unwrap_or(0),
            "Tool execution audit"
        );
        Ok(HookAction::Continue)
    }
}
```

**Step 2: 测试 + Commit**

---

#### Task 6: 在 AgentRuntime 中注册内置 Handler

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: 添加内置 handler 注册**

在 `AgentRuntime::new()` 中，`HookRegistry::new()` 之后添加：

```rust
let hook_registry = Arc::new(HookRegistry::new());

// Register builtin handlers
{
    let registry = &hook_registry;
    let rt = tokio::runtime::Handle::current();
    rt.block_on(async {
        // SecurityPolicyHandler: PreToolUse — blocks forbidden paths
        registry.register(
            HookPoint::PreToolUse,
            Arc::new(crate::hooks::builtin::SecurityPolicyHandler::new(
                security_policy.clone(),
            )),
        ).await;

        // AuditLogHandler: PostToolUse — structured audit logging
        registry.register(
            HookPoint::PostToolUse,
            Arc::new(crate::hooks::builtin::AuditLogHandler),
        ).await;
    });
}
```

**Step 2: 编译验证 + Commit**

---

### G3: 声明式加载与 Command 执行 — P1（4 个任务）

#### Task 7: hooks.yaml 配置解析器

**Files:**
- Create: `crates/octo-engine/src/hooks/declarative/mod.rs`
- Create: `crates/octo-engine/src/hooks/declarative/config.rs`
- Modify: `crates/octo-engine/Cargo.toml` (添加 `serde_yaml` 依赖)

**目标:** 解析 hooks.yaml 为类型安全的 Rust 结构体。

**关键结构体:**

```rust
#[derive(Debug, Deserialize)]
pub struct HooksConfig {
    pub version: u32,
    pub hooks: HashMap<String, Vec<HookEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct HookEntry {
    pub matcher: String,
    pub actions: Vec<HookActionConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum HookActionConfig {
    #[serde(rename = "prompt")]
    Prompt {
        prompt: String,
        #[serde(default = "default_timeout")]
        timeout: u32,
    },
    #[serde(rename = "command")]
    Command {
        command: String,
        #[serde(default = "default_timeout")]
        timeout: u32,
        #[serde(default)]
        failure_mode: FailureMode,
    },
    #[serde(rename = "webhook")]
    Webhook {
        url: String,
        #[serde(default = "default_method")]
        method: String,
        #[serde(default = "default_timeout")]
        timeout: u32,
        #[serde(default)]
        failure_mode: FailureMode,
    },
}
```

---

#### Task 8: Command 执行器

**Files:**
- Create: `crates/octo-engine/src/hooks/declarative/command_executor.rs`

**目标:** 执行外部脚本，通过 env + stdin 传入上下文，解析 stdout JSON 返回。

```rust
pub async fn execute_command(
    command: &str,
    ctx: &HookContext,
    timeout_secs: u32,
) -> anyhow::Result<HookDecision> {
    use tokio::process::Command;

    let env_vars = ctx.to_env_vars();
    let stdin_json = serde_json::to_string(&ctx.to_json())?;

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .envs(env_vars)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Write stdin
    if let Some(mut stdin) = child.stdin.take() {
        tokio::io::AsyncWriteExt::write_all(&mut stdin, stdin_json.as_bytes()).await?;
    }

    // Wait with timeout
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs as u64),
        child.wait_with_output(),
    ).await??;

    parse_command_output(&output)
}
```

---

#### Task 9: DeclarativeHookBridge — 桥接 handler

**Files:**
- Create: `crates/octo-engine/src/hooks/declarative/bridge.rs`

**目标:** 实现 `HookHandler` trait 的桥接器，从 hooks.yaml 配置驱动执行。

```rust
pub struct DeclarativeHookBridge {
    config: Arc<HooksConfig>,
}

#[async_trait]
impl HookHandler for DeclarativeHookBridge {
    fn name(&self) -> &str { "declarative-bridge" }
    fn priority(&self) -> u32 { 500 } // Layer 3, 最后执行

    async fn execute(&self, ctx: &HookContext) -> anyhow::Result<HookAction> {
        let event_name = /* 从 ctx 推断当前 HookPoint 名 */;
        let entries = self.config.hooks.get(&event_name);
        // 遍历 entries，匹配 matcher，执行 actions
        // ...
    }
}
```

---

#### Task 10: 配置加载与注册集成

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`
- Create: `crates/octo-engine/src/hooks/declarative/loader.rs`

**目标:** 在 AgentRuntime 初始化时加载 hooks.yaml，注册 DeclarativeHookBridge。

---

### G4: Prompt 类型 LLM 评估 — P2（2 个任务）

#### Task 11: Prompt 模板渲染器

**Files:**
- Create: `crates/octo-engine/src/hooks/declarative/prompt_renderer.rs`

**目标:** 将模板变量 `{{tool_name}}` 替换为实际值，未使用变量时附加完整上下文 JSON。

---

#### Task 12: Prompt 类型 LLM 评估执行器

**Files:**
- Create: `crates/octo-engine/src/hooks/declarative/prompt_executor.rs`

**目标:** 调用 LLM provider 评估 prompt，解析返回的 JSON 决策。

注意事项：
- 使用独立的 short-context LLM 调用（不使用 agent 的主上下文）
- Token 预算控制：单次评估 ≤ 500 output tokens
- 超时默认 10s

---

### G5: 策略引擎 — P3（3 个任务）

#### Task 13: policies.yaml 配置解析器

**Files:**
- Create: `crates/octo-engine/src/hooks/policy/mod.rs`
- Create: `crates/octo-engine/src/hooks/policy/config.rs`

---

#### Task 14: 策略规则匹配器

**Files:**
- Create: `crates/octo-engine/src/hooks/policy/matcher.rs`

**目标:** 实现路径匹配、命令模式匹配、频率限制等内置规则。

---

#### Task 15: PolicyEngineBridge

**Files:**
- Create: `crates/octo-engine/src/hooks/policy/bridge.rs`

**目标:** 实现 HookHandler trait，从 policies.yaml 驱动规则匹配。

---

## Deferred 项（暂缓项）

> 本阶段已知但暂未实现的功能点。每次开始新 Task 前先检查此列表。

| ID | 内容 | 前置条件 | 优先级 | 状态 |
|----|------|---------|--------|------|
| AH-D1 | Webhook 类型执行器 (HTTP POST body JSON) | G3 完成 ✅ | P4 | ✅ 已补 @ 4ebc7fa |
| AH-D2 | WASM 插件 hook (Wasmtime WIT 接口) | octo-sandbox WASM 基础 | P5/未来 | ✅ 已补 (Phase AI @ ef54b2f) |
| AH-D3 | 平台租户策略合并逻辑 | octo-platform-server 推进 | P4 | ⏳ |
| AH-D4 | TUI hook 状态面板（显示已注册 hooks、执行统计） | G2 完成 ✅ | P4 | ⏳ |
| AH-D5 | Stop / SubagentStop 事件声明式支持 | G3 完成 ✅ | P3 | ✅ 已补 @ c68c373 |
| AH-D6 | `ask` 决策类型 → ApprovalGate 集成 | G3 + approval 系统 | P3 | ⏳ |
| AH-D7 | DeclarativeHookBridge + PolicyEngineBridge 在 AgentRuntime 中注册接线 | hooks.yaml/policies.yaml 文件存在 | P1 | ✅ 已补 @ 4ebc7fa |
| AH-D8 | Prompt 类型 action 在 bridge.rs 中调用 execute_prompt (需 Provider 传入) | G4 完成 ✅ + Provider 注入路径 | P2 | ✅ 已补 @ 4ebc7fa |

---

## 文件结构预览

```
crates/octo-engine/src/hooks/
├── mod.rs                           # 现有，添加 mod builtin, declarative, policy
├── context.rs                       # 现有，增强字段 + 序列化
├── handler.rs                       # 现有，不变
├── registry.rs                      # 现有，不变
├── builtin/                         # 新增：内置 handler
│   ├── mod.rs
│   ├── security_policy.rs           # Task 4
│   └── audit_log.rs                 # Task 5
├── declarative/                     # 新增：声明式 hook 系统
│   ├── mod.rs
│   ├── config.rs                    # Task 7: YAML 配置结构
│   ├── loader.rs                    # Task 10: 文件加载
│   ├── command_executor.rs          # Task 8: 脚本执行
│   ├── prompt_renderer.rs           # Task 11: 模板渲染
│   ├── prompt_executor.rs           # Task 12: LLM 评估
│   └── bridge.rs                    # Task 9: HookHandler 桥接
└── policy/                          # 新增：策略引擎
    ├── mod.rs
    ├── config.rs                    # Task 13: YAML 配置
    ├── matcher.rs                   # Task 14: 规则匹配
    └── bridge.rs                    # Task 15: HookHandler 桥接
```

---

## 验收标准

- [ ] G1: `HookContext` 包含环境上下文，可序列化为 JSON 和 env vars
- [ ] G2: SecurityPolicyHandler 在 PreToolUse 时阻断 forbidden_paths，AuditLogHandler 在 PostToolUse 时记录日志
- [ ] G3: hooks.yaml 能解析并驱动 command 类型 hook 执行，stdin/stdout 协议正确
- [ ] G4: prompt 类型能渲染模板变量，调用 LLM 评估并解析决策
- [ ] G5: policies.yaml 能驱动路径黑名单和命令安全检查
- [ ] 全程 `cargo check --workspace` 编译通过
- [ ] 新增测试 ≥ 30，覆盖各层核心逻辑
