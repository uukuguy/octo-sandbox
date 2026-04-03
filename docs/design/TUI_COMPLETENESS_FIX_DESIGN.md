# TUI 完整性修复设计

> 基于 2026-04-03 全面审计，修复 octo-cli TUI 在斜杠命令、状态栏显示、帮助系统、快捷键可发现性方面的 15+ 缺口。
> 根因：REPL (repl/slash.rs) 和 TUI (tui/key_handler.rs) 是两套独立系统，功能未统一。

---

## 一、问题清单

### 1.1 斜杠命令 — 文档声称可用但实际不可用

| 命令 | /help 列出 | TUI 实际 | 问题 |
|------|-----------|---------|------|
| `/compact` | ✅ | ❌ Unknown command | 仅 REPL 实现 |
| `/cost` | ✅ | ❌ Unknown command | 仅 REPL 实现 |
| `/model` | ✅ | ❌ stub | "coming in Phase 5" |
| `/mode` | ✅ | ❌ Unknown command | 仅 REPL 实现 |
| `/theme` | ✅ | ❌ Unknown command | 仅 REPL 实现 |
| `/vim` | ❌ | ❌ 不存在 | VimState 完整但无入口 |

### 1.2 快捷键 — 已接线但 /help 未提及

| 快捷键 | 功能 | /help 提及 |
|--------|------|-----------|
| `Ctrl+R` | 历史反向搜索 | ❌ |
| `Ctrl+D` | 调试面板 | ❌ |
| `Ctrl+E` | 评估面板 / 行尾 | ❌ |
| `Ctrl+A` | 会话选择 / 行首 | ❌ |
| `Ctrl+X` | 外部编辑器 ($EDITOR) | ❌ |
| `Alt+P` | 模型选择器弹窗 | ❌ |
| `Shift+Tab` | 权限模式循环 | ❌ |
| `Shift+Enter` | 多行输入 | ❌ |
| `Home` / `End` | 跳转顶部/底部 | ❌ |
| `PageUp` / `PageDown` | 翻页滚动 | ❌ |

### 1.3 状态栏 — 有状态对象但无显示

| 功能 | 状态对象 | 渲染到状态栏 | 用户可切换 |
|------|---------|-------------|-----------|
| Vim 模式 | VimState (Normal/Insert/Visual) | ❌ | ❌ 无入口 |
| 权限模式 | PermissionMode (ReadOnly/Supervised/Full) | ❌ | ✅ Shift+Tab 但不可见 |
| Reduced Motion | ReducedMotion | ❌ | ❌ 无入口 |
| 模型选择 | ModelSelectorState | ✅ 模型名 | ⚠️ Alt+P 可选但不通知后端 |
| Effort Level | effort_level | ✅ 有显示 | ❌ 只读 |

### 1.4 功能断链

- **模型选择器 (Alt+P)**：用户选择新模型 → `state.model_name` 更新 → 但 agent 后端不知道 → 实际 LLM 调用仍用旧模型
- **权限模式 (Shift+Tab)**：循环切换生效 → 但状态栏不显示 → 用户不知道当前处于哪个模式
- **Vim 模式**：完整实现 Normal/Insert/Visual → 但没有 `/vim` 命令或快捷键开启

---

## 二、修复方案

### Wave 1：帮助文本 + 状态栏（~150 行）

#### W1-T1：修正 /help 内容

**文件**: `tui/key_handler.rs` (help 文本)

修改原则：
- 删除未实现的命令声明（`/compact`, `/cost`, `/model`, `/mode`, `/theme`）
- 补全所有已接线的快捷键
- 分类展示：命令 → 面板 → 快捷键 → 模式切换

```
Available commands:
  /help       — Show this help
  /clear      — Clear conversation history
  /exit       — Exit the session (/quit, /q)
  /mouse      — Toggle mouse capture (off = native text selection)
  /vim        — Toggle vim keybinding mode
  /theme <n>  — Change color theme (cyan/blue/violet/emerald/amber/rose)

Panels (toggle):
  /debug      — Agent debug panel                    Ctrl+D
  /eval       — Evaluation panel                     Ctrl+E
  /sessions   — Session picker                       Ctrl+A

Navigation:
  Shift+Enter — Insert newline (multiline input)
  Ctrl+R      — Reverse history search
  Ctrl+X      — Open $EDITOR for long input
  Home / End  — Jump to top / bottom
  PgUp / PgDn — Page scroll

Mode switching:
  Alt+P       — Model selector popup
  Shift+Tab   — Cycle permission mode (ReadOnly → Supervised → Full)

Tool results:
  Ctrl+O      — Cycle tool results (expand one at a time)
  Alt+O       — Toggle ALL tool results

Clipboard:
  Ctrl+Y      — Copy last assistant response
```

**~50 行修改**

#### W1-T2：状态栏新增 vim 模式 + 权限模式标签

**文件**: `tui/widgets/status_bar.rs`

在 Row 1（信息行）末尾追加两个 Span：

```rust
// Vim 模式标签（仅当 enabled 时显示）
if state.vim.enabled {
    let (label, color) = state.vim.mode.label_and_color();
    // 显示 "[NORMAL]" / "[INSERT]" / "[VISUAL]"
    spans.push(Span::styled(format!(" [{}]", label), Style::default().fg(color)));
}

// 权限模式标签（始终显示）
let pm = &state.permission_mode;
spans.push(Span::styled(
    format!(" {} {}", figures::DIAMOND_FILLED, pm.label()),
    Style::default().fg(pm.color_rgb()),
));
```

**~30 行修改**

#### W1-T3：上下文感知快捷键提示完善

**文件**: `tui/widgets/figures.rs` — hotkey_hints() 函数

增加空闲状态的常驻提示：

```rust
fn hotkey_hints(is_streaming: bool, has_overlay: bool, has_approval: bool) -> Vec<(&'static str, &'static str)> {
    if has_approval {
        vec![("Y", "accept"), ("N", "deny"), ("A", "always")]
    } else if has_overlay {
        vec![("Esc", "close")]
    } else if is_streaming {
        vec![("Esc", "stop")]
    } else {
        // 空闲状态：显示可发现性提示
        vec![
            ("/help", "commands"),
            ("Alt+P", "model"),
            ("Ctrl+R", "search"),
        ]
    }
}
```

**~20 行修改**

---

### Wave 2：斜杠命令迁移 + 功能接线（~200 行）

#### W2-T1：/vim 命令

**文件**: `tui/key_handler.rs`

```rust
"/vim" => {
    state.vim.enabled = !state.vim.enabled;
    if state.vim.enabled {
        state.vim.mode = VimMode::Normal;
        push_system_msg("Vim mode enabled. Press 'i' to enter Insert mode.");
    } else {
        push_system_msg("Vim mode disabled.");
    }
}
```

**~15 行**

#### W2-T2：/theme 命令迁移

**文件**: `tui/key_handler.rs`

```rust
cmd if cmd.starts_with("/theme") => {
    let arg = cmd.strip_prefix("/theme").unwrap().trim();
    if arg.is_empty() {
        let themes = ["cyan", "blue", "violet", "emerald", "amber", "rose", "orange", "pink", "teal", "indigo", "slate", "neutral"];
        push_system_msg(&format!("Available themes: {}. Current: {}", themes.join(", "), state.theme_name));
    } else {
        state.set_theme(arg);
        push_system_msg(&format!("Theme changed to: {}", arg));
    }
}
```

**~20 行**

#### W2-T3：/compact 命令（触发上下文压缩）

**文件**: `tui/key_handler.rs`

发送特殊消息给 agent，触发上下文压缩。

```rust
"/compact" => {
    // 注入系统指令触发压缩
    push_system_msg("Compacting conversation context...");
    // 实际压缩由 harness 的 compaction_pipeline 处理
    // TUI 端发送 /compact 作为用户消息，harness 识别并执行
}
```

**~15 行**

#### W2-T4：/cost 命令（显示 token 使用）

**文件**: `tui/key_handler.rs`

从 state 中读取已有的 token 统计并显示。

```rust
"/cost" => {
    let msg = format!(
        "Token usage this session:\n  Input:  {}k\n  Output: {}k\n  Total:  {}k",
        state.session_input_tokens / 1000,
        state.session_output_tokens / 1000,
        (state.session_input_tokens + state.session_output_tokens) / 1000,
    );
    push_system_msg(&msg);
}
```

**~15 行**

#### W2-T5：/model 命令（调用模型选择器）

**文件**: `tui/key_handler.rs`

```rust
"/model" => {
    // 直接打开模型选择器弹窗（复用 Alt+P 逻辑）
    state.model_selector.visible = !state.model_selector.visible;
}
```

**~5 行**

#### W2-T6：模型选择器确认后通知后端

**文件**: `tui/key_handler.rs` — model selector Enter 处理

当前代码只更新 `state.model_name`，需要额外：

```rust
// 通知后端切换模型
if let Some(ref tx) = state.agent_tx {
    let _ = tx.send(AgentMessage::UserMessage {
        content: format!("/model {}", selected_model),
        channel_id: "tui".into(),
    });
}
```

**~10 行**

#### W2-T7：/mode 命令

**文件**: `tui/key_handler.rs`

```rust
"/mode" => {
    // 切换 plan mode / normal mode
    state.plan_mode = !state.plan_mode;
    let mode = if state.plan_mode { "plan" } else { "normal" };
    push_system_msg(&format!("Switched to {} mode", mode));
}
```

**~10 行**

---

### Wave 3：可发现性增强（~100 行）

#### W3-T1：快捷键速查卡（/keys 命令）

新增 `/keys` 命令，显示所有快捷键分类速查：

```
Keyboard Quick Reference:

── Input ──
  Enter           Send message
  Shift+Enter     Newline
  Ctrl+X          Open $EDITOR
  Tab             Accept autocomplete
  Esc             Cancel / Clear

── Navigation ──
  Up/Down         History / Scroll
  Home/End        Jump top/bottom
  PgUp/PgDn       Page scroll
  Ctrl+R          History search

── Panels ──
  Ctrl+D          Debug panel
  Ctrl+E          Eval panel
  Ctrl+A          Session picker

── Mode ──
  Alt+P           Model selector
  Shift+Tab       Permission mode
  /vim            Toggle vim mode

── Tools ──
  Ctrl+O          Cycle tool results
  Alt+O           Toggle all tools
  Ctrl+Y          Copy last response
```

**~60 行**

#### W3-T2：首次使用提示

当 TUI 启动且没有历史记录时，显示简短提示：

```
Tip: Type /help for commands, /keys for shortcuts, Alt+P to switch model
```

**~15 行**

#### W3-T3：权限模式切换视觉反馈

Shift+Tab 切换后，在对话区临时显示切换确认：

```rust
// Shift+Tab handler
let next = state.permission_mode.next();
state.permission_mode = next;
push_system_msg(&format!("Permission mode: {}", next.label()));
```

**~10 行**

---

## 三、文件变更矩阵

| 文件 | W1 | W2 | W3 | 变更类型 |
|------|----|----|----|----|
| `tui/key_handler.rs` | T1 | T1-T7 | T1,T3 | help 文本 + 命令处理 |
| `tui/widgets/status_bar.rs` | T2 | | | +vim/permission 标签 |
| `tui/widgets/figures.rs` | T3 | | | hotkey_hints 完善 |
| `tui/render.rs` | | | T2 | 首次使用提示 |

---

## 四、代码量估算

| Wave | 任务数 | 新增行 | 修改行 |
|------|--------|--------|--------|
| W1 帮助+状态栏 | 3 | ~60 | ~40 |
| W2 命令迁移 | 7 | ~80 | ~30 |
| W3 可发现性 | 3 | ~80 | ~5 |
| **合计** | **13** | **~220** | **~75** |
| **总计** | | | **~295 行** |

---

## 五、验证标准

### 编译验证
```bash
cargo check -p octo-cli
```

### 功能验证点

| 验证项 | 验证方式 |
|--------|---------|
| /help 显示准确 | 运行 `octo tui` → `/help` |
| 无 Unknown command | 逐个测试 /vim /theme /cost /compact /model /mode /keys |
| 状态栏 vim 标签 | `/vim` → 状态栏显示 [NORMAL] |
| 状态栏权限标签 | Shift+Tab → 状态栏显示 ◆ Supervised |
| 模型切换生效 | Alt+P → 选择 → 验证下次 LLM 调用用新模型 |
| 空闲提示可见 | 无输入时底栏显示 /help · Alt+P · Ctrl+R |
| /keys 速查卡 | `/keys` 显示完整快捷键列表 |
