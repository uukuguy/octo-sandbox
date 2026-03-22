# Phase U: TUI Production Hardening 实施方案

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标:** 补齐 octo-cli TUI 与 grid-tui 的关键差距，将 TUI 从 MVP 提升到生产级别。

**架构:** 4 组渐进式改动 — G1 基础设施（approval wiring, event batching, 滚动加速）→ G2 渲染优化（per-message cache, formatter registry, tool 折叠）→ G3 新 Widget（StatusBar 增强, Todo Panel, Input 改进）→ G4 品牌升级（Welcome Panel 重做）。

**技术栈:** Rust, ratatui 0.29, crossterm, tokio (async event loop)

**分支:** `feat/tui-opendev-integration`（延续 Phase T）

**基线测试数:** 2259（Phase T 完成时）

---

## 设计决策记录

| # | 决策 | 方案 | 理由 |
|---|------|------|------|
| D1 | Approval Gate wiring | TuiState 直接持有 `ApprovalGate` | 已持有 AgentExecutorHandle（engine 类型），不改变耦合关系；30s 超时压力下延迟最低 |
| D2 | Per-message cache key | `Vec<(u64, Vec<Line>)>` 索引对齐 | 消息是 append-only Vec，索引天然对齐，比 HashMap 轻量 |
| D3 | ToolFormatter registry | `HashMap<String, Box<dyn ToolFormatter>>` 动态注册 | 支持 MCP 动态工具，可扩展 |
| D4 | 滚动加速参数 | 3/6/12 行，200ms 窗口 | 复用 grid-tui 验证过的参数 |
| D5 | Tool 折叠 | CC 风格默认折叠 + Ctrl+O 展开 | 兼顾简洁和精细控制 |
| D6 | Active Tools 面板 | 删除 | 与 ConversationWidget inline spinner 重复，空间给 Todo Panel |
| D7 | StatusBar 图标 | Unicode 符号（✳ ▸ ◦ ↕ ⊞），不用 emoji | 终端宽度一致性 |
| D8 | StatusBar 品牌 | `✳ octo`（U+2733 Eight Spoked Asterisk） | 八条辐射线呼应 octo=8 |
| D9 | Welcome Panel 品牌 | ASCII Art "OCTO" (Tier 3) + 🦑 极简 (Tier 1/2) | 大终端视觉冲击，小终端降级 |
| D10 | Debug 面板 | 暂缓深度重设计（U-D1），本阶段只做数据提升到 StatusBar | 当前 Debug 仍可用，优先级低 |

---

## G1: 基础设施（无外部依赖）

### Task U1-1: Approval Gate Wiring

**Files:**
- Modify: `crates/octo-engine/src/tools/approval.rs` — 确认 `ApprovalGate` 是 `Clone`
- Modify: `crates/octo-cli/src/tui/app_state.rs` — 新增 `approval_gate` 字段
- Modify: `crates/octo-cli/src/tui/mod.rs` — 注入 gate 到 TuiState
- Modify: `crates/octo-cli/src/tui/key_handler.rs` — Y/N 调用 `gate.respond()`
- Test: `crates/octo-cli/src/tui/key_handler.rs` (inline tests)

**Step 1: 确认 ApprovalGate 的 Clone 能力**

读取 `crates/octo-engine/src/tools/approval.rs`，确认 `ApprovalGate` 实现了 `Clone`（内部是 `Arc<Mutex<HashMap>>`）。如果没有 `Clone`，需要添加 `#[derive(Clone)]`。

**Step 2: TuiState 新增 approval_gate 字段**

在 `crates/octo-cli/src/tui/app_state.rs` 的 `TuiState` struct 中添加：

```rust
use octo_engine::tools::approval::ApprovalGate;

pub struct TuiState {
    // ... existing fields ...

    // ── Approval ──
    /// Gate for responding to tool approval requests.
    pub approval_gate: Option<ApprovalGate>,
}
```

在 `with_history()` 构造函数中初始化为 `None`。

新增 setter：

```rust
pub fn set_approval_gate(&mut self, gate: ApprovalGate) {
    self.approval_gate = Some(gate);
}
```

**Step 3: 注入 gate 到 TuiState**

在 `crates/octo-cli/src/tui/mod.rs` 的 `run_tui()` 或 `run_conversation_loop()` 入口处，从 `AgentLoopConfig` 提取 `ApprovalGate` 并注入：

```rust
if let Some(gate) = agent_loop_config.approval_gate.clone() {
    state.set_approval_gate(gate);
}
```

如果当前入口没有 `AgentLoopConfig` 的访问权，需要通过参数传入。

**Step 4: key_handler 调用 gate.respond()**

修改 `crates/octo-cli/src/tui/key_handler.rs` 的 `handle_approval_key()`：

```rust
async fn handle_approval_key(state: &mut TuiState, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('y') | KeyCode::Char('Y')) => {
            if let Some(ref approval) = state.pending_approval {
                if let Some(ref gate) = state.approval_gate {
                    gate.respond(&approval.tool_id, true).await;
                }
            }
            state.pending_approval = None;
        }
        (KeyModifiers::NONE, KeyCode::Char('n') | KeyCode::Char('N')) | (KeyModifiers::NONE, KeyCode::Esc) => {
            if let Some(ref approval) = state.pending_approval {
                if let Some(ref gate) = state.approval_gate {
                    gate.respond(&approval.tool_id, false).await;
                }
            }
            state.pending_approval = None;
        }
        (KeyModifiers::NONE, KeyCode::Char('a') | KeyCode::Char('A')) => {
            // Always approve — respond true, future: persist preference
            if let Some(ref approval) = state.pending_approval {
                if let Some(ref gate) = state.approval_gate {
                    gate.respond(&approval.tool_id, true).await;
                }
            }
            state.pending_approval = None;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            if state.interrupt_manager.handle_ctrl_c().await {
                state.running = false;
            }
        }
        _ => {}
    }
}
```

**Step 5: 写测试**

```rust
#[tokio::test]
async fn test_approval_y_clears_pending() {
    let mut state = make_test_state();
    state.pending_approval = Some(PendingApproval {
        tool_id: "t1".into(),
        tool_name: "bash".into(),
        risk_level: RiskLevel::HighRisk,
    });
    handle_key(&mut state, make_key(KeyCode::Char('y'))).await;
    assert!(state.pending_approval.is_none());
}

#[tokio::test]
async fn test_approval_n_clears_pending() {
    let mut state = make_test_state();
    state.pending_approval = Some(PendingApproval {
        tool_id: "t1".into(),
        tool_name: "bash".into(),
        risk_level: RiskLevel::HighRisk,
    });
    handle_key(&mut state, make_key(KeyCode::Char('n'))).await;
    assert!(state.pending_approval.is_none());
}
```

**Step 6: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-engine/src/tools/approval.rs crates/octo-cli/src/tui/app_state.rs crates/octo-cli/src/tui/mod.rs crates/octo-cli/src/tui/key_handler.rs
git commit -m "feat(tui): wire ApprovalGate — Y/N/A keys now respond to engine (U1-1)"
```

---

### Task U1-2: Event Batch Drain

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs` — 事件循环加 drain loop

**Step 1: 在事件循环的 render 之前加 batch drain**

在 `crates/octo-cli/src/tui/mod.rs` 的主事件循环中，收到第一个事件后，用 `try_next()` 循环 drain 所有排队事件再 render：

```rust
// Main event loop
loop {
    let event = event_handler.next().await;
    handle_event(&mut state, event).await;

    // Batch drain: consume all queued events before rendering
    while let Some(queued) = event_handler.try_next() {
        handle_event(&mut state, queued).await;
    }

    if state.dirty {
        // rebuild + render
    }
    if !state.running { break; }
}
```

**Step 2: 确认 event_handler.try_next() 存在**

读取 `crates/octo-cli/src/tui/event_handler.rs`，确认有 `try_next()` 方法。如果没有，添加：

```rust
pub fn try_next(&mut self) -> Option<AppEvent> {
    self.rx.try_recv().ok()
}
```

**Step 3: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/mod.rs crates/octo-cli/src/tui/event_handler.rs
git commit -m "feat(tui): event batch drain — consume all queued events before render (U1-2)"
```

---

### Task U1-3: 滚动加速

**Files:**
- Modify: `crates/octo-cli/src/tui/app_state.rs` — 新增滚动加速状态字段
- Modify: `crates/octo-cli/src/tui/key_handler.rs` — 加速计算逻辑
- Test: inline tests in key_handler.rs

**Step 1: TuiState 新增滚动加速字段**

```rust
use std::time::Instant;

pub struct TuiState {
    // ... existing fields ...

    // ── Scroll acceleration ──
    pub scroll_last_dir: Option<bool>,     // true=up, false=down
    pub scroll_last_time: Option<Instant>,
    pub scroll_accel: u8,                  // 0=3行, 1=6行, 2=12行
}
```

在构造函数中初始化为 `None, None, 0`。

**Step 2: 提取滚动量计算函数**

在 `key_handler.rs` 新增：

```rust
const SCROLL_AMOUNTS: [u16; 3] = [3, 6, 12];
const SCROLL_ACCEL_WINDOW_MS: u128 = 200;

fn compute_scroll_amount(state: &mut TuiState, direction_up: bool) -> u16 {
    let now = Instant::now();
    let same_dir = state.scroll_last_dir == Some(direction_up);
    let within_window = state.scroll_last_time
        .map(|t| now.duration_since(t).as_millis() < SCROLL_ACCEL_WINDOW_MS)
        .unwrap_or(false);

    if same_dir && within_window {
        state.scroll_accel = (state.scroll_accel + 1).min(2);
    } else {
        state.scroll_accel = 0;
    }

    state.scroll_last_dir = Some(direction_up);
    state.scroll_last_time = Some(now);
    SCROLL_AMOUNTS[state.scroll_accel as usize]
}
```

**Step 3: 替换 Up/Down 的固定滚动量**

将 key_handler 中 `scroll_offset.saturating_add(3)` / `.saturating_sub(3)` 改为 `compute_scroll_amount(state, true/false)`。

**Step 4: 写测试**

```rust
#[tokio::test]
async fn test_scroll_acceleration() {
    let mut state = make_test_state();
    // First scroll: level 0 = 3 lines
    let amount = compute_scroll_amount(&mut state, true);
    assert_eq!(amount, 3);
    // Immediately again: level 1 = 6 lines
    let amount = compute_scroll_amount(&mut state, true);
    assert_eq!(amount, 6);
    // Again: level 2 = 12 lines
    let amount = compute_scroll_amount(&mut state, true);
    assert_eq!(amount, 12);
    // Caps at 12
    let amount = compute_scroll_amount(&mut state, true);
    assert_eq!(amount, 12);
}

#[tokio::test]
async fn test_scroll_direction_change_resets() {
    let mut state = make_test_state();
    compute_scroll_amount(&mut state, true);
    compute_scroll_amount(&mut state, true); // level 1
    let amount = compute_scroll_amount(&mut state, false); // direction change → reset
    assert_eq!(amount, 3);
}
```

**Step 5: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/app_state.rs crates/octo-cli/src/tui/key_handler.rs
git commit -m "feat(tui): scroll acceleration — 3-level (3/6/12 lines) with 200ms window (U1-3)"
```

---

## G2: 渲染优化

### Task U2-1: Per-message Markdown Cache

**Files:**
- Modify: `crates/octo-cli/src/tui/app_state.rs` — 新增 `per_message_cache` + 改写 `rebuild_cached_lines()`
- Test: inline tests in app_state.rs

**Step 1: 新增缓存字段**

```rust
pub struct TuiState {
    // ... existing fields ...

    // ── Per-message cache ──
    /// (content_hash, rendered_lines) indexed by message position.
    pub per_message_cache: Vec<(u64, Vec<Line<'static>>)>,
}
```

构造函数初始化为 `Vec::new()`。

**Step 2: 新增 hash 函数**

```rust
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

fn hash_message(msg: &ChatMessage) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Hash role
    format!("{:?}", msg.role).hash(&mut hasher);
    // Hash content blocks
    for block in &msg.content {
        match block {
            ContentBlock::Text { text } => text.hash(&mut hasher),
            ContentBlock::ToolUse { id, name, input, .. } => {
                id.hash(&mut hasher);
                name.hash(&mut hasher);
                input.to_string().hash(&mut hasher);
            }
            ContentBlock::ToolResult { tool_use_id, content, is_error, .. } => {
                tool_use_id.hash(&mut hasher);
                content.hash(&mut hasher);
                is_error.hash(&mut hasher);
            }
            _ => {}
        }
    }
    hasher.finish()
}
```

**Step 3: 改写 rebuild_cached_lines() 为增量**

```rust
pub fn rebuild_cached_lines(&mut self) {
    use super::formatters::markdown::MarkdownRenderer;

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut new_cache = Vec::with_capacity(self.messages.len());

    for (i, msg) in self.messages.iter().enumerate() {
        let hash = hash_message(msg);

        // Cache hit?
        if let Some((cached_hash, cached_lines)) = self.per_message_cache.get(i) {
            if *cached_hash == hash {
                lines.extend(cached_lines.iter().cloned());
                new_cache.push((hash, cached_lines.clone()));
                continue;
            }
        }

        // Cache miss — render this message
        let rendered = Self::render_single_message(msg);
        lines.extend(rendered.iter().cloned());
        new_cache.push((hash, rendered));
    }

    // Streaming text — always re-render (not cached)
    if !self.streaming_text.is_empty() {
        lines.push(Line::from(ratatui::text::Span::styled(
            "─── Assistant ───",
            ratatui::style::Style::default()
                .fg(ratatui::style::Color::Green)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )));
        let md_lines = MarkdownRenderer::render(&self.streaming_text);
        lines.extend(md_lines);
    }

    // Thinking text — always re-render
    if !self.thinking_text.is_empty() {
        // ... existing thinking rendering ...
    }

    self.per_message_cache = new_cache;
    self.cached_lines = lines;
    self.lines_generation = self.message_generation;
}
```

提取 `render_single_message()` 为独立方法，包含 role header + content blocks + spacing。

**Step 4: invalidate_cache() 也清理 per_message_cache（只在 terminal resize 时）**

新增 `invalidate_all_cache()` 用于 resize，`invalidate_cache()` 只 bump generation（增量缓存自动处理）。

**Step 5: 写测试**

```rust
#[test]
fn test_per_message_cache_reuse() {
    let mut state = make_test_state_for_test();
    state.messages.push(ChatMessage::user("Hello"));
    state.messages.push(ChatMessage::assistant("Hi"));
    state.message_generation = 1;
    state.rebuild_cached_lines();
    assert_eq!(state.per_message_cache.len(), 2);

    // Second rebuild without changes — should reuse cache
    let old_cache = state.per_message_cache.clone();
    state.message_generation = 2;
    state.rebuild_cached_lines();
    assert_eq!(state.per_message_cache[0].0, old_cache[0].0); // same hash
}
```

**Step 6: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/app_state.rs
git commit -m "feat(tui): per-message markdown cache — incremental rebuild via content hash (U2-1)"
```

---

### Task U2-2: ToolFormatter 动态注册

**Files:**
- Create: `crates/octo-cli/src/tui/formatters/formatter_registry.rs`
- Modify: `crates/octo-cli/src/tui/formatters/mod.rs` — pub mod
- Modify: `crates/octo-cli/src/tui/formatters/base.rs` — 更新 trait 签名
- Modify: `crates/octo-cli/src/tui/app_state.rs` — TuiState 持有 registry
- Modify: `crates/octo-cli/src/tui/widgets/conversation/mod.rs` — 用 registry 替代硬编码

**Step 1: 更新 ToolFormatter trait**

在 `crates/octo-cli/src/tui/formatters/base.rs` 中更新 trait：

```rust
pub trait ToolFormatter: Send + Sync {
    /// Format full tool output for expanded display.
    fn format(&self, tool_name: &str, output: &str, width: u16) -> Vec<Line<'static>>;

    /// Format collapsed one-line summary.
    fn format_collapsed(&self, tool_name: &str, output: &str) -> Line<'static>;
}
```

**Step 2: 为现有 formatter 实现 trait**

为 `BashFormatter`、`DiffFormatter`（即 diff.rs 模块）、`FileFormatter` 实现 `ToolFormatter` trait。
新增 `GenericFormatter` 作为默认（长文本截断 + JSON pretty-print 检测）。

**Step 3: 创建 ToolFormatterRegistry**

在 `crates/octo-cli/src/tui/formatters/formatter_registry.rs`：

```rust
use std::collections::HashMap;
use super::base::ToolFormatter;

pub struct ToolFormatterRegistry {
    formatters: HashMap<String, Box<dyn ToolFormatter>>,
    default: Box<dyn ToolFormatter>,
}

impl ToolFormatterRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            formatters: HashMap::new(),
            default: Box::new(super::GenericFormatter),
        };
        // Register built-in formatters
        registry.register("bash", Box::new(super::bash_formatter::BashOutputFormatter));
        registry.register("run_command", Box::new(super::bash_formatter::BashOutputFormatter));
        registry.register("Bash", Box::new(super::bash_formatter::BashOutputFormatter));
        registry.register("file_edit", Box::new(super::diff::DiffOutputFormatter));
        registry.register("edit_file", Box::new(super::diff::DiffOutputFormatter));
        registry.register("Edit", Box::new(super::diff::DiffOutputFormatter));
        registry.register("file_read", Box::new(super::file_formatter::FileOutputFormatter));
        registry.register("read_file", Box::new(super::file_formatter::FileOutputFormatter));
        registry.register("Read", Box::new(super::file_formatter::FileOutputFormatter));
        registry
    }

    pub fn register(&mut self, tool_name: &str, formatter: Box<dyn ToolFormatter>) {
        self.formatters.insert(tool_name.to_string(), formatter);
    }

    pub fn format(&self, tool_name: &str, output: &str, width: u16) -> Vec<Line<'static>> {
        self.formatters
            .get(tool_name)
            .unwrap_or(&self.default)
            .format(tool_name, output, width)
    }

    pub fn format_collapsed(&self, tool_name: &str, output: &str) -> Line<'static> {
        self.formatters
            .get(tool_name)
            .unwrap_or(&self.default)
            .format_collapsed(tool_name, output)
    }
}
```

**Step 4: TuiState 持有 registry**

在 `app_state.rs` 中新增 `tool_formatter_registry: ToolFormatterRegistry` 字段，构造时 `ToolFormatterRegistry::new()`。

**Step 5: ConversationWidget 使用 registry**

修改 `widgets/conversation/mod.rs`，将硬编码的工具结果格式化替换为 `state.tool_formatter_registry.format(name, output, width)`。

**Step 6: 写测试**

```rust
#[test]
fn test_registry_returns_bash_formatter() {
    let registry = ToolFormatterRegistry::new();
    let lines = registry.format("bash", "hello world", 80);
    assert!(!lines.is_empty());
}

#[test]
fn test_registry_unknown_tool_uses_default() {
    let registry = ToolFormatterRegistry::new();
    let lines = registry.format("unknown_mcp_tool", "some output", 80);
    assert!(!lines.is_empty());
}

#[test]
fn test_registry_collapsed_format() {
    let registry = ToolFormatterRegistry::new();
    let line = registry.format_collapsed("bash", "line1\nline2\nline3");
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("3")); // should mention line count
}
```

**Step 7: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/formatters/
git commit -m "feat(tui): ToolFormatterRegistry — dynamic tool output formatting (U2-2)"
```

---

### Task U2-3: Tool 折叠/展开

**Files:**
- Modify: `crates/octo-cli/src/tui/app_state.rs` — 折叠状态
- Modify: `crates/octo-cli/src/tui/key_handler.rs` — Ctrl+O / Ctrl+Shift+O
- Modify: `crates/octo-cli/src/tui/widgets/conversation/mod.rs` — 折叠渲染
- Modify: `crates/octo-cli/src/tui/app_state.rs:rebuild_cached_lines()` — 根据折叠状态选择渲染

**Step 1: TuiState 新增折叠状态**

```rust
pub struct TuiState {
    // ... existing fields ...

    // ── Tool collapse ──
    /// Global default: tools collapsed by default.
    pub tools_default_collapsed: bool,
    /// Per-tool override: tool_id → expanded state. `Some(true)` = force expand.
    pub tool_expanded_overrides: HashMap<String, bool>,
}
```

构造函数中 `tools_default_collapsed: true`, `tool_expanded_overrides: HashMap::new()`。

新增 helper：

```rust
pub fn is_tool_collapsed(&self, tool_id: &str) -> bool {
    match self.tool_expanded_overrides.get(tool_id) {
        Some(expanded) => !expanded,
        None => self.tools_default_collapsed,
    }
}
```

**Step 2: key_handler 添加 Ctrl+O / Ctrl+Shift+O**

```rust
// Ctrl+O: toggle most recent completed tool
(KeyModifiers::CONTROL, KeyCode::Char('o')) => {
    if let Some(last_tool) = state.messages.iter().rev()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        })
    {
        let currently_collapsed = state.is_tool_collapsed(&last_tool);
        state.tool_expanded_overrides.insert(last_tool, currently_collapsed); // toggle
        state.invalidate_cache();
    }
}

// Ctrl+Shift+O: toggle global collapse
// Note: crossterm may report this differently; use ALT+O as fallback
(KeyModifiers::ALT, KeyCode::Char('o')) => {
    state.tools_default_collapsed = !state.tools_default_collapsed;
    state.tool_expanded_overrides.clear(); // reset per-tool overrides
    state.invalidate_cache();
}
```

**Step 3: rebuild_cached_lines() 根据折叠状态渲染**

在 `render_single_message()` 中处理 `ContentBlock::ToolResult`：

```rust
ContentBlock::ToolResult { tool_use_id, content, is_error, .. } => {
    if self.is_tool_collapsed(tool_use_id) {
        // Collapsed: one-line summary
        let line_count = content.lines().count();
        let summary = format!("⚙ {} ✓ — {} lines (Ctrl+O to expand)", tool_name, line_count);
        lines.push(Line::from(Span::styled(summary, collapse_style)));
    } else {
        // Expanded: full output via formatter registry
        let formatted = self.tool_formatter_registry.format(tool_name, content, width);
        lines.extend(formatted);
    }
}
```

**Step 4: 写测试**

```rust
#[test]
fn test_tool_collapsed_by_default() {
    let state = make_test_state_for_test();
    assert!(state.is_tool_collapsed("any-tool-id"));
}

#[test]
fn test_tool_expand_override() {
    let mut state = make_test_state_for_test();
    state.tool_expanded_overrides.insert("t1".into(), true);
    assert!(!state.is_tool_collapsed("t1"));
    assert!(state.is_tool_collapsed("t2")); // others still collapsed
}

#[test]
fn test_global_toggle_clears_overrides() {
    let mut state = make_test_state_for_test();
    state.tool_expanded_overrides.insert("t1".into(), true);
    state.tools_default_collapsed = false;
    state.tool_expanded_overrides.clear();
    assert!(!state.is_tool_collapsed("t1")); // follows global now
}
```

**Step 5: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/app_state.rs crates/octo-cli/src/tui/key_handler.rs crates/octo-cli/src/tui/widgets/conversation/mod.rs
git commit -m "feat(tui): tool collapse/expand — CC-style default-collapsed + Ctrl+O toggle (U2-3)"
```

---

## G3: 新 Widget

### Task U3-1: StatusBar 增强

**Files:**
- Modify: `crates/octo-cli/src/tui/widgets/status_bar.rs` — 品牌 + 状态指示器 + 配色
- Modify: `crates/octo-cli/src/tui/app_state.rs` — 新增 working_dir, git_branch, context_usage_pct, session_cost, mcp_status, agent_state
- Modify: `crates/octo-cli/src/tui/render.rs` — 替换旧 render_status_bar() 为 StatusBarWidget
- Modify: `crates/octo-cli/src/tui/mod.rs` — TokenBudgetUpdate 事件 → 更新状态
- Modify: `crates/octo-cli/src/tui/formatters/style_tokens.rs` — 新增 amber 色常量

**Step 1: style_tokens 新增品牌色**

```rust
pub const AMBER: Color = Color::Rgb(212, 160, 23);    // ✳ octo 品牌色
pub const AMBER_DIM: Color = Color::Rgb(140, 105, 15); // 暗 amber
```

**Step 2: TuiState 新增字段**

```rust
pub enum AgentState {
    Idle,
    Streaming,
    Thinking,
}

pub struct TuiState {
    // ... existing fields ...

    // ── StatusBar data ──
    pub working_dir: String,
    pub git_branch: Option<String>,
    pub context_usage_pct: f64,
    pub session_cost: f64,
    pub mcp_status: Option<(usize, usize)>,
}
```

构造函数中 `working_dir` 用 `std::env::current_dir()` 初始化，`git_branch` 用 `Command::new("git").args(["rev-parse", "--abbrev-ref", "HEAD"])` 一次性获取。

新增 helper：

```rust
pub fn agent_state(&self) -> AgentState {
    if self.is_thinking { AgentState::Thinking }
    else if self.is_streaming { AgentState::Streaming }
    else { AgentState::Idle }
}
```

**Step 3: 更新 StatusBarWidget 品牌和配色**

修改 `widgets/status_bar.rs`：
- 将 `◆` 品牌符号改为 `✳`
- 将品牌色从 `CYAN` 改为 `AMBER`
- 新增 agent state 指示器段：`▸ streaming` (Green) / `◦ thinking` (Magenta) / `· idle` (DarkGray)
- 调整分隔符为 `│` (U+2502)
- Context % 改用 mini 进度条 `▮▮▮▯▯`

**Step 4: render.rs 替换旧 status bar**

删除 `render_status_bar()` 函数，改为：

```rust
fn render_status_bar(state: &TuiState, frame: &mut Frame, area: Rect) {
    let widget = super::widgets::status_bar::StatusBarWidget::new(
        &state.model_name,
        &state.working_dir,
        state.git_branch.as_deref(),
    )
    .context_usage_pct(state.context_usage_pct)
    .session_cost(state.session_cost)
    .mcp_status(state.mcp_status, false)
    .tokens(state.total_input_tokens, state.total_output_tokens)
    .agent_state(state.agent_state());

    frame.render_widget(widget, area);
}
```

**Step 5: 同时删除 render.rs 中的旧 hints 行**（StatusBar 已包含关键状态，hints 移到 StatusBar 内或删除）

将 status_height 从 `2u16` 改为 `1u16`（省 1 行给对话区）。

**Step 6: 事件循环中处理 TokenBudgetUpdate**

在 `mod.rs` 处理 `AgentEvent::TokenBudgetUpdate` 时更新 `state.context_usage_pct`。

**Step 7: 写测试**

```rust
#[test]
fn test_status_bar_brand_symbol() {
    let area = Rect::new(0, 0, 100, 2);
    let mut buf = Buffer::empty(area);
    let widget = StatusBarWidget::new("test-model", "/home/user", Some("main"))
        .tokens(5000, 1500);
    widget.render(area, &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("✳"), "Should contain brand symbol ✳");
}
```

**Step 8: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/widgets/status_bar.rs crates/octo-cli/src/tui/app_state.rs crates/octo-cli/src/tui/render.rs crates/octo-cli/src/tui/mod.rs crates/octo-cli/src/tui/formatters/style_tokens.rs
git commit -m "feat(tui): StatusBar enhancement — ✳ brand, agent state, context%, cost, MCP (U3-1)"
```

---

### Task U3-2: Todo Panel + 删除 Active Tools 面板

**Files:**
- Modify: `crates/octo-engine/src/agent/events.rs` — 新增 `AgentEvent::PlanUpdate`
- Modify: `crates/octo-engine/src/agent/dual.rs` — 广播 PlanUpdate 事件
- Create: `crates/octo-cli/src/tui/widgets/todo_panel.rs`
- Modify: `crates/octo-cli/src/tui/widgets/mod.rs` — pub mod todo_panel
- Modify: `crates/octo-cli/src/tui/app_state.rs` — plan_steps, todo_visible
- Modify: `crates/octo-cli/src/tui/key_handler.rs` — Ctrl+P toggle
- Modify: `crates/octo-cli/src/tui/render.rs` — 替换 Active Tools 为 Todo Panel
- Modify: `crates/octo-cli/src/tui/mod.rs` — 处理 PlanUpdate 事件

**Step 1: Engine 新增 PlanUpdate 事件**

在 `crates/octo-engine/src/agent/events.rs` 中新增：

```rust
/// Plan steps updated (from dual-mode agent).
PlanUpdate {
    steps: Vec<crate::agent::dual::PlanStep>,
},
```

**Step 2: DualAgentManager 广播 PlanUpdate**

在 `crates/octo-engine/src/agent/dual.rs` 中，每当 `plan_steps` 变化时（parse plan response、mark step complete），通过事件 channel 广播 `AgentEvent::PlanUpdate { steps: self.plan_steps.clone() }`。

**Step 3: 创建 TodoPanelWidget**

在 `crates/octo-cli/src/tui/widgets/todo_panel.rs`：

```rust
pub struct TodoPanelWidget<'a> {
    steps: &'a [PlanStep],
    visible: bool,
}

impl Widget for TodoPanelWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Header: ┌ Plan (3/5) ────── Ctrl+P ┐
        // Steps: ✅ 1. xxx / ⏳ 2. xxx / ○ 3. xxx
        // Progress bar: ━━━━━━━━━━━ 60%
    }
}
```

**Step 4: TuiState 新增字段**

```rust
use octo_engine::agent::dual::PlanStep;

pub struct TuiState {
    // ... existing fields ...
    pub plan_steps: Vec<PlanStep>,
    pub todo_visible: bool,
}
```

**Step 5: key_handler 添加 Ctrl+P**

```rust
(KeyModifiers::CONTROL, KeyCode::Char('p')) => {
    state.todo_visible = !state.todo_visible;
    state.dirty = true;
}
```

**Step 6: render.rs 替换 Active Tools → Todo Panel**

删除 `render_progress()` 函数。修改 layout：

```rust
let todo_height = if state.todo_visible && !state.plan_steps.is_empty() {
    (state.plan_steps.len() as u16 + 3).min(10) // header + steps + progress bar
} else {
    0
};

// Layout: conversation | todo_panel | input | status_bar
```

**Step 7: 事件循环处理 PlanUpdate**

```rust
AgentEvent::PlanUpdate { steps } => {
    state.plan_steps = steps;
    if !state.plan_steps.is_empty() && !state.todo_visible {
        state.todo_visible = true; // auto-show on first plan
    }
    state.dirty = true;
}
```

**Step 8: 写测试**

```rust
#[test]
fn test_todo_panel_renders() {
    let steps = vec![
        PlanStep { number: 1, description: "Setup".into(), completed: true },
        PlanStep { number: 2, description: "Build".into(), completed: false },
    ];
    let widget = TodoPanelWidget::new(&steps, true);
    let area = Rect::new(0, 0, 60, 5);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("Plan"));
    assert!(content.contains("Setup"));
}
```

**Step 9: 运行测试并提交**

```bash
cargo test --workspace -- --test-threads=1 2>&1 | tail -5
git add crates/octo-engine/src/agent/events.rs crates/octo-engine/src/agent/dual.rs crates/octo-cli/src/tui/widgets/todo_panel.rs crates/octo-cli/src/tui/widgets/mod.rs crates/octo-cli/src/tui/app_state.rs crates/octo-cli/src/tui/key_handler.rs crates/octo-cli/src/tui/render.rs crates/octo-cli/src/tui/mod.rs
git commit -m "feat(tui): Todo Panel + delete Active Tools panel — PlanUpdate event from dual-mode (U3-2)"
```

---

### Task U3-3: InputWidget 改进

**Files:**
- Modify: `crates/octo-cli/src/tui/widgets/input.rs` — 去底边框, 模式色, dim, ghost text
- Modify: `crates/octo-cli/src/tui/key_handler.rs` — 所有 input_buffer 修改 → dirty = true
- Modify: `crates/octo-cli/src/tui/render.rs` — input_lines 计算 +2 → +1（去底边框）

**Step 1: key_handler 所有 input 修改触发 dirty**

在以下分支末尾添加 `state.dirty = true`：
- `KeyCode::Char(c)` / `KeyCode::Shift+Char(c)` — 字符输入
- `KeyCode::Backspace` — 删除
- `KeyCode::Delete` — 删除
- `Shift+Enter` / `Alt+Enter` / `Ctrl+J` — 新行
- `KeyCode::Enter` — 提交（已有 invalidate_cache 间接触发）

**Step 2: InputWidget 去掉底部边框**

在 `widgets/input.rs` 中：
- 删除底部 `───` 渲染代码（lines 143-151）
- `text_height` 改为 `area.height.saturating_sub(1)`（只减顶部分隔线）

在 `render.rs` 中：
- `Constraint::Length(input_lines + 2)` 改为 `Constraint::Length(input_lines + 1)`

**Step 3: 模式颜色强化**

```rust
let (accent, mode_label) = match self.mode {
    "Streaming" => (style_tokens::GREEN_LIGHT, "▸ Streaming"),
    "Thinking"  => (style_tokens::MAGENTA, "◦ Thinking"),
    "PLAN"      => (style_tokens::GREEN_LIGHT, "Plan"),
    _           => (style_tokens::BORDER, "Normal"),  // DarkGray for idle
};
```

**Step 4: Streaming 时 dim 输入**

```rust
let text_style = if self.mode == "Streaming" || self.mode == "Thinking" {
    Style::default().fg(style_tokens::GREY) // dimmed
} else {
    Style::default() // normal
};
```

**Step 5: 去掉 placeholder，加 ghost text 支持**

InputWidget 新增 `ghost_text: Option<&'a str>` 字段。当 input 为空时不显示 "Type a message..."，而是显示 ghost text（如果有）以极浅色。Tab 键在 key_handler 中采纳 ghost text。

```rust
// In render, when buffer is empty and ghost_text is Some:
if self.buffer.is_empty() {
    if let Some(ghost) = self.ghost_text {
        let spans = vec![
            Span::styled("❯ ", prefix_style),
            Span::styled(ghost, Style::default().fg(Color::Rgb(60, 60, 60))), // very dim
        ];
        Paragraph::new(Line::from(spans)).render(text_area, buf);
    } else {
        // Just show cursor
        let spans = vec![
            Span::styled("❯ ", prefix_style),
            Span::styled(" ", cursor_style), // block cursor
        ];
        Paragraph::new(Line::from(spans)).render(text_area, buf);
    }
}
```

Ghost text 来源：autocomplete engine 的当前建议。在 `render.rs` 构造 InputWidget 时传入。

**Step 6: key_handler Tab 采纳 ghost text**

```rust
(KeyModifiers::NONE, KeyCode::Tab) => {
    // If autocomplete has a suggestion, accept it
    // (具体实现取决于 autocomplete engine 的 API)
    state.dirty = true;
}
```

**Step 7: 写测试**

```rust
#[test]
fn test_input_no_bottom_border() {
    let area = Rect::new(0, 0, 60, 3);
    let mut buf = Buffer::empty(area);
    let widget = InputWidget::new("hello", 5, "NORMAL", 0);
    widget.render(area, &mut buf);
    // Bottom row should be text, not border
    let bottom: String = (0..area.width)
        .map(|x| buf.cell((x, 2)).map_or(' ', |c| c.symbol().chars().next().unwrap_or(' ')))
        .collect();
    assert!(!bottom.contains("─"), "Bottom row should not have border");
}

#[test]
fn test_input_streaming_mode_label() {
    let area = Rect::new(0, 0, 60, 2);
    let mut buf = Buffer::empty(area);
    let widget = InputWidget::new("", 0, "Streaming", 0);
    widget.render(area, &mut buf);
    let top: String = (0..area.width)
        .map(|x| buf.cell((x, 0)).map_or(' ', |c| c.symbol().chars().next().unwrap_or(' ')))
        .collect();
    assert!(top.contains("Streaming"), "Should show streaming mode");
}
```

**Step 8: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/widgets/input.rs crates/octo-cli/src/tui/key_handler.rs crates/octo-cli/src/tui/render.rs
git commit -m "feat(tui): InputWidget improvements — no bottom border, mode colors, ghost text, instant expand (U3-3)"
```

---

## G4: 品牌升级

### Task U4-1: Welcome Panel 重做

**Files:**
- Modify: `crates/octo-cli/src/tui/widgets/welcome_panel/mod.rs` — ASCII Art + 降级
- Test: inline tests

**Step 1: 设计 ASCII Art "OCTO"**

```
  ██████   ██████ ████████  ██████
 ██    ██ ██         ██    ██    ██
 ██    ██ ██         ██    ██    ██
 ██    ██ ██         ██    ██    ██
  ██████   ██████    ██     ██████
```

5 行高、约 36 字符宽。每个字符用 `write_gradient_line()` 渲染 amber 呼吸渐变。

**Step 2: 更新 Tier 渲染**

```rust
let title_ascii = [
    "  ██████   ██████ ████████  ██████ ",
    " ██    ██ ██         ██    ██    ██",
    " ██    ██ ██         ██    ██    ██",
    " ██    ██ ██         ██    ██    ██",
    "  ██████   ██████    ██     ██████ ",
];
let subtitle = "Autonomous AI Workbench";

if area.height < 5 {
    // Tier 1: 极简
    Self::center_text(buf, area, cy, "🦑 octo — autonomous ai workbench", dim);
} else if area.height < 12 {
    // Tier 2: 单行品牌 + 边框
    self.write_gradient_line(buf, area, by + 2, "O C T O", 0.55);
    Self::center_text(buf, area, by + 4, subtitle, dim);
} else {
    // Tier 3: ASCII Art + 边框 + subtitle + model + help
    for (i, line) in title_ascii.iter().enumerate() {
        self.write_gradient_line(buf, area, start_y + 1 + i as u16, line, 0.55);
    }
    Self::center_text(buf, area, start_y + 7, subtitle, dim);
    // model + help below
}
```

**Step 3: 更新边框高度**

Tier 3 边框高度从 6 改为 8（ASCII art 5 行 + subtitle 1 行 + 2 行 padding）。

**Step 4: 写测试**

```rust
#[test]
fn welcome_panel_tier3_ascii_art() {
    let state = WelcomePanelState::new();
    let widget = WelcomePanel::new(&state, "test-model");
    let area = Rect::new(0, 0, 80, 20);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("██"), "Should contain ASCII art block chars");
    assert!(!content.contains("AGENT"), "Should NOT contain AGENT");
}

#[test]
fn welcome_panel_tier1_emoji() {
    let state = WelcomePanelState::new();
    let widget = WelcomePanel::new(&state, "test-model");
    let area = Rect::new(0, 0, 60, 4);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("octo"), "Should contain 'octo' brand");
}
```

**Step 5: 运行测试并提交**

```bash
cargo test -p octo-cli -- --test-threads=1 2>&1 | tail -5
git add crates/octo-cli/src/tui/widgets/welcome_panel/mod.rs
git commit -m "feat(tui): Welcome Panel brand upgrade — ASCII Art OCTO + 🦑 fallback (U4-1)"
```

---

## 最终验证

### Task U-Final: 全量测试 + cargo check

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1
```

预期：所有测试通过，测试数 ≥ 2259 + 新增测试。

提交：

```bash
git add -A
git commit -m "chore: mark Phase U complete — TUI production hardening"
```

---

## Deferred 暂缓项

| ID | 描述 | 原因 |
|----|------|------|
| U-D1 | Debug Panel 深度重设计 — 分列重组、删除冗余数据、增加可交互面板 | 当前仍可用，优先级低于核心交互改进 |

---

## 任务依赖图

```
G1: U1-1 (Approval) ──┐
     U1-2 (Batch)  ───┤── G2: U2-1 (Cache) ──┐
     U1-3 (Scroll) ───┘       U2-2 (Registry) ┤── G3: U3-1 (StatusBar) ──┐
                               U2-3 (Collapse) ┘       U3-2 (Todo)  ─────┤── G4: U4-1 (Welcome)
                                                        U3-3 (Input) ─────┘
```

G1 内部任务互相独立，可并行。G2 依赖 G1 的 batch drain。G3 依赖 G2 的 formatter registry。G4 独立但放最后。
