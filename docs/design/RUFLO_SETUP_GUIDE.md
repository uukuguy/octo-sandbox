# RuFlo 项目级安装与自动化指南

> 本文档记录 RuFlo（claude-flow）在 octo-sandbox 项目中的安装配置方式，
> 以及 ADR/DDD 文档自动生成机制的工作原理。
> 供新项目复用时参考。

---

## 一、RuFlo 当前安装方式（项目级）

RuFlo 安装在项目目录下，核心文件位于：

```
octo-sandbox/
├── .mcp.json                    # MCP 服务器配置（Claude Code 读取）
├── .claude/
│   ├── agents/                  # 90+ 声明式 agent 定义
│   ├── commands/                # 自定义斜杠命令
│   └── helpers/                 # 核心自动化脚本（hooks、ADR、代码审查等）
├── .claude-flow/                # 运行时状态（gitignore）
└── .swarm/                      # swarm 协调状态（gitignore）
```

`.gitignore` 中已排除运行时数据：
```
.claude-flow/
.claude/memory.db
.claude/skills/
.swarm/
```

---

## 二、新项目安装步骤

### 第一步：初始化 claude-flow

```bash
# 生成 .claude/ 目录结构（agents、commands、helpers）
npx @claude-flow/cli@latest init --wizard

# 启动后台 daemon（hooks 依赖它）
npx @claude-flow/cli@latest daemon start

# 自动修复常见配置问题
npx @claude-flow/cli@latest doctor --fix
```

### 第二步：创建 `.mcp.json`

在项目根目录创建：

```json
{
  "mcpServers": {
    "claude-flow": {
      "command": "npx",
      "args": ["-y", "@claude-flow/cli@latest", "mcp", "start"],
      "env": {
        "npm_config_update_notifier": "false",
        "CLAUDE_FLOW_MODE": "v3",
        "CLAUDE_FLOW_HOOKS_ENABLED": "true",
        "CLAUDE_FLOW_TOPOLOGY": "hierarchical-mesh",
        "CLAUDE_FLOW_MAX_AGENTS": "15",
        "CLAUDE_FLOW_MEMORY_BACKEND": "hybrid"
      },
      "autoStart": false
    }
  }
}
```

### 第三步：更新 `.gitignore`

追加以下内容：

```gitignore
# RuFlo / Claude Flow 运行时状态（禁止提交）
.claude-flow/
.claude/memory.db
.claude/skills/
.swarm/
```

### 第四步：复制核心 helper 文件

从本项目复制 `.claude/helpers/` 目录，以下文件是自动化的核心：

| 文件 | 职责 |
|------|------|
| `hook-handler.cjs` | hooks 核心调度器（post-edit / post-task 入口） |
| `adr-generator.cjs` | ADR 文件生成逻辑（按类别写入 docs/adr/） |
| `intelligence.cjs` | 架构变更检测（识别 ARCH_PATTERNS） |
| `code-review.cjs` | 代码审查流水线（3 个审查 agent） |
| `adr-compliance.sh` | ADR 合规性检查脚本 |

### 第五步：复制基础 agent 定义

从本项目复制 `.claude/agents/core/` 目录：
- `coder.md`、`reviewer.md`、`tester.md`、`planner.md`、`researcher.md`

### 第六步：创建 `docs/adr/` 目录

```bash
mkdir -p docs/adr
mkdir -p docs/plans
```

### 第七步：配置 `CLAUDE.md`

参考本项目 `CLAUDE.md`，重点包含以下规则：
- RuFlo swarm 启动命令
- 研发闭环铁律（必须通过 RuFlo 编排）
- commit message 规范（末尾两行）

---

## 三、复杂任务的正确启动流程

每次开始复杂开发任务时，先执行：

```bash
npx @claude-flow/cli@latest swarm init \
  --topology hierarchical \
  --max-agents 8 \
  --strategy specialized
```

然后通过 Claude Code Task tool 启动并行 agent，让 RuFlo hooks 自动处理文档。

**完整研发闭环：**

```
用户请求
  └─► RuFlo swarm init（hierarchical topology）
        ├─► 编码 agent（coder）执行变更
        │     └─► PostToolUse hook → hook-handler.cjs post-edit
        │               → intelligence.detectArchChange()
        │               → 若是架构文件，recordArchChange()
        ├─► 审查 agent（reviewer / security-auditor）
        │     └─► 审查结果 → GitHub PR comment
        └─► PostToolUse hook → hook-handler.cjs post-task
                  → consumeArchChanges()
                  → adrGenerator.generateAdr()    → docs/adr/*.md
                  → updateDddTracking()            → docs/adr/DDD_CHANGE_LOG.md
```

---

## 四、ADR/DDD 自动生成机制

### 触发条件

`intelligence.cjs` 中定义了 `ARCH_PATTERNS`，匹配以下路径模式时触发架构变更记录：

| 类别 | 匹配规则 |
|------|---------|
| `agent-architecture` | `agent/`, `executor`, `runtime`, `catalog` |
| `mcp-integration` | `mcp/`, `McpManager`, `McpClient` |
| `security` | `auth/`, `security/`, `audit/` |
| `memory-system` | `memory/`, `vector_index`, `embedding` |
| `hook-system` | `hooks/`, `HookPoint`, `HookAction` |
| `provider` | `providers/`, `ProviderChain` |

### 工作流程

1. **post-edit**：每次文件编辑后，`hook-handler.cjs` 调用 `detectArchChange(file)`
   - 若匹配 `ARCH_PATTERNS` → 调用 `recordArchChange(file, category)`，追加到内存队列
   - 打印：`[ADR] Architecture change detected: <category> (<count> pending)`

2. **post-task**：任务完成后，`hook-handler.cjs` 调用 `consumeArchChanges()`
   - 消费队列中所有积累的变更
   - 调用 `adrGenerator.generateAdr(changes)` → 写入 `docs/adr/ADR_<CATEGORY>.md`
   - 调用 `updateDddTracking()` → 追加到 `docs/adr/DDD_CHANGE_LOG.md`

### 生效前提

| 条件 | 验证方式 |
|------|---------|
| daemon 运行中 | `npx @claude-flow/cli@latest daemon status` |
| `.claude-flow/data/` 目录存在 | 首次 `swarm init` 后自动创建 |
| hooks 已注册 | Claude Code 全局 `settings.json` 或项目级配置 |

### 当前已生成的 ADR 文件

```
docs/adr/
├── ADR_AGENT_ARCHITECTURE.md       # agent 架构决策
├── ADR_MCP_INTEGRATION.md          # MCP 集成决策
├── ADR_MULTI_AGENT_ORCHESTRATION.md # 多 agent 编排决策
├── ADR_SECURITY_REFACTORING.md     # 安全重构决策
├── DDD_CHANGE_LOG.md               # DDD 变更追踪日志
└── DDD_DOMAIN_ANALYSIS.md          # DDD 领域分析
```

---

## 五、最小文件集（新项目快速复用）

只需复制以下文件即可获得完整的研发闭环能力：

```
.mcp.json
.claude/helpers/hook-handler.cjs
.claude/helpers/adr-generator.cjs
.claude/helpers/intelligence.cjs
.claude/helpers/code-review.cjs
.claude/agents/core/coder.md
.claude/agents/core/reviewer.md
.claude/agents/core/tester.md
.claude/agents/core/planner.md
.claude/agents/core/researcher.md
```

然后运行：
```bash
npx @claude-flow/cli@latest daemon start
```

---

## 六、常见问题

**Q: ADR 没有自动生成？**
- 检查 daemon 是否运行：`npx @claude-flow/cli@latest daemon status`
- 检查 `.claude-flow/data/` 是否存在
- 确认编辑的文件路径匹配 `ARCH_PATTERNS`（纯业务逻辑文件不触发）

**Q: hook-handler.cjs 报错找不到模块？**
- 确认 `.claude/helpers/` 目录完整复制
- 运行 `npx @claude-flow/cli@latest doctor --fix`

**Q: swarm init 超时？**
- 检查网络连接（需要下载 `@claude-flow/cli@latest`）
- 尝试先 `npm install -g @claude-flow/cli` 本地安装

**Q: 直接用 Task tool 绕过 RuFlo 可以吗？**
- 可以执行代码，但会跳过 post-edit / post-task hooks
- ADR/DDD 文档不会自动更新，架构文档会脱节
- 建议：简单单文件修改可直接 Edit，架构级变更必须走 RuFlo
