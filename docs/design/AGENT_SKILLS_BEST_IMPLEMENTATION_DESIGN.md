# Octo-Sandbox Agent Skills 最佳实现方案

> 研究日期: 2026-03-09
> 数据来源: 7 个 Rust 项目源码分析 + 2 个 Baseline 项目 + agentskills.io 标准规范 + Tavily 行业研究
> 研究方法: RuFlo 3 智能体并行分析 + 综合比较

---

## 一、研究背景与目标

### 研究范围

Agent Skills 作为 AI Agent 生态的公开标准（agentskills.io），已被 30+ 产品采用。本研究分析了以下项目对 Agent Skills 标准的支持情况：

**Rust 项目（7 个）**:

| 项目 | Skills 评分 | 核心特点 |
|------|------------|---------|
| **IronClaw** | 9.5/10 | 标准完整实现，Trust Attenuation，ClawHub Registry，安全纵深防御 |
| **OpenFang** | 9/10 | 多运行时（Python + Node.js），REST API，触发器系统 |
| **Moltis** | 8.5/10 | 最细粒度 crate 拆分，SkillWatcher 热重载，Requirements 预检 |
| **ZeroClaw** | 8/10 | SkillForge 自动发现，MetadataOnly 加载模式，always 标记 |
| **Goose** | 7/10 | MCP-first 架构，Extension 作为 Skill 载体 |
| **LocalGPT** | 5/10 | 简化实现，安全分级但无标准支持 |
| **pi_agent_rust** | 4/10 | 内置 Tools 为主，无独立 Skill 系统 |

**Baseline 项目（2 个）**:

| 项目 | 语言 | Skills 特点 |
|------|------|------------|
| **nanoclaw** | TypeScript | 委托 Claude SDK Skills，Credential Proxy 隔离 |
| **nanobot** | Python | 无独立 Skill 系统 |

### 研究目标

1. 确定 Agent Skills 标准的完整功能集
2. 评估 octo-sandbox 当前 Skills 支持现状
3. 给出**全功能支持 Agent Skills 标准**的最佳实现方案

---

## 二、Agent Skills 标准规范（agentskills.io）

### 2.1 标准目录结构

```
.octo/skills/                    # 项目级
~/.octo/skills/                  # 用户级
  └── my-skill/
      ├── SKILL.md               # 必需：技能描述文件
      ├── scripts/               # 可选：可执行脚本
      │   ├── setup.py
      │   └── run.js
      ├── references/            # 可选：参考资料
      │   ├── api-docs.md
      │   └── examples.json
      └── assets/                # 可选：静态资源
          └── logo.png
```

### 2.2 SKILL.md 格式规范

```yaml
---
name: my-skill                    # 必需：唯一标识符
description: "Skill description"  # 必需：简短描述
version: "1.0.0"                  # 可选：语义版本
user-invocable: true              # 可选：是否可通过 /skill-name 调用
allowed-tools:                    # 可选：工具白名单
  - bash
  - read
  - edit
model: claude-sonnet-4-6       # 可选：模型覆盖
context-fork: true                # 可选：独立上下文
---

# Skill Body (Markdown)

技能的完整指令内容，支持模板变量如 ${baseDir}
```

### 2.3 三级渐进式加载（Progressive Disclosure）

这是标准的核心设计理念，解决了"42 个 Skill 在 compact 模式下占用 21,000 tokens"的问题：

| 层级 | 名称 | 加载时机 | Token 开销 | 内容 |
|------|------|---------|-----------|------|
| **L1** | Discovery | 启动时 | ~100 tokens/skill | name + description（仅 frontmatter） |
| **L2** | Instructions | 激活时 | <5000 tokens | SKILL.md 完整 body |
| **L3** | Resources | 需要时 | 按需 | scripts/ + references/ + assets/ |

**效果**: 42 个 Skill 启动仅需 ~630 tokens（97% 节省），单个 Skill 激活才加载完整内容。

### 2.4 安全模型

标准定义的安全层次：

| 安全特性 | 说明 | 重要性 |
|---------|------|--------|
| **allowed-tools** | 限制 Skill 可使用的工具 | 🔴 关键 |
| **Trust Level** | Trusted / Installed / Unknown 分级 | 🔴 关键 |
| **Script Sandboxing** | 脚本在隔离环境执行 | 🟡 重要 |
| **Path Traversal 防护** | 防止 `../` 逃逸 | 🔴 关键 |
| **Prompt Injection 防御** | 防止 Skill body 注入恶意指令 | 🔴 关键 |
| **Credential 隔离** | 脚本不直接接触 API key | 🟡 重要 |

---

## 三、各项目 Skills 实现横向对比

### 3.1 标准目录结构支持

| 项目 | SKILL.md | scripts/ | references/ | assets/ | 自定义目录 |
|------|----------|----------|-------------|---------|-----------|
| **IronClaw** | ✅ | ✅ | ✅ | ✅ | ❌ |
| **OpenFang** | ✅ | ✅ | ❌ | ❌ | ✅ config/ |
| **Moltis** | ✅ YAML frontmatter | ✅ | ✅ | ❌ | ✅ Dockerfile |
| **ZeroClaw** | ✅ TOML frontmatter | ✅ | ❌ | ❌ | ❌ |
| **octo-sandbox** | ✅ | ✅ (类型存根) | ✅ (仅验证) | ✅ (仅验证) | ❌ |

### 3.2 加载策略

| 项目 | L1 Discovery | L2 Lazy Load | L3 On-demand | 热重载 | 缓存 |
|------|-------------|-------------|-------------|--------|------|
| **IronClaw** | ✅ index 扫描 | ✅ 按需激活 | ✅ 脚本按需 | ✅ notify | ✅ LRU |
| **OpenFang** | ✅ | ✅ | ✅ REST API 触发 | ✅ | ❌ |
| **Moltis** | ✅ 分离 SkillMetadata | ✅ | ✅ | ✅ SkillWatcher debounce | ✅ |
| **ZeroClaw** | ✅ MetadataOnly 模式 | ✅ | ✅ | ❌ | ✅ |
| **octo-sandbox** | ✅ build_index() | ✅ load_skill() | ❌ 未连接 | ✅ notify debounce | ❌ |

### 3.3 脚本执行

| 项目 | Python | Node.js | WASM | Shell/Bash | 沙箱隔离 | 超时控制 |
|------|--------|---------|------|-----------|---------|---------|
| **IronClaw** | ✅ venv | ✅ | ✅ Wasmtime | ✅ | ✅ Zone A/B | ✅ |
| **OpenFang** | ✅ 原生 | ✅ 原生 | ❌ | ✅ | ✅ 进程隔离 | ✅ |
| **Moltis** | ✅ venv | ✅ | ❌ | ✅ | ❌ | ✅ 2min |
| **ZeroClaw** | ✅ | ✅ | ❌ | ✅ | ✅ trusted_roots | ✅ |
| **octo-sandbox** | ✅ PythonRuntime | ❌ 类型存根 | ❌ 类型存根 | ❌ | ❌ | ❌ |

### 3.4 安全机制

| 项目 | Trust Level | allowed-tools 运行时强制 | Symlink 防护 | XML Escape | ReDoS 防护 | 注入防御 |
|------|------------|------------------------|------------|-----------|-----------|---------|
| **IronClaw** | ✅ 3 级 | ✅ 运行时拦截 | ✅ canonicalize + reject | ✅ | ✅ | ✅ SafetyLayer |
| **OpenFang** | ✅ 2 级 | ✅ | ❌ | ❌ | ❌ | ✅ |
| **Moltis** | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| **ZeroClaw** | ✅ allow_scripts + trusted_roots | ✅ | ❌ | ❌ | ❌ | ✅ MetadataOnly |
| **octo-sandbox** | ❌ 无 | ❌ 仅加载时验证格式 | ❌ | ❌ | ❌ | ❌ |

### 3.5 Skill 注册与发现

| 项目 | 本地注册 | 远程 Registry | 自动发现 | 版本管理 | 依赖声明 |
|------|---------|-------------|---------|---------|---------|
| **IronClaw** | ✅ | ✅ ClawHub | ❌ | ✅ semver | ❌ |
| **OpenFang** | ✅ | ✅ REST API | ❌ | ✅ | ✅ |
| **Moltis** | ✅ manifest | ❌ | ❌ | ✅ | ✅ binary requirements |
| **ZeroClaw** | ✅ | ✅ open-skills repo | ✅ SkillForge | ✅ | ❌ |
| **octo-sandbox** | ✅ SkillRegistry | ❌ | ❌ | ✅ | ❌ |

### 3.6 与其他组件集成

| 项目 | MCP 集成 | Tool 系统集成 | Context 集成 | Memory 集成 | Agent Loop 集成 |
|------|---------|-------------|-------------|-------------|----------------|
| **IronClaw** | ✅ MCP tools in allowed | ✅ SkillTool | ✅ compact 豁免 | ❌ | ✅ |
| **OpenFang** | ✅ | ✅ | ✅ prompt 14 段 | ❌ | ✅ triggers |
| **Moltis** | ✅ | ✅ | ✅ | ❌ | ✅ |
| **ZeroClaw** | ✅ | ✅ | ✅ always 标记 | ❌ | ✅ |
| **octo-sandbox** | ❌ 未连接 | ✅ SkillTool (仅返回 body) | ❌ | ❌ | ❌ 未连接到 AgentLoop |

---

## 四、octo-sandbox 当前实现评估

### 4.1 综合评分: 5.5/10

### 4.2 已有优势

| 优势 | 实现位置 | 说明 |
|------|---------|------|
| **清晰分层架构** | `skills/` + `skill_runtime/` | types → loader → registry → tool → runtime 层次分明 |
| **L1 Index 支持** | `SkillLoader::build_index()` | 仅解析 frontmatter，不加载 body |
| **L2 Lazy Load** | `SkillLoader::load_skill()` | 按名称按需加载完整 Skill |
| **热重载** | `SkillRegistry::start_watching()` | notify_debouncer_mini 300ms 防抖 |
| **目录结构验证** | `standards.rs` | SKILL.md 存在性 + 标准目录验证 |
| **allowed-tools 格式验证** | `validate_allowed_tools()` | 加载时验证工具名合法性 |
| **路径遍历防护** | `SkillLoader::execute_script()` | `..` component 检查 |
| **模板变量** | `${baseDir}` 替换 | body 中引用 Skill 目录路径 |
| **多优先级搜索** | project → user 目录顺序 | 项目级覆盖用户级 |

### 4.3 关键缺陷

| 缺陷 | 严重性 | 说明 |
|------|--------|------|
| **SkillTool.execute() 仅返回 body 文本** | 🔴 关键 | `Ok(ToolOutput::success(&self.skill.body))` — 只做 prompt injection，完全不执行脚本 |
| **SkillRuntimeBridge 与 SkillTool 断联** | 🔴 关键 | SkillRuntimeBridge 能执行脚本但未被 SkillTool 调用 |
| **allowed-tools 不在运行时强制执行** | 🔴 关键 | 仅验证格式合法性，不拦截实际 Tool 调用 |
| **无 Trust Level 系统** | 🟡 重要 | 所有 Skill 等同对待，无信任分级 |
| **NodeJS/WASM/Builtin 运行时仅为类型存根** | 🟡 重要 | 只有 Python Runtime 有实际实现 |
| **无 REST API** | 🟡 重要 | 无法通过 API 管理 Skills |
| **无远程 Registry** | 🟠 中等 | 无法从远程仓库安装 Skill |
| **无 context-fork** | 🟠 中等 | Skill 不能在独立上下文中执行 |
| **无 model 覆盖** | 🟠 中等 | Skill 不能指定使用的模型 |
| **SkillDefinition 缺少高级字段** | 🟠 中等 | ADR-041 描述了 triggers/actions 但未实现 |
| **无缓存层** | 🟢 低 | 每次加载都读文件系统 |

### 4.4 核心断联图解

```
当前状态（断联）:
┌─────────────┐     ┌───────────────┐     ┌──────────────────┐
│ SkillLoader │────►│ SkillRegistry │────►│ SkillTool        │
│ (加载/解析)  │     │ (存储/索引)    │     │ execute() 返回   │
│             │     │               │     │ body 文本 ❌     │
└─────────────┘     └───────────────┘     └──────────────────┘
                                                    ↕ 断联!
                    ┌───────────────────┐
                    │ SkillRuntimeBridge │
                    │ (脚本执行能力)      │  ← 被孤立，未被调用
                    └───────────────────┘

期望状态（连通）:
┌─────────────┐     ┌───────────────┐     ┌──────────────────┐
│ SkillLoader │────►│ SkillRegistry │────►│ SkillTool        │
│ (加载/解析)  │     │ (存储/索引)    │     │ execute():       │
│             │     │               │     │ 1. 注入 body     │
└─────────────┘     └───────────────┘     │ 2. 执行 scripts  │
                                          │ 3. 返回结果       │
                    ┌───────────────────┐  └────────┬─────────┘
                    │ SkillRuntimeBridge │◄──────────┘
                    │ (脚本执行引擎)      │  调用
                    └───────────────────┘
```

---

## 五、最佳实现方案

### 5.1 设计原则

1. **标准优先**: 完整实现 agentskills.io 规范，不发明私有扩展
2. **渐进加载**: 严格遵循 L1/L2/L3 三级加载，启动开销 < 1000 tokens
3. **安全纵深**: Trust Level + allowed-tools 运行时强制 + 脚本沙箱
4. **组件集成**: Skills ↔ Tools ↔ MCP ↔ Context ↔ AgentLoop 全面互通
5. **可扩展**: 支持远程 Registry、自定义运行时、SkillForge 自动发现

### 5.2 架构总览

```
                        ┌─────────────────────────────────────────┐
                        │             AgentLoop                   │
                        │  ┌──────────────────────────────────┐   │
                        │  │ ContextManager                    │   │
                        │  │  ├─ L1: skill index → prompt      │   │
                        │  │  ├─ L2: active skill body → prompt│   │
                        │  │  └─ compact: respect always flag  │   │
                        │  └──────────────────────────────────┘   │
                        │                                         │
                        │  ┌──────────────────────────────────┐   │
                        │  │ ToolRegistry                      │   │
                        │  │  ├─ SkillTool (per skill)         │   │
                        │  │  └─ allowed-tools enforcer ←──────┤───┤── SecurityPolicy
                        │  └──────────────────────────────────┘   │
                        └─────────────────────────────────────────┘
                                          │
                        ┌─────────────────┴─────────────────┐
                        │         SkillManager               │
                        │  ┌────────────┐  ┌──────────────┐  │
                        │  │ SkillIndex │  │SkillRegistry │  │
                        │  │ (L1 meta)  │  │(L2 activated)│  │
                        │  └────────────┘  └──────────────┘  │
                        │  ┌────────────┐  ┌──────────────┐  │
                        │  │ SkillLoader│  │TrustManager  │  │
                        │  │ (parse)    │  │(trust levels)│  │
                        │  └────────────┘  └──────────────┘  │
                        │  ┌────────────────────────────────┐ │
                        │  │ SkillRuntimeBridge (L3 scripts)│ │
                        │  │  ├─ PythonRuntime              │ │
                        │  │  ├─ NodeJSRuntime              │ │
                        │  │  ├─ ShellRuntime               │ │
                        │  │  └─ WasmRuntime                │ │
                        │  └────────────────────────────────┘ │
                        └─────────────────────────────────────┘
                                          │
                        ┌─────────────────┴─────────────────┐
                        │       SkillSource (可扩展)         │
                        │  ├─ LocalSource (项目/用户目录)     │
                        │  ├─ RegistrySource (远程仓库)      │
                        │  └─ SkillForge (自动发现)          │
                        └─────────────────────────────────────┘
```

### 5.3 SkillDefinition 增强

```rust
/// 增强后的 SkillDefinition — 完整支持 Agent Skills 标准
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    // === 必需字段 ===
    pub name: String,
    pub description: String,

    // === 标准可选字段 ===
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "user-invocable")]
    pub user_invocable: bool,
    #[serde(default, rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,

    // === 新增标准字段 ===
    #[serde(default)]
    pub model: Option<String>,           // 模型覆盖
    #[serde(default, rename = "context-fork")]
    pub context_fork: bool,              // 独立上下文执行
    #[serde(default)]
    pub always: bool,                    // compact 模式不裁剪（ZeroClaw 模式）
    #[serde(default, rename = "trust-level")]
    pub trust_level: TrustLevel,         // 信任等级
    #[serde(default)]
    pub triggers: Vec<SkillTrigger>,     // 自动触发条件
    #[serde(default)]
    pub dependencies: Vec<String>,       // 依赖的其他 Skill
    #[serde(default)]
    pub tags: Vec<String>,              // 分类标签

    // === 内部字段（不序列化）===
    #[serde(skip)]
    pub body: String,
    #[serde(skip)]
    pub base_dir: PathBuf,
    #[serde(skip)]
    pub source_path: PathBuf,
    #[serde(skip)]
    pub body_loaded: bool,
    #[serde(skip)]
    pub source_type: SkillSourceType,    // 来源类型
}

/// 信任等级（IronClaw Trust Attenuation 模式）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// 完全信任 — 可使用所有工具（项目内置 Skill）
    Trusted,
    /// 已安装 — 仅可使用 allowed-tools 中声明的工具
    #[default]
    Installed,
    /// 未知来源 — 仅可使用只读工具
    Unknown,
}

/// Skill 触发条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillTrigger {
    /// 文件模式匹配触发
    FilePattern { glob: String },
    /// 命令前缀触发（如 /skill-name）
    Command { prefix: String },
    /// 关键词触发
    Keyword { pattern: String },
}

/// Skill 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillSourceType {
    #[default]
    ProjectLocal,    // .octo/skills/
    UserLocal,       // ~/.octo/skills/
    PluginBundled,   // 插件自带
    Registry,        // 远程仓库安装
}
```

### 5.4 SkillTool — 修复核心断联

**当前问题**: `SkillTool::execute()` 只返回 `body` 文本（prompt injection），不执行脚本。

**修复方案**: SkillTool 成为 Skill 执行的统一入口，同时提供 prompt injection 和脚本执行。

```rust
/// 增强后的 SkillTool — 统一 prompt injection + 脚本执行
pub struct SkillTool {
    skill: SkillDefinition,
    runtime_bridge: Arc<SkillRuntimeBridge>,
    trust_manager: Arc<TrustManager>,
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        &self.skill.name
    }

    fn description(&self) -> &str {
        &self.skill.description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["activate", "run_script", "list_scripts"],
                    "description": "Action to perform"
                },
                "script": {
                    "type": "string",
                    "description": "Script name (for run_script action)"
                },
                "args": {
                    "type": "string",
                    "description": "Arguments for the skill or script"
                }
            },
            "required": ["action"]
        })
    }

    fn source(&self) -> ToolSource {
        ToolSource::Skill(self.skill.name.clone())
    }

    /// 风险等级取决于 Skill 信任等级
    fn risk_level(&self) -> RiskLevel {
        match self.skill.trust_level {
            TrustLevel::Trusted => RiskLevel::LowRisk,
            TrustLevel::Installed => RiskLevel::LowRisk,
            TrustLevel::Unknown => RiskLevel::HighRisk,
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        let action = params.get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("activate");

        match action {
            // L2: 返回 Skill body 作为 prompt injection
            "activate" => {
                Ok(ToolOutput::success(&self.skill.body))
            }

            // L3: 执行 Skill 脚本
            "run_script" => {
                let script_name = params.get("script")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("script name required"))?;

                // 信任检查
                self.trust_manager.check_script_permission(
                    &self.skill,
                    script_name,
                )?;

                let args = params.get("args")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                let result = self.runtime_bridge
                    .execute_script_file(
                        &self.skill.base_dir.join("scripts").join(script_name),
                        args,
                        &SkillContext::new(
                            self.skill.name.clone(),
                            self.skill.base_dir.clone(),
                        ),
                    )
                    .await?;

                Ok(ToolOutput::success(
                    &serde_json::to_string_pretty(&result)?
                ))
            }

            // 列出可用脚本
            "list_scripts" => {
                let scripts_dir = self.skill.base_dir.join("scripts");
                if !scripts_dir.is_dir() {
                    return Ok(ToolOutput::success("No scripts available"));
                }
                let mut scripts = Vec::new();
                for entry in std::fs::read_dir(&scripts_dir)?.flatten() {
                    if entry.path().is_file() {
                        if let Some(name) = entry.path().file_name() {
                            scripts.push(name.to_string_lossy().to_string());
                        }
                    }
                }
                Ok(ToolOutput::success(
                    &format!("Available scripts:\n{}", scripts.join("\n"))
                ))
            }

            _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
        }
    }
}
```

### 5.5 TrustManager — 信任等级管理

```rust
/// 信任管理器（IronClaw Trust Attenuation 模式）
///
/// 核心规则:
/// - Trusted: 项目内置 Skill → 可使用所有工具
/// - Installed: 用户安装 Skill → 仅可使用 allowed-tools 中的工具
/// - Unknown: 未知来源 Skill → 仅可使用只读工具
pub struct TrustManager {
    /// 只读工具白名单（Unknown 级别可用）
    readonly_tools: HashSet<String>,
    /// 信任根路径（类似 ZeroClaw trusted_skill_roots）
    trusted_roots: Vec<PathBuf>,
}

impl TrustManager {
    pub fn new(trusted_roots: Vec<PathBuf>) -> Self {
        let readonly_tools = HashSet::from_iter([
            "read".into(), "glob".into(), "grep".into(),
            "list_directory".into(),
        ]);
        Self { readonly_tools, trusted_roots }
    }

    /// 计算 Skill 的有效信任等级
    pub fn effective_trust_level(&self, skill: &SkillDefinition) -> TrustLevel {
        // 显式声明的信任等级
        let declared = skill.trust_level;

        // 来源自动推断
        let source_trust = match skill.source_type {
            SkillSourceType::ProjectLocal => TrustLevel::Trusted,
            SkillSourceType::PluginBundled => TrustLevel::Installed,
            SkillSourceType::UserLocal => TrustLevel::Installed,
            SkillSourceType::Registry => TrustLevel::Unknown,
        };

        // 取较低的信任等级（安全原则）
        match (declared, source_trust) {
            (TrustLevel::Trusted, TrustLevel::Trusted) => TrustLevel::Trusted,
            (TrustLevel::Unknown, _) | (_, TrustLevel::Unknown) => TrustLevel::Unknown,
            _ => TrustLevel::Installed,
        }
    }

    /// 检查 Skill 是否可以使用指定的 Tool
    pub fn check_tool_permission(
        &self,
        skill: &SkillDefinition,
        tool_name: &str,
    ) -> Result<()> {
        let trust = self.effective_trust_level(skill);

        match trust {
            TrustLevel::Trusted => Ok(()), // 可使用所有工具

            TrustLevel::Installed => {
                // 必须在 allowed-tools 中声明
                if let Some(ref allowed) = skill.allowed_tools {
                    if allowed.iter().any(|t| t == tool_name) {
                        Ok(())
                    } else {
                        bail!(
                            "Skill '{}' (trust: Installed) cannot use tool '{}'. \
                             Allowed: {:?}",
                            skill.name, tool_name, allowed
                        )
                    }
                } else {
                    bail!(
                        "Skill '{}' (trust: Installed) has no allowed-tools declaration",
                        skill.name
                    )
                }
            }

            TrustLevel::Unknown => {
                // 仅可使用只读工具
                if self.readonly_tools.contains(tool_name) {
                    Ok(())
                } else {
                    bail!(
                        "Skill '{}' (trust: Unknown) can only use read-only tools. \
                         '{}' is not permitted",
                        skill.name, tool_name
                    )
                }
            }
        }
    }

    /// 检查 Skill 是否可以执行脚本
    pub fn check_script_permission(
        &self,
        skill: &SkillDefinition,
        script_name: &str,
    ) -> Result<()> {
        let trust = self.effective_trust_level(skill);

        match trust {
            TrustLevel::Trusted => Ok(()),
            TrustLevel::Installed => {
                // 已安装的 Skill 可以执行脚本（但受 allowed-tools 限制）
                Ok(())
            }
            TrustLevel::Unknown => {
                bail!(
                    "Skill '{}' (trust: Unknown) cannot execute scripts. \
                     Script: '{}'",
                    skill.name, script_name
                )
            }
        }
    }
}
```

### 5.6 allowed-tools 运行时强制执行

**关键设计**: 在 AgentLoop 的 Tool 调用路径中拦截，而非在 SkillTool 内部。

```rust
/// ToolCallInterceptor — 在 AgentLoop 中拦截 Tool 调用
///
/// 当前活跃的 Skill 会限制可用的 Tools：
/// - 如果没有活跃 Skill → 所有 Tools 可用
/// - 如果有活跃 Skill → 只有 allowed-tools 中的 Tools 可用
pub struct ToolCallInterceptor {
    trust_manager: Arc<TrustManager>,
    active_skill: Option<SkillDefinition>,
}

impl ToolCallInterceptor {
    /// 在 AgentLoop 调用 Tool 前检查权限
    pub fn check_permission(&self, tool_name: &str) -> Result<()> {
        if let Some(ref skill) = self.active_skill {
            self.trust_manager.check_tool_permission(skill, tool_name)
        } else {
            Ok(()) // 无活跃 Skill，不限制
        }
    }

    /// 获取当前活跃 Skill 下可用的 Tool 列表
    /// 用于发送给 LLM 的 tools 参数
    pub fn filter_available_tools(
        &self,
        all_tools: &[Box<dyn Tool>],
    ) -> Vec<&dyn Tool> {
        if let Some(ref skill) = self.active_skill {
            let trust = self.trust_manager.effective_trust_level(skill);
            match trust {
                TrustLevel::Trusted => all_tools.iter().map(|t| t.as_ref()).collect(),
                TrustLevel::Installed => {
                    if let Some(ref allowed) = skill.allowed_tools {
                        all_tools.iter()
                            .filter(|t| allowed.contains(&t.name().to_string()))
                            .map(|t| t.as_ref())
                            .collect()
                    } else {
                        vec![] // 无 allowed-tools → 无可用工具
                    }
                }
                TrustLevel::Unknown => {
                    all_tools.iter()
                        .filter(|t| {
                            self.trust_manager
                                .check_tool_permission(skill, t.name())
                                .is_ok()
                        })
                        .map(|t| t.as_ref())
                        .collect()
                }
            }
        } else {
            all_tools.iter().map(|t| t.as_ref()).collect()
        }
    }
}
```

### 5.7 SkillManager — 统一管理入口

```rust
/// SkillManager — Skill 全生命周期管理
///
/// 整合 SkillLoader, SkillRegistry, SkillRuntimeBridge, TrustManager
pub struct SkillManager {
    loader: SkillLoader,
    registry: SkillRegistry,
    runtime_bridge: Arc<SkillRuntimeBridge>,
    trust_manager: Arc<TrustManager>,
    index_cache: RwLock<Vec<SkillMetadata>>,  // L1 缓存
}

impl SkillManager {
    pub fn new(
        project_dir: Option<&Path>,
        user_dir: Option<&Path>,
        venv_base: PathBuf,
        trusted_roots: Vec<PathBuf>,
    ) -> Self {
        let loader = SkillLoader::new(project_dir, user_dir);
        let registry = SkillRegistry::new();
        let runtime_bridge = Arc::new(SkillRuntimeBridge::new(venv_base));
        let trust_manager = Arc::new(TrustManager::new(trusted_roots));
        let index_cache = RwLock::new(Vec::new());

        Self {
            loader, registry, runtime_bridge, trust_manager, index_cache,
        }
    }

    /// L1: 构建轻量索引（启动时调用）
    pub fn build_index(&self) -> Vec<SkillMetadata> {
        let index = self.loader.build_index();
        *self.index_cache.write().unwrap() = index.clone();
        index
    }

    /// L2: 激活指定 Skill（按需加载 body）
    pub fn activate_skill(&self, name: &str) -> Result<SkillDefinition> {
        let skill = self.loader.load_skill(name)?;
        self.registry.load_single(skill.clone())?;
        Ok(skill)
    }

    /// L3: 执行 Skill 脚本
    pub async fn execute_script(
        &self,
        skill_name: &str,
        script_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // 获取 Skill
        let skill = self.registry.get(skill_name)
            .ok_or_else(|| anyhow::anyhow!("Skill not activated: {}", skill_name))?;

        // 信任检查
        self.trust_manager.check_script_permission(&skill, script_name)?;

        // 执行
        self.loader.execute_script(
            &self.runtime_bridge, skill_name, script_name, args,
        ).await
    }

    /// 生成 SkillTool 列表（注册到 ToolRegistry）
    pub fn create_skill_tools(&self) -> Vec<SkillTool> {
        self.registry.invocable_skills()
            .into_iter()
            .map(|skill| SkillTool::new(
                skill,
                self.runtime_bridge.clone(),
                self.trust_manager.clone(),
            ))
            .collect()
    }

    /// 生成 L1 prompt section（用于 SystemPromptBuilder）
    pub fn prompt_section_l1(&self) -> String {
        let index = self.index_cache.read().unwrap();
        if index.is_empty() {
            return String::new();
        }

        let mut section = String::from("<available-skills>\n");
        for meta in index.iter() {
            section.push_str(&format!(
                "- /{}: {}\n",
                meta.name, meta.description
            ));
        }
        section.push_str("</available-skills>");
        section
    }

    /// 生成 L2 prompt section（激活 Skill 的完整 body）
    pub fn prompt_section_l2(&self, skill_name: &str) -> Option<String> {
        self.registry.get(skill_name).map(|skill| {
            format!(
                "<active-skill name=\"{}\">\n{}\n</active-skill>",
                skill.name, skill.body
            )
        })
    }

    /// 启动热重载监听
    pub fn start_watching(&self) -> Result<()> {
        self.registry.start_watching(
            SkillLoader::new(
                // 重新创建 loader 用于 watcher 线程
                self.loader.search_dirs().first().map(|p| p.parent().unwrap().parent().unwrap()),
                self.loader.search_dirs().get(1).map(|p| p.parent().unwrap().parent().unwrap()),
            )
        )
    }
}
```

### 5.8 Context 集成 — always 标记与 compact 豁免

```rust
/// ContextManager 集成点
///
/// 在 SystemPromptBuilder 和 ContextPruner 中处理 Skills
impl SystemPromptBuilder {
    /// 构建 system prompt 时集成 Skills
    fn build_with_skills(&self, skill_manager: &SkillManager) -> String {
        let mut prompt = self.build_base();

        // L1: 始终包含 skill index（开销 ~100 tokens/skill）
        let l1_section = skill_manager.prompt_section_l1();
        if !l1_section.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&l1_section);
        }

        // L2: 包含已激活 skill 的 body
        // （由 AgentLoop 在 Skill 被调用时动态添加）

        prompt
    }
}

impl ContextPruner {
    /// Compact 模式下保留 always=true 的 Skill
    fn should_preserve(&self, skill: &SkillDefinition) -> bool {
        skill.always // ZeroClaw 模式：标记为 always 的 Skill 不被裁剪
    }
}
```

### 5.9 跨 Skills/Tools 的工具执行

**核心需求**: Skill 内的脚本能调用其他 Tools 和 Skills。

```rust
/// SkillContext 增强 — 支持跨组件调用
pub struct SkillContext {
    pub skill_name: String,
    pub base_dir: PathBuf,

    // 新增：跨组件调用能力
    pub tool_invoker: Option<Arc<dyn ToolInvoker>>,
    pub skill_invoker: Option<Arc<dyn SkillInvoker>>,
    pub mcp_bridge: Option<Arc<McpToolBridge>>,

    // 环境变量（不含敏感信息）
    pub env_vars: HashMap<String, String>,
}

/// Tool 调用接口（供脚本使用）
#[async_trait]
pub trait ToolInvoker: Send + Sync {
    /// 脚本调用其他 Tool
    async fn invoke_tool(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value>;
}

/// Skill 调用接口（供脚本使用）
#[async_trait]
pub trait SkillInvoker: Send + Sync {
    /// 脚本调用其他 Skill
    async fn invoke_skill(
        &self,
        skill_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value>;
}
```

### 5.10 运行时补全 — NodeJS 和 Shell

```rust
/// Node.js 运行时实现
pub struct NodeJSRuntime {
    node_path: PathBuf,
}

#[async_trait]
impl SkillRuntime for NodeJSRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::NodeJS
    }

    async fn execute(
        &self,
        script: &str,
        args: serde_json::Value,
        context: &SkillContext,
    ) -> Result<serde_json::Value> {
        let args_json = serde_json::to_string(&args)?;
        let output = tokio::process::Command::new(&self.node_path)
            .arg("-e")
            .arg(script)
            .env("SKILL_ARGS", &args_json)
            .env("SKILL_NAME", &context.skill_name)
            .env("SKILL_BASE_DIR", &context.base_dir.to_string_lossy().to_string())
            .current_dir(&context.base_dir)
            .output()
            .await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // 尝试解析为 JSON，否则返回字符串
            match serde_json::from_str(&stdout) {
                Ok(json) => Ok(json),
                Err(_) => Ok(serde_json::json!({ "output": stdout.trim() })),
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Node.js script failed: {}", stderr)
        }
    }

    async fn check_environment(&self) -> Result<()> {
        let output = tokio::process::Command::new(&self.node_path)
            .arg("--version")
            .output()
            .await?;
        if output.status.success() {
            Ok(())
        } else {
            bail!("Node.js not available at {:?}", self.node_path)
        }
    }
}

/// Shell/Bash 运行时实现
pub struct ShellRuntime;

#[async_trait]
impl SkillRuntime for ShellRuntime {
    fn runtime_type(&self) -> RuntimeType {
        RuntimeType::Builtin  // 复用 Builtin 类型或新增 Shell 类型
    }

    async fn execute(
        &self,
        script: &str,
        args: serde_json::Value,
        context: &SkillContext,
    ) -> Result<serde_json::Value> {
        let args_json = serde_json::to_string(&args)?;
        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(script)
            .env("SKILL_ARGS", &args_json)
            .env("SKILL_NAME", &context.skill_name)
            .env("SKILL_BASE_DIR", &context.base_dir.to_string_lossy().to_string())
            .current_dir(&context.base_dir)
            .output()
            .await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(serde_json::json!({ "output": stdout.trim() }))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Shell script failed: {}", stderr)
        }
    }

    async fn check_environment(&self) -> Result<()> {
        Ok(()) // bash 总是可用
    }
}
```

---

## 六、实现优先级矩阵

### P0 — 必须立即修复（1-2 周）

| # | 任务 | 涉及文件 | 复杂度 |
|---|------|---------|--------|
| 1 | **修复 SkillTool 核心断联** — 连接 SkillRuntimeBridge | `skills/tool.rs` | 中 |
| 2 | **增强 SkillDefinition** — 添加 model、context-fork、always、trust-level 字段 | `octo-types/src/skill.rs` | 低 |
| 3 | **实现 TrustManager** — 三级信任等级 | 新文件 `skills/trust.rs` | 中 |
| 4 | **allowed-tools 运行时强制** — 在 AgentLoop 中拦截 | `agent/loop.rs` | 中 |
| 5 | **实现 SkillManager** — 统一管理入口 | 新文件 `skills/manager.rs` | 中 |

### P1 — 核心功能完善（2-4 周）

| # | 任务 | 涉及文件 | 复杂度 |
|---|------|---------|--------|
| 6 | **NodeJS Runtime 实现** | 新文件 `skill_runtime/nodejs.rs` | 低 |
| 7 | **Shell Runtime 实现** | 新文件 `skill_runtime/shell.rs` | 低 |
| 8 | **Context 集成** — always 标记 + compact 豁免 | `context/pruner.rs`, `context/builder.rs` | 中 |
| 9 | **跨组件调用** — SkillContext 支持 ToolInvoker | `skill_runtime/mod.rs` | 高 |
| 10 | **脚本超时控制** — tokio::time::timeout 包装 | `skill_runtime/*.rs` | 低 |
| 11 | **Symlink 防护** — canonicalize + 路径检查 | `skills/loader.rs` | 低 |
| 12 | **REST API** — Skill 管理端点 | `octo-server/src/api/skills.rs` | 中 |

### P2 — 高级特性（4-8 周）

| # | 任务 | 涉及文件 | 复杂度 |
|---|------|---------|--------|
| 13 | **context-fork 实现** — 独立上下文执行 | `agent/executor.rs` | 高 |
| 14 | **model 覆盖** — Skill 指定模型 | `agent/loop.rs`, `providers/` | 中 |
| 15 | **触发器系统** — FilePattern / Command / Keyword | `skills/trigger.rs` | 中 |
| 16 | **LRU 缓存** — L2 加载结果缓存 | `skills/cache.rs` | 低 |
| 17 | **MCP Tools 在 allowed-tools 中** — 支持 `mcp__server__tool` 格式 | `skills/trust.rs` | 低 |
| 18 | **依赖管理** — Skill 间依赖解析 | `skills/dependency.rs` | 中 |

### P3 — 生态扩展（远期）

| # | 任务 | 涉及文件 | 复杂度 |
|---|------|---------|--------|
| 19 | **远程 Registry** — 类似 ClawHub 的 Skill 仓库 | 新 crate `octo-registry` | 高 |
| 20 | **SkillForge 自动发现** — GitHub/HuggingFace 扫描 | `skills/forge.rs` | 高 |
| 21 | **WASM Runtime** — 利用已有 Wasmtime 基础设施 | `skill_runtime/wasm.rs` | 中 |
| 22 | **Skill 签名验证** — 加密签名防篡改 | `skills/verify.rs` | 中 |
| 23 | **Skill 版本迁移** — 版本升级路径 | `skills/migration.rs` | 中 |
| 24 | **Requirements 预检** — 二进制依赖检查（Moltis 模式） | `skills/requirements.rs` | 低 |

---

## 七、参考项目索引

| 特性 | 最佳参考 | 参考路径 |
|------|---------|---------|
| Trust Attenuation | IronClaw | `3th-party/harnesses/rust-projects/ironclaw/` |
| SkillForge 自动发现 | ZeroClaw | `3th-party/harnesses/rust-projects/zeroclaw/src/skillforge/` |
| 多运行时脚本执行 | OpenFang | `3th-party/harnesses/rust-projects/openfang/` |
| 热重载 SkillWatcher | Moltis | `3th-party/harnesses/rust-projects/moltis/crates/skills/` |
| MetadataOnly 加载 | ZeroClaw | `3th-party/harnesses/rust-projects/zeroclaw/src/skills/` |
| Requirements 预检 | Moltis | `3th-party/harnesses/rust-projects/moltis/crates/skills/src/types.rs` |
| always 标记 | ZeroClaw | `3th-party/harnesses/rust-projects/zeroclaw/src/skills/mod.rs` |
| 社区 Registry | IronClaw (ClawHub) | `3th-party/harnesses/rust-projects/ironclaw/` |
| Credential Proxy | nanoclaw | `3th-party/harnesses/baselines/nanoclaw/src/container-runner.ts` |
| Agent Skills 标准 | agentskills.io | 官方规范 |

---

## 八、与 Agent Harness 方案的配合关系

本方案与 `AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` 紧密配合：

| Harness 组件 | Skills 配合点 | 说明 |
|-------------|-------------|------|
| **AgentLoop** | ToolCallInterceptor | allowed-tools 运行时强制在 Loop 中拦截 |
| **Tool 系统** | SkillTool + RiskLevel | Skill 通过 SkillTool 注册为 Tool，继承安全分级 |
| **Provider** | model 覆盖 | Skill 可指定使用的 Provider/Model |
| **Context** | L1/L2 prompt + always | 渐进加载集成到 SystemPromptBuilder |
| **SecurityPolicy** | TrustManager | Trust Level 与 SecurityPolicy 的 AutonomyLevel 协同 |
| **ContextPruner** | always 标记 | compact 模式豁免标记为 always 的 Skill |
| **MCP** | McpToolBridge | allowed-tools 支持 MCP 工具名格式 |
| **Extension** | SkillSourceType::PluginBundled | 扩展可捆绑自带 Skill |
| **SandboxManager** | 脚本执行隔离 | Skill 脚本通过 SandboxManager 执行 |

---

## 九、安全审计清单

| 检查项 | IronClaw 参考 | octo-sandbox 现状 | 目标 |
|--------|-------------|-------------------|------|
| Symlink 攻击 | canonicalize + reject | ❌ 无 | ✅ P1 实现 |
| 路径遍历 | 多层检查 | ✅ `..` 检查 | ✅ 已有 |
| Prompt Injection | SafetyLayer 扫描 | ❌ 无 | ✅ P1 与 AIDefence 集成 |
| XML/Tag Escape | XML entity escape | ❌ 无 | ✅ P2 |
| ReDoS | 正则复杂度检查 | ❌ 无 | ✅ P2 |
| 脚本超时 | tokio timeout | ❌ 无 | ✅ P1 实现 |
| 环境变量泄露 | env 过滤 | ❌ 无 | ✅ P1 实现 |
| allowed-tools 绕过 | 运行时拦截 | ❌ 仅格式验证 | ✅ P0 实现 |
| 信任降级 | Trust Attenuation | ❌ 无 | ✅ P0 实现 |
| 供应链攻击 | 签名验证 | ❌ 无 | ✅ P3 实现 |

---

## 十、结论

octo-sandbox 当前的 Skills 实现（5.5/10）具有良好的分层架构基础（loader → registry → tool → runtime），但存在一个**关键断联**：SkillTool.execute() 仅返回 body 文本，完全不执行脚本。同时缺乏运行时安全强制（Trust Level、allowed-tools 运行时拦截）。

**最佳实现策略**:

1. **P0 修复断联 + 安全**: 连通 SkillTool ↔ SkillRuntimeBridge，实现 TrustManager 和 allowed-tools 运行时强制
2. **P1 补全运行时**: 实现 NodeJS/Shell 运行时，集成 Context 系统
3. **P2 高级特性**: context-fork、model 覆盖、触发器
4. **P3 生态建设**: 远程 Registry、SkillForge、签名验证

以 IronClaw（9.5/10）为主要参考，结合 ZeroClaw 的 MetadataOnly/always/SkillForge 和 Moltis 的热重载/依赖管理，可在保持 octo-sandbox 现有优势的基础上实现 Agent Skills 标准的**完整支持（目标 9.5/10）**。
