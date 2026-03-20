# TUI OpenDev 整合设计文档

> 创建日期: 2026-03-20
> 状态: 待实施
> 分支: `feat/tui-opendev-integration`

---

## 一、背景与动机

### 1.1 现状问题

octo-cli 当前 TUI (`crates/octo-cli/src/tui/`) 存在以下问题：

1. **多屏架构不适合终端**：12 个 Tab + Ops/Dev 双模式，三栏布局在非全屏终端下无法正常使用
2. **大量 stub/mock**：Chat 屏为 echo mock，Sessions/Memory/MCP/Tools 等屏幕数据未接入
3. **无真实 LLM 交互**：TUI 无法发送消息给 agent 并接收流式回复
4. **与 REPL 重复**：`cli-run` 已能正常工作，但 TUI 无法使用相同路径

### 1.2 opendev TUI 的优势

`3th-party/harnesses/rust-projects/opendev/crates/opendev-tui/` 提供了一个生产级 TUI：

- **对话中心布局**：纯垂直堆叠（对话 + 输入 + 状态栏），在 80×24 小终端也能正常工作
- **完整交互特性**：autocomplete、markdown 渲染、tool approval dialog、diff viewer、spinner、streaming chunk、subagent 嵌套显示
- **性能优化**：dirty flag、line cache、event batching
- **丰富的 widget 库**：30+ 文件的专业 UI 组件

### 1.3 决策

将 opendev TUI 的 widget/formatter/event 基础设施移植进 octo-cli，在此之上重建对话中心界面，直接对接 octo-engine 的 `AgentExecutorHandle`。

---

## 二、核心设计决策

### 2.1 类型统一——直接使用 octo 类型，零适配层

**决策**：不引入 opendev 的类型系统（`DisplayMessage`、`DisplayRole`、`DisplayToolCall`），TUI 层直接使用 octo 类型。

**理由**：

| 方面 | opendev 类型 | octo 类型 | 选择 |
|------|-------------|----------|------|
| 消息内容 | `content: String` | `content: Vec<ContentBlock>` | octo（多模态，Anthropic 对齐） |
| 工具模型 | `Vec<ToolCall>` 内嵌 message | `ContentBlock::ToolUse/ToolResult` | octo（标准做法） |
| 事件流 | 35+ AppEvent（UI+Agent 混合） | `AgentEvent` 15 变种（分层） | octo（已分层设计） |
| 审批 | `oneshot::Sender<ToolApprovalDecision>` | `ApprovalGate`（register/respond） | octo（已集成） |
| 中断 | `InterruptToken`（flag + background） | `CancellationToken` + `AgentMessage::Cancel` | octo（够用） |

**实施方式**：移植 opendev widget 时，改写其内部渲染逻辑，将 `DisplayMessage` 替换为 `ChatMessage`，将 `String` 内容替换为 `Vec<ContentBlock>` 的 match 分支。

### 2.2 布局策略——对话中心 + 浮层调试

**决策**：废弃 12-Tab 多屏架构，改为 opendev 风格的垂直堆叠。

```
主界面（永驻）：
┌─────────────────────────────────┐
│ 对话区 (Constraint::Min(5))     │ scrollable, markdown, inline tools
├─────────────────────────────────┤
│ 进度面板 (0-8 行, 可折叠)       │ 仅在工具执行时显示
├─────────────────────────────────┤
│ 输入区 (1-8 行, 多行)           │ autocomplete, Shift+Enter
├─────────────────────────────────┤
│ 状态栏 (2 行固定)               │ model, tokens, session, MCP
└─────────────────────────────────┘

浮层 Modal（按需唤起，85%宽×80%高居中）：
  Ctrl+D → Agent 调试面板（sessions | conversation | inspector）
  Ctrl+E → Eval 结果面板（run history | tasks | timeline）
  Ctrl+A → Session/Agent 选择器
  Ctrl+P → 后台任务管理
  Y/N    → Tool Approval 弹窗（agent 主动触发）
```

**理由**：
- 终端不总是全屏，对话是最高频操作
- 调试信息低频查看，适合浮层按需唤起
- 80×24 小终端对话区仍有 Min(5) 行可用

### 2.3 Engine 对接——与 CLI REPL 共用路径

**决策**：TUI 使用与 REPL 完全相同的 engine API，不增加任何中间层。

```
AppState::new() → AgentRuntime
    → runtime.start_primary(session_id, ...) → AgentExecutorHandle
        → handle.send(AgentMessage::UserMessage { content, channel_id: "tui" })
        → handle.subscribe() → broadcast::Receiver<AgentEvent>
        → approval_gate.respond(&tool_id, approved)
        → handle.send(AgentMessage::Cancel)
```

**数据流**：

```
用户键入 → InputWidget → AppEvent::UserSubmit(text)
    → handle.send(AgentMessage::UserMessage)
    → AgentExecutor 处理
    → broadcast: AgentEvent::TextDelta / ToolStart / ToolResult / ...
    → EventHandler 桥接为 AppEvent::Agent(event)
    → handle_agent_event() 更新 TuiState
    → dirty flag → 下一帧渲染
```

### 2.4 事件系统设计

**决策**：AppEvent 直接包装 `AgentEvent`，不做拆解。

```rust
pub enum AppEvent {
    Key(KeyEvent),           // 键盘
    Resize(u16, u16),        // 终端尺寸变化
    Tick,                    // 动画帧
    Agent(AgentEvent),       // Agent 生命周期事件（直接包装）
    UserSubmit(String),      // 用户提交输入
    Quit,                    // 退出
}
```

**EventHandler** 使用三个 tokio 任务合并三个事件源：
1. `crossterm::event::EventStream` → Key/Resize
2. `broadcast::Receiver<AgentEvent>` → Agent
3. `tokio::time::interval` → Tick

全部汇入 `mpsc::unbounded_channel<AppEvent>`，主循环从中消费。

### 2.5 渲染优化

移植 opendev 的三个关键优化：

1. **Dirty Flag**：只在状态变化时重绘，`state.dirty = true` 设置、帧末清除
2. **Line Cache**：对话区的 `Vec<Line<'static>>` 预渲染，仅在 `message_generation` 变化时重建
3. **Event Batching**：每帧前 `try_next()` 排空所有排队事件，避免打字时每个字符一帧

---

## 三、模块架构

### 3.1 新目录结构

```
crates/octo-cli/src/tui/
├── mod.rs              # 入口、run_tui()、主循环
├── app_state.rs        # TuiState 定义
├── event.rs            # AppEvent enum
├── event_handler.rs    # 合并 crossterm + AgentEvent
├── render.rs           # 主布局渲染
├── key_handler.rs      # 键盘事件处理
├── cache.rs            # Line cache 管理
├── theme.rs            # TuiTheme（保留+合并 style_tokens）
├── widgets/
│   ├── mod.rs
│   ├── conversation/   # 对话渲染（ContentBlock → Lines）
│   │   ├── mod.rs
│   │   ├── tool_format.rs
│   │   └── spinner.rs
│   ├── input.rs        # 多行输入
│   ├── spinner.rs      # 动画帧
│   ├── progress.rs     # 进度条
│   ├── status_bar.rs   # 底部状态栏
│   ├── toast.rs        # 通知气泡
│   └── welcome_panel.rs # 启动画面
├── formatters/
│   ├── mod.rs
│   ├── markdown.rs     # Markdown → Ratatui Lines
│   ├── wrap.rs         # 自动换行
│   ├── path_shortener.rs # ~/... 缩写
│   ├── tool_registry.rs  # 工具分类/颜色
│   ├── bash_formatter.rs # Shell 输出
│   ├── file_formatter.rs # 文件内容
│   └── diff.rs         # Unified diff
├── autocomplete/
│   ├── mod.rs          # AutocompleteEngine
│   ├── strategies.rs   # 补全算法
│   ├── completers.rs   # Completer trait
│   ├── file_finder.rs  # gitignore-aware 文件搜索
│   └── formatters.rs   # 建议格式化
├── controllers/
│   ├── mod.rs
│   └── approval.rs     # Tool approval dialog
├── overlays/
│   ├── mod.rs
│   ├── agent_debug.rs  # Ctrl+D 调试浮层
│   ├── eval.rs         # Ctrl+E 评估浮层
│   └── session_picker.rs # Ctrl+A 选择器
└── managers/
    ├── mod.rs
    ├── spinner.rs      # 集中动画服务
    ├── message_history.rs # 命令历史
    ├── interrupt.rs    # Ctrl+C 中断管理
    └── display_ledger.rs # 消息去重
```

### 3.2 数据流架构图

```
┌─────────────┐    AgentMessage    ┌──────────────────┐
│  TUI Input  │ ──────────────────►│  AgentExecutor   │
│  (keyboard) │                    │  (tokio task)     │
└─────────────┘                    └────────┬─────────┘
                                            │ broadcast
┌─────────────┐    AppEvent::Agent  ┌───────▼─────────┐
│  TuiState   │ ◄──────────────────│  EventHandler   │
│  (model)    │                    │  (merge 3 src)   │
└──────┬──────┘                    └──────────────────┘
       │ dirty flag
┌──────▼──────┐
│  render()   │
│  (view)     │
└─────────────┘
```

---

## 四、快捷键映射

| 快捷键 | 功能 | 上下文 |
|--------|------|--------|
| Enter | 发送输入 | 输入区有内容时 |
| Shift+Enter | 换行 | 输入区（需 Kitty 协议） |
| Ctrl+C | 取消当前操作 / 双击退出 | 全局 |
| Ctrl+D | 切换调试浮层 | 全局 |
| Ctrl+E | 切换 Eval 浮层 | 全局 |
| Ctrl+A | 切换 Session 选择器 | 全局 |
| Ctrl+L | 清屏 | 全局 |
| Up/Down | 滚动对话 | 输入为空时 |
| PageUp/PageDown | 大幅滚动 | 全局 |
| Tab | 补全选择 | autocomplete 激活时 |
| Esc | 关闭浮层/autocomplete | 浮层打开时 |
| Y/N/A | 审批/拒绝/总是允许 | Tool approval 弹窗 |
| / | 触发斜杠命令补全 | 输入行首 |
| @ | 触发文件引用补全 | 输入中 |

---

## 五、与现有系统的兼容性

### 5.1 CLI REPL 不受影响

`octo run` 继续使用 rustyline REPL + `ui/streaming.rs` 的 print-based 渲染。
`octo tui` 启动新的对话中心 TUI。两者共用同一个 `AgentRuntime` 和 `start_primary()` API。

### 5.2 Web Server 不受影响

`octo-server` 的 WebSocket handler 同样使用 `AgentExecutorHandle.subscribe()`，三个消费者（REPL、TUI、WebSocket）共用同一套事件模型。

### 5.3 测试影响

- TUI 模块为纯渲染代码，不涉及 engine 核心逻辑
- 现有 2250 测试不应受影响
- 新增测试主要覆盖 formatters 和 managers（纯函数，易测试）

---

## 六、回退策略

1. **Feature branch 隔离**：所有工作在 `feat/tui-opendev-integration` 分支
2. **每 Task 一个 commit**：可精确回退到任意 Task
3. **旧代码在 main 不变**：`git checkout main` 即可恢复
4. **渐进合并**：如果 Phase T1（基础设施）成功但 T2（主界面）有问题，可只保留 T1 的 formatters/managers/widgets 作为公共库

---

## 七、不做的事情

1. **不引入 opendev 的 opendev-runtime / opendev-agents / opendev-models crate**——只移植 TUI 层代码
2. **不实现 undo/redo**——opendev 有此功能但 octo 暂不需要
3. **不实现 background agent pool**——opendev 的多后台 agent 管理，octo 当前是单 session 模式
4. **不做 model picker UI**——通过 `/model` 命令切换即可
5. **不做 MCP server 管理 UI**——通过 `/mcp` 命令管理
