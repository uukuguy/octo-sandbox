# Phase BC — TUI Deferred Items 补齐 实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 补齐 Phase BB 和 AQ 中暂缓的 TUI 增强项：formatters 全量主题化、消息间距增强、状态栏响应式布局。

**Architecture:** 三步走 — (1) 将 `&TuiTheme` 线索贯穿所有 formatter 和 widget，替换 128 处 `style_tokens::*` 硬编码引用；(2) 增强 conversation 消息间距和角色分隔线；(3) 状态栏根据终端宽度渐进式披露信息。

**Tech Stack:** Rust + ratatui 0.29, 无新依赖。

**参考文档:**
- `docs/design/Grid/GRID_UI_UX_DESIGN.md` — 色彩/排版规范
- `crates/grid-cli/src/tui/theme.rs` — TuiTheme 结构

**基线:** 499 studio tests pass @ commit `e4e3246`, branch `Grid`
**完成:** 499 studio tests pass @ commit `41a0fcf`, 5/5 tasks done

**来源 Deferred Items:**
- BB-D1: 消息间距增强 + 角色分隔线 → BC-4
- BB-D2: 状态栏渐进式披露 → BC-5
- BB-D4: Conversation formatters 全量主题化 → BC-1~BC-3

**不含:**
- BB-D3 (Welcome 边框) — 双线边框已是现代风格，无具体设计方向
- AQ-D1 (InteractionGate TUI) — 涉及新对话框类型 + 事件循环接入，复杂度高，独立 phase

---

## Wave 1: Formatters 全量主题化（BB-D4, 3 tasks）

核心策略：给 `MdPalette` 和各 formatter 添加 `from_theme(&TuiTheme)` 构造器，使 `ConversationWidget` 可将主题传入，同时保留 `Default` 实现不破坏现有调用链。

### Task BC-1: MdPalette + MarkdownRenderer 主题化

**Files:**
- Modify: `crates/grid-cli/src/tui/formatters/markdown.rs`
- Modify: `crates/grid-cli/src/tui/theme.rs` (新增 diff/tool 相关色值到 TuiTheme)

**目标:** `MdPalette` 新增 `from_theme()` 方法从 TuiTheme 取色，`MarkdownRenderer` 新增 `render_themed()` 接受 `&MdPalette`。

**实现:**

1. 在 `theme.rs` 的 `TuiTheme` struct 中新增 diff 和 tool 相关色值：

```rust
// 在 md_bullet 之后新增
pub md_link: Color,
pub diff_add_bg: Color,
pub diff_del_bg: Color,
pub tool_icon: Color,        // 工具图标色 (替代 BLUE_BRIGHT/ORANGE)
pub thinking_bg: Color,      // 思考区域背景
```

2. 在 `from_cli_theme()` 中初始化这些字段：

```rust
md_link: Color::Rgb(74, 158, 255),    // was BLUE_BRIGHT
diff_add_bg: Color::Rgb(10, 35, 25),
diff_del_bg: Color::Rgb(40, 15, 15),
tool_icon: accent,                     // follow accent
thinking_bg: Color::Rgb(78, 81, 88),
```

3. 在 `markdown.rs` 给 `MdPalette` 添加：

```rust
impl MdPalette {
    /// Construct from a TuiTheme — all colors follow the theme.
    pub fn from_theme(theme: &crate::tui::theme::TuiTheme) -> Self {
        Self {
            heading: theme.md_heading,
            code_fg: theme.md_code_fg,
            code_bg: theme.md_code_bg,
            bullet: theme.md_bullet,
            bold_fg: theme.md_bold,
            link: theme.md_link,
            text: theme.text,
            base_modifier: Modifier::empty(),
        }
    }

    /// A muted variant for thinking/reasoning display.
    pub fn muted_from_theme(theme: &crate::tui::theme::TuiTheme) -> Self {
        Self {
            heading: theme.text_secondary,
            code_fg: theme.text_secondary,
            code_bg: theme.surface_1,
            bullet: theme.text_faint,
            bold_fg: theme.text_secondary,
            link: theme.text_secondary,
            text: theme.text_secondary,
            base_modifier: Modifier::ITALIC,
        }
    }
}
```

4. `MarkdownRenderer` 新增 `render_themed()` 方法：

```rust
impl MarkdownRenderer {
    /// Render markdown with a themed palette.
    pub fn render_themed(text: &str, palette: &MdPalette) -> Vec<Line<'static>> {
        // 与现有 render() 相同逻辑，但使用传入的 palette 而非 MdPalette::default()
        Self::render_with_palette(text, palette)
    }
}
```

如果现有 `render()` 内部已经用 `MdPalette::default()`，则重构为 `render()` 调用 `render_themed(&MdPalette::default())`。

---

### Task BC-2: ConversationWidget 接受 TuiTheme + 替换所有 style_tokens 引用

**Files:**
- Modify: `crates/grid-cli/src/tui/widgets/conversation/mod.rs` (~25 处)
- Modify: `crates/grid-cli/src/tui/widgets/conversation/spinner.rs` (~4 处)
- Modify: `crates/grid-cli/src/tui/widgets/conversation/tool_format.rs` (~3 处)
- Modify: `crates/grid-cli/src/tui/render.rs` (传递 theme)

**目标:** `ConversationWidget` 新增 `.theme()` builder 方法，所有内部 `style_tokens::*` 引用替换为 `self.theme.xxx`。

**实现:**

1. `ConversationWidget` struct 新增字段：

```rust
pub struct ConversationWidget<'a> {
    // ... existing fields ...
    theme: &'a crate::tui::theme::TuiTheme,
}
```

2. 构造器中使用 `lazy_static` 或 `thread_local` 的默认 theme 做 fallback（避免 breaking change）：

```rust
// 使用 const 或 static 的默认 theme
static DEFAULT_THEME: std::sync::LazyLock<crate::tui::theme::TuiTheme> =
    std::sync::LazyLock::new(crate::tui::theme::TuiTheme::default);

impl<'a> ConversationWidget<'a> {
    pub fn new(messages: &'a [ChatMessage], scroll_offset: u16) -> Self {
        Self {
            // ... existing ...
            theme: &DEFAULT_THEME,
        }
    }

    pub fn theme(mut self, theme: &'a crate::tui::theme::TuiTheme) -> Self {
        self.theme = theme;
        self
    }
}
```

3. 替换 `build_user_lines` 中的 style_tokens（约 6 处）：
   - `style_tokens::BLUE_BRIGHT` → `self.theme.info` (user role color)
   - `style_tokens::PRIMARY` → `self.theme.text`
   - `style_tokens::ACCENT` → `self.theme.accent`
   - `style_tokens::GREY` → `self.theme.text_faint`
   - `style_tokens::SUBTLE` → `self.theme.text_secondary`

4. 替换 `build_assistant_lines` 中的 style_tokens（约 6 处）：
   - `style_tokens::SUBTLE` → `self.theme.text_secondary`
   - `style_tokens::GREY` → `self.theme.text_faint`
   - `style_tokens::GREEN_BRIGHT` → `self.theme.success`

5. 替换 `build_tool_result_lines` 中的 style_tokens（约 8 处）：
   - `style_tokens::BORDER` → `self.theme.border`
   - `style_tokens::SUBTLE` → `self.theme.text_secondary`
   - `style_tokens::ERROR` → `self.theme.error`
   - `style_tokens::GREY` → `self.theme.text_faint`
   - `style_tokens::CONTINUATION_CHAR` 保持不变（字符常量不是颜色）

6. 替换 `build_system_lines` 中的 style_tokens（约 1 处）：
   - `style_tokens::GREY` → `self.theme.text_faint`

7. 用 `MdPalette::from_theme()` 替换 `MarkdownRenderer::render()` 调用：
   - `MarkdownRenderer::render(&cleaned)` → `MarkdownRenderer::render_themed(&cleaned, &MdPalette::from_theme(self.theme))`

8. `spinner.rs` 和 `tool_format.rs`：
   - 给函数签名加 `theme: &TuiTheme` 参数
   - 替换 `style_tokens::*` 为 theme 字段
   - 调用点传入 `self.theme`

9. `render.rs` 中传递 theme：

```rust
let conversation = super::widgets::conversation::ConversationWidget::new(
    &messages,
    state.scroll_offset,
)
.theme(&state.theme)  // NEW
.active_tools(&state.active_tools, spinner_char)
.formatter_registry(&state.tool_formatter_registry)
.collapse_state(collapse);
```

---

### Task BC-3: 其余 formatter 文件主题化 + StatusBar/Input/Progress/TodoPanel 残留清理

**Files:**
- Modify: `crates/grid-cli/src/tui/formatters/bash_formatter.rs` (~6 处)
- Modify: `crates/grid-cli/src/tui/formatters/file_formatter.rs` (~10 处)
- Modify: `crates/grid-cli/src/tui/formatters/formatter_registry.rs` (~6 处)
- Modify: `crates/grid-cli/src/tui/formatters/diff.rs` (~5 处)
- Modify: `crates/grid-cli/src/tui/formatters/tool_registry.rs` (~2 处)
- Modify: `crates/grid-cli/src/tui/formatters/wrap.rs` (~1 处)
- Modify: `crates/grid-cli/src/tui/widgets/status_bar.rs` (~24 处)
- Modify: `crates/grid-cli/src/tui/widgets/input.rs` (~5 处)
- Modify: `crates/grid-cli/src/tui/widgets/progress.rs` (~3 处)
- Modify: `crates/grid-cli/src/tui/widgets/todo_panel.rs` (~17 处)
- Modify: `crates/grid-cli/src/tui/formatters/base.rs` (ToolFormatter trait)

**目标:** 全量清理剩余 `style_tokens::*` 颜色引用。保留 `style_tokens` 模块中的非颜色常量（字符常量如 `THINKING_ICON`、`CONTINUATION_CHAR`、`Indent`）。

**策略:**

**A. Formatter trait 扩展** — `ToolFormatter::format()` 和 `format_collapsed()` 签名不变（避免 trait object 破坏），改为在 `FormattedOutput` 构建时从 `style_tokens` 取色。由于 formatter 是 struct 实例，给各 formatter struct 加 `theme` 字段：

```rust
// base.rs — 保持 ToolFormatter trait 不变
// 各具体 formatter 在 ConversationWidget 构造时注入 theme

// formatter_registry.rs
pub struct ToolFormatterRegistry {
    // ... existing formatters ...
    // 不修改 trait，而是让各 formatter 在构造时接收 theme
}
```

实际上，由于 `ToolFormatterRegistry` 是在 `TuiState` 中初始化的（不是每帧重建），最简单的方式是：
- `style_tokens` 模块中的常量全部保留但标记 `#[deprecated]`
- 各 formatter 的 `format()`/`format_collapsed()` 内部仍使用 `style_tokens::*`，但把 `style_tokens` 的常量值改为动态从全局 theme 读取

**更优方案:** 把 `style_tokens` 的颜色常量保留为 fallback 默认值（Indigo theme），不标记 deprecated。具体 formatter 不改签名，而是：

1. StatusBar 已有 theme（通过 `self.brand_color` 等），扩展为接受完整 `&TuiTheme`
2. Input/Progress/TodoPanel 的 `Widget::render()` 中直接用 `style_tokens::*`——这些 widget 在 `render.rs` 中被构造时，可以传入 theme
3. Formatter 文件中的 `style_tokens::*` 用法：
   - `bash_formatter.rs`: SUCCESS/ERROR/SUBTLE/ORANGE → 语义色不需要跟随 accent，保留
   - `file_formatter.rs`: BLUE_PATH/SUCCESS/ERROR/CODE_BG 等 → 语义色保留
   - `diff.rs`: DIFF_ADD_BG/DIFF_DEL_BG → 从 theme 取色
   - `formatter_registry.rs`: SUBTLE/GREY → 从 theme 取色

**实际执行:** 由于 `ToolFormatter` trait 是 trait object（`Box<dyn ToolFormatter>`），不适合加泛型参数。最务实方案：

1. **StatusBar**: 构造器接受 `&TuiTheme`，替换所有 24 处 style_tokens
2. **Input**: 已部分主题化（BB-10），清理剩余 5 处
3. **Progress**: 已部分主题化（BB-8），清理剩余 3 处
4. **TodoPanel**: 17 处 style_tokens → 构造器接受 `&TuiTheme`
5. **Formatters (bash/file/diff/formatter_registry/tool_registry/wrap)**: 保持 `style_tokens::*` 因为这些是语义色（SUCCESS=绿色, ERROR=红色），不应随 accent 变化。仅 `diff.rs` 中 DIFF_ADD_BG/DIFF_DEL_BG 从 theme 取值。

**最终清理:** `style_tokens.rs` 保留所有非颜色常量（`THINKING_ICON`, `CONTINUATION_CHAR`, `RESULT_PREFIX`, `Indent`）。颜色常量保留作为 Indigo theme 默认值的文档参考。

**编译检查:** W1 结束后执行 `cargo check -p grid-cli --features studio`。

---

## Wave 2: 消息间距 + 状态栏响应式（BB-D1 + BB-D2, 2 tasks）

### Task BC-4: 消息角色分隔线（BB-D1）

**Files:**
- Modify: `crates/grid-cli/src/tui/widgets/conversation/mod.rs`

**目标:** 当消息角色从 User → Assistant 或 Assistant → User 切换时，插入一条样式化的分隔线，增强视觉层次。

**实现:**

1. 在 `build_lines()` 方法中，track 上一条消息的 role：

```rust
fn build_lines(&self) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut prev_role: Option<MessageRole> = None;

    for (msg_idx, msg) in self.messages.iter().enumerate() {
        // Role transition separator
        if let Some(prev) = prev_role {
            if prev != msg.role && msg.role != MessageRole::System {
                // Insert thin separator line: ─ ─ ─ ─ ─
                lines.push(Line::from(Span::styled(
                    " \u{2500} \u{2500} \u{2500} \u{2500} \u{2500} \u{2500} \u{2500} \u{2500}",
                    Style::default().fg(self.theme.border),
                )));
            }
        }

        match msg.role {
            // ... existing match arms ...
        }

        prev_role = Some(msg.role.clone());

        // Blank line between messages (existing)
        if msg_idx + 1 < self.messages.len() {
            lines.push(Line::from(""));
        }
    }
    lines
}
```

2. 分隔线风格：使用 `theme.border` 色的虚线 `─ ─ ─`，宽度 16 字符，给出轻微的视觉断点但不过于突兀。

---

### Task BC-5: 状态栏渐进式披露（BB-D2）

**Files:**
- Modify: `crates/grid-cli/src/tui/widgets/status_bar.rs`

**目标:** 当终端宽度较窄时，状态栏 Row 1 渐进式隐藏次要信息，确保核心信息（model + context%）始终可见。

**实现:**

在 `StatusBarWidget::render()` 方法 Row 1 构建处加入宽度检测：

```rust
// Row 1: brand ◆ Grid | model | tokens | ... | context%
if area.height >= 2 {
    let w = area.width as usize;
    let mut spans: Vec<Span> = Vec::new();

    // Tier 1 (always): model + context%
    // Tier 2 (w >= 60): + brand
    // Tier 3 (w >= 80): + tokens + elapsed
    // Tier 4 (w >= 100): + sandbox + effort + thinking

    if w >= 60 {
        // Brand
        spans.push(Span::styled(
            " \u{25C6} Grid",
            Style::default().fg(self.brand_color).add_modifier(Modifier::BOLD),
        ));
        spans.push(sep.clone());
    }

    // Model name (always shown)
    spans.push(Span::styled(
        self.model.to_string(),
        Style::default().fg(/* theme.text */).add_modifier(Modifier::BOLD),
    ));
    spans.push(sep.clone());

    if w >= 80 {
        // Tokens
        if self.input_tokens > 0 || self.output_tokens > 0 { ... }
        // Elapsed
        if let Some(elapsed) = self.session_elapsed { ... }
    }

    if w >= 100 {
        // Sandbox profile, Effort, Thinking
        ...
    }

    // Context% (always shown) — unchanged
    ...
}
```

Row 2 同样处理：
```rust
if area.height >= 3 {
    let w = area.width as usize;
    if w >= 50 {
        // Full: dir + git
    } else {
        // Narrow: git branch only
    }
}
```

**编译检查 + 测试:** W2 结束后执行 `cargo check -p grid-cli --features studio` + `cargo test -p grid-cli --features studio -- --test-threads=1`。

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| BC-D1 | ToolFormatter trait 添加 theme 参数（需 trait object 兼容方案） | 确定是否放弃 trait object 或用 Any downcast | ⏳ |
| BC-D2 | Thinking block 专用 MdPalette（muted palette 实际调用路径） | conversation/mod.rs thinking 渲染重构 | ⏳ |
| BC-D3 | style_tokens.rs 颜色常量标记 deprecated 并最终移除 | 所有 formatter 迁移完成 | ⏳ |
