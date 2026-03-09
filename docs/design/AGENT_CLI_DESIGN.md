# 自主智能体 CLI 应用设计

## 概述

本文档基于对 8 个本地 Rust 自主智能体项目的 CLI 实现分析、行业最佳实践研究，以及 octo-cli 现状评估，给出 octo-sandbox CLI 应用的设计方案。

---

## 第一部分：本地项目 CLI 比较分析

### 1.1 项目 CLI 成熟度总览

| 项目 | CLI 成熟度 | 命令数 | 代码量 | CLI 框架 | TUI | 交互模式 |
|------|-----------|--------|--------|----------|-----|----------|
| **Goose** | ★★★★★ 完整 | 15+ | ~60KB | Clap 4 | 无 | Session + Terminal |
| **IronClaw** | ★★★★★ 完整 | 14+ | 中等 | Clap 4 | 无 | 多通道 |
| **ZeroClaw** | ★★★★★ 完整 | 26+ | 1500+ 行 | Clap 4 | Ratatui | 全屏 TUI |
| **Moltis** | ★★★★ 聚焦 | 20+ | 500+ 行 | Clap 4 | 无 | Gateway-first |
| **LocalGPT** | ★★★★★ 完整 | 24+ | 5.7K 行 | Clap 4 | Ratatui | rustyline REPL |
| **OpenFang** | ★★★★★ 完整 | 30+ | 8.8K 行 | Clap 4 | Ratatui | Daemon + TUI |
| **Pi Agent** | ★★★★★ 完整 | 10 子命令 | 237K 行 | Clap 4 + 自定义 | BubbleTea | 高级 TUI |
| **AutoAgents** | ★☆☆☆☆ 库 | 无 | 示例仅用 | Clap (示例) | 无 | 无 |
| **octo-cli** | ★★☆☆☆ 原型 | 5 命名空间 | 632 行 | Clap 4 | 无 | 无 |

### 1.2 各项目独特亮点

**Goose** — 最佳终端集成
- `goose term`：每终端持久化会话，Shell 别名 (@goose)
- Recipe 系统：YAML 配置的 Agent 行为模板，支持 deeplink
- 计划任务：Cron 调度 + 服务状态管理

**IronClaw** — 最佳企业级安全
- 多通道架构：TUI / HTTP / Webhook / WASM 同时运行
- Sandbox + 凭证注入：Secret 不进容器环境变量
- 配对系统：未知发送者 OTP 验证审批

**ZeroClaw** — 最佳全屏交互
- 原生 Ratatui TUI：非退化方案，而是主要交互模式
- 硬件外设支持：STM32、RPi GPIO、传感器作为一等公民
- 紧急停止命令：`--estop` 优雅关闭

**Moltis** — 最佳 Gateway 设计
- Gateway-first：CLI 是辅助，Web 浏览器是主 UX
- 远程节点协调：Token 配对的多节点管理
- Feature-gated 命令：Tailscale、Import 等可选功能

**LocalGPT** — 最佳多模态体验
- 桌面 GUI (eframe/egui) 通过 feature flag
- Bevy 3D 场景生成 + FunDSP 音频
- Bridge 系统：Telegram / Discord / WhatsApp 机器人扩展

**OpenFang** — 最全面集成
- 30+ 命令，40+ 通道支持
- Vault 加密凭证存储
- Workflow 编排 + 事件触发器
- 设备配对 QR 码生成
- A2A (Agent-to-Agent) 通信

**Pi Agent** — 最佳代码智能体 CLI
- BubbleTea TUI 移植（Go→Rust），Lipgloss 样式，Glamour 渲染
- Extension 策略（safe / balanced / permissive）
- Session 持久性模式（strict / balanced / throughput）
- 多模型循环切换（Ctrl+P + glob 模式）
- HTML 会话导出

### 1.3 共同模式（7/7 有 CLI 的项目一致采用）

| 模式 | 细节 |
|------|------|
| **Clap 4 derive** | 所有项目使用 `clap` 4.x + `#[derive(Parser)]` |
| **Shell 补全** | 全部使用 `clap_complete` 生成 bash/zsh/fish 补全 |
| **doctor 命令** | 健康检查 + 诊断 + 可选修复 |
| **config 子命令** | list / get / set / show / validate |
| **分层配置** | 配置文件 < CLI 参数 < 环境变量 |
| **anyhow 错误** | 链式上下文 + 用户友好信息 |
| **session 管理** | 创建 / 列表 / 恢复 / 删除 |

### 1.4 功能覆盖矩阵

| 功能 | Goose | IronClaw | ZeroClaw | Moltis | LocalGPT | OpenFang | Pi Agent |
|------|-------|----------|----------|--------|----------|----------|----------|
| 交互式 REPL | ✅ | ✅ | ✅ | Web | ✅ | ✅ | ✅ |
| 流式输出 | ✅ SSE | ✅ | ✅ | ✅ WS | ✅ tokio | ✅ HTTP | ✅ TUI |
| MCP 支持 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Skill 系统 | ✅ Recipe | ✅ WASM | ✅ Trust | ✅ GitHub | ✅ YAML | ✅ 市场 | ✅ Extension |
| 内存搜索 | — | ✅ 混合 | ✅ FTS | ✅ 混合 | ✅ SQLite | ✅ KV | — |
| 计划任务 | ✅ Cron | — | ✅ Cron | — | ✅ Cron | ✅ Cron | — |
| Sandbox | — | ✅ Docker | — | ✅ Docker | ✅ Landlock | — | — |
| 审计追踪 | — | — | — | — | — | ✅ | — |
| JSON 输出 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 多 Provider | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Hook 系统 | — | — | — | — | ✅ | — | — |
| 硬件支持 | — | — | ✅ | — | — | ✅ | — |

---

## 第二部分：行业最佳实践

### 2.1 标杆产品分析

**Tier 1：产品级领导者**

| 产品 | 技术栈 | 关键创新 |
|------|--------|----------|
| **Claude Code** | TypeScript | Session resume、Hook 系统、Auto-memory、Sub-agent (Ctrl+B)、Output modes (text/json/stream-json)、Headless `-p` |
| **Codex CLI** | Rust | OS 级 Sandbox、Cloud Sandbox、MCP Server 内建 |
| **OpenCode/Crush** | Go (BubbleTea) | 双 Agent（plan+build Tab 切换）、75+ Provider、HTTP API 远程控制 |

**Tier 2：强力竞争者**

| 产品 | 技术栈 | 关键创新 |
|------|--------|----------|
| **Goose** | Rust | 模型无关、原生 MCP、桌面 App + CLI |
| **Aider** | Python | Tree-sitter Repo Map、Git 原生（自动 commit）、多模式 |
| **OpenDev/Pi** | Node | 五角色模型路由、Adaptive Context Compaction、Extended ReAct Loop |

### 2.2 现代智能体 CLI 必备功能（按优先级）

#### P0 — 必须有（Core MVP）

| 功能 | 说明 | 标杆 |
|------|------|------|
| **交互式 REPL** | 流式显示 + 多行输入 + 历史记录 | Claude Code |
| **Headless 模式** | `-p/--print` 单次查询，`--output-format` json/stream-json | Claude Code |
| **流式响应** | Token 逐个显示，工具执行 Spinner | 全部 |
| **Session 管理** | `--continue` 恢复最近 / `--resume <id>` 恢复指定 | Claude Code |
| **Shell 补全** | `octo completions bash/zsh/fish` | 全部 |
| **分层配置** | 文件 < CLI 参数 < 环境变量 | 全部 |
| **MCP 管理** | list / add / remove / status | Claude Code |
| **Markdown 渲染** | 终端内 Markdown + 语法高亮 | Aider, OpenCode |
| **Spinner/进度** | LLM 调用时旋转器 + Token 计数 | 全部 |

#### P1 — 应该有（Competitive Parity）

| 功能 | 说明 | 标杆 |
|------|------|------|
| **Slash 命令** | `/compact` `/undo` `/cost` `/model` `/clear` `/help` | Claude Code |
| **模式切换** | Plan 模式（只读）/ Build 模式（完整权限） | OpenCode |
| **@file 引用** | 用户消息内展开文件内容 | OpenDev |
| **Hook 系统** | PreToolUse / PostToolUse / SessionStart 等生命周期事件 | Claude Code |
| **Cost/Token 追踪** | 每轮 + 累计 Token 用量显示 | Claude Code |
| **doctor 诊断** | 健康检查 + 自动修复 | 全部 |
| **--add-dir** | 额外目录上下文 | Claude Code |

#### P2 — 差异化（Differentiation）

| 功能 | 说明 | 标杆 |
|------|------|------|
| **双 Agent 模式** | Plan + Build Agent 可 Tab 切换 | OpenCode |
| **自定义命令** | Markdown 文件定义 Slash 命令 | Claude Code |
| **Auto-memory** | 跨 Session 项目学习记忆持久化 | Claude Code |
| **上下文压缩** | 自动 + `/compact` 手动触发 | OpenDev |
| **模型路由** | main/fast/thinking 角色 + 懒加载 | OpenDev, ADR-026 |
| **TUI 仪表板** | Ratatui 全屏仪表板模式 | ZeroClaw, LocalGPT |

### 2.3 架构模式：三入口一核心

所有标杆产品验证的最佳架构：

```
                    +------------------+
                    |   octo-engine    |  （共享核心逻辑）
                    +--------+---------+
                             |
              +--------------+--------------+
              |              |              |
      +-------v------+ +----v-----+ +------v-------+
      | CLI 单次模式  | | REPL/TUI | | HTTP Server  |
      | octo -p "..." | | octo run | | octo-server  |
      +--------------+ +----------+ +--------------+
```

CLI 和 REPL 是 octo-engine 上的薄壳层。所有 Agent 智能逻辑在 engine 中。

---

## 第三部分：octo-cli 现状与差距

### 3.1 当前状态

octo-cli 于 2026-03-07 创建（commit c1fa953），目前是 **MVP 原型**：

| 命名空间 | 子命令 | 实现状态 |
|----------|--------|----------|
| `agent` | list, run, info | 部分实现（run 未实现交互） |
| `session` | list, create, show, delete | 部分实现（delete 未实现） |
| `memory` | search, list, add | 全部占位符 |
| `tools` | list, invoke, info | 全部占位符 |
| `config` | show, validate | 完整 |

**代码量**: 632 行（8 个源文件）
**依赖**: clap 4.5, rusqlite, tokio, dialoguer（未使用）, tracing
**缺失**: 无交互 REPL、无流式输出、无 MCP 管理、无 Shell 补全、无 JSON 输出、无 Spinner

### 3.2 差距分析

| 维度 | 当前 | 目标（参考 7 个完整 CLI） | 差距 |
|------|------|--------------------------|------|
| 命令覆盖 | 5 命名空间 13 子命令 | 10+ 命名空间 30+ 子命令 | 大 |
| 交互体验 | 无 | REPL + 流式 + Slash 命令 | 大 |
| 输出格式 | println! 纯文本 | Markdown + 语法高亮 + JSON + Spinner | 大 |
| 配置系统 | 仅环境变量 | 分层配置 + init 向导 | 中 |
| Session | 基本 CRUD | 恢复 + 分支 + 导出 | 中 |
| MCP | 未暴露 | 完整管理（list/add/remove/status） | 大 |
| 诊断 | 无 | doctor + 健康检查 + 修复 | 中 |
| 扩展 | 无 | Shell 补全 + Hook + 自定义命令 | 大 |

---

## 第四部分：octo-cli 设计方案

### 4.1 命令结构设计

```
octo [全局选项] <命令> [子命令] [参数]

全局选项:
  -v, --verbose              详细日志输出
  -q, --quiet                静默模式（仅错误）
  --config <PATH>            自定义配置文件路径
  --db <PATH>                自定义数据库路径
  --output-format <FORMAT>   输出格式: text | json | stream-json
  --no-color                 禁用彩色输出

═══════════════════════════════════════════════════════════
交互模式
═══════════════════════════════════════════════════════════

octo run [OPTIONS]                    # 启动交互式 REPL 会话
  -c, --continue                      # 恢复最近会话
  -r, --resume [SESSION_ID]           # 恢复指定会话（无 ID 则弹出选择器）
  --session <NAME>                    # 使用命名会话
  --model <MODEL>                     # 指定模型
  --provider <PROVIDER>               # 指定 Provider
  --system-prompt <TEXT>              # 覆盖系统提示词
  --add-dir <PATH>                    # 添加额外上下文目录（可重复）
  --no-tools                          # 禁用所有工具
  --tools <LIST>                      # 指定工具白名单（逗号分隔）
  --max-turns <N>                     # 最大轮次

octo ask <MESSAGE> [OPTIONS]          # 单次提问（非交互）
  -p, --print                         # 等同于 ask（兼容 Claude Code 习惯）
  --model <MODEL>                     # 指定模型
  --output-format <FORMAT>            # text | json | stream-json

REPL 内 Slash 命令:
  /help                               # 显示帮助
  /compact                            # 压缩上下文
  /undo                               # 撤销最近工具操作
  /cost                               # 显示 Token 用量和费用
  /model <NAME>                       # 切换模型
  /mode <plan|build>                  # 切换模式
  /clear                              # 清除当前对话
  /save [NAME]                        # 保存会话
  /exit                               # 退出

═══════════════════════════════════════════════════════════
Agent 管理
═══════════════════════════════════════════════════════════

octo agent list                       # 列出所有 Agent
octo agent info <ID>                  # 显示 Agent 详情
octo agent create <MANIFEST>          # 从清单创建 Agent
octo agent start <ID>                 # 启动 Agent
octo agent pause <ID>                 # 暂停 Agent
octo agent stop <ID>                  # 停止 Agent
octo agent delete <ID>                # 删除 Agent

═══════════════════════════════════════════════════════════
Session 管理
═══════════════════════════════════════════════════════════

octo session list                     # 列出所有会话
octo session create [--name NAME]     # 创建新会话
octo session show <ID>                # 显示会话详情（含消息历史）
octo session delete <ID>              # 删除会话
octo session export <ID> [--format html|json|md]  # 导出会话

═══════════════════════════════════════════════════════════
Memory 管理
═══════════════════════════════════════════════════════════

octo memory search <QUERY>            # 语义搜索记忆
  --layer <0|1|2|all>                 # 指定搜索层级
  --limit <N>                         # 结果数量
octo memory list                      # 列出记忆条目
  --layer <0|1|2>                     # 指定层级
octo memory add <CONTENT>             # 添加记忆
  --layer <LAYER>                     # 目标层级
  --tags <TAGS>                       # 标签（逗号分隔）
octo memory graph                     # 显示知识图谱概览

═══════════════════════════════════════════════════════════
Tool 管理
═══════════════════════════════════════════════════════════

octo tool list                        # 列出所有可用工具
octo tool info <NAME>                 # 显示工具详情（参数、风险级别）
octo tool invoke <NAME> [ARGS]        # 调用工具

═══════════════════════════════════════════════════════════
MCP Server 管理
═══════════════════════════════════════════════════════════

octo mcp list                         # 列出已配置的 MCP Server
octo mcp add <NAME> <COMMAND>         # 添加 MCP Server
  --args <ARGS>                       # Server 启动参数
  --env <KEY=VALUE>                   # 环境变量（可重复）
octo mcp remove <NAME>                # 移除 MCP Server
octo mcp status [NAME]                # 查看运行状态
octo mcp logs <NAME>                  # 查看 Server 日志
  --follow                            # 实时跟踪
octo mcp test <NAME>                  # 测试 Server 连通性

═══════════════════════════════════════════════════════════
Configuration 管理
═══════════════════════════════════════════════════════════

octo config show                      # 显示当前配置
octo config validate                  # 验证配置完整性
octo config init [--wizard]           # 初始化配置（可选交互向导）
octo config get <KEY>                 # 获取配置项
octo config set <KEY> <VALUE>         # 设置配置项
octo config paths                     # 显示配置/数据/缓存路径

═══════════════════════════════════════════════════════════
诊断与工具
═══════════════════════════════════════════════════════════

octo doctor                           # 环境健康检查
  --repair                            # 自动修复发现的问题
  --format <text|json|md>             # 输出格式
octo completions <bash|zsh|fish|powershell>  # 生成 Shell 补全脚本
octo version                          # 版本信息
```

### 4.2 模块结构设计

```
crates/octo-cli/src/
├── main.rs                          # 入口：Clap 解析 + 全局选项 + 路由
├── commands/
│   ├── mod.rs                       # 命令路由 dispatch
│   ├── types.rs                     # Clap 命令/子命令枚举定义
│   ├── state.rs                     # AppState 初始化（复用 octo-engine）
│   ├── run.rs                       # octo run — REPL 启动入口
│   ├── ask.rs                       # octo ask — 单次查询（Headless）
│   ├── agent.rs                     # Agent 生命周期管理
│   ├── session.rs                   # Session CRUD + 导出
│   ├── memory.rs                    # Memory 搜索/列表/添加
│   ├── tool.rs                      # Tool 列表/详情/调用
│   ├── mcp.rs                       # MCP Server 管理（NEW）
│   ├── config.rs                    # 配置管理
│   ├── doctor.rs                    # 健康诊断（NEW）
│   └── completions.rs              # Shell 补全生成（NEW）
├── repl/
│   ├── mod.rs                       # REPL 主循环
│   ├── input.rs                     # 输入处理（多行、@file 展开）
│   ├── slash.rs                     # Slash 命令解析和执行
│   └── history.rs                   # 会话历史 + 搜索
├── ui/
│   ├── mod.rs                       # UI 抽象层
│   ├── streaming.rs                 # Token 流式渲染
│   ├── markdown.rs                  # Markdown 终端渲染（termimad）
│   ├── syntax.rs                    # 语法高亮（syntect）
│   ├── spinner.rs                   # Spinner 和进度（indicatif）
│   ├── status.rs                    # 状态栏（Token 计数、Context %）
│   ├── table.rs                     # 表格格式化输出
│   └── theme.rs                     # 颜色主题管理
└── output/
    ├── mod.rs                       # 输出格式抽象
    ├── text.rs                      # 纯文本输出
    ├── json.rs                      # JSON 输出（单次结果）
    └── stream_json.rs               # Stream-JSON 输出（行分隔）
```

### 4.3 依赖清单

```toml
[dependencies]
# 核心
octo-engine = { path = "../octo-engine" }
octo-types = { path = "../octo-types" }
octo-sandbox = { path = "../octo-sandbox" }

# CLI 解析
clap = { version = "4.5", features = ["derive", "env"] }
clap_complete = "4.5"

# 异步运行时
tokio = { version = "1.42", features = ["full"] }
futures = "0.3"

# REPL / 行编辑
reedline = "0.38"                    # Nushell 团队的现代行编辑器
                                     # 支持 vi/emacs 模式、语法高亮、补全

# 终端渲染
termimad = "0.31"                    # Markdown 终端渲染
syntect = "5.2"                      # 语法高亮（代码块）
indicatif = "0.17"                   # Spinner、进度条
owo-colors = "4"                     # 零分配彩色输出
crossterm = "0.28"                   # 终端控制（流式输出需要）

# 配置 & 数据
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
directories = "5"                    # XDG 路径规范

# 错误 & 日志
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dotenvy = "0.15"
```

### 4.4 REPL 核心循环设计

```rust
/// REPL 主循环 — 与 octo-engine AgentExecutor 集成
async fn run_repl(state: &AppState, opts: &RunOptions) -> Result<()> {
    // 1. 解析 Session：恢复或新建
    let session_id = resolve_session(state, opts).await?;

    // 2. 初始化行编辑器
    let mut editor = reedline::Reedline::create()
        .with_history(load_history(&session_id)?)
        .with_completer(SlashCommandCompleter::new())
        .with_edit_mode(detect_edit_mode());  // vi 或 emacs

    // 3. 显示欢迎信息 + Session 信息
    show_welcome(&session_id, state);

    // 4. 主循环
    loop {
        let prompt = build_prompt(state, &session_id);
        match editor.read_line(&prompt)? {
            Signal::Success(line) if line.starts_with('/') => {
                // Slash 命令处理
                handle_slash_command(&line, state, &session_id).await?;
            }
            Signal::Success(line) => {
                // 展开 @file 引用
                let expanded = expand_file_refs(&line)?;
                // 发送给 Agent，流式渲染响应
                let stream = state.agent_runtime
                    .send_message(&session_id, &expanded).await?;
                render_streaming(stream, &state.output_config).await?;
                // 显示 Token 用量
                show_turn_usage(&session_id, state).await?;
            }
            Signal::CtrlC => {
                // 中断当前操作（如果有）
                state.agent_runtime.cancel(&session_id).await?;
            }
            Signal::CtrlD => {
                // 退出，显示恢复提示
                show_resume_hint(&session_id);
                break;
            }
        }
    }
    Ok(())
}
```

### 4.5 流式渲染设计

```rust
/// 根据 OutputMode 渲染流式响应
async fn render_streaming(
    mut stream: impl Stream<Item = AgentEvent> + Unpin,
    config: &OutputConfig,
) -> Result<()> {
    match config.format {
        OutputFormat::Text => {
            let mut stdout = std::io::stdout();
            let mut in_code_block = false;

            while let Some(event) = stream.next().await {
                match event {
                    AgentEvent::TextDelta(text) => {
                        // 直接写入终端，无缓冲
                        write!(stdout, "{}", text)?;
                        stdout.flush()?;
                    }
                    AgentEvent::ThinkingDelta(text) => {
                        // 灰色显示思考过程
                        write!(stdout, "{}", text.dimmed())?;
                        stdout.flush()?;
                    }
                    AgentEvent::ToolStart { name, input } => {
                        // 显示工具调用 Spinner
                        show_tool_spinner(&name);
                    }
                    AgentEvent::ToolResult { name, output, duration } => {
                        // 停止 Spinner，显示结果摘要
                        stop_spinner();
                        show_tool_result(&name, &output, duration);
                    }
                    AgentEvent::TokenUsage { input, output } => {
                        // 更新状态栏
                        update_status_bar(input, output);
                    }
                    AgentEvent::Done(reason) => break,
                }
            }
            // 最终 Markdown 渲染（对完整响应做语法高亮）
            println!();
        }
        OutputFormat::Json => {
            let result = collect_stream(stream).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::StreamJson => {
            while let Some(event) = stream.next().await {
                println!("{}", serde_json::to_string(&event)?);
            }
        }
    }
    Ok(())
}
```

### 4.6 实现优先级路线图

#### Phase 1：核心 MVP（2 周）

| 任务 | 涉及模块 | 说明 |
|------|----------|------|
| R1: REPL 交互循环 | `repl/`, `commands/run.rs` | reedline 集成，基本聊天循环 |
| R2: 流式响应渲染 | `ui/streaming.rs` | Token 逐字显示 + Spinner |
| R3: Headless 模式 | `commands/ask.rs`, `output/` | `-p/--print` + `--output-format` |
| R4: MCP 管理 | `commands/mcp.rs` | 复用 McpManager/McpStorage |
| R5: Shell 补全 | `commands/completions.rs` | `clap_complete` 生成 |
| R6: Markdown 渲染 | `ui/markdown.rs` | termimad 集成 |

#### Phase 2：竞争力对齐（2 周）

| 任务 | 涉及模块 | 说明 |
|------|----------|------|
| R7: Slash 命令 | `repl/slash.rs` | /compact /undo /cost /model /help |
| R8: Session 恢复 | `commands/session.rs` | --continue / --resume + 退出提示 |
| R9: Memory 搜索 | `commands/memory.rs` | 复用 WorkingMemory/FtsStore |
| R10: Tool 管理 | `commands/tool.rs` | 复用 ToolRegistry |
| R11: doctor 诊断 | `commands/doctor.rs` | 环境检查 + --repair |
| R12: Cost 追踪 | `ui/status.rs` | 每轮 + 累计 Token + 费用 |

#### Phase 3：差异化功能（2 周）

| 任务 | 涉及模块 | 说明 |
|------|----------|------|
| R13: 模式切换 | `repl/slash.rs` | Plan（只读）/ Build（完整权限） |
| R14: @file 引用 | `repl/input.rs` | 用户消息内文件展开 |
| R15: Config init 向导 | `commands/config.rs` | 首次运行交互式设置 |
| R16: Session 导出 | `commands/session.rs` | HTML / JSON / Markdown 格式 |
| R17: Hook 系统 | `repl/` 集成 | PreToolUse / PostToolUse 生命周期 |
| R18: 上下文压缩 | `/compact` 实现 | 复用 ContextPruner |

### 4.7 octo-cli 的差异化优势

基于 octo-engine 已有能力，octo-cli 可以在以下方面超越竞品：

1. **多层记忆系统** — L0/L1/L2 + KnowledgeGraph + FTS，比任何开源竞品都更成熟。通过 `octo memory search/graph` 暴露此能力。

2. **Agent 状态机** — Created → Running → Paused → Stopped 的正式生命周期，通过 `octo agent start/pause/stop/resume` 提供细粒度控制。

3. **MCP 原生** — McpManager + McpClient + McpToolBridge 已就绪，做成一等 CLI 体验（类似 `claude mcp add`）。

4. **Rust 性能** — 仅 Codex CLI 和 Goose 是 Rust 实现。亚秒启动、最小内存占用、原生 async。

5. **Engine 共享** — CLI 和 Server 共享同一个 octo-engine，架构与 Claude Code/OpenDev 验证的最佳实践一致。

6. **Zone A/B 上下文工程** — 已有的系统提示构建器和上下文预算管理，可直接通过 Status Bar 展示。

7. **Hook 系统** — 已有 10 个生命周期扩展点，可直接暴露给 CLI 用户。

---

## 第五部分：基于 OpenFang 的 TUI 前端实现方案

> 第四部分设计的模块结构和 REPL 循环保持不变，本节补充 **TUI 前端**的详细实现方案。
> 决策依据：OpenFang CLI/TUI 是分析的 8 个项目中最专业完整的实现（29,310 行 / 38 文件 / 21 屏幕），采用 Ratatui 0.29，与 octo 技术栈完全兼容。

### 5.1 方案概述

**策略**：Fork OpenFang TUI 架构，按 octo 实际需求裁剪和适配。

| 维度 | OpenFang | octo 适配 |
|------|----------|-----------|
| TUI 框架 | Ratatui 0.29 + crossterm | **复用** |
| 屏幕数 | 21 个 Tab | **精简为 12 个** |
| 后端 | Daemon HTTP + InProcess Kernel | **改为 octo-engine InProcess + octo-server HTTP** |
| 代码量 | 29,310 行 | **预计 15,000-20,000 行** |
| 品牌 | OpenFang 橙色 (#FF5C00) | **替换为 octo 主题色** |

### 5.2 OpenFang TUI 源码结构（供参考）

```
openfang-cli/src/
├── main.rs           (6,602 行) — CLI 入口 + 子命令
├── launcher.rs       (604 行)   — 交互式启动菜单
├── tui/
│   ├── mod.rs        (2,436 行) — 主 App 状态机 + Tab 导航 + Event Loop
│   ├── event.rs      (2,784 行) — 事件系统 (crossterm + streaming + tick)
│   ├── chat_runner.rs(815 行)   — 独立 Chat TUI
│   ├── theme.rs      (139 行)   — 颜色 + 样式
│   └── screens/      (21 个 Tab，共 ~9,600 行)
├── progress.rs       (322 行)   — Spinner + 进度条
├── table.rs          (248 行)   — ASCII 表格
└── ui.rs             (122 行)   — 彩色输出辅助
```

### 5.3 octo-cli TUI 目标结构

```
crates/octo-cli/src/
├── main.rs                          # CLI 入口（Clap 4，保持第四部分设计）
├── commands/                        # 非 TUI 子命令（保持第四部分设计）
│   ├── mod.rs, types.rs, state.rs
│   ├── ask.rs                       # Headless 模式
│   ├── agent.rs, session.rs, memory.rs, tool.rs
│   ├── mcp.rs, config.rs, doctor.rs, completions.rs
│   └── skills.rs                    # 新增：Skills 管理
├── repl/                            # REPL 模式（保持第四部分设计）
│   ├── mod.rs, input.rs, slash.rs, history.rs
├── tui/                             # ★ 新增：全屏 TUI（基于 OpenFang）
│   ├── mod.rs                       # App 状态机 + Tab 导航 + Event Loop
│   ├── event.rs                     # 统一事件系统
│   ├── theme.rs                     # octo 品牌主题
│   ├── backend.rs                   # ★ 后端抽象层（InProcess / HTTP）
│   ├── stream_bridge.rs             # AgentEvent → TUI Event 映射
│   └── screens/
│       ├── mod.rs                   # 屏幕路由
│       ├── welcome.rs               # 欢迎页 + 首次配置向导
│       ├── dashboard.rs             # 系统概览（Agent/Session/MCP 统计）
│       ├── agents.rs                # Agent 管理（列表/创建/详情）
│       ├── chat.rs                  # ★ Chat 交互（流式 + 工具可视化）
│       ├── sessions.rs              # Session 列表 + 恢复
│       ├── memory.rs                # 多层内存浏览器（L0/L1/L2/KG）
│       ├── skills.rs                # Skills 管理（安装/列表/详情）
│       ├── mcp.rs                   # MCP Server 管理
│       ├── tools.rs                 # 工具列表 + 执行历史
│       ├── security.rs              # 安全策略 + 审计日志
│       ├── settings.rs              # 配置管理
│       └── logs.rs                  # 结构化日志查看
├── ui/                              # 非 TUI 渲染（REPL 模式用）
│   ├── streaming.rs, markdown.rs, syntax.rs
│   ├── spinner.rs, status.rs, table.rs, theme.rs
└── output/
    ├── text.rs, json.rs, stream_json.rs
```

### 5.4 后端抽象层设计

OpenFang 的双后端模式（Daemon + InProcess）与 octo 的 CLI/Server 架构天然对齐：

```rust
/// 后端抽象 — TUI 通过此 trait 与 octo-engine 交互
#[async_trait]
pub trait TuiBackend: Send + Sync {
    // Agent 操作
    async fn list_agents(&self) -> Result<Vec<AgentManifest>>;
    async fn send_message(&self, session_id: &str, msg: &str)
        -> Result<BoxStream<AgentEvent>>;
    async fn cancel(&self, session_id: &str) -> Result<()>;

    // Session 操作
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;
    async fn create_session(&self, agent_id: &str) -> Result<String>;

    // Memory 操作
    async fn search_memory(&self, query: &str) -> Result<Vec<MemoryEntry>>;

    // MCP 操作
    async fn list_mcp_servers(&self) -> Result<Vec<McpServerInfo>>;
    async fn start_mcp_server(&self, name: &str) -> Result<()>;
    async fn stop_mcp_server(&self, name: &str) -> Result<()>;

    // Skills 操作
    async fn list_skills(&self) -> Result<Vec<SkillMetadata>>;
    async fn load_skill(&self, name: &str) -> Result<SkillDefinition>;

    // Tool 操作
    async fn list_tools(&self) -> Result<Vec<ToolSpec>>;

    // Config / Metrics
    async fn get_config(&self) -> Result<serde_json::Value>;
    async fn get_metrics(&self) -> Result<MetricsSnapshot>;
}

/// InProcess 后端 — CLI 直接持有 octo-engine
pub struct InProcessBackend {
    runtime: Arc<AgentRuntime>,
}

/// HTTP 后端 — 连接远程 octo-server
pub struct HttpBackend {
    base_url: String,
    client: reqwest::Client,
}
```

### 5.5 事件系统适配

```rust
/// TUI 统一事件（基于 OpenFang AppEvent 简化）
pub enum AppEvent {
    // 终端事件
    Key(KeyEvent),
    Tick,                                    // ~100ms 刷新

    // Agent 流事件（从 octo-engine AgentEvent 映射）
    StreamTextDelta(String),
    StreamThinkingDelta(String),
    StreamToolStart { name: String, input: serde_json::Value },
    StreamToolResult { name: String, output: String, duration_ms: u64 },
    StreamDone(StopReason),
    StreamError(String),

    // 数据加载事件（异步后台加载）
    AgentsLoaded(Vec<AgentManifest>),
    SessionsLoaded(Vec<SessionInfo>),
    McpServersLoaded(Vec<McpServerInfo>),
    SkillsLoaded(Vec<SkillMetadata>),
    MemoryResults(Vec<MemoryEntry>),
    MetricsLoaded(MetricsSnapshot),
}
```

### 5.6 屏幕对应关系

| # | octo 屏幕 | 来源 | 复用度 | 改动说明 |
|---|-----------|------|--------|----------|
| 1 | Welcome | OF welcome.rs + init_wizard.rs | 70% | 替换品牌，简化为 3 步向导（API Key + Provider + 首次 Chat） |
| 2 | Dashboard | OF dashboard.rs | 80% | 数据源改为 octo MetricsRegistry + SessionStore |
| 3 | Agents | OF agents.rs | 60% | 适配 AgentCatalog + AgentStore 类型 |
| 4 | **Chat** | OF chat.rs + chat_runner.rs | 75% | **核心**：替换 StreamEvent → AgentEvent，保留流式渲染 |
| 5 | Sessions | OF sessions.rs | 85% | 适配 SessionStore 类型 |
| 6 | Memory | OF memory.rs | 50% | 重构为多层内存浏览器（L0/L1/L2 + KG） |
| 7 | Skills | OF skills.rs | 60% | 适配 SkillLoader + SkillRegistry |
| 8 | **MCP** | **新建** | 0% | MCP Server 管理（启停/工具列表/日志） |
| 9 | **Tools** | **新建** | 0% | 工具列表 + 执行历史 + 手动调用 |
| 10 | Security | OF security.rs + audit.rs | 70% | 合并安全策略 + 审计日志 |
| 11 | Settings | OF settings.rs | 75% | 适配 octo config 系统 |
| 12 | Logs | OF logs.rs | 90% | 几乎直接复用 |

**删除的 OpenFang 屏幕**（9 个，octo 无对应功能）：
Workflows, Triggers, Channels, Hands, Extensions, Templates, Peers, Comms, Usage

### 5.7 三种交互模式统一

```
octo ask "question"          → Headless 模式（单次，JSON/文本输出）
octo run                     → REPL 模式（reedline，轻量级）
octo tui                     → 全屏 TUI 模式（Ratatui，专业交互）
octo serve                   → HTTP Server 模式（Web 前端）
```

四种模式共享同一个 `octo-engine` 和 `TuiBackend` trait：
- `ask` / `run` 使用 `InProcessBackend` 直接调用
- `tui` 使用 `InProcessBackend`（本地）或 `HttpBackend`（连接远程 server）
- `serve` 不使用 TUI 后端（是 Axum HTTP 服务）

### 5.8 依赖更新

在第四部分的依赖基础上，新增 TUI 相关依赖：

```toml
# TUI 框架（新增）
ratatui = "0.29"                     # 终端 UI（OpenFang 同版本）

# 以下保持第四部分设计不变
# reedline = "0.38"                  # REPL 行编辑
# termimad = "0.31"                  # Markdown 渲染
# syntect = "5.2"                    # 语法高亮
# indicatif = "0.17"                 # Spinner
# owo-colors = "4"                   # 彩色输出
# crossterm = "0.28"                 # 终端控制（已被 ratatui 依赖）
```

注意：`ratatui` 已包含 `crossterm`，无需重复声明。

### 5.9 实现路线图更新

原第四部分路线图（R1-R18）保持不变，新增 TUI 专项任务：

#### Phase 1-TUI：核心 TUI MVP（2 周，与 R1-R6 并行或后续）

| 任务 | 说明 | 预计行数 |
|------|------|---------|
| T1: App Shell + Event Loop | 从 OpenFang fork mod.rs + event.rs，替换后端 | ~3,000 行 |
| T2: Backend trait | 实现 InProcessBackend + HttpBackend | ~500 行 |
| T3: Stream Bridge | AgentEvent → AppEvent 映射 | ~200 行 |
| T4: Chat 屏幕 | 从 OpenFang fork chat.rs + chat_runner.rs | ~1,500 行 |
| T5: Theme | 替换品牌色，保持设计系统 | ~150 行 |
| T6: Welcome + Dashboard | 基础引导 + 系统概览 | ~800 行 |

#### Phase 2-TUI：功能屏幕（2 周）

| 任务 | 说明 | 预计行数 |
|------|------|---------|
| T7: Agents 屏幕 | Agent 列表/创建/详情 | ~1,200 行 |
| T8: Sessions 屏幕 | Session 列表/恢复/删除 | ~400 行 |
| T9: Memory 屏幕 | 多层内存浏览器 | ~800 行 |
| T10: Skills 屏幕 | Skill 列表/安装/详情 | ~600 行 |
| T11: MCP 屏幕 | MCP Server 管理（新建） | ~800 行 |
| T12: Tools 屏幕 | 工具列表/执行历史（新建） | ~600 行 |

#### Phase 3-TUI：完善（1 周）

| 任务 | 说明 | 预计行数 |
|------|------|---------|
| T13: Security + Audit | 安全策略 + 审计日志 | ~600 行 |
| T14: Settings | 配置管理 | ~500 行 |
| T15: Logs | 结构化日志查看 | ~400 行 |
| T16: 键盘快捷键 + 帮助 | 全局快捷键、每屏提示栏 | ~300 行 |

**总计**：~12,350 行新代码 + ~5,000 行 fork 适配 = ~17,000 行 TUI 代码

### 5.10 OpenFang 核心可复用组件清单

| 组件 | 文件 | 行数 | 复用方式 |
|------|------|------|----------|
| Event Loop | tui/mod.rs | 2,436 | Fork + 替换 Backend enum |
| Event System | tui/event.rs | 2,784 | Fork + 替换 StreamEvent |
| Theme System | tui/theme.rs | 139 | Fork + 替换品牌色 |
| Chat Runner | tui/chat_runner.rs | 815 | Fork + 适配 AgentEvent |
| Chat Screen | screens/chat.rs | 887 | Fork + 适配数据类型 |
| Dashboard | screens/dashboard.rs | 278 | Fork + 适配数据源 |
| Sessions | screens/sessions.rs | 313 | Fork + 适配 SessionStore |
| Init Wizard | screens/init_wizard.rs | 2,314 | Fork + 简化为 3 步 |
| Progress | progress.rs | 322 | **直接复用** |
| Table | table.rs | 248 | **直接复用** |
| Launcher | launcher.rs | 604 | Fork + 品牌替换 |

---

## 第六部分：基于 OpenFang 的嵌入式 Web Dashboard 迁移方案

### 6.1 战略决策：替代 `web/` React 前端

octo 现有的 `web/` 目录（React 19 + Vite + Jotai，3,572 行）尚未调试成功。与其继续调试，不如直接迁移 OpenFang 的嵌入式 Web Dashboard，获得以下核心优势：

| 维度 | 现有 `web/` (React) | 迁移后 (Alpine.js) |
|------|---------------------|---------------------|
| **构建依赖** | Node.js + npm + Vite | **零构建**，`cargo build` 即可 |
| **部署方式** | 需要前端构建产物 + 静态服务 | **单二进制**，`include_str!()` 编译时嵌入 |
| **开发门槛** | React + TypeScript + Jotai 状态管理 | Alpine.js 模板语法，接近原生 HTML |
| **运行时依赖** | 浏览器加载 ~2MB JS bundle | Alpine.js 46KB + marked 40KB + highlight 127KB |
| **热更新** | 需要 Vite HMR | 修改 static/ 文件后 `cargo build` |
| **端口** | 5180 (Vite) + 3001 (API) 双端口 | **3001 单端口**，`/` 即 Dashboard |

**决策**：废弃 `web/` 目录，在 `octo-server` 中嵌入 Alpine.js Dashboard 作为唯一 Web 前端。

### 6.2 OpenFang WebChat Dashboard 源码分析

#### 6.2.1 技术架构

OpenFang 的 Web Dashboard 是一个 **编译时嵌入的 Alpine.js SPA**：

```
crates/openfang-api/
  src/
    webchat.rs          — include_str!() 组装 HTML/CSS/JS，Axum handler
    server.rs           — 路由注册：GET / → webchat_page
    routes.rs           — 10,395 行 REST API 端点
    ws.rs               — 1,283 行 WebSocket 实时通信
  static/
    index_head.html     — HTML head（12 行）
    index_body.html     — Alpine.js SPA 主体（4,777 行）
    css/
      theme.css         — CSS 变量暗/亮主题（276 行）
      layout.css        — 响应式布局（314 行）
      components.css    — 组件样式（3,240 行）
    js/
      api.js            — API 客户端封装（321 行）
      app.js            — Alpine.js 应用初始化 + 路由（319 行）
      pages/            — 17 个页面模块（共 6,685 行）
    vendor/
      alpine.min.js     — Alpine.js 3.x（46KB）
      marked.min.js     — Markdown 渲染（40KB）
      highlight.min.js  — 代码高亮（127KB）
```

总计：**15,944 行**前端代码 + **12,611 行**后端 API。

#### 6.2.2 页面清单与代码量

| 页面 | 文件 | 行数 | 功能 |
|------|------|------|------|
| Chat | chat.js | 1,214 | WebSocket 实时对话、Markdown 渲染、代码高亮 |
| Agents | agents.js | 719 | Agent 列表、创建、配置、模型切换 |
| Settings | settings.js | 721 | 全局配置、Provider 密钥管理 |
| Workflow Builder | workflow-builder.js | 596 | 可视化工作流编辑器 |
| Wizard | wizard.js | 546 | 首次配置向导 |
| Hands (Tools) | hands.js | 504 | MCP 工具管理、依赖安装 |
| Scheduler | scheduler.js | 393 | Cron 任务调度 |
| Skills | skills.js | 333 | Skill 安装/卸载、市场搜索 |
| Channels | channels.js | 309 | Telegram/WhatsApp/Discord 通道 |
| Overview | overview.js | 292 | 系统概览仪表盘 |
| Logs | logs.js | 255 | SSE 实时日志流 |
| Usage | usage.js | 251 | Token 用量、模型分布、每日统计 |
| Comms | comms.js | 201 | Agent 间通信拓扑 |
| Sessions | sessions.js | 147 | 会话列表管理 |
| Workflows | workflows.js | 79 | 工作流列表 |
| Approvals | approvals.js | 66 | 人工审批队列 |
| Runtime | runtime.js | 59 | 运行时状态 |

#### 6.2.3 关键设计模式

**编译时嵌入**（`webchat.rs`）：
```rust
const WEBCHAT_HTML: &str = concat!(
    include_str!("../static/index_head.html"),
    "<style>\n",
    include_str!("../static/css/theme.css"),
    // ... 其余 CSS/JS 文件
    include_str!("../static/vendor/alpine.min.js"),  // Alpine 必须最后加载
    "\n</script>\n</body></html>"
);
```

**Alpine.js SPA 路由**（hash-based）：
```javascript
// app.js — 基于 location.hash 的客户端路由
Alpine.data('app', () => ({
    currentPage: 'overview',
    init() {
        this.currentPage = location.hash.slice(1) || 'overview';
        window.addEventListener('hashchange', () => {
            this.currentPage = location.hash.slice(1);
        });
    }
}));
```

**WebSocket 实时通信**：
```
Client → Server: {"type":"message","content":"..."}
Server → Client: {"type":"typing","state":"start|tool|stop"}
Server → Client: {"type":"text_delta","content":"..."}
Server → Client: {"type":"response","content":"...","input_tokens":N,"output_tokens":N}
Server → Client: {"type":"error","content":"..."}
```

### 6.3 octo Dashboard 目标架构

#### 6.3.1 目录结构

```
crates/octo-server/
  static/                          — 嵌入式 Dashboard 静态文件
    index_head.html                — HTML head
    index_body.html                — Alpine.js SPA 主体
    css/
      theme.css                    — 暗/亮主题 CSS 变量
      layout.css                   — 响应式布局
      components.css               — 组件样式
    js/
      api.js                       — octo REST API 客户端
      app.js                       — Alpine.js 应用初始化 + 路由
      pages/
        overview.js                — 系统概览仪表盘
        chat.js                    — 核心对话界面
        agents.js                  — Agent 目录管理
        sessions.js                — 会话管理
        memory.js                  — [新建] 多层记忆浏览器
        mcp.js                     — [新建] MCP 服务器管理
        tools.js                   — 工具注册与执行历史
        skills.js                  — Skill 安装/加载
        config.js                  — 配置管理
        metrics.js                 — Metrics + Token 用量
        audit.js                   — 审计日志
        logs.js                    — 实时日志流
        security.js                — 安全策略仪表盘
        scheduler.js               — 计划任务
    fonts/
      JetBrainsMono-Regular.woff2  — 代码字体（~100KB）
      JetBrainsMono-Bold.woff2     — 代码字体粗体（~100KB）
      Inter-Regular.woff2          — UI 字体（~90KB）
      Inter-SemiBold.woff2         — UI 字体半粗（~90KB）
    vendor/
      alpine.min.js                — Alpine.js 3.x
      marked.min.js                — Markdown 渲染
      highlight.min.js             — 代码高亮
  src/
    webchat.rs                     — include_str!()/include_bytes!() 组装 + Axum handler
    router.rs                      — 注册 GET / → webchat_page, GET /fonts/* → 字体
```

#### 6.3.2 页面映射（OpenFang → octo）

| OpenFang 页面 | octo 页面 | 动作 | 适配要点 |
|--------------|-----------|------|----------|
| Overview | Overview | Fork | 适配 octo metrics API |
| Chat | Chat | Fork | 适配 AgentExecutor WS 协议 |
| Agents | Agents | Fork | 适配 AgentCatalog/AgentStore |
| Sessions | Sessions | Fork | 适配 SessionStore |
| Skills | Skills | Fork | 适配 SkillLoader `.octo/skills/` |
| Hands | Tools | Fork + 重命名 | 适配 ToolRegistry |
| Settings | Config | Fork | 适配 octo config.yaml 结构 |
| Usage | Metrics | Fork | 适配 MetricsRegistry + Metering |
| Logs | Logs | Fork | 适配 EventBus SSE |
| Scheduler | Scheduler | Fork | 适配 octo Scheduler |
| Wizard | Wizard | Fork | 简化为 Provider + Model 配置 |
| — | **Memory** | **新建** | L0/L1/L2 记忆浏览、KG 可视化、FTS 搜索 |
| — | **MCP** | **新建** | MCP 服务器列表/启停/日志、工具发现/调用 |
| — | **Audit** | **新建** | AuditStorage 事件查看、完整性验证 |
| — | **Security** | **新建** | SecurityPolicy、CommandRiskLevel、AutonomyLevel |
| Workflows | 删除 | — | octo 无工作流引擎 |
| Workflow Builder | 删除 | — | octo 无工作流引擎 |
| Channels | 删除 | — | octo 无通道系统 |
| Comms | 删除 | — | octo 无 Agent 间通信 |
| Approvals | 删除 | — | octo 暂无审批系统 |
| Runtime | 删除 | — | 功能合并到 Overview |

最终：**14 个页面**（Fork 11 + 新建 4 - 删除 5 - 合并 1）

### 6.4 后端 API 适配

#### 6.4.1 webchat.rs 实现

在 `octo-server` 中新增 `webchat.rs`，采用与 OpenFang 相同的 `include_str!()` 模式：

```rust
//! 嵌入式 Web Dashboard，编译时嵌入 HTML/CSS/JS。
//! 单二进制部署，GET / 即可访问完整 Dashboard。

use axum::http::header;
use axum::response::IntoResponse;

const ETAG: &str = concat!("\"octo-", env!("CARGO_PKG_VERSION"), "\"");

pub async fn dashboard_page() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::ETAG, ETAG),
            (header::CACHE_CONTROL, "public, max-age=3600, must-revalidate"),
        ],
        DASHBOARD_HTML,
    )
}

const DASHBOARD_HTML: &str = concat!(
    include_str!("../static/index_head.html"),
    "<style>\n",
    include_str!("../static/css/theme.css"),
    "\n",
    include_str!("../static/css/layout.css"),
    "\n",
    include_str!("../static/css/components.css"),
    "\n</style>\n",
    include_str!("../static/index_body.html"),
    "<script>\n",
    include_str!("../static/vendor/marked.min.js"),
    "\n</script>\n<script>\n",
    include_str!("../static/vendor/highlight.min.js"),
    "\n</script>\n<script>\n",
    include_str!("../static/js/api.js"),
    "\n",
    include_str!("../static/js/app.js"),
    // ... 各页面 JS
    "\n</script>\n<script>\n",
    include_str!("../static/vendor/alpine.min.js"),
    "\n</script>\n</body></html>"
);
```

#### 6.4.2 API 端点映射

Dashboard JS 调用的 API 需要映射到 octo-server 现有端点：

| Dashboard 调用 | octo-server 端点 | 状态 |
|---------------|-----------------|------|
| `GET /api/health` | `/api/health` | 已有 |
| `GET /api/agents` | `/api/agents` | 已有 |
| `POST /api/agents` | `/api/agents` | 已有 |
| `POST /api/agents/{id}/message` | `/api/sessions/{id}/message` | 需适配 |
| `GET /api/agents/{id}/ws` | `/ws` | 需适配（WS 协议差异） |
| `GET /api/sessions` | `/api/sessions` | 已有 |
| `GET /api/skills` | `/api/skills` | 需新增 |
| `GET /api/tools` | `/api/tools` | 已有 |
| `GET /api/config` | `/api/config` | 已有 |
| `GET /api/metrics` | `/api/metrics` | 已有 |
| `GET /api/audit/recent` | `/api/audit` | 已有 |
| `GET /api/logs/stream` | — | 需新增（SSE） |
| `GET /api/memory/agents/{id}/kv` | `/api/memories` | 需适配 |
| `GET /api/mcp/servers` | `/api/mcp/servers` | 已有 |
| `GET /api/usage` | — | 需新增 |
| `GET /api/budget` | `/api/budget` | 已有 |
| `GET /api/schedules` | `/api/scheduler/tasks` | 需适配路径 |

#### 6.4.3 WebSocket 协议适配

OpenFang WS 协议与 octo 现有 WS 的差异需要桥接：

```
OpenFang WS 协议:                    octo WS 协议:
{"type":"message","content":"..."}  → {"type":"user_message","content":"..."}
{"type":"text_delta","content":"…"} ← AgentEvent::StreamDelta
{"type":"typing","state":"start"}   ← AgentEvent::ToolCallStart
{"type":"response","content":"…"}   ← AgentEvent::TurnComplete
{"type":"error","content":"…"}      ← AgentEvent::Error
```

在 `ws.rs` 中实现协议转换层，将 octo AgentEvent 映射到 Dashboard 期望的 JSON 格式。

### 6.5 实现路线图

| 阶段 | 任务 | 预计代码量 | 优先级 |
|------|------|-----------|--------|
| **W1: 骨架** | webchat.rs + 路由注册 + vendor 库 + theme/layout CSS | 500 行 | P0 |
| **W1: Overview** | Fork overview.js → 适配 octo health/metrics | 300 行 | P0 |
| **W1: Chat** | Fork chat.js → 适配 WS 协议 + AgentEvent 映射 | 1,200 行 | P0 |
| **W2: Agents** | Fork agents.js → 适配 AgentCatalog API | 700 行 | P0 |
| **W2: Sessions** | Fork sessions.js → 适配 SessionStore API | 200 行 | P1 |
| **W2: Config** | Fork settings.js → 适配 octo config 结构 | 500 行 | P1 |
| **W3: Tools** | Fork hands.js → 重命名 + 适配 ToolRegistry | 500 行 | P1 |
| **W3: Skills** | Fork skills.js → 适配 SkillLoader API | 350 行 | P1 |
| **W3: Scheduler** | Fork scheduler.js → 适配 octo Scheduler | 400 行 | P2 |
| **W3: Logs** | Fork logs.js → 适配 EventBus SSE 流 | 250 行 | P1 |
| **W4: Memory** | 新建 — L0/L1/L2 记忆浏览 + FTS 搜索 | 600 行 | P1 |
| **W4: MCP** | 新建 — 服务器管理 + 工具发现/调用 | 500 行 | P1 |
| **W4: Metrics** | Fork usage.js → 适配 MetricsRegistry | 300 行 | P2 |
| **W5: Audit** | 新建 — 审计事件查看 + 完整性验证 | 400 行 | P2 |
| **W5: Security** | 新建 — 安全策略仪表盘 | 350 行 | P2 |
| **W5: Wizard** | Fork wizard.js → 简化为 Provider+Model 配置 | 300 行 | P2 |

| **W2: 双模式布局** | CSS Grid 切换 + Alpine.js 状态 + Agent Tree | 320 行 | P0 |
| **W3: Context Budget** | Inspector 面板 — token 预算实时可视化 | 80 行 | P1 |
| **W3: Tool Trace** | Inspector 面板 — 工具调用链实时追踪 | 60 行 | P1 |
| **W4: Memory Live** | Inspector 面板 — 记忆写入实时流 | 60 行 | P1 |

总计：**~7,370 行**前端代码（含双模式布局 + Inspector 面板增量 520 行）

### 6.6 与 Part 5 TUI 的关系

三种前端交互模式并存，共享同一个 octo-engine 核心：

```
                     ┌─────────────────────┐
                     │    octo-engine      │
                     │  (AgentRuntime)     │
                     └─────┬───┬───┬───────┘
                           │   │   │
              ┌────────────┤   │   ├────────────┐
              │            │   │   │            │
     ┌────────▼──────┐ ┌──▼───▼──┐ ┌──────────▼────────┐
     │  octo tui     │ │ octo   │ │ octo-server       │
     │  (Ratatui)    │ │ ask/run│ │ (Axum + Dashboard) │
     │  Part 5       │ │ (REPL) │ │ Part 6             │
     │  终端全屏TUI  │ │ Part 4 │ │ 浏览器 Web UI      │
     └───────────────┘ └────────┘ └────────────────────┘
     InProcessBackend  InProcess   HTTP (localhost:3001)
```

- **`octo ask "..."`** — 无头模式，脚本/CI 使用
- **`octo run`** — REPL 交互模式，开发者日常使用
- **`octo tui`** — 终端全屏 TUI，高级用户使用（Part 5）
- **`octo serve`** — 启动 HTTP 服务器 + 嵌入式 Dashboard，浏览器访问（Part 6）

### 6.7 `web/` 目录处置

迁移完成后：

1. `web/` 目录标记为 **deprecated**，保留但不再维护
2. 所有前端开发转移到 `crates/octo-server/static/`
3. `Makefile` 中移除 `web-*` 相关命令
4. `CLAUDE.md` 中更新前端技术栈说明
5. `vite.config.ts` 代理配置不再需要（单端口）

### 6.8 备选方案：Tauri Desktop 桌面应用

当 Part 6 嵌入式 Web Dashboard 完成后，可以低成本地封装为原生桌面应用。此方案作为**备选**，不在主线路线图中，需要时可随时执行。

#### 6.8.1 OpenFang Desktop 架构参考

OpenFang 的 `openfang-desktop` crate 已验证此模式可行：

```
openfang-desktop/
  src/
    main.rs       — 入口（7 行）
    lib.rs        — Tauri 应用初始化（212 行）
    server.rs     — 内嵌 Kernel + API Server 启动（随机端口）
    commands.rs   — Tauri IPC 命令（状态查询、文件导入等）
    tray.rs       — 系统托盘菜单
    shortcuts.rs  — 全局快捷键
    updater.rs    — 自动更新检查
  tauri.conf.json — Tauri 配置（窗口、CSP、打包、更新）
```

工作原理：启动时内嵌启动 octo-engine + octo-server HTTP 服务 → Tauri WebView 窗口指向 `http://127.0.0.1:{port}` → 渲染的就是嵌入式 Dashboard（与浏览器访问完全一致）。

#### 6.8.2 octo-desktop 设计

```
crates/octo-desktop/
  Cargo.toml
  tauri.conf.json
  icons/                           — 应用图标（ico/png/icns）
  src/
    main.rs                        — 入口
    lib.rs                         — Tauri 应用初始化
    server.rs                      — 内嵌 AgentRuntime + octo-server 启动
    commands.rs                    — Tauri IPC 命令
    tray.rs                        — 系统托盘
```

#### 6.8.3 核心实现

**Cargo.toml 依赖**：

```toml
[package]
name = "octo-desktop"
version.workspace = true
edition.workspace = true

[dependencies]
octo-engine = { path = "../octo-engine" }
octo-server = { path = "../octo-server", features = ["embed"] }  # 复用 server 逻辑
octo-types  = { path = "../octo-types" }
tokio   = { workspace = true }
axum    = { workspace = true }
tauri   = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-notification    = "2"
tauri-plugin-shell           = "2"
tauri-plugin-single-instance = "2"
tauri-plugin-dialog          = "2"
tauri-plugin-autostart       = "2"
serde      = { workspace = true }
serde_json = { workspace = true }
tracing    = { workspace = true }

[[bin]]
name = "octo-desktop"
path = "src/main.rs"
```

**lib.rs 核心流程**：

```rust
pub fn run() {
    tracing_subscriber::fmt().init();

    // 1. 启动内嵌 server（随机端口）
    let handle = server::start_embedded_server()
        .expect("Failed to start octo server");
    let port = handle.port;
    let url = format!("http://127.0.0.1:{port}");

    // 2. 构建 Tauri 应用
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            // 聚焦已有窗口
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_port,
        ])
        .setup(move |app| {
            // 3. 创建窗口指向内嵌 Dashboard
            WebviewWindowBuilder::new(
                app, "main",
                WebviewUrl::External(url.parse().unwrap()),
            )
            .title("Octo Workbench")
            .inner_size(1280.0, 800.0)
            .min_inner_size(800.0, 600.0)
            .center()
            .build()?;

            tray::setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭时最小化到托盘
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("Failed to build Tauri app")
        .run(|_, _| {});

    handle.shutdown();
}
```

#### 6.8.4 功能清单

| 功能 | 来源 | 复杂度 | 优先级 |
|------|------|--------|--------|
| 内嵌 Server + WebView 窗口 | Fork OpenFang | 低 | 必需 |
| 系统托盘（显示/退出） | Fork OpenFang | 低 | 必需 |
| 单实例锁 | tauri-plugin-single-instance | 零 | 必需 |
| 原生通知（Agent 崩溃/配额耗尽） | Fork OpenFang | 低 | 建议 |
| 开机自启 | tauri-plugin-autostart | 零 | 可选 |
| 全局快捷键 | tauri-plugin-global-shortcut | 低 | 可选 |
| 自动更新 | tauri-plugin-updater | 中 | 可选 |
| 文件导入（Agent TOML / Skill） | Fork OpenFang commands.rs | 低 | 可选 |

#### 6.8.5 构建与发布

```bash
# 开发模式（热重载 WebView）
cargo tauri dev -p octo-desktop

# 生产构建（生成 .dmg / .msi / .AppImage）
cargo tauri build -p octo-desktop
```

Tauri 2.0 自动生成各平台安装包：
- **macOS**: `.dmg` + `.app` (最小 12.0)
- **Windows**: `.msi` + `.exe` (内含 WebView2 引导)
- **Linux**: `.deb` + `.AppImage`

#### 6.8.6 预估工作量

由于 Dashboard 完全复用（同一套 Alpine.js 代码），桌面壳只需 ~500 行 Rust：

| 文件 | 行数 | 说明 |
|------|------|------|
| lib.rs | ~150 | Tauri 初始化 + 窗口创建 |
| server.rs | ~100 | 内嵌 server 启动/关闭 |
| commands.rs | ~80 | IPC 命令（状态、端口） |
| tray.rs | ~100 | 托盘菜单 |
| main.rs | ~10 | 入口 |
| tauri.conf.json | ~60 | 配置 |
| **合计** | **~500 行** | **1-2 天即可完成** |

#### 6.8.7 执行前提

- Part 6 嵌入式 Web Dashboard 已完成并可正常运行
- `octo-server` 支持 `embed` feature flag 暴露 `build_router()` 供桌面壳调用
- 准备好应用图标（ico/png/icns 各尺寸）

---

### 6.9 差异化设计：双模式布局 (Workbench + Dashboard)

#### 6.9.1 设计原则：借鉴架构模式，不借鉴视觉外观

OpenFang 验证了以下架构模式的可行性，octo 借鉴这些**模式**：
- `include_str!()` 编译时嵌入，单二进制部署
- Alpine.js 轻量级响应式框架，零构建依赖
- Hash-based SPA 路由
- WebSocket 实时流式通信

但 octo 的**视觉外观和交互范式**必须有自己的设计语言：
- OpenFang 是 **"控制面板"** 风格（侧栏 + 页面切换，运维导向）
- octo 应该是 **"开发者工作台"** 风格（多面板同屏，调试导向）

#### 6.9.2 双模式布局设计

同一套 Alpine.js 代码支持两种视图模式，用户可一键切换：

**Workbench 模式**（开发者日常 — 默认）：

```
┌─────────────────────────────────────────────────────────────┐
│ octo workbench  Session: dev-01  ⚡claude-3.5  [⊞ WB] [☰ DB]│
├──────────┬──────────────────────────┬────────────────────────┤
│          │                          │   ┌─ Context ───────┐ │
│  Agent   │     Chat / Canvas        │   │ ████████░░ 62%  │ │
│  Tree    │                          │   │ System:  2.1K   │ │
│          │  User: 帮我重构这个模块    │   │ History: 8.4K   │ │
│ ▸ agents │  Agent: 好的，我来分析... │   │ Tools:   1.5K   │ │
│   ▸ dev  │  [streaming...]          │   └─────────────────┘ │
│   ▸ test │                          │   ┌─ Memory Live ───┐ │
│ ▸ skills │                          │   │ L0: 3 entries   │ │
│ ▸ tools  │                          │   │ L1: 12 entries  │ │
│ ▸ mcp    │                          │   │ L2: 47 entries  │ │
│   ▸ fs   │                          │   └─────────────────┘ │
│   ▸ git  │                          │   ┌─ Tool Trace ────┐ │
│          │                          │   │ ▸ file_read  OK  │ │
│          │                          │   │ ▸ bash    2.3s   │ │
│          │                          │   │ ▸ grep    0.1s   │ │
│          │                          │   └─────────────────┘ │
├──────────┴──────────────────────────┴────────────────────────┤
│ MCP: 3 servers ✓  │ Memory: 62 entries │ Tools: 24 │ 12K tok │
└─────────────────────────────────────────────────────────────┘
```

特点：三栏布局，Chat 居中，右侧 Inspector 面板实时展示 Agent 内部状态。

**Dashboard 模式**（运维监控）：

```
┌─────────────────────────────────────────────────────────────┐
│ octo workbench  Session: dev-01  ⚡claude-3.5  [⊞ WB] [☰ DB]│
├──────────┬──────────────────────────────────────────────────┤
│          │                                                  │
│ Overview │          当前页面全宽渲染                          │
│ Chat     │                                                  │
│ Agents   │   （与 OpenFang 类似的页面切换模式）                │
│ Sessions │                                                  │
│ Memory   │          每个页面独占主区域                        │
│ MCP      │          展示完整数据表格/图表                     │
│ Tools    │                                                  │
│ Skills   │                                                  │
│ Config   │                                                  │
│ Metrics  │                                                  │
│ Logs     │                                                  │
│ Audit    │                                                  │
│ Security │                                                  │
├──────────┴──────────────────────────────────────────────────┤
│ MCP: 3 servers ✓  │ Memory: 62 entries │ Tools: 24 │ 12K tok │
└─────────────────────────────────────────────────────────────┘
```

特点：传统侧栏 + 全宽页面切换，适合查看完整数据表格和详细配置。

#### 6.9.3 技术实现：零额外复杂性

**布局切换仅需 CSS class + Alpine.js 状态**，不引入任何新框架或构建工具：

```javascript
// app.js — 布局模式管理
Alpine.data('app', () => ({
    layoutMode: localStorage.getItem('octo-layout') || 'workbench',
    currentPage: 'chat',
    inspectorPanels: {
        context: true,    // Context Budget 面板
        memory: true,     // Memory Live 面板
        toolTrace: true,  // Tool Trace 面板
    },

    toggleLayout() {
        this.layoutMode = this.layoutMode === 'workbench' ? 'dashboard' : 'workbench';
        localStorage.setItem('octo-layout', this.layoutMode);
    },

    togglePanel(name) {
        this.inspectorPanels[name] = !this.inspectorPanels[name];
    }
}));
```

```css
/* layout.css — 双模式布局 */

/* Workbench 模式：三栏 (Tree | Chat | Inspector) */
.layout-workbench .main-grid {
    display: grid;
    grid-template-columns: 200px 1fr 280px;
    height: calc(100vh - 80px);  /* 减去顶栏 + 状态栏 */
}

/* Dashboard 模式：双栏 (Nav | Content) */
.layout-dashboard .main-grid {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: calc(100vh - 80px);
}

/* Inspector 面板在 Dashboard 模式下隐藏 */
.layout-dashboard .inspector-panel { display: none; }

/* Inspector 内各子面板可折叠 */
.inspector-section { border-bottom: 1px solid var(--border); }
.inspector-section.collapsed .inspector-body { display: none; }
```

```html
<!-- index_body.html 核心结构 -->
<body x-data="app" :class="'layout-' + layoutMode">
    <!-- 顶栏 -->
    <header class="top-bar">
        <span>octo workbench</span>
        <span x-text="'Session: ' + currentSession"></span>
        <button @click="toggleLayout"
                x-text="layoutMode === 'workbench' ? '☰ Dashboard' : '⊞ Workbench'">
        </button>
    </header>

    <!-- 主区域 -->
    <div class="main-grid">
        <!-- 左侧：Tree (Workbench) / Nav (Dashboard) -->
        <nav class="sidebar">
            <!-- Workbench: Agent/Skill/Tool 树 -->
            <template x-if="layoutMode === 'workbench'">
                <div x-data="agentTree">...</div>
            </template>
            <!-- Dashboard: 页面导航 -->
            <template x-if="layoutMode === 'dashboard'">
                <div class="nav-list">...</div>
            </template>
        </nav>

        <!-- 中间：Chat (Workbench 始终显示) / 页面内容 (Dashboard 切换) -->
        <main class="content-area">
            <template x-if="layoutMode === 'workbench' || currentPage === 'chat'">
                <div x-data="chatPage">...</div>
            </template>
            <template x-if="layoutMode === 'dashboard' && currentPage !== 'chat'">
                <div x-data="currentPageData">...</div>
            </template>
        </main>

        <!-- 右侧：Inspector (仅 Workbench 模式) -->
        <aside class="inspector-panel" x-show="layoutMode === 'workbench'">
            <div class="inspector-section" x-data="contextBudget">...</div>
            <div class="inspector-section" x-data="memoryLive">...</div>
            <div class="inspector-section" x-data="toolTrace">...</div>
        </aside>
    </div>

    <!-- 状态栏 -->
    <footer class="status-bar">...</footer>
</body>
```

#### 6.9.4 Inspector 面板（octo 差异化核心）

三个 Inspector 面板是 octo 独有的，OpenFang 没有这些：

**Context Budget 面板**（实时 token 预算可视化）：
```javascript
// inspector/context_budget.js (~80 行)
Alpine.data('contextBudget', () => ({
    budget: { total: 200000, used: 0, system: 0, history: 0, tools: 0 },
    async init() {
        // 每 2 秒轮询 /api/budget
        setInterval(() => this.refresh(), 2000);
    },
    async refresh() {
        this.budget = await api.get('/api/budget');
    },
    get percentage() {
        return Math.round(this.budget.used / this.budget.total * 100);
    }
}));
```

**Memory Live 面板**（实时记忆写入流）：
```javascript
// inspector/memory_live.js (~60 行)
Alpine.data('memoryLive', () => ({
    layers: { L0: [], L1: [], L2: [] },
    async init() {
        // SSE 订阅 /api/memory/stream
        const es = new EventSource('/api/memory/stream');
        es.onmessage = (e) => {
            const entry = JSON.parse(e.data);
            this.layers[entry.layer].unshift(entry);
            if (this.layers[entry.layer].length > 20) this.layers[entry.layer].pop();
        };
    }
}));
```

**Tool Trace 面板**（工具调用链实时追踪）：
```javascript
// inspector/tool_trace.js (~60 行)
Alpine.data('toolTrace', () => ({
    calls: [],
    async init() {
        // 通过 Chat WS 获取 tool_call 事件
        this.$watch('$store.ws.lastToolEvent', (event) => {
            if (event) {
                this.calls.unshift(event);
                if (this.calls.length > 50) this.calls.pop();
            }
        });
    }
}));
```

#### 6.9.5 视觉差异化要点

| 维度 | OpenFang | octo |
|------|----------|------|
| **色彩** | 蓝色系（#3B82F6）固定 | 6 套内置配色可切换（见下表） |
| **字体** | 系统默认 | JetBrains Mono（代码）+ Inter（UI），woff2 嵌入 |
| **布局** | 固定侧栏 + 页面切换 | 可切换双模式 + 可折叠 Inspector |
| **Logo** | OpenFang 牙齿图标 | 🦑 鱿鱼 emoji（终端/Web 统一） |
| **信息密度** | 中等（运维友好） | 高（开发者工作台，类似 IDE） |
| **实时状态** | 无 Inspector | 三面板实时状态（Context/Memory/Tool） |

**6 套内置配色方案**（默认：国网绿）：

| # | 名称 | 主色 | 暗色 | 亮色 | 光晕 | 风格 |
|---|------|------|------|------|------|------|
| 1 | **国网绿**（默认） | `#00843D` | `#004d23` | `#33b96e` | `rgba(0,132,61,0.15)` | 官方稳重 |
| 2 | 道奇蓝 | `#3B82F6` | `#1d4ed8` | `#60a5fa` | `rgba(59,130,246,0.15)` | 通用科技 |
| 3 | 海洋青 | `#06B6D4` | `#0891b2` | `#22d3ee` | `rgba(6,182,212,0.15)` | 终端感 |
| 4 | 深海靛蓝 | `#6366F1` | `#4f46e5` | `#818cf8` | `rgba(99,102,241,0.15)` | IDE 风格 |
| 5 | 墨紫 | `#8B5CF6` | `#7c3aed` | `#a78bfa` | `rgba(139,92,246,0.15)` | 神秘感 |
| 6 | 翡翠绿 | `#10B981` | `#059669` | `#34D399` | `rgba(16,185,129,0.15)` | Matrix 经典 |

配色切换通过 CSS 变量实现，用户在 Settings 页面或 config.yaml 中选择：

```yaml
# config.yaml
ui:
  theme: sgcc       # sgcc | blue | cyan | indigo | violet | emerald
  layout: workbench # workbench | dashboard
```

```javascript
// app.js — 配色切换
const themes = {
    sgcc:    { accent: '#00843D', dim: '#004d23', text: '#33b96e', glow: 'rgba(0,132,61,0.15)' },
    blue:    { accent: '#3B82F6', dim: '#1d4ed8', text: '#60a5fa', glow: 'rgba(59,130,246,0.15)' },
    cyan:    { accent: '#06B6D4', dim: '#0891b2', text: '#22d3ee', glow: 'rgba(6,182,212,0.15)' },
    indigo:  { accent: '#6366F1', dim: '#4f46e5', text: '#818cf8', glow: 'rgba(99,102,241,0.15)' },
    violet:  { accent: '#8B5CF6', dim: '#7c3aed', text: '#a78bfa', glow: 'rgba(139,92,246,0.15)' },
    emerald: { accent: '#10B981', dim: '#059669', text: '#34D399', glow: 'rgba(16,185,129,0.15)' },
};

function applyTheme(name) {
    const t = themes[name] || themes.sgcc;
    document.documentElement.style.setProperty('--accent', t.accent);
    document.documentElement.style.setProperty('--accent-dim', t.dim);
    document.documentElement.style.setProperty('--accent-text', t.text);
    document.documentElement.style.setProperty('--accent-glow', t.glow);
    localStorage.setItem('octo-theme', name);
}
```

TUI（Ratatui）使用相同的色值，通过 `Color::Rgb()` 映射：

```rust
// tui/theme.rs
pub struct ThemeColors {
    pub accent: Color,
    pub accent_dim: Color,
    pub accent_text: Color,
}

pub fn theme_sgcc() -> ThemeColors {
    ThemeColors {
        accent: Color::Rgb(0, 132, 61),
        accent_dim: Color::Rgb(0, 77, 35),
        accent_text: Color::Rgb(51, 185, 110),
    }
}
```

视觉对比预览页面：`docs/design/color_comparison.html`

#### 6.9.6 复杂性控制

增量代码量评估：

| 新增内容 | 代码量 | 说明 |
|---------|--------|------|
| 双模式 CSS 布局 | ~150 行 | CSS Grid 切换 |
| 布局切换 JS | ~50 行 | Alpine.data + localStorage |
| Agent Tree 组件 | ~120 行 | Workbench 左侧树形导航 |
| Context Budget 面板 | ~80 行 | 轮询 /api/budget |
| Memory Live 面板 | ~60 行 | SSE 订阅 |
| Tool Trace 面板 | ~60 行 | WS 事件监听 |
| **总增量** | **~520 行** | 占总代码量 ~7% |

**复杂性风险评估**：

- **CSS 布局切换**：纯 CSS，无 JS 框架依赖，无状态管理复杂性 → **低风险**
- **Inspector 面板**：独立 Alpine.data 组件，与页面解耦 → **低风险**
- **SSE/WS 实时流**：Chat 页面已有 WS 基础，Inspector 复用连接 → **低风险**
- **构建依赖**：零变化，仍然是 `include_str!()` → **零风险**

结论：~520 行增量代码实现 octo 与 OpenFang 的视觉和交互差异化，不引入新框架、新构建工具或新架构模式。

---

## 附录：参考资料索引

| 项目 | 路径 | CLI 代码量 | 关键学习 |
|------|------|-----------|----------|
| Goose | `3th-party/harnesses/rust-projects/goose/crates/goose-cli/` | ~60KB | Recipe 系统、Terminal 集成 |
| IronClaw | `3th-party/harnesses/rust-projects/ironclaw/src/cli/` | 中等 | 安全配对、多通道、WASM 工具 |
| ZeroClaw | `3th-party/harnesses/rust-projects/zeroclaw/src/main.rs` | 1500+ 行 | Ratatui TUI、硬件支持、estop |
| Moltis | `3th-party/harnesses/rust-projects/moltis/crates/cli/` | 500+ 行 | Gateway-first、远程节点 |
| LocalGPT | `3th-party/harnesses/rust-projects/localgpt/crates/cli/` | 5.7K 行 | rustyline REPL、桌面 GUI |
| OpenFang | `3th-party/harnesses/rust-projects/openfang/crates/openfang-cli/` | 8.8K 行 | 30+ 命令、Vault、Workflow |
| Pi Agent | `3th-party/harnesses/rust-projects/pi_agent_rust/src/` | 237K 行 | BubbleTea TUI、Extension 策略 |
| octo-cli | `crates/octo-cli/` | 632 行 | 当前原型，需大幅扩展 |
