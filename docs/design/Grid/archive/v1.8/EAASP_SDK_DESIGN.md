# EAASP Enterprise SDK 全景设计蓝图

> **版本**: v1.0
> **创建日期**: 2026-04-07
> **权威参考**: `EAASP_-_企业自主智能体支撑平台设计规范_v1.7_.pdf`
> **基线**: Phase BF 完成 @ ff5ad56（L2 统一资产层 + L1 抽象机制）

---

## 一、定位与目标

### 1.1 SDK 是什么

Enterprise SDK 是**企业业务开发者**与 EAASP 平台交互的主要编程接口。它封装 7 个核心抽象概念（Agent / Skill / Tool / Policy / Playbook / Session / Message），让开发者：

1. **创作内容**：编写 Skill、定义 Policy、编排 Playbook — 不感知 gRPC/proto/L1-L4 层次
2. **推演验证**：在 Grid 产品矩阵（grid-cli / grid-server / grid-runtime）上测试 Skill — 不感知运行时选择器和容器化
3. **调用平台**：通过统一 API 创建会话、发送消息、管理 Skill 生命周期 — 不感知 L3 三方握手和 hook 注入

### 1.2 SDK 不是什么

- **不是 L1 Runtime**：SDK 不包含运行时引擎（规范 KD-5 明确区分）
- **不是运行模拟器**：SDK 不内嵌"Mini L1"，避免规范反模式"双重抽象"
- **不是 gRPC 客户端库**：开发者看到的是 Skill/Policy/Playbook，不是 InitializeRequest/SendRequest

### 1.3 与规范的对应关系

| 规范概念 | SDK 封装 | 开发者接触面 |
|--------|---------|-----------|
| L2 Skill 仓库 §5 | `eaasp.authoring` — Skill 创作工具链 | 编写 SKILL.md、校验、提交 |
| L3 策略 §4 | `eaasp.models.Policy` — 策略规则模型 | 定义业务规则 |
| L4 五个 API 契约 §8 | `eaasp.client` — REST API 客户端 | 会话管理、意图分发 |
| L1 运行时契约 §6.5 | `eaasp.sandbox` — 推演适配器 | 跨运行时 Skill 测试 |
| 跨层 Hooks §10 | `eaasp.authoring.hook_builder` — Hook 脚手架 | 编写 hook handler |

---

## 二、全景架构

### 2.1 SDK 包结构

```
sdk/
├── specs/                         # 跨语言抽象规范（JSON Schema）
│   ├── skill.schema.json          # Skill 数据模型 + SKILL.md 格式
│   ├── policy.schema.json         # Policy 规则模型
│   ├── playbook.schema.json       # Playbook 编排模型
│   ├── tool.schema.json           # Tool/MCP Server 注册模型
│   ├── message.schema.json        # Message 格式
│   ├── session.schema.json        # Session 生命周期
│   └── agent.schema.json          # Agent 能力清单
│
├── python/                        # Python SDK（参考实现）
│   ├── pyproject.toml             # 包名: eaasp-sdk
│   └── src/eaasp/
│       ├── __init__.py            # 顶层 re-export
│       ├── models/                # Pydantic v2 模型（对齐 specs/）
│       │   ├── skill.py           # Skill, SkillFrontmatter, ScopedHook
│       │   ├── policy.py          # PolicyRule, Policy
│       │   ├── playbook.py        # Playbook, PlaybookStep
│       │   ├── tool.py            # ToolDef, McpServerConfig
│       │   ├── message.py         # UserMessage, ResponseChunk
│       │   ├── session.py         # SessionConfig, SessionState
│       │   └── agent.py           # AgentCapability, CapabilityManifest
│       │
│       ├── authoring/             # 创作工具链
│       │   ├── skill_parser.py    # SKILL.md ↔ Skill 双向解析
│       │   ├── skill_validator.py # 多层校验器
│       │   ├── skill_scaffold.py  # 脚手架生成
│       │   └── hook_builder.py    # Hook handler 脚本生成
│       │
│       ├── sandbox/               # 沙盒推演适配器
│       │   ├── base.py            # SandboxAdapter ABC
│       │   ├── grid_cli.py        # GridCliSandbox（本地子进程）
│       │   ├── grid_server.py     # GridServerSandbox（HTTP/WS）
│       │   ├── runtime.py         # RuntimeSandbox（gRPC 直连 L1）
│       │   ├── multi_runtime.py   # MultiRuntimeSandbox（跨运行时对比）
│       │   └── platform.py        # PlatformSandbox（L4 API 网关）
│       │
│       ├── client/                # L3/L4 REST API 客户端
│       │   ├── base.py            # 认证、重试、错误处理
│       │   ├── sessions.py        # 契约 5: 会话控制
│       │   ├── intents.py         # 契约 2: 意图网关
│       │   ├── skills.py          # 契约 3: 技能生命周期
│       │   ├── policies.py        # 契约 1: 策略部署
│       │   └── telemetry.py       # 契约 4: 遥测采集
│       │
│       └── cli/                   # 命令行工具
│           ├── __main__.py        # eaasp CLI 入口
│           ├── init_cmd.py        # eaasp init
│           ├── validate_cmd.py    # eaasp validate
│           ├── test_cmd.py        # eaasp test / eaasp test --compare
│           └── submit_cmd.py      # eaasp submit
│
├── typescript/                    # TypeScript SDK（镜像 Python 结构）
│   └── (S6 阶段实现)
│
└── examples/                      # 企业场景示例
    ├── hr-onboarding/             # HR 入职 workflow-skill
    ├── legal-review/              # 法务审查 domain-skill + policy
    └── sales-report/              # 销售报告 production-skill
```

### 2.2 分层依赖

```
┌─────────────────────────────────────────────────┐
│ CLI (click + rich)                               │
│   eaasp init / validate / test / submit          │
├──────────┬──────────┬───────────────────────────┤
│ authoring│ sandbox  │ client                     │
│ 创作工具链│ 沙盒推演 │ REST API 客户端             │
├──────────┴──────────┴───────────────────────────┤
│ models (Pydantic v2)                             │
│ 7 个抽象概念的数据模型                             │
├─────────────────────────────────────────────────┤
│ specs/ (JSON Schema)                             │
│ 跨语言契约 — Python/TS 模型的共同源头              │
└─────────────────────────────────────────────────┘
```

**依赖规则**：
- `models` 只依赖 `pydantic` + `pyyaml`（零运行时依赖）
- `authoring` 只依赖 `models`（纯离线操作）
- `sandbox` 依赖 `models` + 可选 `grpcio`（连接运行时时）
- `client` 依赖 `models` + `httpx`（REST API 调用）
- `cli` 依赖以上所有 + `click` + `rich`

---

## 三、7 个抽象概念

### 3.1 Skill（技能）— 核心创作对象

Skill 是 EAASP 平台的智能化知识资产。开发者通过编写 SKILL.md 文件创建 Skill。

**SKILL.md 格式**（规范 §5.3）：

```markdown
---
name: hr-onboarding
version: "1.0.0"
description: 新员工入职流程自动化
author: hr-team
tags: [hr, onboarding, workflow]
skill_type: workflow
preferred_runtime: grid
scope: bu

hooks:
  - event: PreToolUse
    handler_type: command
    config:
      command: "python hooks/check_pii.py"
      match: { tool_name: "file_write" }
  - event: Stop
    handler_type: prompt
    config:
      prompt: "验证入职清单是否全部完成"

dependencies:
  - org/it-account-setup
  - org/badge-provisioning
---

你是一位经验丰富的 HR 专家...

## 工作流程
1. 收集新员工信息...
```

**SDK 数据模型**：

```python
class SkillFrontmatter(BaseModel):
    name: str
    version: str = "0.1.0"
    description: str
    author: str
    tags: list[str] = []
    skill_type: Literal["workflow", "production", "domain", "meta"]
    preferred_runtime: str | None = None
    compatible_runtimes: list[str] | None = None
    hooks: list[ScopedHook] = []
    dependencies: list[str] = []
    scope: Literal["global", "bu", "dept", "team"] = "global"

class ScopedHook(BaseModel):
    event: Literal["PreToolUse", "PostToolUse", "Stop"]
    handler_type: Literal["command", "http", "prompt", "agent"]
    config: dict

class Skill(BaseModel):
    frontmatter: SkillFrontmatter
    prose: str

    def to_skill_md(self) -> str: ...
    @classmethod
    def from_skill_md(cls, content: str) -> "Skill": ...
    @classmethod
    def from_file(cls, path: Path) -> "Skill": ...
```

**Skill 类型**（规范 §5.2）：

| 类型 | 用途 | Hook 集成方式 |
|------|------|-----------|
| **workflow** | 业务流程编码（入职、审批、报告） | frontmatter hooks 做质量保障 |
| **production** | 输出最佳实践（docx、pdf 生成规范） | command hooks 做格式校验 |
| **domain** | 专业领域知识（法律术语、医学编码） | prompt hooks 做合规检查 |
| **meta** | Skill 管理工具（评估器、优化器） | agent hooks 做深度评估 |

### 3.2 Policy（策略）— 治理核心

Policy 是企业管理员在 L4 管理控制台定义的业务规则，由 L3 策略编译器翻译为可执行的受管 hooks。

```python
class PolicyRule(BaseModel):
    name: str
    description: str                    # 业务语言描述
    scope: Literal["enterprise", "bu", "dept", "team"]
    event: Literal["SessionStart", "PreToolUse", "PostToolUse",
                    "Stop", "UserPromptSubmit", "PermissionRequest"]
    match: dict = {}                    # 匹配条件
    action: Literal["deny", "allow", "modify", "audit", "escalate"]
    handler_type: Literal["command", "http", "prompt", "agent"]
    handler_config: dict = {}

class Policy(BaseModel):
    name: str
    version: str
    rules: list[PolicyRule]
```

**SDK 在 BG 阶段**只提供数据模型，不提供 DSL。Policy DSL（如 `@deny_if(tool="bash", role="intern")`）留 S3 阶段配合 L3 策略编译器实现。

### 3.3 Playbook（剧本）— 多 Skill 编排

Playbook 定义多个 Skill 的执行顺序和条件分支。

```python
class PlaybookStep(BaseModel):
    skill_id: str
    input_template: str | None = None   # 传递给 Skill 的输入模板
    condition: str | None = None        # 执行条件表达式
    on_failure: Literal["stop", "skip", "retry"] = "stop"

class Playbook(BaseModel):
    name: str
    version: str
    description: str
    steps: list[PlaybookStep]
    trigger: dict = {}                  # 触发条件（cron/webhook/event）
```

**BG 阶段只定义骨架模型**，Playbook 编排引擎留 S4 阶段配合 L4 事件总线实现。

### 3.4 Tool（工具）— MCP Server 封装

```python
class ToolDef(BaseModel):
    name: str
    description: str
    input_schema: dict                  # JSON Schema
    handler: str                        # 实现指向

class McpServerConfig(BaseModel):
    name: str
    transport: Literal["stdio", "sse", "streamable-http"]
    command: str | None = None          # stdio 模式
    args: list[str] = []
    url: str | None = None              # sse/http 模式
    env: dict[str, str] = {}
```

### 3.5 Message（消息）

```python
class UserMessage(BaseModel):
    content: str
    message_type: Literal["text", "intent"] = "text"
    metadata: dict[str, str] = {}

class ResponseChunk(BaseModel):
    chunk_type: Literal["text_delta", "tool_start", "tool_result",
                         "thinking", "done", "error"]
    content: str = ""
    tool_name: str | None = None
    tool_id: str | None = None
    is_error: bool = False
```

### 3.6 Session（会话）

```python
class SessionConfig(BaseModel):
    user_id: str
    user_role: str = ""
    org_unit: str = ""
    skill_ids: list[str] = []           # 预加载的 Skill IDs
    quotas: dict[str, str] = {}
    context: dict[str, str] = {}

class SessionState(BaseModel):
    session_id: str
    state_data: bytes                   # opaque 序列化状态
    runtime_id: str
    created_at: str
    state_format: str                   # "rust-serde-v1" | "python-json"
```

### 3.7 Agent（智能体）

```python
class AgentCapability(BaseModel):
    runtime_id: str
    runtime_name: str
    tier: Literal["harness", "aligned", "framework"]
    model: str
    context_window: int = 0
    supported_tools: list[str] = []
    native_hooks: bool = False
    native_mcp: bool = False
    native_skills: bool = False
    cost_input_per_1k: float = 0.0
    cost_output_per_1k: float = 0.0
    requires_hook_bridge: bool = False
    deployment_mode: Literal["shared", "per_session"] = "shared"
```

---

## 四、创作工具链

### 4.1 SkillParser — 双向解析

```python
class SkillParser:
    @staticmethod
    def parse(content: str) -> Skill:
        """SKILL.md 文本 → Skill 模型（分离 YAML frontmatter + prose）"""

    @staticmethod
    def render(skill: Skill) -> str:
        """Skill 模型 → SKILL.md 文本"""

    @staticmethod
    def parse_file(path: Path) -> Skill:
        """从文件读取并解析"""
```

### 4.2 SkillValidator — 多层校验

```python
class ValidationResult(BaseModel):
    valid: bool
    errors: list[ValidationError] = []      # 必须修复
    warnings: list[ValidationWarning] = []  # 建议修复

class SkillValidator:
    def validate(self, skill: Skill) -> ValidationResult:
        """执行全部校验规则"""

    # 校验规则：
    # 1. frontmatter 结构完整性（必填字段非空）
    # 2. hook event 合法性（仅 PreToolUse / PostToolUse / Stop）
    # 3. handler_type 合法性（command / http / prompt / agent）
    # 4. 依赖 ID 格式（org/name 格式）
    # 5. scope 层级合法性
    # 6. prose 非空且有实质内容（>50 字符）
    # 7. 运行时亲和性 × hook handler 兼容性
    #    （agent handler 需要运行时支持子智能体能力）
    # 8. skill_type × hook 组合合理性
    #    （workflow 应有 Stop hook 做完成性校验）
```

### 4.3 SkillScaffold — 脚手架生成

```python
class SkillScaffold:
    @staticmethod
    def create(name: str, skill_type: str = "workflow",
               output_dir: Path = Path(".")) -> Path:
        """
        生成 Skill 项目骨架：
        my-skill/
        ├── SKILL.md              # 按 skill_type 选择模板
        ├── hooks/                # hook handler 脚本
        │   └── example_hook.py   # 示例 command handler
        └── tests/
            └── test_cases.jsonl  # 示例测试用例
        """
```

### 4.4 HookBuilder — Hook 脚本生成

```python
class HookBuilder:
    @staticmethod
    def command_handler(name: str, event: str) -> str:
        """生成 command handler 骨架（读 stdin JSON → 判断 → stdout allow/deny）"""

    @staticmethod
    def http_handler(name: str, event: str) -> str:
        """生成 http handler 骨架（FastAPI endpoint）"""

    @staticmethod
    def prompt_handler(prompt: str) -> dict:
        """生成 prompt handler 配置字典"""
```

---

## 五、沙盒推演体系

### 5.1 设计原则

1. **SDK 不造运行器** — 复用 Grid 产品矩阵作为推演后端
2. **开发阶段可直连 L1** — gRPC 直连用于跨运行时验证（非生产路径）
3. **生产阶段走 L4** — PlatformSandbox 通过 L4 API 网关（BH+ 可用）

### 5.2 SandboxAdapter 抽象

```python
class SandboxAdapter(ABC):
    @abstractmethod
    async def initialize(self, config: SessionConfig,
                         skills: list[Skill]) -> str:
        """创建会话，加载 Skills，返回 session_id"""

    @abstractmethod
    async def send(self, session_id: str,
                   message: UserMessage) -> AsyncIterator[ResponseChunk]:
        """发送消息，流式返回响应"""

    @abstractmethod
    async def terminate(self, session_id: str) -> TelemetrySummary:
        """结束会话，返回遥测摘要"""

    @abstractmethod
    async def validate_skill(self, skill: Skill) -> ValidationResult:
        """在真实运行时中校验 Skill 可加载性"""
```

### 5.3 推演结果模型

```python
class TelemetrySummary(BaseModel):
    session_id: str
    total_turns: int
    tools_called: list[str]
    hooks_fired: list[HookFiredEvent]
    input_tokens: int
    output_tokens: int
    duration_ms: int
    skill_loaded: bool
    completed_normally: bool

class HookFiredEvent(BaseModel):
    event: str                # "PreToolUse" / "PostToolUse" / "Stop"
    hook_source: str          # "managed" / "skill_scoped" / "project" / "user"
    decision: str             # "allow" / "deny" / "modify"
    tool_name: str | None
    latency_ms: int

class ComparisonResult(BaseModel):
    results: dict[str, TelemetrySummary]   # endpoint → summary
    consistency: ConsistencyReport

class ConsistencyReport(BaseModel):
    all_completed: bool
    tools_diff: list[str]
    hooks_diff: list[str]
    output_similarity: float               # 0.0 - 1.0
```

### 5.4 适配器实现矩阵

| 适配器 | 后端 | 连接方式 | 用途 | 阶段 |
|--------|------|---------|------|------|
| **GridCliSandbox** | grid-cli 子进程 | stdin/stdout pipe | 本地快速推演 | S1 (BG) |
| **RuntimeSandbox** | L1 gRPC server | grpc:// 直连 | 跨运行时验证 | S1 (BG) |
| **MultiRuntimeSandbox** | 多 L1 gRPC | 并行 grpc:// | 盲盒对比 | S1 (BG) |
| **GridServerSandbox** | grid-server | HTTP/WS | 远程推演 | S2 (BG-D5) |
| **PlatformSandbox** | L4 API 网关 | HTTPS REST | 生产路径 | S5 (BH+) |

### 5.5 与 Grid 产品矩阵的对接

| SDK 操作 | grid-cli | grid-server | grid-runtime (gRPC) | L4 API 网关 |
|---------|----------|-------------|---------------------|------------|
| initialize | `grid eval` 子进程 | `POST /api/sessions` | `rpc Initialize` | `POST /v1/sessions/create` |
| send | stdin/stdout | `WS /ws` | `rpc Send` (stream) | `POST /v1/sessions/{id}/message` |
| load_skill | `--skill` 参数 | `POST /api/.../skills` | `rpc LoadSkill` | 三方握手 payload |
| terminate | 进程退出 | `DELETE /api/sessions/{id}` | `rpc Terminate` | `DELETE /v1/sessions/{id}` |
| validate | `grid eval config` | `POST /api/.../validate` | `rpc GetCapabilities` | L2 Skill Registry |

---

## 六、CLI 工具

### 6.1 命令集

```bash
# 项目初始化
eaasp init <name> [--type workflow|production|domain|meta]

# Skill 校验
eaasp validate <path>                          # 结构+hooks+依赖校验

# 沙盒推演
eaasp test <path> [--sandbox local]            # 本地 grid-cli 推演
eaasp test <path> --sandbox grpc://<addr>      # 指定运行时推演
eaasp test <path> --compare <addr1>,<addr2>    # 多运行时对比

# 提交到 L2 Skill Registry
eaasp submit <path> --registry <url>           # draft 状态

# 未来命令（S3+）
eaasp promote <skill-id> --to tested|reviewed|production
eaasp policy compile <path>                    # 策略编译
eaasp playbook run <path>                      # Playbook 执行
```

### 6.2 技术选型

- **框架**: click（Python CLI 标准）
- **输出**: rich（彩色表格、进度条、树状结构）
- **配置**: `eaasp.toml`（项目级配置，指定默认 registry URL、sandbox 后端等）

---

## 七、长期演进路线

### 7.1 阶段总览

| 阶段 | Phase | 交付内容 | 前置条件 | 测试目标 |
|------|-------|--------|--------|---------|
| **S1: 基石** | BG | specs + models + authoring + sandbox(local/gRPC/multi) + CLI | L2 Skill Registry (BF ✅) | ~50 |
| **S2: 推演增强** | BG-D/BH | GridServerSandbox + test 报告 + coverage 分析 | grid-server Skill API | ~20 |
| **S3: 治理** | BH | Policy DSL + hook handler 脚手架增强 + L3 策略编译器对接 | L3 治理层 (BH) | ~30 |
| **S4: 编排** | BH/BI | Playbook DSL + 多 Skill 编排引擎 + 事件触发 | L4 事件总线 (BI) | ~25 |
| **S5: 客户端** | BI | 5 个 REST API 完整客户端 + OAuth 认证 + PlatformSandbox | 5 API 契约 (BH) | ~30 |
| **S6: TypeScript** | BI/BJ | TS SDK 从 specs/ 生成 + 平行实现全部功能 | Python SDK 稳定 | ~50 |
| **S7: 生态** | BJ+ | MCP Tool 封装辅助 + Java/Go SDK + 第三方插件 | 平台开放 API | ~30 |

### 7.2 分支策略

- **BG（S1 基石）在当前 Grid 分支完成**
- **后续 SDK 演进在独立 `eaasp-sdk` 分支并行**
- **里程碑式合流**：当 EAASP 主线完成 L3/L4 后，SDK 分支 rebase 并合入

### 7.3 版本策略

| SDK 版本 | 对应 EAASP 阶段 | API 稳定性 |
|---------|--------------|---------|
| 0.1.x | S1 (BG) | Alpha — API 可能变化 |
| 0.2.x | S2-S3 (BH) | Beta — 核心 API 稳定 |
| 0.5.x | S4-S5 (BI) | RC — 全功能覆盖 |
| 1.0.0 | S6+ (BJ) | Stable — 多语言发布 |

---

## 八、关键设计决策

| ID | 决策 | 理由 | 规范依据 |
|----|------|------|---------|
| SDK-KD1 | Python SDK 先行（参考实现） | AI 生态最成熟，claude-code-runtime 也是 Python | §15.3 Python 团队 |
| SDK-KD2 | specs/ JSON Schema 是跨语言源头 | 避免 Python/TS 模型不一致 | §14 完整设计决策轨迹 |
| SDK-KD3 | SDK 不内嵌运行模拟器 | 规范反模式"双重抽象" | §13 设计反模式 |
| SDK-KD4 | sandbox 支持 gRPC 直连 L1 | 跨运行时 Skill 可移植性验证 | §6.1 运行时选择器 |
| SDK-KD5 | 核心零运行时依赖 | authoring 纯离线，不强制装 grpcio | 最小依赖原则 |
| SDK-KD6 | 创作优先于调用 | Skill/Policy 质量决定平台价值 | §5 L2 技能资产层 |
| SDK-KD7 | Grid 产品矩阵即推演环境 | grid-cli/server/runtime 已覆盖全场景 | §7.1 运行时分层 |
| SDK-KD8 | 生产路径走 L4，不直连 L1 | 架构一致性，L1 容器不应被外部直接调用 | §11.5 L1 执行区域 |
| SDK-KD9 | Skill 是第一公民 | 字段最丰富、工具链最完整、开发者最常用 | §5.3 Skill 内容结构 |
| SDK-KD10 | BG 不做 Policy DSL | 需要 L3 策略编译器配合才有实际价值 | §4.1 策略引擎 |

---

## 九、开放问题（后续阶段决策）

| # | 问题 | 影响阶段 | 选项 |
|---|------|---------|------|
| SDK-OQ1 | TS SDK 是手写还是从 JSON Schema 生成 | S6 | 手写（灵活）vs codegen（一致性） |
| SDK-OQ2 | Policy DSL 用 Python 装饰器还是独立语言 | S3 | `@deny_if(...)` 装饰器 vs YAML/TOML 规则文件 |
| SDK-OQ3 | Playbook 用 Python 代码还是声明式文件 | S4 | 代码编排 vs YAML 声明 |
| SDK-OQ4 | SDK 是否提供 MCP Server 开发框架 | S7 | 独立包 vs SDK 内置 |
| SDK-OQ5 | eaasp CLI 是否与 grid CLI 合并 | S2 | 独立 CLI vs `grid sdk ...` 子命令 |
