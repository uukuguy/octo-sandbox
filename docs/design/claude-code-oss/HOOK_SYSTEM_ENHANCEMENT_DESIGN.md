# Octo-Engine Hook 系统增强设计

> 基于 Claude Code OSS hooks 架构（28 种事件, 5 种执行类型）与 Octo hooks 架构（17 种事件, 4 种执行类型, 3 层注册）的代码级对比。
> 日期：2026-04-01
> 优先级：P2（在 P0 上下文管理 + P1 权限引擎之后）
> 前置依赖：P1-1 PermissionEngine（`if` 条件过滤复用 PermissionRule）

---

## 一、Octo 现有 Hook 系统评估修正

> **重要修正**：之前评估 Hook 系统差距为 ⭐⭐⭐~⭐⭐⭐⭐。深入阅读代码后发现 Octo 的 hooks 系统远比预期完善，实际差距仅 ⭐⭐。

### Octo 已有能力

| 能力 | 实现状态 |
|------|---------|
| 17 种 Hook 事件 (PreToolUse, PostToolUse, PreTask, PostTask, SessionStart/End, LoopTurnStart/End, ContextDegraded, AgentRoute, Skills*, ToolConstraintViolated, Stop, SubagentStop) | 完整 |
| 3 层注册系统 (Builtin → PolicyEngine → Declarative) | 完整 |
| 5 种 HookAction (Continue, Modify, Abort, Block, Redirect) | 完整 |
| 丰富 HookContext (20+ 字段, 含环境/历史/技能) | 完整 |
| Policy Engine (YAML 配置: deny_paths, deny_patterns, rate_limit 等) | 完整 |
| WASM 插件 hook | 完整 |
| 环境变量导出 (OCTO_* env vars) | 完整 |
| FailOpen/FailClosed 失败模式 | 完整 |
| 优先级排序执行 | 完整 |
| SecurityPolicyHandler + AuditLogHandler 内置 | 完整 |

### 与 CC-OSS 的精确差距

| CC 独有能力 | 重要度 | Octo 改进建议 |
|------------|--------|-------------|
| **ModifyInput** (修改工具输入) | **高** | HookAction 新增 ModifyInput 变体 |
| **InjectContext** (注入对话上下文) | **高** | HookAction 新增 InjectContext 变体 |
| **PermissionDecision** (参与权限链) | **高** | HookAction 新增 PermissionDecision 变体 |
| **UserPromptSubmit** hook 事件 | **高** | HookPoint 新增 |
| **`if` 条件过滤** (通配符匹配工具输入) | **高** | 复用 PermissionRule::matches() |
| **Async hook registry** (fire-and-forget) | 中 | HookHandler 新增 is_async() |
| **updatedMCPToolOutput** (修改工具输出) | 中 | 可后续按需添加 |
| **Stop hook** (阻止 LLM 结束) | 中 | 可通过 LoopTurnEnd + Block 近似 |
| FileChanged/CwdChanged hook | 低 | 按需添加 |
| Agent hook type (50 轮 agent 会话) | 低 | 不需要（prompt hook 够用） |
| Session-scoped dynamic hooks | 低 | 3 层注册已足够 |

---

## 二、改进项设计

### P2-H1: HookAction 扩展（输出能力增强）

**文件**: `crates/octo-engine/src/hooks/handler.rs`

当前：
```rust
pub enum HookAction {
    Continue,
    Modify(HookContext),
    Abort(String),
    Block(String),
    Redirect(String),
}
```

新增：
```rust
pub enum HookAction {
    Continue,
    Modify(HookContext),
    Abort(String),
    Block(String),
    Redirect(String),
    // 新增：
    /// 修改工具输入参数（PreToolUse 专用）
    ModifyInput(serde_json::Value),
    /// 向下一轮 LLM 调用注入额外上下文
    InjectContext(String),
    /// 参与权限决策链（PreToolUse 专用）
    PermissionOverride(PermissionHookDecision),
}

/// Hook 返回的权限决策
#[derive(Debug, Clone)]
pub enum PermissionHookDecision {
    /// 允许工具执行
    Allow,
    /// 拒绝执行（带理由）
    Deny(String),
    /// 需要人工确认
    Ask,
}
```

**Harness 集成** (`agent/harness.rs`, PreToolUse 处理)：

```rust
// 在工具执行前
let hook_action = hooks.execute(HookPoint::PreToolUse, &ctx).await;
match hook_action {
    HookAction::ModifyInput(new_input) => {
        // 替换工具输入
        input = new_input;
        debug!(tool = %tu.name, "Hook modified tool input");
    }
    HookAction::InjectContext(extra) => {
        // 存储到 pending_injections，在下一轮 LLM 调用时注入
        pending_context_injections.push(extra);
    }
    HookAction::PermissionOverride(PermissionHookDecision::Deny(reason)) => {
        warn!(tool = %tu.name, %reason, "Hook denied tool execution");
        tool_results.push((tu.name.clone(), ToolOutput::error(format!("Hook denied: {reason}"))));
        continue;
    }
    HookAction::PermissionOverride(PermissionHookDecision::Ask) => {
        // 转交给 ApprovalManager
        let approved = request_approval(&tu.name, &tu.id, risk, &tx, &config.approval_gate).await;
        if !approved {
            tool_results.push((tu.name.clone(), ToolOutput::error("Approval denied".into())));
            continue;
        }
    }
    HookAction::Block(reason) => {
        warn!(tool = %tu.name, %reason, "Hook blocked tool");
        // ... 现有逻辑
    }
    _ => {} // Continue, Abort, Redirect 走现有逻辑
}
```

**Context 注入实现**（在 CompletionRequest 构建时）：

```rust
// 在构建 CompletionRequest 之前，注入 pending context
if !pending_context_injections.is_empty() {
    let injection = pending_context_injections.join("\n\n");
    messages.push(ChatMessage {
        role: MessageRole::User,
        content: vec![ContentBlock::Text {
            text: format!("<system-reminder>\n{}\n</system-reminder>", injection),
        }],
    });
    pending_context_injections.clear();
}
```

**预估**: ~80 行

### P2-H2: UserPromptSubmit Hook 事件

**文件**: `crates/octo-engine/src/hooks/mod.rs` + `agent/harness.rs`

新增 HookPoint：
```rust
pub enum HookPoint {
    // ... 现有 ...
    /// 用户输入提交时触发（在 LLM 调用前）
    UserPromptSubmit,
}
```

Harness 集成（在 round 0 的 CompletionRequest 构建前）：
```rust
// 在 round 0，用户消息提交后、LLM 调用前
if round == 0 {
    if let Some(ref hooks) = config.hook_registry {
        let user_text = messages.last()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.text_content())
            .unwrap_or_default();

        let ctx = build_rich_hook_context(&config, round, total_tool_calls, &recent_tools)
            .with_user_query(&user_text);

        match hooks.execute(HookPoint::UserPromptSubmit, &ctx).await {
            HookAction::Abort(reason) => {
                let _ = tx.send(AgentEvent::Error { message: reason }).await;
                let _ = tx.send(AgentEvent::Done).await;
                return;
            }
            HookAction::InjectContext(extra) => {
                pending_context_injections.push(extra);
            }
            _ => {}
        }
    }
}
```

**预估**: ~30 行

### P2-H3: `if` 条件过滤

**文件**: `crates/octo-engine/src/hooks/declarative/config.rs`

扩展 declarative hook 配置：
```yaml
hooks:
  PreToolUse:
    - matcher: "bash"
      if: "bash(git *)"       # 新增：只对 git 命令触发
      actions:
        - type: command
          command: "python3 validate_git.py"
```

实现（复用 P1 的 PermissionRule）：
```rust
use crate::security::permission_rule::PermissionRule;

pub struct DeclarativeHookEntry {
    pub matcher: Option<String>,      // 工具名正则
    pub if_condition: Option<String>,  // 新增：PermissionRule 语法条件
    pub actions: Vec<HookActionConfig>,
}

impl DeclarativeHookEntry {
    /// 检查 hook 是否应该对此工具调用触发
    fn should_trigger(&self, tool_name: &str, tool_input: &Value) -> bool {
        // 1. 先检查 matcher（工具名）
        if let Some(ref matcher) = self.matcher {
            if !regex_match(matcher, tool_name) {
                return false;
            }
        }
        // 2. 再检查 if 条件（工具输入内容）
        if let Some(ref condition) = self.if_condition {
            if let Ok(rule) = PermissionRule::parse(condition) {
                return rule.matches(tool_name, tool_input);
            }
        }
        true
    }
}
```

**预估**: ~40 行

### P2-H4: Async Hook 支持

**文件**: `crates/octo-engine/src/hooks/handler.rs` + `registry.rs`

Handler trait 扩展：
```rust
pub trait HookHandler: Send + Sync {
    // ... 现有 ...
    /// 是否异步执行（fire-and-forget，不阻塞主循环）
    fn is_async(&self) -> bool { false }
}
```

Registry 执行修改：
```rust
pub async fn execute(&self, point: HookPoint, context: &HookContext) -> HookAction {
    let handlers = self.handlers.read().await;
    let mut final_action = HookAction::Continue;

    for handler in sorted_handlers {
        if handler.is_async() {
            // Fire-and-forget: spawn 后不等待
            let ctx = context.clone();
            let h = handler.clone();
            tokio::spawn(async move {
                if let Err(e) = h.execute(&ctx).await {
                    tracing::warn!(hook = h.name(), "Async hook error: {e}");
                }
            });
            continue;
        }
        // ... 现有同步执行逻辑
    }
    final_action
}
```

**预估**: ~20 行

---

## 三、实施分组

| 编号 | 内容 | 依赖 | 代码量 |
|------|------|------|--------|
| P2-H1 | HookAction 扩展 (ModifyInput/InjectContext/PermissionOverride) | 无 | ~80 行 |
| P2-H2 | UserPromptSubmit hook | P2-H1 (InjectContext) | ~30 行 |
| P2-H3 | `if` 条件过滤 | P1-1 (PermissionRule) | ~40 行 |
| P2-H4 | Async hook 支持 | 无 | ~20 行 |
| **合计** | | | **~170 行** |

推荐顺序：H1 → H2 → H4 → H3（H3 依赖 P1-1）

---

## 四、改进后对比

| 维度 | CC-OSS | Octo 当前 | Octo 改进后 |
|------|--------|----------|-----------|
| Hook 事件数 | 28 | 17 | 18 (+UserPromptSubmit) |
| 执行类型 | 5 (command/prompt/agent/http/function) | 4 (command/prompt/webhook/wasm) | 4 (不变，WASM > function) |
| 输出能力 | ModifyInput + InjectContext + PermissionDecision + ModifyOutput | Modify(HookContext) 但不反映到实际操作 | **ModifyInput + InjectContext + PermissionOverride** |
| 条件过滤 | `if: "Bash(git *)"` 通配符 | matcher 只匹配工具名 | **`if` 条件 + PermissionRule 语法** |
| 异步执行 | AsyncHookRegistry | 全部同步 | **is_async() fire-and-forget** |
| 注册层次 | settings.json + plugin + session + function | 3 层 (builtin + policy + declarative) | 3 层 (不变，已足够) |
| WASM 支持 | 无 | **有** | **有 (Octo 优势)** |
| Policy Engine | 无 | **有 (policies.yaml)** | **有 (Octo 优势)** |
| **差距评估** | — | ⭐⭐ | ⭐ (基本持平) |
