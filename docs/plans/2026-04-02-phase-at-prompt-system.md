# Phase AT — Octo 提示词体系增强

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 补齐 Octo 与 CC-OSS 的提示词体系差距，实现 prompt caching、静态段增强、动态上下文注入、tool description 接线。

**Architecture:** System prompt 保持静态/动态分离（`PromptParts`），静态段头部不变（可缓存），新增内容追加到静态段尾部或动态段。Anthropic API 加 `cache_control` 标记。Tool descriptions 从 `prompts.rs` 常量接线到各工具的 `fn description()`。

**Tech Stack:** Rust, octo-engine, octo-types, Anthropic Messages API, serde_json

---

## 任务总览

| Wave | Task | 内容 | 预估改动 |
|------|------|------|----------|
| W1 | T1 | Prompt caching 基础设施（CompletionRequest + Anthropic API） | ~80 行 |
| W1 | T2 | 静态段增强（Git 工作流、Cyber risk、权限模式） | ~120 行 |
| W2 | T3 | 动态段增强（Environment info、MCP instructions、Token budget） | ~100 行 |
| W2 | T4 | Tool description 接线（prompts.rs → 各工具 fn description） | ~50 行 |
| W3 | T5 | SubAgent prompt 专属化 | ~60 行 |
| W3 | T6 | Harness 集成 + 编译验证 + 测试 | ~40 行 |

---

## Wave 1: Prompt Caching + 静态段

### Task 1: Prompt Caching 基础设施

**目的**: 让 Anthropic API 支持 prompt caching，减少 ~90% 的 system prompt token 费用。

**Files:**
- Modify: `crates/octo-types/src/provider.rs:28-36` — CompletionRequest 加 system_prompt_parts
- Modify: `crates/octo-engine/src/providers/anthropic.rs:50-61` — ApiRequest system 改为数组
- Modify: `crates/octo-engine/src/agent/harness.rs:242-269` — 用 build_separated()
- Modify: `crates/octo-engine/src/agent/harness.rs:659-662` — 传 PromptParts 到 request

**Step 1: 扩展 CompletionRequest**

`crates/octo-types/src/provider.rs`:
```rust
pub struct CompletionRequest {
    pub model: String,
    pub system: Option<String>,
    /// Separated system prompt for prompt caching (Anthropic).
    /// If set, provider should use this instead of `system`.
    /// `static_system` = cacheable prefix, `dynamic_system` = per-request suffix.
    pub static_system: Option<String>,
    pub dynamic_system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub tools: Vec<ToolSpec>,
    pub stream: bool,
}
```

**Step 2: Anthropic ApiRequest 支持 system 数组**

`crates/octo-engine/src/providers/anthropic.rs`:
```rust
#[derive(Serialize)]
struct ApiSystemBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    cache_type: String,
}

// ApiRequest.system 改为:
#[serde(skip_serializing_if = "Option::is_none")]
system: Option<Vec<ApiSystemBlock>>,
```

在 `complete()` 和 `stream()` 中，根据 `static_system`/`dynamic_system` 构建 system blocks：
- 如果有 `static_system`：构建两个 block，第一个带 `cache_control: { type: "ephemeral" }`
- 否则：fallback 到单 block（从 `system` 字段）

**Step 3: Harness 用 build_separated()**

`crates/octo-engine/src/agent/harness.rs:242-269`:
```rust
let prompt_parts = builder.build_separated();
let mut static_system = prompt_parts.system_prompt;
let mut dynamic_system = prompt_parts.dynamic_context;
// ... 后续 cross-session memory / pinned memories 追加到 dynamic_system
```

`harness.rs:659-662`:
```rust
let request = CompletionRequest {
    model: config.model.clone(),
    system: None, // deprecated path
    static_system: Some(static_system.clone()),
    dynamic_system: if dynamic_system.is_empty() { None } else { Some(dynamic_system.clone()) },
    messages: masked_messages,
    ...
};
```

**Step 4: 编译验证**

```bash
cargo check -p octo-types -p octo-engine
```

**Step 5: Commit**

```bash
git add crates/octo-types/src/provider.rs crates/octo-engine/src/providers/anthropic.rs crates/octo-engine/src/agent/harness.rs
git commit -m "feat(engine): prompt caching — split system prompt for Anthropic cache_control"
```

---

### Task 2: 静态段增强

**目的**: 在 system_prompt.rs 的静态段（cacheable）中补充 Git 工作流、Cyber risk、权限模式说明。

**Files:**
- Modify: `crates/octo-engine/src/context/system_prompt.rs` — 新增 3 个 const 段落

**Step 1: 新增 GIT_WORKFLOW_SECTION**

`crates/octo-engine/src/context/system_prompt.rs`，在 `AUTONOMOUS_PROMPT` 之前添加：

```rust
const GIT_WORKFLOW_SECTION: &str = r#"## Git Operations

### Committing Changes
- Only create commits when requested by the user. If unclear, ask first.
- Summarize the nature of the changes in the commit message (feat/fix/refactor/docs).
- Do not commit files that likely contain secrets (.env, credentials.json, etc.).
- Prefer specific `git add <files>` over `git add -A` to avoid committing sensitive files.
- NEVER amend published commits without explicit user approval.

### Safety Protocol
- NEVER force-push to main/master — warn the user if they request it.
- NEVER skip hooks (--no-verify) unless the user explicitly requests it.
- NEVER run destructive git commands (push --force, reset --hard, checkout .) unless explicitly requested.
- When pre-commit hooks fail, fix the issue and create a NEW commit instead of amending.
- Before destructive operations, check for uncommitted work that could be lost.

### Pull Requests
- Keep PR titles short (under 70 chars), use the body for details.
- Include a summary section and test plan in the PR body.
- Always push to remote before creating a PR.
"#;

const CYBER_RISK_SECTION: &str = r#"## Security Constraints

- Assist with authorized security testing, defensive security, CTF challenges, and educational contexts.
- Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.
- NEVER generate or guess URLs unless confident they help with programming tasks.
- When working with sensitive data, flag potential privacy or security concerns to the user.
"#;

const PERMISSION_SECTION: &str = r#"## Permission Awareness

The system enforces a security policy that controls which tools you can use and what actions are auto-approved.
- **Development mode**: Most operations auto-approved. Destructive operations still require confirmation.
- **Preferred mode**: Standard operations auto-approved. File writes, git operations require approval.
- **Strict mode**: All operations require explicit user approval.

When a tool call is denied, do NOT re-attempt the exact same call. Adjust your approach or ask the user for guidance. The current security policy level will be communicated via system tags when relevant.
"#;
```

**Step 2: 在 build_static() 中组装**

在 `parts.push(OUTPUT_EFFICIENCY_SECTION...)` 之后添加：

```rust
parts.push(GIT_WORKFLOW_SECTION.to_string());
parts.push(CYBER_RISK_SECTION.to_string());
parts.push(PERMISSION_SECTION.to_string());
```

**Step 3: 更新测试**

```rust
#[test]
fn test_git_workflow_section_present() {
    let builder = SystemPromptBuilder::new();
    let result = builder.build();
    assert!(result.contains("## Git Operations"));
    assert!(result.contains("NEVER force-push"));
}

#[test]
fn test_cyber_risk_section_present() {
    let builder = SystemPromptBuilder::new();
    let result = builder.build();
    assert!(result.contains("## Security Constraints"));
}

#[test]
fn test_permission_section_present() {
    let builder = SystemPromptBuilder::new();
    let result = builder.build();
    assert!(result.contains("## Permission Awareness"));
}
```

**Step 4: 编译 + 测试**

```bash
cargo test -p octo-engine system_prompt -- --test-threads=1
```

**Step 5: Commit**

```bash
git add crates/octo-engine/src/context/system_prompt.rs
git commit -m "feat(engine): add Git workflow, Cyber risk, Permission sections to system prompt"
```

---

## Wave 2: 动态段 + Tool Description

### Task 3: 动态段增强

**目的**: 在动态段（per-request, 不缓存）中注入 Environment info、MCP instructions、Token budget。

**Files:**
- Modify: `crates/octo-engine/src/context/system_prompt.rs` — 新增 with_environment_info(), with_mcp_instructions(), with_token_budget()
- Modify: `crates/octo-engine/src/agent/harness.rs` — 在 builder 链中调用新方法

**Step 1: SystemPromptBuilder 新增动态段 setter**

`crates/octo-engine/src/context/system_prompt.rs`:

```rust
/// Inject runtime environment information (platform, shell, OS, model).
pub fn with_environment_info(mut self, platform: &str, shell: &str, os_version: &str, model: &str) -> Self {
    let info = format!(
        "## Environment\n- Platform: {}\n- Shell: {}\n- OS: {}\n- Model: {}\n- Knowledge cutoff: 2025-04",
        platform, shell, os_version, model
    );
    // Append to existing session_state or set new
    if let Some(ref mut ss) = self.session_state {
        ss.push_str("\n\n");
        ss.push_str(&info);
    } else {
        self.session_state = Some(info);
    }
    self
}

/// Inject MCP server instructions from connected servers.
pub fn with_mcp_instructions(mut self, instructions: &[(String, String)]) -> Self {
    if instructions.is_empty() {
        return self;
    }
    let mut section = String::from("## MCP Server Instructions\n\n");
    for (server_name, instruction) in instructions {
        section.push_str(&format!("### {}\n{}\n\n", server_name, instruction));
    }
    if let Some(ref mut uc) = self.user_context {
        uc.push_str("\n\n");
        uc.push_str(&section);
    } else {
        self.user_context = Some(section);
    }
    self
}

/// Inject token budget awareness.
pub fn with_token_budget(mut self, max_tokens: usize, model_context_window: usize) -> Self {
    let info = format!(
        "## Token Budget\n- Model context window: {} tokens\n- Max output tokens per response: {}",
        model_context_window, max_tokens
    );
    if let Some(ref mut ss) = self.session_state {
        ss.push_str("\n\n");
        ss.push_str(&info);
    } else {
        self.session_state = Some(info);
    }
    self
}
```

**Step 2: Harness 中调用**

`crates/octo-engine/src/agent/harness.rs`，在 builder 构建链中（`with_git_status` 附近）：

```rust
// Environment info
let platform = std::env::consts::OS;
let arch = std::env::consts::ARCH;
let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into());
builder = builder.with_environment_info(
    &format!("{}-{}", platform, arch),
    &shell,
    &std::env::var("OSTYPE").unwrap_or_else(|_| platform.to_string()),
    &config.model,
);

// Token budget
builder = builder.with_token_budget(
    config.max_tokens as usize,
    200_000, // default context window; could read from provider config
);

// MCP server instructions (if McpManager available)
if let Some(ref mcp) = config.mcp_manager {
    let instructions = mcp.server_instructions().await;
    if !instructions.is_empty() {
        builder = builder.with_mcp_instructions(&instructions);
    }
}
```

**Step 3: McpManager 添加 server_instructions() 方法**

`crates/octo-engine/src/mcp/manager.rs`：

```rust
/// Collect instructions from all connected MCP servers.
/// Returns Vec<(server_name, instruction_text)>.
pub async fn server_instructions(&self) -> Vec<(String, String)> {
    let mut result = Vec::new();
    // Iterate running servers, check if they have instructions metadata
    for entry in self.servers.iter() {
        let name = entry.key().clone();
        if let Some(ref instructions) = entry.value().instructions {
            if !instructions.is_empty() {
                result.push((name, instructions.clone()));
            }
        }
    }
    result
}
```

如果 MCP server entry 没有 `instructions` 字段，需要在 server metadata struct 中添加。根据 rmcp SDK 的 server info，instructions 通常在 `InitializeResult` 中返回。

**Step 4: 测试**

```rust
#[test]
fn test_environment_info_in_dynamic() {
    let builder = SystemPromptBuilder::new()
        .with_environment_info("darwin-aarch64", "/bin/zsh", "Darwin 25.3", "claude-sonnet-4-20250514");
    let parts = builder.build_separated();
    assert!(parts.dynamic_context.contains("## Environment"));
    assert!(parts.dynamic_context.contains("darwin-aarch64"));
}

#[test]
fn test_token_budget_in_dynamic() {
    let builder = SystemPromptBuilder::new()
        .with_token_budget(8192, 200_000);
    let parts = builder.build_separated();
    assert!(parts.dynamic_context.contains("## Token Budget"));
    assert!(parts.dynamic_context.contains("200000"));
}

#[test]
fn test_mcp_instructions() {
    let builder = SystemPromptBuilder::new()
        .with_mcp_instructions(&[
            ("github".into(), "Use this for GitHub operations".into()),
        ]);
    let parts = builder.build_separated();
    assert!(parts.dynamic_context.contains("### github"));
}
```

**Step 5: Commit**

```bash
git add crates/octo-engine/src/context/system_prompt.rs crates/octo-engine/src/agent/harness.rs crates/octo-engine/src/mcp/manager.rs
git commit -m "feat(engine): dynamic prompt sections — environment info, MCP instructions, token budget"
```

---

### Task 4: Tool Description 接线

**目的**: 将 `prompts.rs` 中的详细 tool descriptions 接线到各工具的 `fn description()`，替换当前的短描述。

**Files:**
- Modify: `crates/octo-engine/src/tools/bash.rs` — 使用 BASH_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/file_read.rs` — 使用 FILE_READ_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/file_edit.rs` — 使用 FILE_EDIT_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/file_write.rs` — 使用 FILE_WRITE_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/grep_tool.rs` — 使用 GREP_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/glob_tool.rs` — 使用 GLOB_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/web_search.rs` — 使用 WEB_SEARCH_DESCRIPTION
- Modify: `crates/octo-engine/src/tools/web_fetch.rs` — 使用 WEB_FETCH_DESCRIPTION

**Step 1: 逐个工具替换 fn description()**

模式相同，以 BashTool 为例：

```rust
// bash.rs
use super::prompts::BASH_DESCRIPTION;

fn description(&self) -> &str {
    BASH_DESCRIPTION
}
```

对每个工具文件重复此模式。确认 prompts.rs 已 `pub` 导出所有常量。

**Step 2: 验证 prompts.rs 覆盖所有工具**

检查哪些工具有详细 description，哪些还缺：
- ✅ bash, file_read, file_edit, file_write, grep, glob, web_search, web_fetch, subagent
- ❓ memory_search, memory_store, memory_edit, memory_timeline, memory_forget, memory_compress
- ❓ execute_skill, ask_user, tool_search, plan_mode
- ❓ notebook_edit, interaction

缺失的工具暂时保持现有短描述，不在本 Task 范围内。后续可按需补充。

**Step 3: 编译 + 测试**

```bash
cargo check -p octo-engine
cargo test -p octo-engine -- --test-threads=1 -q
```

**Step 4: Commit**

```bash
git add crates/octo-engine/src/tools/
git commit -m "feat(engine): wire detailed tool descriptions from prompts.rs to Tool::description()"
```

---

## Wave 3: SubAgent Prompt + 集成

### Task 5: SubAgent Prompt 专属化

**目的**: 为 fork/playbook sub-agent 提供专属的 system prompt 段落，指导其行为与父 agent 的差异。

**Files:**
- Modify: `crates/octo-engine/src/context/system_prompt.rs` — 新增 SUBAGENT_SECTION
- Modify: `crates/octo-engine/src/agent/executor.rs` 或 `skill_runtime/` — sub-agent 构建时注入

**Step 1: 新增 SUBAGENT_SECTION**

```rust
const SUBAGENT_SECTION: &str = r#"## Sub-Agent Mode

You are running as a sub-agent spawned by a parent agent. Your scope is limited to the delegated task.

### Behavior
- Focus exclusively on the delegated task. Do not explore unrelated code or make unsolicited changes.
- Return results concisely — the parent agent will synthesize your output with other sub-agents.
- Do not ask the user questions directly — if you need clarification, include it in your result and let the parent decide.
- Do not commit code or push to remote — the parent agent handles git operations.

### Communication
- Your text output goes back to the parent agent, not directly to the user.
- Be factual and structured in your responses (use bullet points, code blocks).
- If you encounter an error or blocker, describe it clearly so the parent can decide next steps.
"#;
```

**Step 2: SystemPromptBuilder 支持 sub-agent 模式**

```rust
/// Enable sub-agent mode prompt section.
pub fn with_subagent_mode(mut self, enabled: bool) -> Self {
    self.subagent_mode = enabled;
    self
}
```

在 `build_static()` 中，autonomous 段之后：
```rust
if self.subagent_mode {
    parts.push(SUBAGENT_SECTION.to_string());
}
```

**Step 3: SubAgentContext 构建时启用**

在 `executor.rs` 或 `skill_runtime/context.rs` 中，构建 sub-agent 的 SystemPromptBuilder 时加 `.with_subagent_mode(true)`。

**Step 4: 测试**

```rust
#[test]
fn test_subagent_mode_section() {
    let builder = SystemPromptBuilder::new().with_subagent_mode(true);
    let result = builder.build();
    assert!(result.contains("## Sub-Agent Mode"));
    assert!(result.contains("Do not commit code"));
}

#[test]
fn test_subagent_mode_off_by_default() {
    let builder = SystemPromptBuilder::new();
    let result = builder.build();
    assert!(!result.contains("## Sub-Agent Mode"));
}
```

**Step 5: Commit**

```bash
git add crates/octo-engine/src/context/system_prompt.rs crates/octo-engine/src/agent/executor.rs
git commit -m "feat(engine): add sub-agent mode system prompt section"
```

---

### Task 6: 集成验证

**目的**: 全量编译、测试、确认无回归。

**Step 1: Workspace 编译**

```bash
cargo check --workspace
```

**Step 2: 相关测试**

```bash
cargo test -p octo-engine system_prompt -- --test-threads=1
cargo test -p octo-engine memory_injector -- --test-threads=1
cargo test -p octo-engine --test auto_memory -- --test-threads=1
```

**Step 3: 确认 prompt 内容完整性**

写一个集成测试，构建完整的 SystemPromptBuilder 并验证所有段落存在：

```rust
#[test]
fn test_full_prompt_contains_all_sections() {
    let builder = SystemPromptBuilder::new()
        .with_environment_info("darwin-aarch64", "/bin/zsh", "Darwin 25.3", "claude-sonnet-4-20250514")
        .with_token_budget(8192, 200_000)
        .with_git_status("main", "M src/lib.rs", "abc1234 feat: add feature");
    let parts = builder.build_separated();

    // Static sections (cacheable)
    let sp = &parts.system_prompt;
    assert!(sp.contains("You are Octo"));
    assert!(sp.contains("## System"));
    assert!(sp.contains("## Code Style"));
    assert!(sp.contains("## Executing Actions with Care"));
    assert!(sp.contains("## Using Your Tools"));
    assert!(sp.contains("## Output Efficiency"));
    assert!(sp.contains("## Output Format"));
    assert!(sp.contains("## Git Operations"));
    assert!(sp.contains("## Security Constraints"));
    assert!(sp.contains("## Permission Awareness"));

    // Dynamic sections (per-request)
    let dc = &parts.dynamic_context;
    assert!(dc.contains("## Environment"));
    assert!(dc.contains("## Token Budget"));
    assert!(dc.contains("## Git Status"));
}
```

**Step 4: Commit**

```bash
git add crates/octo-engine/
git commit -m "test(engine): Phase AT integration — full prompt structure verification"
```

---

## Deferred（暂缓项）

| ID | 内容 | 前置条件 |
|----|------|----------|
| AT-D1 | MCP instructions 从 rmcp InitializeResult 提取 | 需确认 rmcp 0.16 是否暴露 instructions 字段 |
| AT-D2 | SecurityPolicy 当前值动态注入 | 需要 SecurityPolicy 可序列化为人类可读文本 |
| AT-D3 | Coordinator prompt（多 agent 编排模式） | 需要 Coordinator 架构设计 |
| AT-D4 | 补全所有 memory/skill 工具的详细 description | 当前 9 个核心工具优先 |
| AT-D5 | OpenAI provider prompt caching 验证 | OpenAI 自动缓存，无需代码改动，但需验证前缀稳定性 |

---

## 实施顺序

```
Wave 1 (T1 + T2): prompt caching 基础设施 + 静态段内容 → 一次 commit
Wave 2 (T3 + T4): 动态段 + tool description 接线 → 一次 commit
Wave 3 (T5 + T6): sub-agent prompt + 集成验证 → 一次 commit
```

预估总改动：~450 行新增/修改，跨 ~15 个文件。
