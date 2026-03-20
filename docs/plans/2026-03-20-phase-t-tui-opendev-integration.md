# Phase T: TUI OpenDev 整合 — 对话中心终端界面

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 opendev TUI 的完整特性移植进 octo-cli，重建为对话中心界面，直接对接 octo-engine，替换当前 12-Tab 多屏架构。

**Architecture:** 废弃当前 Ops/Dev 双模式 + 12 Tab 布局，改为 opendev 风格的垂直堆叠对话中心界面（对话区 + 可折叠面板 + 输入区 + 状态栏），调试功能通过浮层 modal 按需唤起。TUI 直接使用 octo-types + AgentEvent + AgentExecutorHandle，零中间适配层。

**Tech Stack:** Ratatui 0.29+, Crossterm 0.28+, Tokio, octo-engine (AgentExecutorHandle, AgentEvent, ApprovalGate, CancellationToken)

**Branch:** `feat/tui-opendev-integration`（从 main 创建，完成前不合入）

---

## 设计决策摘要

### 类型统一策略（无适配层）

TUI 层直接使用 octo 类型系统，不引入 opendev 类型：

| 概念 | 使用 | 理由 |
|------|------|------|
| 消息 | `octo_types::ChatMessage` (`Vec<ContentBlock>`) | 比 opendev 的 `String` 更丰富，Anthropic 协议对齐 |
| 事件流 | `octo_engine::agent::AgentEvent` (15 变种) | 已包含 TextDelta/ToolStart/ToolResult/ApprovalRequired 等 |
| 审批 | `octo_engine::tools::ApprovalGate` | 已实现 register/respond/wait_for_approval |
| 中断 | `octo_engine::agent::CancellationToken` | 已集成到 AgentLoopConfig |
| 工具上下文 | `octo_types::ToolExecution` / `ToolProgress` | 已有完整工具状态追踪 |

移植 opendev widget 时，将其内部的 `DisplayMessage`/`DisplayRole`/`DisplayToolCall` 替换为直接渲染 `ContentBlock` 变种。

### 布局策略（对话中心 + 浮层调试）

```
┌───────────────────────────────────────┐
│ 对话区 (Constraint::Min(5))           │ ← 永驻主体，scrollable
│  - 用户消息 / AI 回复 / 工具结果      │
│  - 内联 spinner (工具执行中)          │
│  - 内联 diff viewer (文件修改)        │
├───────────────────────────────────────┤
│ 进度面板 (0-8 行，可折叠/隐藏)        │ ← 仅在执行时显示
├───────────────────────────────────────┤
│ 输入区 (1-8 行，多行)                 │
├───────────────────────────────────────┤
│ 状态栏 (2 行固定)                     │ ← 模型/Token/Session/MCP
└───────────────────────────────────────┘

浮层 (按需唤起):
  Ctrl+D → Agent 调试面板
  Ctrl+E → Eval 结果面板
  Ctrl+P → 后台任务管理
  Ctrl+A → Agent/Session 选择器
```

### Engine 对接（与 CLI REPL 共用路径）

```rust
// TUI 使用与 REPL 完全相同的 API：
let handle = runtime.start_primary(session_id, user_id, sandbox_id, history, agent_id).await;
// 发送: handle.send(AgentMessage::UserMessage { content, channel_id: "tui" })
// 订阅: handle.subscribe() → broadcast::Receiver<AgentEvent>
// 审批: state.agent_runtime.approval_gate().respond(&tool_id, approved)
// 取消: handle.send(AgentMessage::Cancel)
```

---

## 文件移植清单

### 从 opendev-tui 移植（改写为 octo 类型）

| opendev 源文件 | 目标路径 (octo-cli/src/tui/) | 改写要点 |
|----------------|------------------------------|----------|
| `widgets/conversation/mod.rs` | `widgets/conversation/mod.rs` | 渲染 `Vec<ContentBlock>` 代替 `String` |
| `widgets/conversation/diff.rs` | `widgets/conversation/diff.rs` | 直接移入 |
| `widgets/conversation/tool_format.rs` | `widgets/conversation/tool_format.rs` | 用 `AgentEvent::ToolStart/ToolResult` |
| `widgets/conversation/spinner.rs` | `widgets/conversation/spinner.rs` | 直接移入 |
| `widgets/input.rs` | `widgets/input.rs` | 替换现有 TextInput |
| `widgets/spinner.rs` | `widgets/spinner.rs` | 直接移入 |
| `widgets/progress.rs` | `widgets/progress.rs` | 适配 `ToolProgress` |
| `widgets/status_bar.rs` | `widgets/status_bar.rs` | 适配 `AppState` 字段 |
| `widgets/toast.rs` | `widgets/toast.rs` | 直接移入 |
| `formatters/markdown.rs` | `formatters/markdown.rs` | 直接移入 |
| `formatters/wrap.rs` | `formatters/wrap.rs` | 直接移入 |
| `formatters/path_shortener.rs` | `formatters/path_shortener.rs` | 直接移入 |
| `formatters/style_tokens.rs` | `formatters/style_tokens.rs` | 合并进 theme.rs |
| `formatters/tool_registry.rs` | `formatters/tool_registry.rs` | 直接移入 |
| `formatters/bash_formatter.rs` | `formatters/bash_formatter.rs` | 直接移入 |
| `formatters/file_formatter.rs` | `formatters/file_formatter.rs` | 直接移入 |
| `formatters/diff (in conversation)` | `formatters/diff.rs` | 直接移入 |
| `autocomplete/*` | `autocomplete/*` | 整体移入，适配 slash 命令 |
| `controllers/approval.rs` | `controllers/approval.rs` | 用 `ApprovalGate.respond()` |
| `controllers/message.rs` | `controllers/message.rs` | 用 `AgentExecutorHandle.send()` |
| `managers/spinner.rs` | `managers/spinner.rs` | 直接移入 |
| `managers/message_history.rs` | `managers/message_history.rs` | 直接移入 |
| `managers/interrupt.rs` | `managers/interrupt.rs` | 用 `CancellationToken` |
| `managers/display_ledger.rs` | `managers/display_ledger.rs` | 直接移入 |
| `app/cache.rs` | `cache.rs` | 适配 `ContentBlock` |
| `app/render.rs` | `render.rs` | 核心布局逻辑 |
| `event/handler.rs` | `event/handler.rs` | 合并 crossterm + AgentEvent |
| `event/mod.rs` | `event.rs` | 扩展为 Agent 事件 |

### 保留的 octo-cli 代码

| 文件 | 保留理由 |
|------|----------|
| `tui/theme.rs` | 已有 12 色 TuiTheme，合并 opendev style_tokens |
| `tui/screens/dev_eval.rs` | 改为浮层 modal，RunStore 集成已有 |
| `tui/screens/dev_agent.rs` | 改为浮层 modal，保留三栏结构 |
| `tui/widgets/text_input.rs` | 被 opendev 的 input.rs 替换（保留为 fallback） |

### 删除的 octo-cli 代码

| 文件 | 删除理由 |
|------|----------|
| `tui/screens/welcome.rs` | 被 opendev welcome_panel 替换 |
| `tui/screens/dashboard.rs` | 信息整合到状态栏和调试浮层 |
| `tui/screens/chat.rs` | 被对话中心主界面替换 |
| `tui/screens/agents.rs` | 整合到 Ctrl+A 选择器浮层 |
| `tui/screens/sessions.rs` | 整合到 Ctrl+A 选择器浮层 |
| `tui/screens/memory.rs` | 整合到调试浮层 |
| `tui/screens/mcp.rs` | 整合到调试浮层或 /mcp 命令 |
| `tui/screens/tools.rs` | 工具执行直接在对话中内联显示 |
| `tui/screens/security.rs` | 整合到调试浮层 |
| `tui/screens/settings.rs` | /config 命令替代 |
| `tui/screens/logs.rs` | 不需要独立屏 |
| `tui/screens/skills.rs` | /skill 命令替代 |
| `tui/screens/dual_chat.rs` | 已废弃 |
| `tui/screens/mod.rs` | Screen trait + ScreenManager 被新架构替代 |
| `tui/backend.rs` | TuiBackend trait 未被使用 |

---

## 实施计划

### Phase T1: 基础设施移植（Widget + Formatter + 事件系统）

目标：把 opendev 的 UI 基础组件移入 octo-cli，编译通过，不改变现有入口。

---

### Task T1-1: 创建 feature branch 并建立目录结构

**Files:**
- Create: `crates/octo-cli/src/tui/formatters/mod.rs`
- Create: `crates/octo-cli/src/tui/formatters/markdown.rs`
- Create: `crates/octo-cli/src/tui/formatters/wrap.rs`
- Create: `crates/octo-cli/src/tui/formatters/path_shortener.rs`
- Create: `crates/octo-cli/src/tui/formatters/tool_registry.rs`
- Create: `crates/octo-cli/src/tui/formatters/bash_formatter.rs`
- Create: `crates/octo-cli/src/tui/formatters/file_formatter.rs`
- Create: `crates/octo-cli/src/tui/formatters/diff.rs`
- Create: `crates/octo-cli/src/tui/managers/mod.rs`
- Create: `crates/octo-cli/src/tui/managers/spinner.rs`
- Create: `crates/octo-cli/src/tui/managers/message_history.rs`
- Create: `crates/octo-cli/src/tui/managers/interrupt.rs`
- Create: `crates/octo-cli/src/tui/managers/display_ledger.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 创建 feature branch**

```bash
git checkout -b feat/tui-opendev-integration
```

**Step 2: 建立新目录结构**

在 `crates/octo-cli/src/tui/` 下创建 `formatters/` 和 `managers/` 目录，每个建立 `mod.rs` 占位。

**Step 3: 修改 `tui/mod.rs` 导出新模块**

在 `mod.rs` 顶部添加：
```rust
pub mod formatters;
pub mod managers;
```

**Step 4: cargo check 确认编译通过**

```bash
cargo check -p octo-cli
```

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(tui): scaffold formatters/ and managers/ directories for opendev integration"
```

---

### Task T1-2: 移植 formatters — markdown、wrap、path_shortener

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/formatters/markdown.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/formatters/wrap.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/formatters/path_shortener.rs`
- Create: `crates/octo-cli/src/tui/formatters/markdown.rs`
- Create: `crates/octo-cli/src/tui/formatters/wrap.rs`
- Create: `crates/octo-cli/src/tui/formatters/path_shortener.rs`
- Modify: `crates/octo-cli/src/tui/formatters/mod.rs`

**Step 1: 阅读 opendev 源文件**

阅读上述三个 opendev formatter 源文件，理解其接口和依赖。

**Step 2: 移植 wrap.rs**

最纯粹的工具模块，无外部类型依赖。直接复制并调整 import。

**Step 3: 移植 path_shortener.rs**

依赖 `dirs` crate。检查 octo-cli 的 Cargo.toml 是否已有 `dirs` 依赖，没有则添加。

**Step 4: 移植 markdown.rs**

这是最复杂的 formatter。将 opendev 的 markdown → Ratatui Line/Span 转换逻辑复制过来。去掉对 `opendev-*` crate 的 import，替换为 `ratatui::text::{Line, Span}` 和 `ratatui::style::*`。

**Step 5: 更新 formatters/mod.rs 导出**

```rust
pub mod markdown;
pub mod wrap;
pub mod path_shortener;
```

**Step 6: cargo check 确认编译通过**

```bash
cargo check -p octo-cli
```

**Step 7: Commit**

```bash
git add -A && git commit -m "feat(tui): port markdown, wrap, path_shortener formatters from opendev"
```

---

### Task T1-3: 移植 formatters — tool_registry、bash_formatter、file_formatter、diff

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/formatters/tool_registry.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/formatters/bash_formatter.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/formatters/file_formatter.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/conversation/diff.rs`
- Create: `crates/octo-cli/src/tui/formatters/tool_registry.rs`
- Create: `crates/octo-cli/src/tui/formatters/bash_formatter.rs`
- Create: `crates/octo-cli/src/tui/formatters/file_formatter.rs`
- Create: `crates/octo-cli/src/tui/formatters/diff.rs`
- Modify: `crates/octo-cli/src/tui/formatters/mod.rs`

**Step 1: 阅读 opendev 源文件**

**Step 2: 移植 tool_registry.rs — 工具名称→颜色/分类映射**

直接移入。不依赖外部类型。

**Step 3: 移植 bash_formatter.rs、file_formatter.rs**

这些 formatter 渲染 bash 输出和文件内容为 Ratatui Spans。去掉 opendev 类型引用。

**Step 4: 移植 diff.rs**

从 `widgets/conversation/diff.rs` 移至 `formatters/diff.rs`。纯粹的 unified diff → Ratatui Line 转换。

**Step 5: 更新 mod.rs 导出，cargo check**

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(tui): port tool_registry, bash/file formatters, diff renderer from opendev"
```

---

### Task T1-4: 移植 managers — spinner、message_history、display_ledger

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/managers/spinner.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/managers/message_history.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/managers/display_ledger.rs`
- Create: `crates/octo-cli/src/tui/managers/spinner.rs`
- Create: `crates/octo-cli/src/tui/managers/message_history.rs`
- Create: `crates/octo-cli/src/tui/managers/display_ledger.rs`
- Modify: `crates/octo-cli/src/tui/managers/mod.rs`

**Step 1: 移植 spinner.rs — 集中动画帧服务**

无外部类型依赖，直接移入。

**Step 2: 移植 message_history.rs — 命令历史导航**

直接移入，rustyline 兼容。

**Step 3: 移植 display_ledger.rs — 消息去重**

直接移入。

**Step 4: 更新 mod.rs，cargo check**

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(tui): port spinner, message_history, display_ledger managers from opendev"
```

---

### Task T1-5: 移植 interrupt manager — 适配 CancellationToken

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/managers/interrupt.rs`
- Read: `crates/octo-engine/src/agent/cancellation.rs`
- Create: `crates/octo-cli/src/tui/managers/interrupt.rs`
- Modify: `crates/octo-cli/src/tui/managers/mod.rs`

**Step 1: 阅读 opendev 中断管理和 octo CancellationToken**

**Step 2: 实现 interrupt manager**

不套用 opendev 的 InterruptToken，直接封装 octo 的 `AgentMessage::Cancel` + `CancellationToken`：

```rust
use octo_engine::agent::{AgentExecutorHandle, AgentMessage};

pub struct InterruptManager {
    handle: AgentExecutorHandle,
    ctrl_c_count: u8,
}

impl InterruptManager {
    pub fn new(handle: AgentExecutorHandle) -> Self {
        Self { handle, ctrl_c_count: 0 }
    }

    /// 返回 true 表示应退出 TUI
    pub async fn handle_ctrl_c(&mut self) -> bool {
        self.ctrl_c_count += 1;
        if self.ctrl_c_count >= 2 {
            return true; // 双击退出
        }
        let _ = self.handle.send(AgentMessage::Cancel).await;
        false
    }

    pub fn reset(&mut self) {
        self.ctrl_c_count = 0;
    }
}
```

**Step 3: cargo check**

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(tui): implement InterruptManager wrapping CancellationToken for Ctrl+C"
```

---

### Task T1-6: 扩展 AppEvent 为 Agent 感知事件系统

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/event/mod.rs`
- Modify: `crates/octo-cli/src/tui/event.rs`

**Step 1: 重写 event.rs**

将当前 10 个 UI 变种扩展为完整的 Agent + UI 事件系统：

```rust
use crossterm::event::KeyEvent;
use octo_engine::agent::AgentEvent;
use octo_types::RiskLevel;

#[derive(Debug, Clone)]
pub enum AppEvent {
    // ── 终端事件 ──
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,

    // ── Agent 事件（从 broadcast::Receiver<AgentEvent> 桥接） ──
    Agent(AgentEvent),

    // ── 用户操作 ──
    UserSubmit(String),
    Quit,
}
```

关键设计：`Agent(AgentEvent)` 直接包装 octo-engine 的事件，不做转换。

**Step 2: cargo check**

**Step 3: Commit**

```bash
git add -A && git commit -m "feat(tui): extend AppEvent with Agent(AgentEvent) for full agent lifecycle"
```

---

### Task T1-7: 移植 event handler — 合并 crossterm + AgentEvent 流

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/event/handler.rs`
- Create: `crates/octo-cli/src/tui/event_handler.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 实现 EventHandler**

核心：用 `tokio::select!` 合并 crossterm 事件流和 AgentEvent broadcast：

```rust
use crossterm::event::{self as ct, Event as CEvent, EventStream};
use futures_util::StreamExt;
use tokio::sync::{broadcast, mpsc};
use std::time::Duration;

use super::event::AppEvent;
use octo_engine::agent::AgentEvent;

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    pub fn new(agent_rx: broadcast::Receiver<AgentEvent>, tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_tick = tx.clone();
        let tx_term = tx.clone();
        let tx_agent = tx.clone();

        // Terminal events
        tokio::spawn(async move {
            let mut stream = EventStream::new();
            while let Some(Ok(event)) = stream.next().await {
                match event {
                    CEvent::Key(key) => { let _ = tx_term.send(AppEvent::Key(key)); }
                    CEvent::Resize(w, h) => { let _ = tx_term.send(AppEvent::Resize(w, h)); }
                    _ => {}
                }
            }
        });

        // Agent events
        tokio::spawn(async move {
            let mut rx = agent_rx;
            while let Ok(event) = rx.recv().await {
                let _ = tx_agent.send(AppEvent::Agent(event));
            }
        });

        // Tick
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                interval.tick().await;
                if tx_tick.send(AppEvent::Tick).is_err() { break; }
            }
        });

        Self { rx }
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    pub fn try_next(&mut self) -> Option<AppEvent> {
        self.rx.try_recv().ok()
    }
}
```

**Step 2: 在 mod.rs 中导出**

```rust
pub mod event_handler;
```

**Step 3: cargo check**

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(tui): implement EventHandler merging crossterm + AgentEvent streams"
```

---

### Task T1-8: 移植核心 widgets — spinner、progress、input、status_bar

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/spinner.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/progress.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/input.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/status_bar.rs`
- Create: `crates/octo-cli/src/tui/widgets/spinner.rs`
- Create: `crates/octo-cli/src/tui/widgets/progress.rs`
- Create: `crates/octo-cli/src/tui/widgets/input.rs`
- Create: `crates/octo-cli/src/tui/widgets/status_bar.rs`
- Modify: `crates/octo-cli/src/tui/widgets/mod.rs`

**Step 1: 移植 spinner.rs — 动画帧字符 widget**

纯渲染组件，无类型依赖，直接移入。

**Step 2: 移植 progress.rs — 进度条 widget**

适配 `octo_types::ToolProgress`（有 `fraction`, `message`, `elapsed_ms`），直接映射。

**Step 3: 移植 input.rs — 多行输入 widget**

替换现有 TextInput。opendev 的 input 支持多行、光标导航、Shift+Enter。保留旧 `text_input.rs` 为 `text_input_legacy.rs` 备用。

**Step 4: 移植 status_bar.rs — 底部状态栏**

适配显示来源：
- 模型名: 从 `AppState.agent_runtime` 获取
- Token 用量: 从 `AgentLoopResult.input_tokens/output_tokens`
- Session ID: 从当前 handle.session_id
- MCP 状态: 从 `AppState.agent_runtime.mcp_manager()`

**Step 5: 更新 widgets/mod.rs 导出，cargo check**

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(tui): port spinner, progress, input, status_bar widgets from opendev"
```

---

### Task T1-9: 移植 conversation widget — 渲染 ContentBlock

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/conversation/mod.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/conversation/tool_format.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/conversation/spinner.rs`
- Create: `crates/octo-cli/src/tui/widgets/conversation/mod.rs`
- Create: `crates/octo-cli/src/tui/widgets/conversation/tool_format.rs`
- Create: `crates/octo-cli/src/tui/widgets/conversation/spinner.rs`
- Modify: `crates/octo-cli/src/tui/widgets/mod.rs`

**Step 1: 阅读 opendev conversation widget**

理解其 message → Line 渲染管线。

**Step 2: 改写核心渲染——ContentBlock 替代 String**

opendev 渲染 `DisplayMessage { role, content: String }` 为 Lines。
我们改为渲染 `ChatMessage { role, content: Vec<ContentBlock> }`：

```rust
fn render_message(msg: &ChatMessage, lines: &mut Vec<Line<'static>>) {
    let role_style = role_to_style(&msg.role);
    for block in &msg.content {
        match block {
            ContentBlock::Text { text } => {
                // markdown 渲染
                let md_lines = markdown::render(text);
                lines.extend(md_lines);
            }
            ContentBlock::ToolUse { name, input, .. } => {
                // 工具调用格式化
                lines.push(tool_format::format_tool_call(name, input));
            }
            ContentBlock::ToolResult { content, is_error, .. } => {
                // 工具结果格式化
                lines.extend(tool_format::format_tool_result(content, *is_error));
            }
            _ => {} // Image, Document etc — future
        }
    }
}
```

**Step 3: 移植 tool_format.rs — 工具调用/结果渲染**

适配使用 `serde_json::Value`（ToolUse 的 input）和 `String`（ToolResult 的 content）。

**Step 4: 移植 conversation spinner — 内联工具执行 spinner**

结合 `managers::spinner` 服务。

**Step 5: cargo check**

**Step 6: Commit**

```bash
git add -A && git commit -m "feat(tui): port conversation widget with ContentBlock rendering"
```

---

### Task T1-10: 移植 toast widget + 编译验证

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/toast.rs`
- Create: `crates/octo-cli/src/tui/widgets/toast.rs`
- Modify: `crates/octo-cli/src/tui/widgets/mod.rs`

**Step 1: 移植 toast.rs**

通知气泡组件，纯 UI，无类型依赖。

**Step 2: 全量编译验证**

```bash
cargo check --workspace
cargo test -p octo-cli -- --test-threads=1
```

**Step 3: Commit**

```bash
git add -A && git commit -m "feat(tui): complete Phase T1 — all formatters, managers, widgets ported"
```

---

## Phase T2: 对话中心主界面 — 真实 Engine 对接

目标：用移植的组件重建 TUI 主循环，连接 AgentExecutorHandle，实现完整对话交互。

---

### Task T2-1: 定义新 TuiApp 状态结构

**Files:**
- Create: `crates/octo-cli/src/tui/app_state.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 定义 TuiState**

```rust
use octo_engine::agent::{AgentExecutorHandle, AgentEvent, AgentLoopResult};
use octo_types::{ChatMessage, ContentBlock, SessionId};

pub struct TuiState {
    // ── 运行状态 ──
    pub running: bool,
    pub dirty: bool,

    // ── 对话 ──
    pub messages: Vec<ChatMessage>,
    pub streaming_text: String,       // 当前流式文本 buffer
    pub is_streaming: bool,

    // ── 工具执行 ──
    pub active_tools: Vec<ActiveTool>,
    pub pending_approval: Option<PendingApproval>,

    // ── 输入 ──
    pub input_buffer: String,
    pub input_cursor: usize,

    // ── 滚动 ──
    pub scroll_offset: u16,
    pub user_scrolled: bool,

    // ── 缓存 ──
    pub cached_lines: Vec<ratatui::text::Line<'static>>,
    pub lines_generation: u64,
    pub message_generation: u64,

    // ── 指标 ──
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub session_id: SessionId,
    pub model_name: String,

    // ── 终端 ──
    pub terminal_width: u16,
    pub terminal_height: u16,
}

pub struct ActiveTool {
    pub tool_id: String,
    pub tool_name: String,
    pub input: serde_json::Value,
    pub started_at: std::time::Instant,
}

pub struct PendingApproval {
    pub tool_id: String,
    pub tool_name: String,
    pub risk_level: octo_types::RiskLevel,
}
```

**Step 2: cargo check**

**Step 3: Commit**

```bash
git add -A && git commit -m "feat(tui): define TuiState with conversation, tools, and metrics fields"
```

---

### Task T2-2: 实现主渲染布局

**Files:**
- Create: `crates/octo-cli/src/tui/render.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 实现垂直堆叠布局**

```rust
use ratatui::prelude::*;

pub fn render(state: &TuiState, frame: &mut Frame) {
    let area = frame.area();

    // 动态面板高度
    let progress_height = if state.active_tools.is_empty() { 0 } else {
        (state.active_tools.len() as u16 + 2).min(8)
    };
    let input_lines = state.input_buffer.lines().count().max(1).min(8) as u16;
    let status_height = 2u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),                    // 对话区
            Constraint::Length(progress_height),    // 进度面板
            Constraint::Length(input_lines + 2),    // 输入区 (+border)
            Constraint::Length(status_height),      // 状态栏
        ])
        .split(area);

    // 渲染各区域
    render_conversation(state, frame, chunks[0]);
    if progress_height > 0 {
        render_progress(state, frame, chunks[1]);
    }
    render_input(state, frame, chunks[2]);
    render_status_bar(state, frame, chunks[3]);

    // 浮层渲染（如有）
    if let Some(ref approval) = state.pending_approval {
        render_approval_dialog(approval, frame, area);
    }
}
```

**Step 2: 实现各子渲染函数**

使用 T1 移植的 widgets。

**Step 3: cargo check**

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(tui): implement vertical conversation-centric layout"
```

---

### Task T2-3: 实现主事件循环 — AgentEvent 驱动

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 重写 run_tui 函数**

```rust
pub async fn run_tui(
    app_state: &crate::commands::AppState,
    session_id: SessionId,
    handle: AgentExecutorHandle,
) -> Result<()> {
    // 终端初始化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 状态初始化
    let mut state = TuiState::new(session_id.clone(), handle.clone());

    // 事件处理器：合并 crossterm + AgentEvent
    let agent_rx = handle.subscribe();
    let mut event_handler = EventHandler::new(agent_rx, Duration::from_millis(100));

    // 主循环
    loop {
        // 重建缓存
        if state.lines_generation != state.message_generation {
            state.rebuild_cached_lines();
        }

        // 条件渲染
        if state.dirty {
            terminal.draw(|frame| render::render(&state, frame))?;
            state.dirty = false;
        }

        // 等待事件
        if let Some(event) = event_handler.next().await {
            handle_event(&mut state, &handle, app_state, event).await;
        }

        // 批量排空
        while let Some(event) = event_handler.try_next() {
            handle_event(&mut state, &handle, app_state, event).await;
            if !state.running { break; }
        }

        if !state.running { break; }
    }

    // 终端恢复
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
```

**Step 2: 实现 handle_event 分发**

```rust
async fn handle_event(
    state: &mut TuiState,
    handle: &AgentExecutorHandle,
    app_state: &crate::commands::AppState,
    event: AppEvent,
) {
    state.dirty = true;
    match event {
        AppEvent::Key(key) => handle_key(state, handle, key).await,
        AppEvent::Resize(w, h) => {
            state.terminal_width = w;
            state.terminal_height = h;
        }
        AppEvent::Tick => {
            // spinner 动画
        }
        AppEvent::Agent(agent_event) => {
            handle_agent_event(state, app_state, agent_event).await;
        }
        AppEvent::UserSubmit(text) => {
            // 追加到 messages，发送到 handle
            state.messages.push(ChatMessage::user(&text));
            state.message_generation += 1;
            let _ = handle.send(AgentMessage::UserMessage {
                content: text,
                channel_id: "tui".into(),
            }).await;
            state.is_streaming = true;
        }
        AppEvent::Quit => {
            state.running = false;
        }
    }
}
```

**Step 3: 实现 handle_agent_event — AgentEvent → TuiState**

```rust
async fn handle_agent_event(
    state: &mut TuiState,
    app_state: &crate::commands::AppState,
    event: AgentEvent,
) {
    match event {
        AgentEvent::TextDelta { text } => {
            state.streaming_text.push_str(&text);
            state.message_generation += 1;
        }
        AgentEvent::TextComplete { text } => {
            // 最终化流式文本为 ChatMessage
            state.messages.push(ChatMessage::assistant(&state.streaming_text));
            state.streaming_text.clear();
            state.is_streaming = false;
            state.message_generation += 1;
        }
        AgentEvent::ToolStart { tool_id, tool_name, input } => {
            state.active_tools.push(ActiveTool {
                tool_id, tool_name, input,
                started_at: std::time::Instant::now(),
            });
        }
        AgentEvent::ToolResult { tool_id, output, success } => {
            state.active_tools.retain(|t| t.tool_id != tool_id);
            state.message_generation += 1;
        }
        AgentEvent::ApprovalRequired { tool_name, tool_id, risk_level } => {
            state.pending_approval = Some(PendingApproval {
                tool_id, tool_name, risk_level,
            });
        }
        AgentEvent::Completed(result) => {
            state.total_input_tokens += result.input_tokens;
            state.total_output_tokens += result.output_tokens;
            state.is_streaming = false;
            state.active_tools.clear();
        }
        AgentEvent::Done => {
            state.is_streaming = false;
        }
        AgentEvent::Error { message } => {
            // 显示 toast
            state.is_streaming = false;
        }
        _ => {} // ThinkingDelta, IterationStart/End, etc.
    }
}
```

**Step 4: cargo check**

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(tui): implement main event loop with AgentEvent-driven state updates"
```

---

### Task T2-4: 实现键盘处理 — 输入、滚动、Ctrl+C

**Files:**
- Create: `crates/octo-cli/src/tui/key_handler.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 实现 handle_key**

```rust
async fn handle_key(state: &mut TuiState, handle: &AgentExecutorHandle, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            if state.interrupt_manager.handle_ctrl_c().await {
                state.running = false;
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if !state.input_buffer.trim().is_empty() {
                let text = std::mem::take(&mut state.input_buffer);
                state.input_cursor = 0;
                // 发送 UserSubmit
                // ... (同 handle_event 中的 UserSubmit 逻辑)
            }
        }
        (KeyModifiers::NONE, KeyCode::Up) if state.input_buffer.is_empty() => {
            state.scroll_offset = state.scroll_offset.saturating_add(3);
            state.user_scrolled = true;
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            state.scroll_offset = state.scroll_offset.saturating_sub(3);
            if state.scroll_offset == 0 { state.user_scrolled = false; }
        }
        (KeyModifiers::NONE, KeyCode::Char(c)) => {
            state.input_buffer.insert(state.input_cursor, c);
            state.input_cursor += 1;
            state.interrupt_manager.reset();
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            if state.input_cursor > 0 {
                state.input_cursor -= 1;
                state.input_buffer.remove(state.input_cursor);
            }
        }
        _ => {}
    }
}
```

**Step 2: cargo check**

**Step 3: Commit**

```bash
git add -A && git commit -m "feat(tui): implement keyboard handler with input, scrolling, Ctrl+C"
```

---

### Task T2-5: 实现 Tool Approval 弹窗

**Files:**
- Create: `crates/octo-cli/src/tui/controllers/mod.rs`
- Create: `crates/octo-cli/src/tui/controllers/approval.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 阅读 opendev 的 approval controller**

**Step 2: 实现 approval dialog — 直接调用 ApprovalGate**

```rust
use octo_engine::tools::ApprovalGate;

pub fn render_approval_dialog(approval: &PendingApproval, frame: &mut Frame, area: Rect) {
    // 居中弹窗
    let popup = centered_rect(60, 8, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(format!(" Tool Approval: {} ", approval.tool_name))
        .style(Style::default().fg(Color::Yellow));

    let text = vec![
        Line::from(format!("Risk: {:?}", approval.risk_level)),
        Line::from(""),
        Line::from("[Y] Approve  [N] Deny  [A] Always approve"),
    ];
    let para = Paragraph::new(text).block(block);
    frame.render_widget(para, popup);
}

// 在 handle_key 中处理审批响应：
// KeyCode::Char('y') => approval_gate.respond(&tool_id, true)
// KeyCode::Char('n') => approval_gate.respond(&tool_id, false)
```

**Step 3: cargo check**

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(tui): implement tool approval dialog backed by ApprovalGate"
```

---

### Task T2-6: 移植 autocomplete 引擎

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/autocomplete/mod.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/autocomplete/strategies.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/autocomplete/completers.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/autocomplete/file_finder.rs`
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/autocomplete/formatters.rs`
- Create: `crates/octo-cli/src/tui/autocomplete/mod.rs`
- Create: `crates/octo-cli/src/tui/autocomplete/strategies.rs`
- Create: `crates/octo-cli/src/tui/autocomplete/completers.rs`
- Create: `crates/octo-cli/src/tui/autocomplete/file_finder.rs`
- Create: `crates/octo-cli/src/tui/autocomplete/formatters.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 整体移植 autocomplete 引擎**

这个子系统几乎无外部类型依赖。主要适配点：
- Slash 命令列表来自 octo 的 `/help`, `/session`, `/config` 等
- File finder 使用 `ignore` crate（检查是否在 Cargo.toml 中）

**Step 2: 适配 octo 的 slash 命令列表**

从 `crates/octo-cli/src/repl/slash.rs` 中提取命令列表，注入 autocomplete strategies。

**Step 3: cargo check**

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(tui): port autocomplete engine with slash commands and file finder"
```

---

### Task T2-7: 连接 TUI 入口 — 替换 `octo tui` 命令

**Files:**
- Modify: `crates/octo-cli/src/main.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 修改 `octo tui` 入口**

当前入口创建 App + ScreenManager 走旧的 12-Tab 路径。改为：
1. 复用 REPL 的 session 解析逻辑
2. 调用 `start_primary()` 获取 handle
3. 调用新的 `run_tui(app_state, session_id, handle)`

```rust
Commands::Tui { theme, session_id, agent_id, resume } => {
    // 复用 REPL 的 session 逻辑
    let (sid, history) = resolve_session(state, session_id, resume).await?;
    let handle = state.agent_runtime.start_primary(sid.clone(), user_id, sandbox_id, history, agent_id.as_ref()).await;
    tui::run_tui(state, sid, handle).await?;
}
```

**Step 2: cargo check + 手动验证**

```bash
cargo run -p octo-cli -- tui
```

应显示对话界面，能输入文本并收到 agent 回复。

**Step 3: Commit**

```bash
git add -A && git commit -m "feat(tui): connect new TUI to engine via AgentExecutorHandle, replace old 12-Tab UI"
```

---

### Task T2-8: 删除旧屏幕文件 + 全量测试

**Files:**
- Delete: `crates/octo-cli/src/tui/screens/` (整个目录)
- Delete: `crates/octo-cli/src/tui/backend.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 删除旧屏幕**

移除 `screens/` 目录下所有文件（welcome, dashboard, chat, agents, sessions, memory, skills, mcp, tools, security, settings, logs, dual_chat, dev_agent, dev_eval, mod.rs）。

保留 `dev_agent.rs` 和 `dev_eval.rs` 到临时位置（后面 T3 改造为浮层时用）。

**Step 2: 删除 backend.rs (TuiBackend trait)**

未被使用。

**Step 3: 清理 mod.rs 中的旧 import**

移除 `Tab`, `ViewMode`, `OpsTab`, `DevTask` 及相关代码。

**Step 4: 全量编译 + 测试**

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1
```

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(tui): remove old 12-Tab screen architecture, clean up unused code"
```

---

## Phase T3: 调试浮层 + 完善

目标：将有价值的调试信息（Eval、Agent inspector）改为浮层 modal，补齐边缘功能。

---

### Task T3-1: 实现 Agent 调试浮层 (Ctrl+D)

**Files:**
- Create: `crates/octo-cli/src/tui/overlays/mod.rs`
- Create: `crates/octo-cli/src/tui/overlays/agent_debug.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: 从保留的 dev_agent.rs 提取核心内容**

保留三栏布局（sessions | conversation | inspector），但改为 85% 宽 × 80% 高的居中浮层。

**Step 2: 真实数据接入**

- Session 列表: `session_store.list_sessions()`
- 对话历史: `session_store.get_messages(&session_id)`
- Context usage: 从 `AgentEvent::TokenBudgetUpdate` 获取
- Tool history: 从 `AgentEvent::ToolExecution` 累积

**Step 3: Ctrl+D 切换显隐**

```rust
(KeyModifiers::CONTROL, KeyCode::Char('d')) => {
    state.show_debug_overlay = !state.show_debug_overlay;
}
```

**Step 4: cargo check**

**Step 5: Commit**

```bash
git add -A && git commit -m "feat(tui): implement agent debug overlay (Ctrl+D) with real engine data"
```

---

### Task T3-2: 实现 Eval 浮层 (Ctrl+E)

**Files:**
- Create: `crates/octo-cli/src/tui/overlays/eval.rs`
- Modify: `crates/octo-cli/src/tui/overlays/mod.rs`

**Step 1: 从保留的 dev_eval.rs 提取核心内容**

保留三栏布局（run history | task results | timeline），改为浮层。

**Step 2: RunStore 集成已有**

dev_eval.rs 已有 RunStore 集成，直接复用。

**Step 3: Ctrl+E 切换显隐**

**Step 4: cargo check + commit**

```bash
git add -A && git commit -m "feat(tui): implement eval results overlay (Ctrl+E) with RunStore integration"
```

---

### Task T3-3: 实现 Session/Agent 选择器浮层 (Ctrl+A)

**Files:**
- Create: `crates/octo-cli/src/tui/overlays/session_picker.rs`
- Modify: `crates/octo-cli/src/tui/overlays/mod.rs`

**Step 1: 参考 opendev 的 session_picker controller**

**Step 2: 实现选择器**

列出所有 session，选中后切换 AgentExecutor：
```rust
// 切换 session
let new_handle = app_state.agent_runtime.start_primary(
    selected_session_id, user_id, sandbox_id, history, None
).await;
state.switch_session(new_handle);
```

**Step 3: Ctrl+A 切换显隐**

**Step 4: cargo check + commit**

```bash
git add -A && git commit -m "feat(tui): implement session/agent picker overlay (Ctrl+A)"
```

---

### Task T3-4: 移植 welcome panel 动画

**Files:**
- Read: `3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/src/widgets/welcome_panel/`
- Create: `crates/octo-cli/src/tui/widgets/welcome_panel.rs`

**Step 1: 简化移植 welcome panel**

opendev 有渐隐动画。移植核心 ASCII art + 版本信息，在首次对话开始时隐藏。

**Step 2: cargo check + commit**

```bash
git add -A && git commit -m "feat(tui): port welcome panel with ASCII art from opendev"
```

---

### Task T3-5: ThinkingDelta 渲染 + ToolProgress 集成

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs` (handle_agent_event)
- Modify: `crates/octo-cli/src/tui/widgets/conversation/mod.rs`

**Step 1: 处理 ThinkingDelta/ThinkingComplete**

在对话区用 dimmed 样式渲染思考内容。

**Step 2: 处理 ToolProgress**

用 progress widget 显示长时工具的进度条。

**Step 3: cargo check + commit**

```bash
git add -A && git commit -m "feat(tui): add ThinkingDelta rendering and ToolProgress display"
```

---

### Task T3-6: 全量测试 + 合并 theme + 最终验证

**Files:**
- Modify: `crates/octo-cli/src/tui/theme.rs` — 合并 opendev style_tokens
- Modify: `crates/octo-cli/Cargo.toml` — 确保所有新依赖

**Step 1: 合并 theme**

将 opendev 的 style_tokens 颜色常量合入现有 TuiTheme。

**Step 2: 检查 Cargo.toml 依赖**

确认以下依赖存在：
- `unicode-width` (字符宽度)
- `ignore` (gitignore-aware file traversal, for autocomplete)
- `dirs` (home directory, for path_shortener)
- `crossterm` with `event-stream` feature

**Step 3: 全量编译 + 测试**

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1
```

**Step 4: 手动验证清单**

- [ ] `cargo run -p octo-cli -- tui` 启动对话界面
- [ ] 输入文本并按 Enter 发送，收到 agent 回复
- [ ] 工具执行显示 spinner + 结果
- [ ] Ctrl+C 取消当前操作
- [ ] 双击 Ctrl+C 退出 TUI
- [ ] Ctrl+D 打开/关闭调试浮层
- [ ] Ctrl+E 打开/关闭 Eval 浮层
- [ ] 上下箭头滚动对话历史
- [ ] `/` 触发 autocomplete
- [ ] 小终端 (80×24) 不 panic

**Step 5: Final commit**

```bash
git add -A && git commit -m "feat(tui): complete Phase T — conversation-centric TUI with full engine integration"
```

---

## 回退策略

整个开发在 `feat/tui-opendev-integration` 分支进行：

- **Phase 级回退**: `git log --oneline` 找到 Phase 末尾 commit，`git reset --hard <hash>`
- **Task 级回退**: 每个 Task 都有 commit，可精确回退
- **全面回退**: `git checkout main` 回到原始状态，旧 TUI 完全不受影响
- **部分合并**: 如果 T1 成功但 T2 有问题，可以只保留 formatters/managers/widgets（它们是独立模块）

---

## 工作量估算

| Phase | Tasks | 预计时间 |
|-------|-------|----------|
| T1: 基础设施移植 | 10 tasks | 2-3 天 |
| T2: 对话中心主界面 | 8 tasks | 3-4 天 |
| T3: 调试浮层 + 完善 | 6 tasks | 2-3 天 |
| **合计** | **24 tasks** | **7-10 天** |

---

## 依赖检查清单

| 依赖 | 用途 | 当前状态 |
|------|------|----------|
| `ratatui` | TUI 框架 | ✅ 已有 |
| `crossterm` (event-stream) | 终端 + 事件流 | 检查 feature flag |
| `unicode-width` | 字符宽度计算 | 可能需要添加 |
| `ignore` | gitignore-aware 遍历 | 可能需要添加 |
| `dirs` | Home 目录检测 | 可能需要添加 |
| `tokio` (full) | 异步运行时 | ✅ 已有 |
| `futures-util` | StreamExt | ✅ 已有 |
