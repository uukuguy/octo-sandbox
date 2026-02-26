# octo-sandbox 架构设计 Brainstorming Checkpoint

**日期**: 2026-02-25
**阶段**: 架构设计 brainstorming（进行中）
**进度**: 8/8 段全部确认 ✅ Brainstorming 完成（含记忆模块）

---

## 项目定位

企业级自主智能体工具模块（Skills/MCP/CLI）安全沙箱调试环境。将主流顶级自主智能体（Claude Code、OpenClaw等）圈养在沙箱中，提供安全可控的工具开发调试环境。

---

## 已完成的参考项目分析

对 `./github.com/` 下 8 个参考项目进行了深度代码分析：

| 项目 | 语言 | 核心定位 | 关键可复用点 |
|------|------|---------|------------|
| pi-mono | TypeScript | 核心智能体框架 | Agent Loop、Tool 系统、Skill 加载、25+ LLM Provider |
| pi_agent_rust | Rust | pi-mono 的 Rust 重写 (225K行) | Provider trait、Tool trait、QuickJS 扩展、安全模型 |
| pi-skills | Markdown+JS | 技能定义库 | SKILL.md 格式、{baseDir} 模板、跨智能体兼容 |
| OpenClaw | TypeScript | 完整 AI 助手平台 | Gateway 架构、20+ 渠道、Plugin 系统、沙箱隔离 |
| nanoclaw | TypeScript | 最简 OpenClaw 替代 | 容器隔离、IPC 协议、Cursor 恢复 |
| happyclaw | TypeScript | 企业级多用户服务器 | RBAC、Docker+Host 双模式、飞书/Telegram、Web 终端 |
| craft-agents-oss | TS/React | 专业桌面 UI | shadcn/ui、多会话 Inbox、Diff 视图、MCP 集成 |
| zeroclaw | Rust | 超轻量 CLI 智能体 | Trait 插件系统、22+ Provider、混合搜索、<5MB |

---

## 已确认的技术决策

### 1. Rust 双重角色
- **独立自主智能体**：自有 Agent Loop / Tool / Provider（参考 pi_agent_rust + zeroclaw）
- **沙箱调度器**：管理 CC/OpenClaw 等被圈养的智能体容器
- 优势：自举测试能力、不依赖外部智能体即可运行、多 LLM 灵活性
- 额外工作量约 20-30%，值得投入

### 2. Skills/MCP 标准兼容
- 原生支持 MCP 开放标准（2025-11-25 最新版）
- MCP: JSON-RPC 2.0, stdio + Streamable HTTP 传输
- Skill: 直接解析标准 SKILL.md（YAML frontmatter + Markdown + `{baseDir}`）
- 与 CC/pi-mono 共用同一生态，零适配

### 3. Session 管理
- **SQLite WAL 主存储**：结构化查询、并发安全、FTS5 全文检索、分支管理
- **JSONL 兼容层**：导入 pi_agent_rust session、导出、审计

### 4. 通信通道架构（四种通道 + Debug Interceptor）
- **MCP stdio**：标准工具调用（所有运行时通用）
- **gRPC/tonic**：高性能控制通道（Docker/VM，双向流）
- **WASM hostcall**：WASM 宿主函数调用（零开销）
- **Unix Socket**：同主机低延迟备选
- **Debug Interceptor**：可选的请求/响应 JSON 文件记录层（替代文件 IPC，用于调试可观测性）

### 5. 运行时优先级
1. **WASM** (wasmtime/wasmer)：首批实现，轻量工具沙箱，毫秒级启动
2. **Docker**：完整智能体运行时（CC/OpenClaw）
3. **Apple Container**：macOS 优化（可选）

---

## 已确认的架构设计段落

### 第一段：系统分层与核心组件（✅ 已确认）

```
Web UI (React) → Rust 核心服务 (octo-engine) → 沙箱运行时
```

六大核心组件：
- Agent Engine（自主智能体）
- Sandbox Manager（容器调度）
- Channel Router（渠道路由）
- Tool Registry（工具注册 + 策略引擎）
- Session Store（SQLite WAL + JSONL）
- Auth & RBAC（多用户 + 审计）

### 第二段：Agent Engine 内部架构（✅ 已确认）

- **Agent Loop**：参考 pi_agent_rust（最大50轮，8并发工具执行）
- **Provider Trait**：MVP 先支持 Anthropic + OpenAI + Gemini，trait 可扩展
- **Tool Trait**：7 内置工具 + MCP 动态工具，统一接口，对 Agent Loop 透明
- **Skill Loader**：直接解析标准 SKILL.md，与 CC/pi-mono 完全兼容
- **Context Manager**：System Prompt 构建、上下文压缩/摘要、Session 分支、消息队列

### 第三段：沙箱管理器与容器隔离（✅ 已确认）

- **Container Lifecycle**：GroupQueue 并发控制（参考 happyclaw: 20容器+5本地）
- **RuntimeAdapter trait**：WASM → Docker → Apple Container
- **Mount Security**：白名单验证、系统路径黑名单、Symlink 遍历防护、非管理员 read-only
- **Transport Trait**：MCP stdio + gRPC/tonic + WASM hostcall + Unix Socket
- **Agent Profiles**：CC/OpenClaw/Custom 配置化，新增类型只需新增 profile
- **Debug Interceptor**：可选开启，所有请求/响应写入 JSON 文件用于调试审计

### 第四段：外部渠道和多用户体系（✅ 已确认）

**Channel Trait（简洁路线，参考 zeroclaw + openclaw capabilities）**：
- 7 个核心方法：`id()`, `capabilities()`, `send()`, `send_stream()`, `listen()`, `request_permission()`, `health_check()`
- `ChannelCapabilities` 声明：streaming / rich_text / file_upload / interactive / bidirectional
- `MessageContent` 四类型：Text / Command / File / Structured
- 不走 openclaw 15 adapter 路线，保持简洁

**MVP 四渠道**：
- **WebChannel**（WebSocket，核心渠道，完整 capabilities）
- **CliChannel**（stdin/stdout，开发者本地调试）
- **ApiChannel**（REST/gRPC，自动化测试和集成）
- **TelegramChannel**（验证 IM trait 设计，参考 zeroclaw 实现）

**消息路由**：
- 路由键：`{user_id}:{sandbox_id}:{session_id}`
- 流程：Channel → Channel Router → Auth Guard → Session Manager → Agent/Sandbox

**三角色 RBAC**：
- **Admin**：管理用户/系统配置/所有沙箱
- **Developer**：创建管理自己的沙箱、安装 MCP/Skills
- **Viewer**：只读访问被授权的沙箱

**双层权限模型**：
- 系统层：RBAC 角色控制
- Session 层：**ReadOnly / Interactive / AutoApprove** 三级权限模式
  - ReadOnly：工具不可写入（只读调试）
  - Interactive：危险操作需确认（默认）
  - AutoApprove：自动批准（需 Admin 或明确授权）

**Per-User 隔离**：
- `data/users/{user_id}/` 独立 workspace（沙箱定义、凭据 AES-256-GCM、私有 Skills/MCP）
- `data/shared/` 全局共享 Skills、MCP Servers、沙箱模板
- `data/system/` 主数据库 SQLite、审计日志、系统配置

**认证 MVP**：密码 bcrypt-12 + HMAC Cookie + 邀请码注册（后期 OAuth2/LDAP）

---

### 第五段：工具调试面板（✅ 已确认）

**五大功能模块（全部进 MVP，分阶段实现）**：

**A. Tool Execution Inspector（工具执行检查器）**：
- 完整请求/响应记录（含 JSON-RPC 原始数据）
- 执行时间线（timeline view）+ 精确计时
- 按工具类型的 Overlay 渲染（参考 craft-agents：Code/Terminal/JSON/Document）
- Replay 功能：**Phase 2 加入**

**B. MCP Server Workbench（MCP 服务器工作台）**：
- 生命周期管理（安装/启动/停止/重启/删除）
- 工具发现（list_tools）实时显示
- 手动工具调用：JSON Schema 驱动参数表单 → 执行 → 查看结果
- MCP Server 日志流实时查看

**C. Skill Development Studio（Skill 开发工作室）**：
- SKILL.md 在线编辑器（YAML frontmatter 语法高亮 + 预览）
- Skill 变量（{baseDir} 等）实时解析预览
- 沙箱测试执行 + 兼容性验证（CC/pi-mono 解析检查）

**D. Cross-Agent Comparison（跨智能体对比测试）**：
- **可选功能**（非核心优先）
- 同一工具在不同智能体（Rust/CC/OpenClaw）中执行
- 并排展示：输入/输出/时间/资源，差异高亮
- 支持批量测试集

**E. Debug Interceptor Dashboard（调试拦截器仪表板）**：
- 第三段 Debug Interceptor 的可视化界面
- 请求/响应实时流 + 过滤/搜索/聚合
- 异常检测（超时、错误、异常响应）

**核心数据模型**：
- `ToolExecution`：执行 ID、工具名、来源（BuiltIn/MCP/Skill）、Agent 标识、输入/输出、原始 JSON-RPC、计时、状态、资源消耗
- `McpServerState`：Server ID、传输类型、工具列表、日志缓冲、健康状态
- `ComparisonTest`：多 Agent 执行结果 + 差异分析

**存储策略**：
- 热数据（24h）：内存 + SQLite
- 温数据（7天）：SQLite
- 冷数据（>7天）：archive 表或 JSONL 导出，可配置保留策略

**UI 布局**：Tab 式切换 — Chat | Tools | MCP | Skills | Compare | Debug

---

### 第六段：Web UI 架构（✅ 已确认）

**技术栈**：
- React 19 + Vite 6 + TypeScript
- shadcn/ui（Radix UI + Tailwind CSS 4）+ lucide-react
- Jotai（atomFamily per-sandbox 状态隔离，参考 craft-agents）
- WebSocket 单例（参考 happyclaw WsManager，指数退避重连）
- react-markdown + Shiki（语法高亮）
- @tanstack/react-virtual（消息/执行记录虚拟化）
- @xterm/xterm 6（Web Terminal）
- React Router 7（SPA 路由）
- Monaco Editor diff（调试场景 diff 视图）

**事件处理架构**：
- 纯函数事件处理器（参考 craft-agents processEvent 模式）
- WebSocket → EventRouter → 按事件类型分发到 Jotai atoms
- 事件类型：session/text/tool/mcp/debug/system/permission

**布局**：
- TopBar（沙箱选择 + 用户 + 权限模式）
- NavRail（左侧沙箱列表）
- Tab Bar（Chat | Tools | MCP | Skills | Compare | Debug）
- Main Content（per-tab 独立面板）
- Bottom Panel（可折叠日志/Terminal）

**组件结构**：
- `atoms/` — sessions, tools, mcp, skills, debug, auth, ui
- `events/` — 纯函数处理器 + 按类型分 handlers
- `ws/` — WsManager 单例
- `pages/` — Chat, Tools, MCP, Skills, Compare, Debug, Login, Settings
- `components/` — layout, chat, tools, mcp, skills, compare, debug, terminal, ui

**WebSocket 协议**：
- 服务端事件：text_delta/complete, tool_start/result/progress, mcp_status/log/tools, debug_intercept, permission_request
- 客户端事件：send_message, cancel, permission_response, terminal_input/resize

---

### 第七段：MVP 分阶段路线图（✅ 已确认）

**四阶段路线图，Phase 1 精简聚焦核心引擎**

#### Phase 1: Core Engine — 核心引擎（精简版）

**目标**：Rust 自有智能体 + WASM 沙箱 + 基础 Web UI 可交互

**后端（Rust）**：
- 项目脚手架（Cargo workspace + Axum web server）
- Provider Trait + Anthropic provider 实现
- Agent Loop（最大 10 轮对话 + 基础工具调用）
- Tool Trait + ToolRegistry + 2 个内置工具（bash, read）
- WASM 运行时（wasmtime，简单工具沙箱）
- WebSocket 服务（text_delta / text_complete / tool_start / tool_result）
- 内存 Session（不持久化，Phase 2 加 SQLite）
- 无认证（本地开发模式）

**前端（React）**：
- Vite + React 19 + TypeScript 项目脚手架
- shadcn/ui 基础组件（Button, Input, ScrollArea）
- AppLayout 骨架（NavRail + TabBar + Main）
- Chat 页面（MessageList + ChatInput + StreamingDisplay）
- WebSocket 连接 + Jotai session atoms
- 纯文本消息渲染（Markdown Phase 2 增强）

**交付物**：通过 Web UI 与 Rust 智能体对话，智能体在 WASM 沙箱中执行 bash/read 工具

#### Phase 2: Debug Tooling — 调试能力

**目标**：工具调试面板可用，MCP Server 可管理，Session 持久化

**后端**：
- SQLite WAL Session Store（持久化 + FTS5）
- MCP Client（stdio transport）+ MCP Server 生命周期管理
- Tool 执行记录存储
- Debug Interceptor（请求/响应记录）
- REST API（/api/sessions, /api/executions, /api/mcp-servers, /api/mcp-servers/{id}/tools/{name}/call）
- WebSocket 扩展（mcp_status/log, debug_intercept）
- 更多内置工具：write, edit, grep, glob, find
- Skill Loader（SKILL.md 解析）
- Agent Loop 增强（多轮对话，工具并发执行，最大 50 轮 8 并发）
- Context Manager（上下文压缩/摘要）
- 单用户认证（bcrypt + Cookie，简化版）

**前端**：
- Tools 页面（ExecutionList + ExecutionDetail + TimelineView + JsonViewer）
- MCP 页面（ServerList + ServerDetail + ToolCallForm + LogStream）
- Skills 页面（SkillEditor + SkillPreview）
- MarkdownRenderer（react-markdown + Shiki）
- Terminal 组件（xterm.js）
- Bottom Panel（实时日志）
- Diff 视图（Monaco Editor diff）

**交付物**：开发者可管理 MCP Server、手动调用工具、查看执行记录、编辑 Skill

#### Phase 3: Full MVP — 完整功能

**目标**：Docker 沙箱圈养 CC/OpenClaw，多用户，所有调试面板

**后端**：
- Docker RuntimeAdapter 实现
- Agent Profiles（CC/OpenClaw/Custom 配置）
- gRPC/tonic Transport
- Channel Trait 扩展（CliChannel + ApiChannel + TelegramChannel）
- 多用户注册/登录（邀请码）
- RBAC（Admin/Developer/Viewer）
- 双层权限（ReadOnly/Interactive/AutoApprove）
- Per-user workspace 隔离
- Skill 测试执行 + 兼容性验证
- Provider 扩展：OpenAI + Gemini
- Session 分支管理 + JSONL 兼容层
- 审计日志

**前端**：
- Skills 页面增强（SkillTest + 兼容性检查）
- Compare 页面（CompareSetup + CompareResult）
- Debug 页面（InterceptorStream + InterceptorFilter）
- Settings 页面 + 用户管理 UI
- 多沙箱管理 UI（NavRail 增强）
- 权限模式切换 UI
- @tanstack/react-virtual 虚拟化

**交付物**：可圈养 CC/OpenClaw 在 Docker 中，多用户可用，全部六个调试 Tab 可用

#### Phase 4: Production Ready — 生产就绪

**后端**：
- Apple Container 支持（可选）
- MCP Streamable HTTP transport
- 工具执行 Replay 功能
- 数据归档策略（热/温/冷）
- 性能优化（并发控制、资源限制）
- 安全加固（AES-256-GCM 凭据加密、mount 白名单验证、symlink 防护）
- OAuth2 认证扩展
- JSONL 导入/导出

**前端**：
- PWA 支持
- 响应式移动端适配
- 主题切换（dark/light）
- 国际化
- 性能优化（懒加载、虚拟化调优）
- 批量对比测试 UI

**交付物**：生产可用的企业级安全沙箱调试环境

---

## 关键参考文件路径

```
happyclaw CLAUDE.md:         github.com/happyclaw/CLAUDE.md (29.5KB)
happyclaw db.ts:             github.com/happyclaw/src/db.ts (60.9KB, schema v1-v13)
happyclaw container-runner:  github.com/happyclaw/src/container-runner.ts (50.3KB)
craft-agents UI:             github.com/craft-agents-oss/apps/electron/src/renderer/
zeroclaw traits:             github.com/zeroclaw/src/lib.rs
pi_agent_rust agent loop:    github.com/pi_agent_rust/src/agent.rs
pi_agent_rust provider:      github.com/pi_agent_rust/src/provider.rs
pi_agent_rust tools:         github.com/pi_agent_rust/src/tools.rs
MCP spec (2025-11-25):       https://modelcontextprotocol.io/specification/2025-11-25/
```

---

## 下一步

Brainstorming 全部完成（8/8 段已确认，含记忆模块）。下一步：
1. 将全部 8 段 brainstorming 整合为正式设计文档（`docs/design/ARCHITECTURE_DESIGN.md`）
2. 创建 Phase 1 详细实施计划（含记忆模块 Phase 1 内容）
3. 初始化 Cargo workspace + Vite 项目脚手架
4. 开始 Phase 1 编码

**记忆模块文档**：`docs/main/CHECKPOINT_MEMORY_BRAINSTORMING.md`（第八段）
