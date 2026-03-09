# Agent Skills 全功能支持设计方案

**版本**: 1.0
**日期**: 2026-03-09
**状态**: 草稿
**范围**: octo-engine skills 模块完整能力对齐

---

## 1. 概述

### 1.1 设计目标

Agent Skills 是 AI Agent 系统的公开标准扩展机制，使 Agent 能够按需加载特定领域的行为指令、工具约束和执行策略。本文档定义 octo-sandbox 实现 Agent Skills 全功能支持的技术方案，目标包括：

1. **完整的目录文件结构标准** — SKILL.md 格式 + 多级目录发现
2. **延迟加载** — 索引优先，按需全量解析
3. **分级动态加载** — 项目级 > 用户级 > 系统级，支持信任等级
4. **skill 内脚本执行** — Python / Node.js / WASM 多运行时
5. **跨 skills / tools 的协同执行** — allowed-tools 运行时强制 + skill 组合调用

### 1.2 参考项目评分矩阵

以下评分基于对 8 个 harness 项目的代码分析（满分 5 分）：

| 能力维度 | ironclaw | openfang | moltis | zeroclaw | localgpt | goose | autoagents | pi_agent |
|---------|---------|---------|--------|---------|---------|------|-----------|---------|
| 标准目录结构 | ★★★★★ | ★★★★☆ | ★★★★★ | ★★★★☆ | ★★★☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 延迟加载 | ★★★★☆ | ★★★★★ | ★★★★☆ | ★★★☆☆ | ★★☆☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 分级动态加载 | ★★★★★ | ★★★★★ | ★★★★☆ | ★★★☆☆ | ★★★☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 语义路由/选择 | ★★★★★ | ★★★★★ | ★★★☆☆ | ★★★☆☆ | ★★★★☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 信任模型 | ★★★★★ | ★★★★★ | ★★★☆☆ | ★★☆☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 工具约束强制 | ★★★★★ | ★★★★★ | ★★☆☆☆ | ★★★☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| skill 内脚本执行 | ★★★☆☆ | ★★★★★ | ★★★☆☆ | ★★★★☆ | ★★★☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 多运行时支持 | ★★★☆☆ | ★★★★★ | ★★☆☆☆ | ★★★☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ��☆☆☆☆ | ★☆☆☆☆ |
| 跨 skill 组合 | ★★★☆☆ | ★★★★☆ | ★★☆☆☆ | ★★★☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 热重载 | ★★★★☆ | ★★★☆☆ | ★★★★★ | ★★★☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 注册表/ClawHub | ★★★★★ | ★★★★★ | ★★★★☆ | ★★★★☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| 生命周期 hooks | ★★★★★ | ★★★★☆ | ★★★☆☆ | ★★☆☆☆ | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ |
| **综合得分** | **54/65** | **56/65** | **42/65** | **38/65** | **32/65** | **14/65** | **8/65** | **8/65** |

### 1.3 最佳实现总结

- **ironclaw** — 最佳信任模型 + 选择管线（4 阶段 Gate→Score→Budget→Attenuate）
- **openfang** — 最佳运行时支持（Python/WASM/Node/Builtin/PromptOnly）+ JSON Schema 工具定义
- **moltis** — 最佳注册表管理（原子写入 manifest + 文件监视热重载）
- **zeroclaw** — Skill 内置工具定义（TOML 内联 shell/http/script 工具）
- **localgpt** — 斜杠命令路由 + emoji 元数据 + `always` 优先标志

---

## 2. octo-sandbox 现状评估

### 2.1 已实现功能（覆盖率 ~35%）

| 功能 | 现状 | 文件 |
|-----|------|------|
| SKILL.md 解析 | ✅ 完整 YAML frontmatter + markdown body | `skills/loader.rs` |
| 延迟加载 | ✅ `build_index()` + `load_skill()` 分离 | `skills/index.rs` |
| 多目录发现 | ✅ 项目级 `.octo/skills/` > 用户级 | `skills/loader.rs` |
| 热重载 | ✅ `notify_debouncer_mini` 300ms 防抖 | `skills/registry.rs` |
| Python 脚本 | ✅ `PythonRuntime` 基础实现 | `skill_runtime/python.rs` |
| SkillTool 包装 | ✅ 将 skill 包装为 `Tool` trait | `skills/tool.rs` |
| 结构验证 | ✅ `validate_skill_structure()` | `skills/standards.rs` |
| 路径遍历保护 | ✅ `canonicalize()` 验证 | `skills/loader.rs` |

### 2.2 缺失功能（需要补全）

| 功能 | 缺失描述 | 优先级 |
|-----|---------|-------|
| **信任等级强制** | `Trusted`/`Installed` 字段解析了但不影响工具访问 | P0 |
| **语义选择** | `SkillRegistry::get()` 仅精确匹配，无 fuzzy/向量搜索 | P0 |
| **工具约束运行时强制** | `allowed-tools` 解析后不传给 `ToolRegistry` | P0 |
| **Node.js 运行时** | `SkillRuntimeBridge` 只有 Python | P1 |
| **WASM 运行时** | skill 脚本无 WASM 执行路径 | P1 |
| **Skill 组合/依赖** | 无 `depends-on` 字段和依赖图 | P1 |
| **跨 skill 调用** | Skill 内无法显式触发另一个 skill | P1 |
| **注册表/ClawHub** | 无远程 skill 发现和安装 | P2 |
| **capability 信任层** | 无 Builtin > User > Remote 三层信任 | P2 |
| **生命周期 hooks 集成** | Skill 激活/停用未触发 HookRegistry | P2 |
| **Slash 命令路由** | 无 `/skill-name` 快速激活语法 | P2 |
| **成本预算强制** | `max_context_tokens` 字段解析但不强制 | P3 |

---

## 3. 核心架构设计

### 3.1 分层架构

```
┌─────────────────────────────────────────────────────────┐
│                   Skill Discovery Layer                  │
│  SkillCatalog (ClawHub) → SkillStore (SQLite) →         │
│  SkillLoader (filesystem) → SkillIndex (frontmatter)    │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  Skill Selection Layer                   │
│  SkillSelector: Gate → Score → Budget → Attenuate        │
│  + HybridQueryEngine (HNSW vector + FTS keyword)        │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  Skill Execution Layer                   │
│  SkillContext → ToolConstraintEnforcer →                 │
│  SkillRuntimeBridge (Python/Node/WASM/Builtin)           │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  Integration Layer                       │
│  HookRegistry (9 hooks) → AgentLoop →                   │
│  SafetyLayer → ToolRegistry → EventBus                  │
└─────────────────────────────────────────────────────────┘
```

### 3.2 数据流

```
用户输入
  │
  ▼
SkillSelector.select(message, context)
  ├─ Phase 1: Gate    — 检查 bins/env/config 前置条件
  ├─ Phase 2: Score   — 关键词+正则+向量综合评分
  ├─ Phase 3: Budget  — 按 max_context_tokens 截断
  └─ Phase 4: Attenuate — 按信任等级裁剪工具访问
         │
         ▼
    激活的 Skills (Vec<ActiveSkill>)
         │
         ▼
AgentLoop.build_context()
  ├─ 注入 <skill_context> XML 块到 system prompt
  ├─ 附加 ToolConstraints 到 ToolRegistry 过滤器
  └─ 触发 HookRegistry::on_skills_activated()
         │
         ▼
    LLM 调用 + 工具执行（受约束）
```

---

## 4. SKILL.md 格式标准

### 4.1 完整格式定义

```yaml
---
# ── 必填字段 ──
name: skill-name          # 唯一标识符，kebab-case
version: "1.0.0"          # Semantic versioning
description: "简短描述"   # 一句话描述

# ── 激活条件 ──
activation:
  patterns:               # 正则表达式列表（任一匹配即激活）
    - "deploy to.*production"
    - "release.*version"
  keywords:               # 关键词列表（任一出现即激活）
    - "deployment"
    - "kubernetes"
  always: false           # 是否始终激活（优先于其他条件）
  slash_command: "/deploy" # 斜杠命令触发（可选）
  max_context_tokens: 2000 # 激活后注入的最大 token 数

# ── 工具访问控制 ──
allowed_tools:             # 允许的工具列表（空 = 继承信任等级默认值）
  - bash
  - file_read
  - file_write
  - mcp:kubectl-server:*  # MCP server 工具通配

denied_tools:              # 显式拒绝的工具（高于 allowed_tools）
  - http_request           # 防止 skill 发起外部请求

# ── 依赖声明 ──
depends_on:                # 依赖的其他 skills（激活时自动加载）
  - base-coding            # skill 名称
  - security-review

# ── 运行时脚本 ──
scripts:
  setup:                   # setup 脚本（skill 首次激活时运行）
    runtime: python        # python | node | wasm | shell
    path: scripts/setup.py
  validate:                # validate 脚本（工具执行前调用）
    runtime: node
    path: scripts/validate.js
  cleanup:                 # cleanup 脚本（skill 停用时运行）
    runtime: python
    path: scripts/cleanup.py

# ── 信任与来源 ──
trust: trusted             # trusted | installed | remote
source: user               # user | project | system | registry

# ── 前置条件 ──
requires:
  bins: [docker, kubectl]  # 必须在 PATH 中存在的可执行文件
  env: [KUBECONFIG]        # 必须设置的环境变量
  config: []               # 必须存在的配置键

# ── 元数据 ──
metadata:
  author: "team@example.com"
  tags: [devops, kubernetes, deployment]
  icon: "🚀"
  homepage: "https://example.com/skill"
  registry: clawhub        # clawhub | local | private
---

# Deployment Skill

这里是注入到 LLM system prompt 的 Markdown 内容。

## 行为指令

当用户请求部署时：
1. 先检查当前 kubernetes 上下文
2. 验证配置文件是否存在
3. ...
```

### 4.2 目录结构标准

```
<project>/
└── .octo/
    └── skills/
        └── <skill-name>/
            ├── SKILL.md          # 必须存在（frontmatter + body）
            └── scripts/          # 可选：运行时脚本
                ├── setup.py
                ├── validate.js
                └── cleanup.wasm

~/.octo/                          # 用户全局 skills（可信等级: trusted）
└── skills/
    └── <skill-name>/
        └── SKILL.md

~/.octo/installed/                # 从注册表安装的 skills（可信等级: installed）
└── skills/
    └── <skill-name>/
        ├── SKILL.md
        └── SKILL.sig             # 签名文件（注册表安装时验证完整性）
```

---

## 5. SkillSelector — 四阶段选择管线

参考 ironclaw 的 4 阶段确定性管线（Gate → Score → Budget → Attenuate），结合 octo-sandbox 的 HybridQueryEngine：

### 5.1 Phase 1: Gate（前置条件检查）

```rust
// crates/octo-engine/src/skills/selector.rs

pub struct SkillGate;

impl SkillGate {
    /// 检查 skill 的前置条件，返回是否通过
    pub fn check(&self, skill: &SkillDefinition) -> GateResult {
        // 1. 检查 bins
        if let Some(bins) = skill.requires.as_ref().and_then(|r| r.bins.as_ref()) {
            for bin in bins {
                if which::which(bin).is_err() {
                    return GateResult::Blocked(format!("missing binary: {}", bin));
                }
            }
        }
        // 2. 检查 env
        if let Some(envs) = skill.requires.as_ref().and_then(|r| r.env.as_ref()) {
            for env in envs {
                if std::env::var(env).is_err() {
                    return GateResult::Blocked(format!("missing env: {}", env));
                }
            }
        }
        GateResult::Passed
    }
}

pub enum GateResult {
    Passed,
    Blocked(String), // 阻止原因
}
```

### 5.2 Phase 2: Score（综合评分）

三级评分，分数越高越优先：

```rust
pub struct SkillScorer {
    vector_engine: Option<Arc<HybridQueryEngine>>, // 可选语义搜索
}

impl SkillScorer {
    pub async fn score(&self, skill: &SkillDefinition, query: &str) -> f32 {
        let mut score = 0.0f32;

        // Tier 1: always 标志（直接得满分）
        if skill.activation.always == Some(true) {
            return 1000.0;
        }

        // Tier 2: 斜杠命令精确匹配
        if let Some(cmd) = &skill.activation.slash_command {
            if query.starts_with(cmd.as_str()) {
                return 900.0;
            }
        }

        // Tier 3: 关键词匹配（每命中 +10 分）
        if let Some(keywords) = &skill.activation.keywords {
            for kw in keywords {
                if query.to_lowercase().contains(&kw.to_lowercase()) {
                    score += 10.0;
                }
            }
        }

        // Tier 4: 正则匹配（每命中 +20 分）
        if let Some(patterns) = &skill.activation.patterns {
            for pat in patterns {
                if let Ok(re) = regex::Regex::new(pat) {
                    if re.is_match(query) {
                        score += 20.0;
                    }
                }
            }
        }

        // Tier 5: 语义向量搜索（0.0-50.0 分）
        if let Some(engine) = &self.vector_engine {
            if let Ok(results) = engine.search(query, 1).await {
                if let Some(top) = results.first() {
                    // 仅当该结果对应此 skill 时才加分
                    if top.source.contains(&skill.name) {
                        score += top.score * 50.0;
                    }
                }
            }
        }

        score
    }
}
```

### 5.3 Phase 3: Budget（Token 预算）

```rust
pub struct SkillBudget {
    max_tokens: usize,
}

impl SkillBudget {
    /// 按评分降序选取，直到达到 token 预算上限
    pub fn select(&self, scored: &[(f32, &SkillDefinition)]) -> Vec<&SkillDefinition> {
        let mut selected = Vec::new();
        let mut used_tokens = 0usize;

        for (_, skill) in scored {
            let skill_tokens = skill.activation.max_context_tokens.unwrap_or(500);
            if used_tokens + skill_tokens <= self.max_tokens {
                selected.push(*skill);
                used_tokens += skill_tokens;
            }
        }
        selected
    }
}
```

### 5.4 Phase 4: Attenuate（信任衰减）

```rust
pub struct SkillAttenuator;

/// 根据信任等级确定工具访问上限
impl SkillAttenuator {
    pub fn attenuate(
        &self,
        skill: &SkillDefinition,
        available_tools: &[String],
    ) -> Vec<String> {
        let trust = skill.trust.unwrap_or(SkillTrust::Installed);

        // 基础允许列表（来自 skill 的 allowed_tools 声明）
        let allowed = match &skill.allowed_tools {
            Some(list) if !list.is_empty() => list.clone(),
            _ => self.default_allowed_for_trust(trust),
        };

        // 信任等级上限（Installed 级别禁止危险工具）
        let ceiling = self.tool_ceiling_for_trust(trust);

        // 取交集：allowed ∩ ceiling ∩ available_tools
        available_tools
            .iter()
            .filter(|t| {
                allowed.contains(t) && ceiling.contains(t)
                    && !skill.denied_tools.as_ref().map_or(false, |d| d.contains(t))
            })
            .cloned()
            .collect()
    }

    fn tool_ceiling_for_trust(&self, trust: SkillTrust) -> Vec<String> {
        match trust {
            SkillTrust::Trusted => vec!["*".to_string()], // 所有工具
            SkillTrust::Installed => vec![
                "file_read".to_string(),
                "memory_read".to_string(),
                "memory_search".to_string(),
                // 无 bash / file_write / http_request
            ],
            SkillTrust::Remote => vec![
                "memory_search".to_string(),
                // 最小权限
            ],
        }
    }

    fn default_allowed_for_trust(&self, trust: SkillTrust) -> Vec<String> {
        self.tool_ceiling_for_trust(trust)
    }
}
```

### 5.5 统一入口

```rust
pub struct SkillSelector {
    gate: SkillGate,
    scorer: SkillScorer,
    budget: SkillBudget,
    attenuator: SkillAttenuator,
}

pub struct ActiveSkill {
    pub definition: SkillDefinition,
    pub allowed_tools: Vec<String>, // 衰减后的工具列表
    pub score: f32,
}

impl SkillSelector {
    pub async fn select(
        &self,
        skills: &[SkillDefinition],
        query: &str,
        available_tools: &[String],
    ) -> Vec<ActiveSkill> {
        // Phase 1: Gate
        let gated: Vec<_> = skills
            .iter()
            .filter(|s| matches!(self.gate.check(s), GateResult::Passed))
            .collect();

        // Phase 2: Score
        let mut scored: Vec<(f32, &&SkillDefinition)> = futures::future::join_all(
            gated.iter().map(|s| async { (self.scorer.score(s, query).await, s) })
        ).await;
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Phase 3: Budget
        let selected = self.budget.select(&scored.iter().map(|(f, s)| (*f, **s)).collect::<Vec<_>>());

        // Phase 4: Attenuate
        selected
            .into_iter()
            .map(|skill| {
                let allowed_tools = self.attenuator.attenuate(skill, available_tools);
                ActiveSkill {
                    definition: skill.clone(),
                    allowed_tools,
                    score: scored.iter().find(|(_, s)| s.name == skill.name).map(|(f, _)| *f).unwrap_or(0.0),
                }
            })
            .collect()
    }
}
```

---

## 6. ToolConstraintEnforcer — 工具约束运行时强制

### 6.1 设计原则

现有代码仅在加载时验证 `allowed_tools` 字段是否引用了合法工具名，但不在运行时拦截违规工具调用。需要在 `AgentLoop` 执行工具前插入约束检查。

### 6.2 实现方案

```rust
// crates/octo-engine/src/skills/constraint.rs

pub struct ToolConstraintEnforcer {
    /// Map<tool_name, is_allowed>，由激活的 skill 合并生成
    allowed: HashSet<String>,
    denied: HashSet<String>,
    trust_level: SkillTrust,
}

impl ToolConstraintEnforcer {
    /// 从激活的 skills 构建约束合集
    pub fn from_active_skills(active_skills: &[ActiveSkill]) -> Self {
        let mut allowed = HashSet::new();
        let mut denied = HashSet::new();
        let min_trust = active_skills
            .iter()
            .map(|s| s.definition.trust.unwrap_or(SkillTrust::Installed))
            .min()
            .unwrap_or(SkillTrust::Installed);

        for skill in active_skills {
            for tool in &skill.allowed_tools {
                allowed.insert(tool.clone());
            }
            if let Some(d) = &skill.definition.denied_tools {
                for tool in d {
                    denied.insert(tool.clone());
                }
            }
        }

        Self { allowed, denied, trust_level: min_trust }
    }

    /// 检查工具是否被允许
    pub fn check(&self, tool_name: &str) -> ConstraintResult {
        // 显式拒绝优先
        if self.denied.contains(tool_name) {
            return ConstraintResult::Denied(format!("{} is explicitly denied by active skill", tool_name));
        }

        // 无激活 skill 时不限制（向后兼容）
        if self.allowed.is_empty() {
            return ConstraintResult::Allowed;
        }

        // 通配符 "*"
        if self.allowed.contains("*") {
            return ConstraintResult::Allowed;
        }

        // 精确匹配或 MCP 通配（mcp:server-name:*）
        if self.allowed.contains(tool_name)
            || self.allowed.iter().any(|pat| glob_match(pat, tool_name))
        {
            return ConstraintResult::Allowed;
        }

        ConstraintResult::Denied(format!(
            "{} is not in allowed_tools for current skill context",
            tool_name
        ))
    }
}

pub enum ConstraintResult {
    Allowed,
    Denied(String),
}

fn glob_match(pattern: &str, name: &str) -> bool {
    // 支持 "mcp:server:*" 格式通配
    if let Some(prefix) = pattern.strip_suffix(":*") {
        return name.starts_with(prefix);
    }
    false
}
```

### 6.3 AgentLoop 集成点

```rust
// crates/octo-engine/src/agent/loop_.rs（伪代码修改示意）

// 在工具调用前检查约束
for tool_call in &tool_calls {
    if let Some(enforcer) = &self.tool_constraint_enforcer {
        match enforcer.check(&tool_call.name) {
            ConstraintResult::Denied(reason) => {
                // 返回错误消息给 LLM，不执行工具
                tool_results.push(ToolResult::error(
                    &tool_call.id,
                    format!("Tool call blocked by skill policy: {}", reason)
                ));
                continue;
            }
            ConstraintResult::Allowed => {}
        }
    }
    // 执行工具...
}
```

---

## 7. 多运行时支持

### 7.1 运行时扩展设计

参考 openfang 的五种运行时（Python/WASM/Node/Builtin/PromptOnly），octo-sandbox 需增加 Node.js 和 WASM 运行时：

```rust
// crates/octo-engine/src/skill_runtime/traits.rs

#[async_trait]
pub trait SkillRuntime: Send + Sync {
    fn runtime_type(&self) -> RuntimeType;
    async fn execute(
        &self,
        script_path: &Path,
        args: &[String],
        context: &SkillContext,
    ) -> Result<SkillScriptOutput, SkillRuntimeError>;
    fn is_available(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeType {
    Python,
    Node,
    Wasm,
    Shell,
    Builtin,
}
```

### 7.2 Node.js 运行时

```rust
// crates/octo-engine/src/skill_runtime/node.rs

pub struct NodeRuntime {
    node_path: PathBuf,
}

impl NodeRuntime {
    pub fn new() -> Option<Self> {
        which::which("node").ok().map(|p| Self { node_path: p })
    }
}

#[async_trait]
impl SkillRuntime for NodeRuntime {
    fn runtime_type(&self) -> RuntimeType { RuntimeType::Node }

    async fn execute(
        &self,
        script_path: &Path,
        args: &[String],
        context: &SkillContext,
    ) -> Result<SkillScriptOutput, SkillRuntimeError> {
        // 注入工具信息到环境变量（JSON 序列化）
        let tools_json = serde_json::to_string(&context.tools)
            .map_err(|e| SkillRuntimeError::SerializationError(e.to_string()))?;

        let output = tokio::process::Command::new(&self.node_path)
            .arg(script_path)
            .args(args)
            .env("SKILL_NAME", &context.skill_name)
            .env("SKILL_TOOLS", &tools_json)
            .env("SKILL_WORK_DIR", context.working_dir.to_str().unwrap_or("."))
            .current_dir(&context.working_dir)
            .output()
            .await
            .map_err(|e| SkillRuntimeError::ExecutionFailed(e.to_string()))?;

        Ok(SkillScriptOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    fn is_available(&self) -> bool {
        self.node_path.exists()
    }
}
```

### 7.3 WASM 运行时（基于现有 SandboxManager）

```rust
// crates/octo-engine/src/skill_runtime/wasm.rs

pub struct WasmSkillRuntime {
    sandbox_manager: Arc<SandboxManager>,
}

#[async_trait]
impl SkillRuntime for WasmSkillRuntime {
    fn runtime_type(&self) -> RuntimeType { RuntimeType::Wasm }

    async fn execute(
        &self,
        script_path: &Path,
        args: &[String],
        context: &SkillContext,
    ) -> Result<SkillScriptOutput, SkillRuntimeError> {
        // 调用现有 SandboxManager 的 WASM 执行路径
        let wasm_bytes = tokio::fs::read(script_path).await
            .map_err(|e| SkillRuntimeError::FileNotFound(e.to_string()))?;

        let result = self.sandbox_manager
            .execute_wasm(&wasm_bytes, args, &context.working_dir)
            .await
            .map_err(|e| SkillRuntimeError::ExecutionFailed(e.to_string()))?;

        Ok(SkillScriptOutput {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        })
    }

    fn is_available(&self) -> bool {
        cfg!(feature = "sandbox-wasm")
    }
}
```

### 7.4 SkillRuntimeBridge 扩展

```rust
// crates/octo-engine/src/skills/runtime_bridge.rs（扩展现有实现）

pub struct SkillRuntimeBridge {
    runtimes: HashMap<RuntimeType, Arc<dyn SkillRuntime>>,
}

impl SkillRuntimeBridge {
    pub fn new() -> Self {
        let mut runtimes: HashMap<RuntimeType, Arc<dyn SkillRuntime>> = HashMap::new();

        // Python（现有）
        runtimes.insert(RuntimeType::Python, Arc::new(PythonRuntime::new()));

        // Node.js（新增）
        if let Some(node) = NodeRuntime::new() {
            runtimes.insert(RuntimeType::Node, Arc::new(node));
        }

        // Shell（新增）
        runtimes.insert(RuntimeType::Shell, Arc::new(ShellRuntime::new()));

        Self { runtimes }
    }

    /// 附加 WASM 运行时（需要 SandboxManager）
    pub fn with_wasm(mut self, sandbox: Arc<SandboxManager>) -> Self {
        self.runtimes.insert(
            RuntimeType::Wasm,
            Arc::new(WasmSkillRuntime { sandbox_manager: sandbox })
        );
        self
    }

    pub async fn execute(
        &self,
        runtime_type: RuntimeType,
        script_path: &Path,
        args: &[String],
        context: &SkillContext,
    ) -> Result<SkillScriptOutput, SkillRuntimeError> {
        let runtime = self.runtimes.get(&runtime_type)
            .ok_or_else(|| SkillRuntimeError::RuntimeNotAvailable(format!("{:?}", runtime_type)))?;

        if !runtime.is_available() {
            return Err(SkillRuntimeError::RuntimeNotAvailable(
                format!("{:?} runtime is not installed", runtime_type)
            ));
        }

        runtime.execute(script_path, args, context).await
    }
}
```

---

## 8. Skill 组合与跨 Skill 调用

### 8.1 depends_on 依赖图

参考 openfang 的 skill 组合机制，支持 `depends_on` 字段：

```rust
// crates/octo-engine/src/skills/dependency.rs

pub struct SkillDependencyGraph {
    // DAG（有向无环图）
    graph: HashMap<String, Vec<String>>,
}

impl SkillDependencyGraph {
    /// 从技能集合构建依赖图，检测循环依赖
    pub fn build(skills: &[SkillDefinition]) -> Result<Self, SkillError> {
        let mut graph = HashMap::new();
        for skill in skills {
            let deps = skill.depends_on.clone().unwrap_or_default();
            // 验证所有依赖都存在
            for dep in &deps {
                if !skills.iter().any(|s| &s.name == dep) {
                    return Err(SkillError::MissingDependency {
                        skill: skill.name.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
            graph.insert(skill.name.clone(), deps);
        }
        // 检测循环依赖（Kahn 算法）
        Self::detect_cycles(&graph)?;
        Ok(Self { graph })
    }

    /// 拓扑排序后返回需要激活的 skill 集合（包含依赖）
    pub fn resolve_with_deps(&self, skill_names: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();

        for name in skill_names {
            self.dfs(name, &mut visited, &mut result);
        }
        result
    }

    fn dfs(&self, name: &str, visited: &mut HashSet<String>, result: &mut Vec<String>) {
        if visited.contains(name) { return; }
        visited.insert(name.to_string());
        if let Some(deps) = self.graph.get(name) {
            for dep in deps {
                self.dfs(dep, visited, result);
            }
        }
        result.push(name.to_string());
    }

    fn detect_cycles(graph: &HashMap<String, Vec<String>>) -> Result<(), SkillError> {
        // Kahn's algorithm for cycle detection
        // ... 实现省略
        Ok(())
    }
}
```

### 8.2 Skill 上下文中的 skill_invoke 工具

允许 skill 的 Prompt body 指示 LLM 调用另一个 skill（通过工具）：

```rust
// crates/octo-engine/src/skills/invoke_tool.rs

/// 供 skill 调用另一个 skill 的特殊工具
pub struct SkillInvokeTool {
    registry: Arc<SkillRegistry>,
    selector: Arc<SkillSelector>,
}

#[async_trait]
impl Tool for SkillInvokeTool {
    fn name(&self) -> &str { "skill_invoke" }
    fn description(&self) -> &str {
        "Invoke another skill by name, loading its instructions into the current context"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_name": { "type": "string", "description": "Name of the skill to invoke" },
                "context": { "type": "string", "description": "Context message for skill selection" }
            },
            "required": ["skill_name"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult, ToolError> {
        let skill_name = params["skill_name"].as_str()
            .ok_or_else(|| ToolError::InvalidInput("skill_name required".to_string()))?;

        let skill = self.registry.get(skill_name)
            .ok_or_else(|| ToolError::NotFound(format!("skill '{}' not found", skill_name)))?;

        Ok(ToolResult::text(format!(
            "<invoked_skill name=\"{}\">\n{}\n</invoked_skill>",
            skill_name,
            skill.body
        )))
    }
}
```

---

## 9. HybridQueryEngine 集成（语义选择）

### 9.1 Skill 向量索引构建

利用已实现的 `HybridQueryEngine`（含 HNSW 向量索引）为 skills 构建语义搜索能力：

```rust
// crates/octo-engine/src/skills/semantic_index.rs

pub struct SkillSemanticIndex {
    engine: Arc<HybridQueryEngine>,
}

impl SkillSemanticIndex {
    /// 将所有 skill 的描述 + keywords 索引到向量引擎
    pub async fn build_from(
        &self,
        skills: &[SkillDefinition],
        embedding_client: &EmbeddingClient,
    ) -> Result<(), SkillError> {
        for skill in skills {
            // 构建索引文本（description + keywords + tags）
            let index_text = format!(
                "{}\n{}\n{}",
                skill.description,
                skill.activation.keywords.as_deref().unwrap_or(&[]).join(" "),
                skill.metadata.as_ref()
                    .and_then(|m| m.tags.as_ref())
                    .map(|t| t.join(" "))
                    .unwrap_or_default(),
            );

            let embedding = embedding_client
                .embed(&index_text)
                .await
                .map_err(|e| SkillError::EmbeddingFailed(e.to_string()))?;

            self.engine
                .index_chunk(MemoryChunk {
                    id: uuid::Uuid::new_v4(),
                    source: format!("skill:{}", skill.name),
                    content: index_text,
                    embedding: Some(embedding),
                    created_at: chrono::Utc::now(),
                    metadata: serde_json::json!({
                        "skill_name": skill.name,
                        "skill_version": skill.version,
                    }),
                })
                .await
                .map_err(|e| SkillError::IndexFailed(e.to_string()))?;
        }
        Ok(())
    }

    /// 语义搜索匹配的 skill
    pub async fn search(&self, query: &str, top_k: usize) -> Vec<String> {
        match self.engine.search(query, top_k).await {
            Ok(results) => results
                .into_iter()
                .filter_map(|r| {
                    r.source.strip_prefix("skill:").map(|s| s.to_string())
                })
                .collect(),
            Err(_) => vec![],
        }
    }
}
```

---

## 10. HookRegistry 集成

### 10.1 Skill 生命周期 Hooks

在现有 9 个 hook 点基础上，新增 skill 特定事件：

```rust
// crates/octo-engine/src/hooks/mod.rs（扩展）

pub enum HookEvent {
    // 现有事件...
    BeforeToolCall { tool_name: String, params: serde_json::Value },
    AfterToolCall { tool_name: String, result: String },

    // Skill 生命周期事件（新增）
    SkillsActivated { skills: Vec<String>, query: String },
    SkillDeactivated { skill_name: String },
    SkillScriptStarted { skill_name: String, script: String, runtime: String },
    SkillScriptCompleted { skill_name: String, script: String, exit_code: i32 },
    ToolConstraintViolated { tool_name: String, skill_name: String, reason: String },
}
```

### 10.2 激活时触发 hooks

```rust
// AgentLoop 中 skill 激活后（伪代码）

let active_skills = skill_selector.select(&all_skills, &user_message, &tools).await;

// 触发 SkillsActivated hook
if !active_skills.is_empty() {
    let skill_names: Vec<String> = active_skills.iter().map(|s| s.definition.name.clone()).collect();
    hook_registry.emit(HookEvent::SkillsActivated {
        skills: skill_names.clone(),
        query: user_message.clone(),
    }).await;

    tracing::info!(
        skill_count = active_skills.len(),
        skills = ?skill_names,
        "Skills activated for this turn"
    );
}
```

---

## 11. 斜杠命令路由

参考 localgpt 的 slash command 触发机制：

```rust
// crates/octo-engine/src/skills/slash_router.rs

pub struct SkillSlashRouter {
    /// Map<slash_command, skill_name>
    routes: HashMap<String, String>,
}

impl SkillSlashRouter {
    pub fn build(skills: &[SkillDefinition]) -> Self {
        let mut routes = HashMap::new();
        for skill in skills {
            if let Some(cmd) = &skill.activation.slash_command {
                routes.insert(cmd.clone(), skill.name.clone());
            }
        }
        Self { routes }
    }

    /// 检查消息是否是斜杠命令，返回对应的 skill 名称
    pub fn route(&self, message: &str) -> Option<&str> {
        let trimmed = message.trim();
        // 支持 "/skill-name arg1 arg2" 格式
        let cmd = trimmed.split_whitespace().next()?;
        self.routes.get(cmd).map(|s| s.as_str())
    }
}
```

---

## 12. SkillCatalog — 注册表集成

### 12.1 设计

参考 ironclaw 的 ClawHub + moltis 的原子注册表管理：

```rust
// crates/octo-engine/src/skills/catalog.rs

pub struct SkillCatalog {
    base_url: String,
    http_client: reqwest::Client,
    store: Arc<SkillStore>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub rating: f32,
    pub tags: Vec<String>,
    pub download_url: String,
    pub checksum: String, // SHA-256
}

impl SkillCatalog {
    /// 搜索注册表
    pub async fn search(&self, query: &str) -> Result<Vec<CatalogEntry>, SkillError> {
        let url = format!("{}/api/v1/skills/search?q={}", self.base_url, query);
        let entries = self.http_client.get(&url).send().await?
            .json::<Vec<CatalogEntry>>().await?;
        Ok(entries)
    }

    /// 安装 skill（带完整性验证）
    pub async fn install(&self, entry: &CatalogEntry, install_dir: &Path) -> Result<(), SkillError> {
        // 1. 下载 skill 包
        let bytes = self.http_client.get(&entry.download_url).send().await?
            .bytes().await?;

        // 2. 验证 SHA-256 checksum
        let hash = sha256::digest(bytes.as_ref());
        if hash != entry.checksum {
            return Err(SkillError::ChecksumMismatch {
                expected: entry.checksum.clone(),
                got: hash,
            });
        }

        // 3. 原子写入（写到 tmp 后 rename，防止部分写入）
        let skill_dir = install_dir.join(&entry.name);
        let tmp_dir = install_dir.join(format!(".tmp-{}", entry.name));
        extract_skill_archive(&bytes, &tmp_dir).await?;
        tokio::fs::rename(&tmp_dir, &skill_dir).await?;

        // 4. 写入安装清单（原子操作）
        self.store.record_installation(entry).await?;

        // 5. 写入签名文件（信任验证用）
        let sig_path = skill_dir.join("SKILL.sig");
        tokio::fs::write(&sig_path, format!(
            "name={}\nversion={}\nchecksum={}\ntrust=installed\n",
            entry.name, entry.version, entry.checksum
        )).await?;

        Ok(())
    }
}
```

---

## 13. octo-sandbox 实施路线图

### 13.1 P0：核心强制（~200 LOC，1 周）

| 任务 | 文件 | 描述 |
|------|------|------|
| T1: 信任模型强制 | `skills/selector.rs` | 实现 `SkillAttenuator`，按信任等级裁剪工具 |
| T2: 工具约束强制 | `skills/constraint.rs` | `ToolConstraintEnforcer` + AgentLoop 集成 |
| T3: 语义选择基础 | `skills/selector.rs` | `SkillScorer` 4 阶段选择管线 |

### 13.2 P1：运行时扩展（~300 LOC，2 周）

| 任务 | 文件 | 描述 |
|------|------|------|
| T4: Node.js 运行时 | `skill_runtime/node.rs` | `NodeRuntime` 实现 |
| T5: WASM 运行时 | `skill_runtime/wasm.rs` | 复用 SandboxManager |
| T6: Skill 依赖图 | `skills/dependency.rs` | `SkillDependencyGraph` + DAG 拓扑排序 |
| T7: skill_invoke 工具 | `skills/invoke_tool.rs` | 跨 skill 调用工具 |

### 13.3 P2：语义搜索 + Hooks（~150 LOC，1 周）

| 任务 | 文件 | 描述 |
|------|------|------|
| T8: HNSW 向量索引 | `skills/semantic_index.rs` | 集成 `HybridQueryEngine` |
| T9: Hook 事件 | `hooks/mod.rs` | 新增 skill 生命周期事件 |
| T10: 斜杠命令路由 | `skills/slash_router.rs` | `/skill-name` 快速激活 |

### 13.4 P3：注册表（~250 LOC，2 周）

| 任务 | 文件 | 描述 |
|------|------|------|
| T11: SkillCatalog | `skills/catalog.rs` | 远程 skill 搜索 + 安装 |
| T12: SkillStore | `skills/store.rs` | SQLite 安装记录持久化 |
| T13: REST API | `octo-server/api/skills.rs` | `/api/v1/skills` CRUD 端点 |

### 13.5 预计总工作量

| 优先级 | LOC | 时间 |
|-------|-----|------|
| P0 | ~200 | 1 周 |
| P1 | ~300 | 2 周 |
| P2 | ~150 | 1 周 |
| P3 | ~250 | 2 周 |
| **合计** | **~900** | **6 周** |

---

## 14. 与其他组件的集成关系

```
Skills System
    │
    ├─ HybridQueryEngine ←──── 语义向量选择（P0-2 已实现）
    │   └── HNSW Index + EmbeddingClient
    │
    ├─ HookRegistry ←────────── 生命周期事件广播
    │   └── 9-point + skill 生命周期 hooks
    │
    ├─ ToolRegistry ←─────────── 工具约束过滤
    │   └── allowed_tools 运行时强制
    │
    ├─ SafetyLayer ←──────────── Tool 输出安全过滤
    │   └── sanitizer → validator → policy → leak_detector
    │
    ├─ SandboxManager ←────────── WASM 脚本执行
    │   └── wasmtime 沙箱执行
    │
    ├─ AgentLoop ←────────────── skill context 注入
    │   └── <skill_context> XML 块插入 system prompt
    │
    ├─ EventBus ←────────────── 可观测性事件
    │   └── SkillActivated / SkillScriptCompleted 事件
    │
    └─ SkillRuntimeBridge ←──── 脚本执行引擎
        ├── Python (现有)
        ├── Node.js (新增)
        └── WASM (新增，复用 SandboxManager)
```

---

## 15. 测试策略

### 15.1 单元测试覆盖点

```rust
// 每个新组件需要的关键测试

// SkillSelector
#[test] fn test_gate_blocks_missing_bin() { ... }
#[test] fn test_gate_blocks_missing_env() { ... }
#[test] fn test_score_keyword_match() { ... }
#[test] fn test_score_regex_match() { ... }
#[test] fn test_budget_respects_token_limit() { ... }
#[test] fn test_attenuate_installed_trust_no_bash() { ... }
#[test] fn test_attenuate_trusted_allows_all() { ... }

// ToolConstraintEnforcer
#[test] fn test_denied_tool_blocked() { ... }
#[test] fn test_allowed_tool_passes() { ... }
#[test] fn test_wildcard_mcp_server() { ... }
#[test] fn test_empty_skills_no_restriction() { ... }

// SkillDependencyGraph
#[test] fn test_dep_resolution_order() { ... }
#[test] fn test_cycle_detection_fails() { ... }
#[test] fn test_missing_dep_fails() { ... }
```

### 15.2 集成测试

```rust
// crates/octo-engine/tests/skill_integration.rs
#[tokio::test]
async fn test_full_skill_pipeline_with_tool_enforcement() {
    // 1. 加载 skill（带 allowed_tools: [file_read]）
    // 2. 激活 skill
    // 3. 尝试调用 bash（应被拒绝）
    // 4. 调用 file_read（应成功）
}

#[tokio::test]
async fn test_skill_semantic_selection() {
    // 1. 索引 3 个 skill
    // 2. 查询 "deploy kubernetes"
    // 3. 验证 deployment skill 排名最高
}
```

---

## 16. 附录：参考实现要点

### ironclaw Skills 关键代码路径
- `src/skills/gating.rs` — Gate 阶段，检查 bins/env/config
- `src/skills/selector.rs` — Score 阶段，确定性关键词/正则评分
- `src/skills/attenuation.rs` — Attenuate 阶段，信任等级工具裁剪
- `src/tools/builtin/skill_tools.rs` — skill_list/search/install/remove 运行时工具

### openfang Skills 关键代码路径
- 多运行时分发：`skill_runtime/` Python/WASM/Node/Builtin/PromptOnly
- JSON Schema 工具内联定义
- capability-based 信任：Builtin > Bundled > OpenClaw > ClawHub

### moltis Skills 关键代码路径
- `skills-manifest.json` 原子写入（write tmp + rename）
- `notify::watcher()` 文件监视热重载
- `skills/migration.rs` 格式版本迁移

---

*本文档由 AI（Claude Sonnet 4.6）自动生成，基于对 ironclaw、openfang、moltis、zeroclaw、localgpt 等 harness 项目的代码级分析。*
