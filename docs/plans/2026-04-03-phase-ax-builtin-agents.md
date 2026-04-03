# Phase AX — Builtin Agents 实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现 CC-OSS 对齐的内建 Agent 系统 — 扩展 AgentManifest 字段、创建 6 个内建 Agent 定义、改造 SpawnSubAgentTool 支持按 agent_type 路由

**Architecture:** 分 3 波实现：W1 扩展 AgentManifest + 创建 BuiltinAgentRegistry；W2 改造 SpawnSubAgentTool 支持 agent_type 路由 + 工具过滤/模型覆盖；W3 创建 6 个内建 Agent 定义 + YAML 文件 + skill preloading。利用 Octo 现有的 AgentCatalog、SubAgentManager、run_agent_loop() 基础设施。

**Tech Stack:** Rust, serde, tokio, octo-engine (AgentManifest, AgentCatalog, SpawnSubAgentTool, SkillRegistry)

---

## CC-OSS 内建 Agent 系统分析

### CC-OSS 6 个内建 Agent

| Agent | model | 工具策略 | 核心特征 |
|-------|-------|---------|---------|
| `general-purpose` | default | `*` (全部) | 通用执行 |
| `Explore` | haiku | disallowedTools: Agent/Edit/Write/NotebookEdit/ExitPlanMode | 只读搜索，omitClaudeMd |
| `Plan` | inherit | 同 Explore | 架构规划，omitClaudeMd |
| `statusline-setup` | sonnet | tools: Read/Edit | 状态栏配置 |
| `claude-code-guide` | haiku | tools: Glob/Grep/Read/WebFetch/WebSearch | 文档助手 |
| `verification` | inherit | disallowedTools: Agent/Edit/Write/NotebookEdit/ExitPlanMode | 对抗性验证，background=true |

### CC-OSS AgentDefinition 关键字段

```typescript
type AgentDefinition = {
  agentType: string;          // 唯一标识
  whenToUse: string;          // LLM 选择依据
  tools?: string[];           // 白名单 (undefined = all)
  disallowedTools?: string[]; // 黑名单
  model?: string;             // "haiku" | "sonnet" | "inherit"
  permissionMode?: string;    // per-agent 权限覆盖
  background?: boolean;       // 后台异步执行
  omitClaudeMd?: boolean;     // 跳过 CLAUDE.md 注入
  maxTurns?: number;          // 最大轮次
  source: 'built-in' | ...;  // 来源标识
  skills?: string[];          // 预加载 skill 列表
  getSystemPrompt(): string;  // 动态 system prompt
}
```

### Octo 现有基础设施

| 组件 | 文件 | 状态 |
|------|------|------|
| AgentManifest | `agent/entry.rs:34` | 有 name/tags/role/goal/backstory/system_prompt/model/tool_filter/coordinator |
| AgentCatalog | `agent/catalog.rs` | 完整 (by_id/by_name/by_tag/by_tenant) |
| AgentYamlDef | `agent/yaml_def.rs` | 完整 (YAML → AgentManifest) |
| AgentManifestLoader | `agent/manifest_loader.rs` | 完整 (扫描目录) |
| SpawnSubAgentTool | `tools/subagent.rs` | 有 task/max_iterations/tools_whitelist |
| QuerySubAgentTool | `tools/subagent.rs:227` | 完整 |
| SubAgentManager | `agent/subagent.rs` | 完整 (depth/concurrency) |
| ExecuteSkillTool | `skills/execute_tool.rs` | Knowledge + Playbook 双模式 |
| SkillRegistry | `skills/registry.rs` | 完整 (DashMap, 热重载) |
| ToolRegistry | `tools/mod.rs:93` | 有 snapshot_filtered() |

### 缺失字段对照

| CC-OSS 字段 | Octo AgentManifest | 需要新增 |
|-------------|-------------------|---------|
| `whenToUse` | 无 | ✅ `when_to_use: Option<String>` |
| `disallowedTools` | 无 (只有 tool_filter 白名单) | ✅ `disallowed_tools: Vec<String>` |
| `background` | 无 | ✅ `background: bool` |
| `omitClaudeMd` | 无 | ✅ `omit_context_docs: bool` |
| `maxTurns` | 无 (AgentConfig.max_rounds 不同) | ✅ `max_turns: Option<u32>` |
| `source` | 无 | ✅ `source: AgentSource` |
| `skills` | 无 | ✅ `skills: Vec<String>` |
| `permissionMode` | 无 | ⏳ Deferred (需 InteractionGate 支持) |
| `mcpServers` | 无 | ⏳ Deferred (per-agent MCP) |
| `hooks` | 无 | ⏳ Deferred (per-agent hooks) |

---

## Wave 1: AgentManifest 扩展 + BuiltinAgentRegistry

### T1: AgentManifest 新增字段

**Files:**
- Modify: `crates/octo-engine/src/agent/entry.rs:34-71`
- Modify: `crates/octo-engine/src/agent/yaml_def.rs:20-46`
- Test: `crates/octo-engine/src/agent/yaml_def.rs` (现有测试)

**Step 1: 扩展 AgentManifest**

在 `entry.rs` 的 `AgentManifest` 结构体中新增 7 个字段：

```rust
// crates/octo-engine/src/agent/entry.rs

/// Agent source type — how this agent was defined.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    /// Hardcoded in Rust (general-purpose, explore, plan, etc.)
    #[default]
    BuiltIn,
    /// Loaded from YAML file on disk
    Yaml,
    /// Loaded from plugin
    Plugin,
}

pub struct AgentManifest {
    // ... 现有字段 ...

    /// LLM-facing description for agent selection (CC-OSS: whenToUse).
    #[serde(default)]
    pub when_to_use: Option<String>,

    /// Tool blacklist — tools the agent cannot use.
    /// Applied after tool_filter (whitelist). CC-OSS: disallowedTools.
    #[serde(default)]
    pub disallowed_tools: Vec<String>,

    /// Run as background task (fire-and-forget). CC-OSS: background.
    #[serde(default)]
    pub background: bool,

    /// Skip CLAUDE.md/project docs injection for read-only agents.
    /// Saves tokens for Explore/Plan type agents. CC-OSS: omitClaudeMd.
    #[serde(default)]
    pub omit_context_docs: bool,

    /// Maximum conversation turns for this agent.
    /// Overrides max_iterations in SpawnSubAgentTool. CC-OSS: maxTurns.
    #[serde(default)]
    pub max_turns: Option<u32>,

    /// How this agent was defined.
    #[serde(default)]
    pub source: AgentSource,

    /// Skill names to preload into the agent's system prompt.
    #[serde(default)]
    pub skills: Vec<String>,
}
```

**Step 2: 扩展 AgentYamlDef**

在 `yaml_def.rs` 中新增对应字段和 `into_manifest()` 映射：

```rust
// crates/octo-engine/src/agent/yaml_def.rs

pub struct AgentYamlDef {
    // ... 现有字段 ...
    #[serde(default)]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
    #[serde(default)]
    pub background: bool,
    #[serde(default)]
    pub omit_context_docs: bool,
    #[serde(default)]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub skills: Vec<String>,
}

// into_manifest() 中追加映射
```

**Step 3: 修复编译**

确认所有构造 `AgentManifest` 的地方补齐新字段默认值：
- `yaml_def.rs:into_manifest()`
- `skills/execute_tool.rs:244` (Playbook 构造 manifest)
- `tools/subagent.rs` 中如有直接构造

**Step 4: 运行测试**

```bash
cargo test --workspace -- --test-threads=1 -q 2>&1 | tail -5
```

期望：所有现有测试通过，新字段有 `#[serde(default)]` 不影响反序列化。

**Step 5: 提交**

```
feat(agent): extend AgentManifest with CC-OSS parity fields
```

---

### T2: BuiltinAgentRegistry 模块

**Files:**
- Create: `crates/octo-engine/src/agent/builtin_agents.rs`
- Modify: `crates/octo-engine/src/agent/mod.rs` (pub mod)
- Modify: `crates/octo-engine/src/agent/runtime.rs` (注册调用)
- Test: `crates/octo-engine/src/agent/builtin_agents.rs`

**Step 1: 创建 builtin_agents.rs**

```rust
//! Built-in agent definitions — registered into AgentCatalog at startup.
//!
//! Equivalent to CC-OSS's builtInAgents.ts.
//! Each agent has a unique name, system prompt, tool restrictions, and model hint.

use super::catalog::AgentCatalog;
use super::entry::{AgentManifest, AgentSource};

/// Register all built-in agents into the catalog.
/// Called during AgentRuntime initialization.
/// Returns the number of agents registered.
pub fn register_builtin_agents(catalog: &AgentCatalog) -> usize {
    let agents = builtin_agent_manifests();
    let count = agents.len();
    for manifest in agents {
        catalog.register(manifest, None);
    }
    count
}

/// Return the list of all built-in agent manifests.
pub fn builtin_agent_manifests() -> Vec<AgentManifest> {
    vec![
        general_purpose_agent(),
        explore_agent(),
        plan_agent(),
        coder_agent(),
        reviewer_agent(),
        verification_agent(),
    ]
}

fn general_purpose_agent() -> AgentManifest {
    AgentManifest {
        name: "general-purpose".to_string(),
        tags: vec!["type:general".to_string(), "cap:code_search".to_string(), "cap:execute".to_string()],
        role: Some("General-purpose agent".to_string()),
        goal: Some("Complete multi-step tasks autonomously".to_string()),
        backstory: None,
        system_prompt: Some(GENERAL_PURPOSE_PROMPT.to_string()),
        model: None, // inherit parent model
        tool_filter: vec![], // all tools (empty = wildcard)
        disallowed_tools: vec![],
        when_to_use: Some(
            "General-purpose agent for researching complex questions, searching for code, \
             and executing multi-step tasks. When you are searching for a keyword or file \
             and are not confident that you will find the right match in the first few tries \
             use this agent to perform the search for you.".to_string()
        ),
        background: false,
        omit_context_docs: false,
        max_turns: None,
        source: AgentSource::BuiltIn,
        skills: vec![],
        ..Default::default()
    }
}

fn explore_agent() -> AgentManifest {
    AgentManifest {
        name: "explore".to_string(),
        tags: vec!["type:explore".to_string(), "cap:code_search".to_string()],
        role: Some("File search specialist".to_string()),
        goal: Some("Thoroughly explore codebases and find relevant code".to_string()),
        backstory: None,
        system_prompt: Some(EXPLORE_PROMPT.to_string()),
        model: Some("haiku".to_string()),
        tool_filter: vec![], // use disallowed_tools instead
        disallowed_tools: vec![
            "spawn_subagent".to_string(),
            "file_edit".to_string(),
            "file_write".to_string(),
            "notebook_edit".to_string(),
            "plan_mode".to_string(),
        ],
        when_to_use: Some(
            "Fast agent specialized for exploring codebases. Use this when you need to \
             quickly find files by patterns, search code for keywords, or answer questions \
             about the codebase. Specify thoroughness: \"quick\", \"medium\", or \"very thorough\".".to_string()
        ),
        background: false,
        omit_context_docs: true,
        max_turns: None,
        source: AgentSource::BuiltIn,
        skills: vec![],
        ..Default::default()
    }
}

fn plan_agent() -> AgentManifest {
    AgentManifest {
        name: "plan".to_string(),
        tags: vec!["type:plan".to_string(), "cap:architecture".to_string()],
        role: Some("Software architect and planning specialist".to_string()),
        goal: Some("Explore codebase and design implementation plans".to_string()),
        backstory: None,
        system_prompt: Some(PLAN_PROMPT.to_string()),
        model: None, // inherit
        tool_filter: vec![],
        disallowed_tools: vec![
            "spawn_subagent".to_string(),
            "file_edit".to_string(),
            "file_write".to_string(),
            "notebook_edit".to_string(),
            "plan_mode".to_string(),
        ],
        when_to_use: Some(
            "Software architect agent for designing implementation plans. Use this when you \
             need to plan the implementation strategy for a task. Returns step-by-step plans, \
             identifies critical files, and considers architectural trade-offs.".to_string()
        ),
        background: false,
        omit_context_docs: true,
        max_turns: None,
        source: AgentSource::BuiltIn,
        skills: vec![],
        ..Default::default()
    }
}

fn coder_agent() -> AgentManifest {
    AgentManifest {
        name: "coder".to_string(),
        tags: vec!["type:coder".to_string(), "cap:code_edit".to_string(), "cap:execute".to_string()],
        role: Some("Implementation specialist".to_string()),
        goal: Some("Write clean, efficient code following existing patterns".to_string()),
        backstory: None,
        system_prompt: Some(CODER_PROMPT.to_string()),
        model: None, // inherit
        tool_filter: vec![], // all tools
        disallowed_tools: vec![],
        when_to_use: Some(
            "Implementation agent for writing code changes. Use this when you need to create \
             or modify files, implement features, or fix bugs. The agent follows existing code \
             patterns and commits frequently.".to_string()
        ),
        background: false,
        omit_context_docs: false,
        max_turns: None,
        source: AgentSource::BuiltIn,
        skills: vec![],
        ..Default::default()
    }
}

fn reviewer_agent() -> AgentManifest {
    AgentManifest {
        name: "reviewer".to_string(),
        tags: vec!["type:reviewer".to_string(), "cap:code_review".to_string()],
        role: Some("Code review specialist".to_string()),
        goal: Some("Review code changes for correctness, security, and quality".to_string()),
        backstory: None,
        system_prompt: Some(REVIEWER_PROMPT.to_string()),
        model: None, // inherit
        tool_filter: vec![],
        disallowed_tools: vec![
            "file_edit".to_string(),
            "file_write".to_string(),
            "notebook_edit".to_string(),
        ],
        when_to_use: Some(
            "Code review agent for analyzing changes. Use this when you need a thorough review \
             of code quality, security, performance, and correctness. Returns structured feedback \
             with specific issues and suggestions.".to_string()
        ),
        background: true,
        omit_context_docs: false,
        max_turns: None,
        source: AgentSource::BuiltIn,
        skills: vec![],
        ..Default::default()
    }
}

fn verification_agent() -> AgentManifest {
    AgentManifest {
        name: "verification".to_string(),
        tags: vec!["type:verification".to_string(), "cap:testing".to_string()],
        role: Some("Verification specialist".to_string()),
        goal: Some("Try to break the implementation — verify correctness adversarially".to_string()),
        backstory: None,
        system_prompt: Some(VERIFICATION_PROMPT.to_string()),
        model: None, // inherit
        tool_filter: vec![],
        disallowed_tools: vec![
            "spawn_subagent".to_string(),
            "file_edit".to_string(),
            "file_write".to_string(),
            "notebook_edit".to_string(),
            "plan_mode".to_string(),
        ],
        when_to_use: Some(
            "Verification agent that tries to break implementations. Use after non-trivial tasks \
             (3+ file edits, backend/API changes). Pass the original task, files changed, and \
             approach taken. Returns PASS/FAIL/PARTIAL verdict with evidence.".to_string()
        ),
        background: true,
        omit_context_docs: false,
        max_turns: None,
        source: AgentSource::BuiltIn,
        skills: vec![],
        ..Default::default()
    }
}

// ─── System Prompts ────────────────────────────────────────────────────────

const GENERAL_PURPOSE_PROMPT: &str = r#"You are a general-purpose agent. Given the user's message, use the tools available to complete the task. Complete the task fully—don't gold-plate, but don't leave it half-done.

When you complete the task, respond with a concise report covering what was done and any key findings.

Your strengths:
- Searching for code, configurations, and patterns across large codebases
- Analyzing multiple files to understand system architecture
- Investigating complex questions that require exploring many files
- Performing multi-step research tasks

Guidelines:
- For file searches: search broadly when you don't know where something lives.
- For analysis: Start broad and narrow down. Use multiple search strategies.
- Be thorough: Check multiple locations, consider different naming conventions.
- NEVER create files unless absolutely necessary. Prefer editing existing files.
- NEVER proactively create documentation files unless explicitly requested."#;

const EXPLORE_PROMPT: &str = r#"You are a file search specialist. You excel at thoroughly navigating and exploring codebases.

=== CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS ===
You are STRICTLY PROHIBITED from:
- Creating, modifying, or deleting any files
- Running commands that change system state
- Using file edit or write tools

Your role is EXCLUSIVELY to search and analyze existing code.

Your strengths:
- Rapidly finding files using glob patterns
- Searching code and text with powerful regex patterns
- Reading and analyzing file contents

Guidelines:
- Use glob for broad file pattern matching
- Use grep for searching file contents with regex
- Use file_read when you know the specific file path
- Use bash ONLY for read-only operations (ls, git status, git log, git diff, find, cat)
- Adapt your search approach based on the thoroughness level specified by the caller
- Make efficient use of tools: spawn multiple parallel tool calls where possible

Complete the search request efficiently and report findings clearly."#;

const PLAN_PROMPT: &str = r#"You are a software architect and planning specialist. Your role is to explore the codebase and design implementation plans.

=== CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS ===
You are STRICTLY PROHIBITED from creating, modifying, or deleting any files.
Your role is EXCLUSIVELY to explore the codebase and design implementation plans.

## Your Process

1. **Understand Requirements**: Focus on the requirements and apply your assigned perspective.
2. **Explore Thoroughly**: Find existing patterns and conventions, understand current architecture, identify similar features as reference, trace through relevant code paths.
3. **Design Solution**: Create implementation approach, consider trade-offs and architectural decisions, follow existing patterns where appropriate.
4. **Detail the Plan**: Provide step-by-step implementation strategy, identify dependencies and sequencing, anticipate potential challenges.

## Required Output

End your response with:

### Critical Files for Implementation
List 3-5 files most critical for implementing this plan.

REMEMBER: You can ONLY explore and plan. You CANNOT write, edit, or modify any files."#;

const CODER_PROMPT: &str = r#"You are an implementation specialist. Write clean, efficient code that follows existing patterns in the codebase.

Guidelines:
- Read existing code before writing new code — understand patterns first
- Follow the existing code style, naming conventions, and architecture
- Write focused, minimal changes — do what was asked, nothing more
- Include proper error handling at system boundaries
- Keep functions short and focused (single responsibility)
- Test your changes by running relevant tests
- Commit frequently with descriptive messages

When you complete the task, provide a concise summary of what was changed and why."#;

const REVIEWER_PROMPT: &str = r#"You are a code review specialist. Review code changes for correctness, security, performance, and maintainability.

=== CRITICAL: READ-ONLY MODE - NO FILE MODIFICATIONS ===
You MUST NOT modify any files. Your role is to review and provide feedback only.

## Review Process

1. **Understand Context**: Read the changed files and surrounding code to understand the purpose.
2. **Check Correctness**: Verify logic, edge cases, error handling, and data flow.
3. **Check Security**: Look for injection vulnerabilities, auth issues, data exposure.
4. **Check Performance**: Identify potential bottlenecks, unnecessary allocations, N+1 queries.
5. **Check Maintainability**: Assess code clarity, naming, abstractions, and test coverage.

## Output Format

For each issue found:
```
### [SEVERITY] Issue Title
**File:** path/to/file:line
**Issue:** Description of the problem
**Suggestion:** How to fix it
```

Severity levels: CRITICAL, HIGH, MEDIUM, LOW, STYLE

End with a summary: total issues by severity, overall assessment (APPROVE/REQUEST_CHANGES/COMMENT)."#;

const VERIFICATION_PROMPT: &str = r#"You are a verification specialist. Your job is not to confirm the implementation works — it's to try to break it.

=== CRITICAL: DO NOT MODIFY THE PROJECT ===
You are STRICTLY PROHIBITED from creating, modifying, or deleting any files IN THE PROJECT DIRECTORY.
You MAY write ephemeral test scripts to /tmp via bash redirection when inline commands aren't sufficient.

## Verification Strategy

Adapt based on what was changed:
- **Backend/API**: Start server → curl endpoints → verify response shapes → test error handling → edge cases
- **CLI/script**: Run with representative inputs → verify stdout/stderr/exit codes → test edge inputs
- **Bug fixes**: Reproduce original bug → verify fix → regression tests → check side effects
- **Refactoring**: Existing tests MUST pass → diff public API surface → spot-check behavior

## Required Steps

1. Read CLAUDE.md / README for build/test commands
2. Run the build (broken build = automatic FAIL)
3. Run test suite (failing tests = automatic FAIL)
4. Run linters/type-checkers if configured
5. Apply type-specific verification

## Output Format

Every check:
```
### Check: [what you're verifying]
**Command run:** [exact command]
**Output observed:** [actual output]
**Result: PASS** (or FAIL with Expected vs Actual)
```

End with: VERDICT: PASS / VERDICT: FAIL / VERDICT: PARTIAL"#;
```

**Step 2: 注册到 mod.rs**

在 `crates/octo-engine/src/agent/mod.rs` 中添加 `pub mod builtin_agents;`

**Step 3: 在 AgentRuntime 初始化时调用注册**

在 `crates/octo-engine/src/agent/runtime.rs` 的 `build()` 方法中，在 YAML manifest 加载之前调用：

```rust
// 在 "17. Load declarative YAML agent definitions" 之前添加：
// 16.5 Register built-in agents
let builtin_count = crate::agent::builtin_agents::register_builtin_agents(&runtime.catalog);
tracing::info!(count = builtin_count, "Registered built-in agents");
```

**Step 4: 写测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_agent_count() {
        let manifests = builtin_agent_manifests();
        assert_eq!(manifests.len(), 6);
    }

    #[test]
    fn test_builtin_agents_have_unique_names() {
        let manifests = builtin_agent_manifests();
        let names: Vec<&str> = manifests.iter().map(|m| m.name.as_str()).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len());
    }

    #[test]
    fn test_builtin_agents_have_when_to_use() {
        for manifest in builtin_agent_manifests() {
            assert!(
                manifest.when_to_use.is_some(),
                "Agent '{}' missing when_to_use",
                manifest.name
            );
        }
    }

    #[test]
    fn test_builtin_agents_have_system_prompt() {
        for manifest in builtin_agent_manifests() {
            assert!(
                manifest.system_prompt.is_some(),
                "Agent '{}' missing system_prompt",
                manifest.name
            );
        }
    }

    #[test]
    fn test_explore_agent_is_read_only() {
        let explore = explore_agent();
        assert!(explore.disallowed_tools.contains(&"file_edit".to_string()));
        assert!(explore.disallowed_tools.contains(&"file_write".to_string()));
        assert!(explore.omit_context_docs);
    }

    #[test]
    fn test_verification_agent_is_background() {
        let verification = verification_agent();
        assert!(verification.background);
        assert!(verification.disallowed_tools.contains(&"file_edit".to_string()));
    }

    #[test]
    fn test_register_builtin_agents() {
        let catalog = AgentCatalog::new();
        let count = register_builtin_agents(&catalog);
        assert_eq!(count, 6);
        assert!(catalog.get_by_name("general-purpose").is_some());
        assert!(catalog.get_by_name("explore").is_some());
        assert!(catalog.get_by_name("plan").is_some());
        assert!(catalog.get_by_name("coder").is_some());
        assert!(catalog.get_by_name("reviewer").is_some());
        assert!(catalog.get_by_name("verification").is_some());
    }

    #[test]
    fn test_all_builtin_agents_source_is_builtin() {
        for manifest in builtin_agent_manifests() {
            assert_eq!(manifest.source, AgentSource::BuiltIn, "Agent '{}' has wrong source", manifest.name);
        }
    }
}
```

**Step 5: 运行测试**

```bash
cargo test -p octo-engine builtin_agents -- --test-threads=1 -v
```

**Step 6: 提交**

```
feat(agent): add BuiltinAgentRegistry with 6 CC-OSS-aligned agents
```

---

## Wave 2: SpawnSubAgentTool agent_type 路由

### T3: ToolRegistry 新增 snapshot_excluded 方法

**Files:**
- Modify: `crates/octo-engine/src/tools/mod.rs:93-151`
- Test: 内联

**Step 1: 添加 snapshot_excluded**

```rust
// crates/octo-engine/src/tools/mod.rs — ToolRegistry impl

/// Create a snapshot excluding the named tools (blacklist filter).
pub fn snapshot_excluded(&self, exclude: &[String]) -> ToolRegistry {
    let exclude_set: std::collections::HashSet<&str> =
        exclude.iter().map(|s| s.as_str()).collect();
    let mut registry = ToolRegistry::new();
    for (name, tool) in self.tools.iter() {
        if !exclude_set.contains(name.as_str()) {
            registry.tools.insert(name.clone(), tool.clone());
        }
    }
    registry
}
```

**Step 2: 添加测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_excluded() {
        let mut registry = ToolRegistry::new();
        registry.register(SleepTool);
        registry.register(DoctorTool);
        registry.register(NotifierTool);

        let excluded = registry.snapshot_excluded(&["sleep".to_string()]);
        assert!(excluded.get("sleep").is_none());
        assert!(excluded.get("doctor").is_some());
        assert!(excluded.get("notifier").is_some());
    }

    #[test]
    fn test_snapshot_excluded_empty_list() {
        let mut registry = ToolRegistry::new();
        registry.register(SleepTool);
        let excluded = registry.snapshot_excluded(&[]);
        assert_eq!(excluded.names().len(), 1);
    }
}
```

**Step 3: 运行测试**

```bash
cargo test -p octo-engine tools::tests -- --test-threads=1 -v
```

**Step 4: 提交**

```
feat(tools): add ToolRegistry::snapshot_excluded for blacklist filtering
```

---

### T4: SpawnSubAgentTool 支持 agent_type 路由

**Files:**
- Modify: `crates/octo-engine/src/tools/subagent.rs`
- Test: 内联测试

**核心改动：**

SpawnSubAgentTool 新增 `agent_type` 可选参数。当提供时：
1. 从 AgentCatalog 查找 manifest
2. 应用 manifest 的工具过滤（tool_filter + disallowed_tools）
3. 使用 manifest 的模型覆盖
4. 使用 manifest 的 system_prompt
5. 使用 manifest 的 max_turns

**Step 1: 扩展 SpawnSubAgentTool 构造函数**

```rust
pub struct SpawnSubAgentTool {
    subagent_manager: Arc<SubAgentManager>,
    parent_config: Arc<AgentLoopConfig>,
    catalog: Option<Arc<AgentCatalog>>,  // 新增
}

impl SpawnSubAgentTool {
    pub fn new(manager: Arc<SubAgentManager>, config: Arc<AgentLoopConfig>) -> Self {
        Self {
            subagent_manager: manager,
            parent_config: config,
            catalog: None,
        }
    }

    pub fn with_catalog(mut self, catalog: Arc<AgentCatalog>) -> Self {
        self.catalog = Some(catalog);
        self
    }
}
```

**Step 2: 扩展 parameters()**

```rust
fn parameters(&self) -> serde_json::Value {
    json!({
        "type": "object",
        "required": ["task"],
        "properties": {
            "task": {
                "type": "string",
                "description": "Description of the task for the sub-agent"
            },
            "agent_type": {
                "type": "string",
                "description": "Optional agent type name. When provided, uses the agent's \
                    configured tools, model, and system prompt from the agent catalog. \
                    Built-in types: general-purpose, explore, plan, coder, reviewer, verification"
            },
            "max_iterations": {
                "type": "integer",
                "description": "Max LLM iterations (default: 10, overridden by agent_type's max_turns)"
            },
            "tools_whitelist": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional tool whitelist (overridden when agent_type is provided)"
            }
        }
    })
}
```

**Step 3: 改造 execute() — 添加 agent_type 路由逻辑**

```rust
async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
    let task = params["task"].as_str().unwrap_or("No task specified").to_string();
    let agent_type = params["agent_type"].as_str();
    let max_iterations = params["max_iterations"].as_u64().unwrap_or(10) as u32;

    // Check recursion depth & concurrent limit (unchanged)
    let child_mgr = match self.subagent_manager.child() { ... };
    if !self.subagent_manager.can_spawn().await { ... }

    // Resolve manifest from agent_type (if provided)
    let manifest = if let Some(agent_type) = agent_type {
        match &self.catalog {
            Some(catalog) => catalog.get_by_name(agent_type).map(|e| e.manifest),
            None => None,
        }
    } else {
        None
    };

    let subagent_id = format!("sa-{}", uuid::Uuid::new_v4());
    let desc = if let Some(ref m) = manifest {
        format!("[{}] {}", m.name, &task[..task.len().min(80)])
    } else {
        task.clone()
    };
    self.subagent_manager.register(subagent_id.clone(), desc).await?;

    // Resolve tools: manifest overrides → coordinator filter → LLM whitelist
    let tools = self.resolve_tools(&params, &manifest);

    // Resolve model: manifest.model → parent model
    let model = manifest.as_ref()
        .and_then(|m| m.model.as_ref())
        .filter(|m| m.as_str() != "inherit")
        .cloned()
        .unwrap_or_else(|| self.parent_config.model.clone());

    // Resolve max_iterations: manifest.max_turns → param → default
    let max_iter = manifest.as_ref()
        .and_then(|m| m.max_turns)
        .unwrap_or(max_iterations);

    // Build system prompt from manifest (if available)
    let manifest_for_config = manifest.clone().map(|mut m| {
        // Prepend task to system prompt
        if let Some(ref sp) = m.system_prompt {
            m.system_prompt = Some(format!("{}\n\n## Your Task\n{}", sp, task));
        }
        m
    });

    let child_config = AgentLoopConfig {
        max_iterations: max_iter,
        provider: self.parent_config.provider.clone(),
        tools,
        memory: self.parent_config.memory.clone(),
        model,
        session_id: octo_types::SessionId::from_string(subagent_id.clone()),
        user_id: self.parent_config.user_id.clone(),
        sandbox_id: self.parent_config.sandbox_id.clone(),
        tool_ctx: self.parent_config.tool_ctx.clone(),
        manifest: manifest_for_config,
        subagent_manager: Some(child_mgr),
        ..AgentLoopConfig::default()
    };

    // Spawn async (unchanged pattern)
    let messages = vec![ChatMessage::user(&task)];
    let mgr = self.subagent_manager.clone();
    let sa_id = subagent_id.clone();
    tokio::spawn(async move { ... }); // same as before

    Ok(ToolOutput::success(json!({
        "session_id": subagent_id,
        "status": "spawned",
        "agent_type": agent_type,
    }).to_string()))
}

impl SpawnSubAgentTool {
    /// Resolve tool set for the child agent.
    /// Priority: manifest (tool_filter + disallowed_tools) → coordinator → LLM whitelist
    fn resolve_tools(
        &self,
        params: &serde_json::Value,
        manifest: &Option<AgentManifest>,
    ) -> Option<Arc<ToolRegistry>> {
        let parent_tools = self.parent_config.tools.as_ref()?;

        if let Some(manifest) = manifest {
            // Step 1: Apply tool_filter (whitelist)
            let base = if manifest.tool_filter.is_empty() {
                parent_tools.snapshot()
            } else {
                parent_tools.snapshot_filtered(&manifest.tool_filter)
            };
            // Step 2: Apply disallowed_tools (blacklist)
            let filtered = if manifest.disallowed_tools.is_empty() {
                base
            } else {
                base.snapshot_excluded(&manifest.disallowed_tools)
            };
            Some(Arc::new(filtered))
        } else {
            // Fallback: existing behavior (coordinator + LLM whitelist)
            // ... existing coordinator_filter + tools_whitelist logic ...
        }
    }
}
```

**Step 4: 在 AgentExecutor 中传入 catalog**

需要确认 `SpawnSubAgentTool` 创建时能接收 catalog。查看 `agent/executor.rs` 中构建 tools_snapshot 的位置，添加 `.with_catalog(catalog.clone())`。

**Step 5: 测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_tools_with_manifest_whitelist() {
        // manifest.tool_filter = ["bash", "file_read"]
        // → only bash and file_read
    }

    #[test]
    fn test_resolve_tools_with_manifest_blacklist() {
        // manifest.disallowed_tools = ["file_edit"]
        // → all tools except file_edit
    }

    #[test]
    fn test_resolve_tools_whitelist_and_blacklist() {
        // tool_filter = ["bash", "file_read", "file_edit"]
        // disallowed_tools = ["file_edit"]
        // → bash and file_read only
    }

    #[test]
    fn test_resolve_tools_no_manifest_uses_existing_behavior() {
        // No manifest → fallback to coordinator/LLM whitelist
    }
}
```

**Step 6: 运行测试**

```bash
cargo test -p octo-engine subagent -- --test-threads=1 -v
```

**Step 7: 提交**

```
feat(tools): SpawnSubAgentTool agent_type routing with catalog lookup
```

---

## Wave 3: Skill Preloading + 集成测试

### T5: Skill Preloading 到 Agent System Prompt

**Files:**
- Modify: `crates/octo-engine/src/agent/builtin_agents.rs`
- Modify: `crates/octo-engine/src/tools/subagent.rs`
- Test: 内联

**Step 1: 添加 skill preloading 辅助函数**

```rust
// crates/octo-engine/src/agent/builtin_agents.rs

use crate::skills::SkillRegistry;

/// Resolve skill bodies from the registry and append to system prompt.
/// Returns the enhanced system prompt with skill instructions appended.
pub fn preload_skills_into_prompt(
    system_prompt: &str,
    skill_names: &[String],
    skill_registry: &SkillRegistry,
) -> String {
    if skill_names.is_empty() {
        return system_prompt.to_string();
    }

    let mut sections = Vec::new();
    for name in skill_names {
        if let Some(skill) = skill_registry.get(name) {
            if !skill.body.is_empty() {
                sections.push(format!("## Skill: {}\n\n{}", skill.name, skill.body));
            }
        } else {
            tracing::warn!(skill = %name, "Skill not found in registry for preloading");
        }
    }

    if sections.is_empty() {
        return system_prompt.to_string();
    }

    format!(
        "{}\n\n---\n\n# Preloaded Skills\n\n{}",
        system_prompt,
        sections.join("\n\n---\n\n")
    )
}
```

**Step 2: 在 SpawnSubAgentTool 中使用**

在 `execute()` 中，构建 `manifest_for_config` 时，如果 manifest 有 skills 且 skill_registry 可用：

```rust
// SpawnSubAgentTool 新增字段
pub struct SpawnSubAgentTool {
    // ...
    skill_registry: Option<Arc<SkillRegistry>>,
}

// execute() 中
if let Some(ref manifest) = manifest {
    if !manifest.skills.is_empty() {
        if let Some(ref sr) = self.skill_registry {
            let enhanced = preload_skills_into_prompt(
                manifest.system_prompt.as_deref().unwrap_or(""),
                &manifest.skills,
                sr,
            );
            // Update system_prompt in manifest_for_config
        }
    }
}
```

**Step 3: 测试**

```rust
#[test]
fn test_preload_skills_into_prompt() {
    let base = "You are a coder.";
    let result = preload_skills_into_prompt(base, &[], &empty_registry());
    assert_eq!(result, base);
}

#[test]
fn test_preload_skills_appends_body() {
    let registry = mock_registry_with_skill("review", "Always check error handling.");
    let result = preload_skills_into_prompt(
        "You are a coder.",
        &["review".to_string()],
        &registry,
    );
    assert!(result.contains("Always check error handling."));
    assert!(result.contains("# Preloaded Skills"));
}
```

**Step 4: 提交**

```
feat(agent): skill preloading into agent system prompts
```

---

### T6: SpawnSubAgentTool description 增强

**Files:**
- Modify: `crates/octo-engine/src/tools/prompts.rs`
- Test: 无（prompt 文本改动）

**改动：** 更新 `SUBAGENT_DESCRIPTION` 常量，添加 `agent_type` 参数说明和内建 agent 列表。让 LLM 知道可以用 `agent_type` 指定专用 agent。

```rust
pub const SUBAGENT_DESCRIPTION: &str = r#"Launch a new agent to handle complex, multi-step tasks autonomously.

The Agent tool launches specialized agents that autonomously handle complex tasks. Each agent type has specific capabilities and tools available to it.

## Built-in Agent Types

| agent_type | Capability | Tools |
|------------|-----------|-------|
| general-purpose | Research, search, execute multi-step tasks | All tools |
| explore | Fast read-only codebase search | Read-only (no edit/write) |
| plan | Architecture planning and design | Read-only (no edit/write) |
| coder | Code implementation | All tools |
| reviewer | Code review and quality analysis | Read-only (no edit/write) |
| verification | Adversarial testing and verification | Read-only + Bash |

## Usage

- Set `agent_type` to use a specialized agent with pre-configured tools and behavior
- Omit `agent_type` for a generic sub-agent that inherits parent's tools
- The agent runs asynchronously; use `query_subagent` to check status/results

## When NOT to use

- Simple file reads → use file_read directly
- Searching for a specific class → use grep/glob directly
- Tasks that need <3 tool calls → do it yourself"#;
```

**提交：**

```
feat(tools): enhance SpawnSubAgentTool description with agent_type docs
```

---

### T7: 编译验证 + 全量测试

**Step 1: 编译**

```bash
cargo check --workspace
```

**Step 2: 运行全量测试**

```bash
cargo test --workspace -- --test-threads=1 -q 2>&1 | tail -10
```

**Step 3: 提交**

```
feat(agent): Phase AX — builtin agents with CC-OSS alignment
```

---

## Deferred Items

| ID | 描述 | 前置条件 |
|----|------|---------|
| AX-D1 | per-agent permissionMode (InteractionGate 覆盖) | InteractionGate 支持 per-session 模式 |
| AX-D2 | per-agent MCP servers (附加专属 MCP) | McpManager 支持 scoped connections |
| AX-D3 | per-agent hooks (agent 级 hook 注册/清理) | Hook 系统支持 scoped registration |
| AX-D4 | ExecuteSkillTool 后台模式 (fire-and-forget playbook) | background 标记 + QuerySubAgentTool 集成 |
| AX-D5 | YAML agent 文件目录创建 (agents/*.yaml) | Phase AX 代码稳定后 | ✅ 已补 @ f941325 |
| AX-D6 | omit_context_docs 在 SystemPromptBuilder 中实现 | SystemPromptBuilder 重构支持条件段 | ✅ 已补 @ 25d1568 |
| AX-D7 | Agent 级 effort 控制 | Effort 系统实现 |

---

## 任务总览

| Wave | Task | 描述 | 预估行数 | 测试数 |
|------|------|------|---------|--------|
| W1 | T1 | AgentManifest 扩展 7 字段 | ~60 | 编译通过 |
| W1 | T2 | BuiltinAgentRegistry + 6 agents + prompts | ~450 | 8 |
| W2 | T3 | ToolRegistry::snapshot_excluded | ~20 | 2 |
| W2 | T4 | SpawnSubAgentTool agent_type 路由 | ~150 | 4 |
| W3 | T5 | Skill preloading | ~40 | 2 |
| W3 | T6 | SpawnSubAgentTool description 增强 | ~30 | 0 |
| W3 | T7 | 编译验证 + 全量测试 | 0 | 全量 |
| | **Total** | | **~750** | **~16** |
