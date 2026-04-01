# Octo-CLI TUI 体验层增强设计

> 基于 CC-OSS TUI 体验层（Ink + React 渲染器，15 个 Spinner 组件，30+ Unicode 符号，6 种动画效果，50+ 快捷键）与 Octo TUI（Ratatui，10 帧 braille spinner，12 主题，呼吸动画，完善快捷键）的对比。
> 日期：2026-04-01
> 目标：将 octo-cli TUI 打磨到 CC 同级的精致程度。

---

## 一、现状评估

### Octo TUI 已有优势

| 维度 | Octo 状态 | 对比 CC |
|------|----------|--------|
| 呼吸动画 (HSL 正弦波) | 状态栏 activity indicator | CC 无此效果 |
| 主题系统 (12 主题) | Cyan/Blue/Violet/Emerald/Amber/... | CC 仅 6 主题 |
| Git 状态集成 | 彩色 staged/modified/untracked/unpushed 计数 | CC 只在 system prompt 注入文本 |
| 5 段上下文条形图 | 绿/黄/橙 渐变 | CC 无 |
| 多行输入 | ❯ 前缀 + 续行缩进 + IME | CC 类似 |
| 工具折叠/展开 | ▶/▼ + 快捷键 | CC 类似 |
| 级联 Escape | 自动完成→取消流→清空输入→重置滚动 | CC 类似 |
| Slash 命令系统 | /help /clear /model /theme 等 | CC 类似 |

### 需要引进的 CC 体验特征

**总共 20 项改进，按 3 级优先排序。**

---

## 二、高价值改进（直接影响用户体验）

### E-1: Unicode 符号集（全量引进）

**新增文件**: `crates/octo-cli/src/tui/figures.rs`

```rust
//! Unicode figures — platform-adaptive visual symbols.

/// Platform-adaptive circle glyph.
pub fn black_circle() -> &'static str {
    if cfg!(target_os = "macos") { "⏺" } else { "●" }
}

// ── Status Indicators ──
pub const BULLET:          &str = "∙";
pub const TEARDROP:        &str = "✻";
pub const CHECK_MARK:      &str = "✓";
pub const CROSS_MARK:      &str = "✗";
pub const FLAG:            &str = "⚑";
pub const REFERENCE_MARK:  &str = "※";

// ── Arrows ──
pub const UP_ARROW:        &str = "↑";
pub const DOWN_ARROW:      &str = "↓";
pub const LIGHTNING:       &str = "↯";
pub const REFRESH_ARROW:   &str = "↻";
pub const LEFT_ARROW:      &str = "←";
pub const RIGHT_ARROW:     &str = "→";

// ── Effort Levels ──
pub const EFFORT_LOW:      &str = "○";
pub const EFFORT_MEDIUM:   &str = "◐";
pub const EFFORT_HIGH:     &str = "●";
pub const EFFORT_MAX:      &str = "◉";

// ── Media Controls ──
pub const PLAY:            &str = "▶";
pub const PAUSE:           &str = "⏸";

// ── Diamonds (state indicators) ──
pub const DIAMOND_OPEN:    &str = "◇";
pub const DIAMOND_FILLED:  &str = "◆";

// ── Visual Separators ──
pub const BLOCKQUOTE_BAR:  &str = "▎";
pub const HEAVY_HORIZONTAL:&str = "━";
pub const ELBOW_BRACKET:   &str = "⎿";

// ── Fork/Agent ──
pub const FORK_GLYPH:      &str = "⑂";

// ── Spinner Characters (CC style, alternative to braille) ──
pub fn spinner_chars() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["·", "✢", "✳", "✶", "✻", "✽"]
    } else {
        &["·", "✢", "*", "✶", "✻", "✽"]
    }
}

// ── Bridge/Connection ──
pub const BRIDGE_FRAMES: &[&str] = &["·|·", "·/·", "·—·", "·\\·"];
pub const BRIDGE_READY:    &str = "·✔·";
pub const BRIDGE_FAILED:   &str = "×";
```

**~60 行**

### E-2: Stalled Animation（超时变色）

当 LLM 调用或工具执行超过阈值无进度时，spinner 从正常色变为警告色/错误色。

```rust
// 在 activity indicator 渲染中
const STALL_WARNING_SECS: u64 = 10;
const STALL_ERROR_SECS: u64 = 30;

fn spinner_color(&self, elapsed_since_last_token: Duration) -> Color {
    let secs = elapsed_since_last_token.as_secs();
    if secs > STALL_ERROR_SECS {
        self.theme.error()       // 红色：可能卡住了
    } else if secs > STALL_WARNING_SECS {
        self.theme.warning()     // 黄色：等待较久
    } else {
        self.theme.accent()      // 正常色
    }
}
```

**~30 行**

### E-3: Spinner 动词（随机活动描述）

替换静态的 "Streaming"/"Thinking" 标签为随机动词，每次 mount 随机选取。

```rust
const SPINNER_VERBS: &[&str] = &[
    "Analyzing", "Architecting", "Building", "Calculating", "Composing",
    "Computing", "Contemplating", "Crafting", "Debugging", "Designing",
    "Developing", "Evaluating", "Examining", "Exploring", "Formulating",
    "Generating", "Implementing", "Investigating", "Iterating", "Mapping",
    "Optimizing", "Orchestrating", "Planning", "Processing", "Reasoning",
    "Refactoring", "Researching", "Resolving", "Reviewing", "Searching",
    "Solving", "Synthesizing", "Testing", "Thinking", "Transforming",
    "Understanding", "Validating", "Weaving", "Working", "Writing",
];

fn random_spinner_verb() -> &'static str {
    use rand::seq::SliceRandom;
    SPINNER_VERBS.choose(&mut rand::thread_rng()).unwrap_or(&"Thinking")
}
```

使用场景：activity indicator 显示 `"⠹ Architecting... 12s"` 而非 `"⠹ Thinking... 12s"`

**~50 行**

### E-4: Shimmer/Glimmer 文字效果

字符级颜色波浪滑过 spinner 标签文字，模拟 CC 的 GlimmerMessage 效果。

```rust
/// 渲染一行带 shimmer 效果的文字
fn render_shimmer(text: &str, tick: u64, base_color: Color, shimmer_color: Color) -> Vec<Span> {
    let cycle_len = text.chars().count() + 20;
    let pos = (tick as usize) % cycle_len;

    text.chars().enumerate().map(|(i, ch)| {
        let dist = (i as isize - pos as isize).unsigned_abs();
        let intensity = if dist < 5 { 1.0 - (dist as f64 / 5.0) } else { 0.0 };
        let color = interpolate_color(base_color, shimmer_color, intensity);
        Span::styled(ch.to_string(), Style::default().fg(color))
    }).collect()
}

fn interpolate_color(c1: Color, c2: Color, t: f64) -> Color {
    if let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (c1, c2) {
        Color::Rgb(
            (r1 as f64 + (r2 as f64 - r1 as f64) * t) as u8,
            (g1 as f64 + (g2 as f64 - g1 as f64) * t) as u8,
            (b1 as f64 + (b2 as f64 - b1 as f64) * t) as u8,
        )
    } else {
        if t > 0.5 { c2 } else { c1 }
    }
}
```

应用场景：
- 思考中 spinner 标签 "⠹ Contemplating..." 带颜色波浪
- 请求中 "Requesting API..." 带快速 shimmer (50ms)
- 正常回复 "Streaming..." 带慢速 glimmer (200ms)

**~80 行**

### E-5: 消息 ⎿ 指示符

CC 用 `⎿` (elbow bracket) 标记助手回复的左侧边距，视觉更简洁优雅。

当前 Octo 用 `─── Assistant ───` header 行占据整行。建议改为：

```
Before (Octo current):
─── Assistant ───
Here is the code...

After (CC style):
  ⎿  Here is the code...
     Second line continues...
```

或者作为可选风格，保留两种。

**~20 行**

### E-6: Effort Indicator

在 status bar 显示当前任务复杂度：

```
🦑 Octo │ claude-sonnet │ ◐ │ 1.5k tokens │ 12s │ ▮▮▮▯▯ 62%
                          ↑ effort level
```

`○` low → `◐` medium → `●` high → `◉` max

可从 LLM 的 thinking 深度或配置的 effort 级别映射。

**~15 行**

---

## 三、中等价值改进（功能增强）

### E-7: Ctrl+R 历史搜索

实现 prompt 历史的增量搜索：

```
(reverse-i-search)`git`: git status --short
```

按 Ctrl+R 进入搜索模式，输入关键词过滤历史条目，Enter 选择，Escape 取消。

**~100 行**

### E-8: Vim 模式

支持基础的 Normal/Insert/Visual 模式切换。Ratatui 已有 `tui-textarea` crate 支持 vim keybinding。

主要需要：
- 模式指示器在 footer 显示 `-- INSERT --` / `-- NORMAL --`
- Normal 模式：h/j/k/l 移动，dd 删除行，yy 复制行，p 粘贴
- 配置项：`settings.vim_mode: true`

**~200 行**

### E-9: Shift+Tab 权限模式循环

在 footer 显示当前权限模式，Shift+Tab 循环切换：

```
ReadOnly → Supervised → Full → ReadOnly
```

配合 P1 PermissionEngine 的权限模式。

**~30 行**

### E-10: Meta+P 模型选择器

弹出浮层列出可用模型，上下选择，Enter 确认：

```
┌─ Select Model ─────────────┐
│ ● claude-sonnet-4-6        │
│   claude-opus-4-6          │
│   gpt-4o                   │
│   qwen-72b                 │
└────────────────────────────┘
```

**~80 行**

### E-11: Meta+O 快速模式切换

单键切换思考深度（Normal ↔ Extended Thinking）。

**~20 行**

### E-12: Ctrl+X Ctrl+E 外部编辑器

将当前输入内容写入临时文件，用 `$EDITOR` 打开，关闭后读回。适合编辑长 prompt。

**~50 行**

### E-13: 选区复制增强

扩展现有 Ctrl+Y（复制最近回复）为支持鼠标选区 + Ctrl+Shift+C 复制选中文本。

**~40 行**

### E-14: 权限请求 UI 增强

增强现有 Approval dialog：
- 显示工具名 + 参数预览
- 风险级别颜色编码（Low=绿, Medium=黄, High=红）
- diff 预览（file_edit 时显示变更内容）
- 快捷键：Y(yes) / N(no) / A(always allow) / D(always deny)

**~60 行**

---

## 四、品质感改进

### E-15: Thinking shimmer

思考模式下，"Thinking" 标签颜色缓慢正弦波动（灰色 ↔ 浅灰色，2 秒周期）。

**~30 行**

### E-16: Byline middot 分隔

底栏提示统一用 `·` (middot) 分隔：

```
Before: Enter: submit | Esc: cancel | Ctrl+C: exit
After:  Enter to submit · Esc to cancel · Ctrl+C to exit
```

**~10 行**

### E-17: 多 Session Spinner Tree

多 agent 并行时，status 区域显示树形进度：

```
⠹ Main session (Streaming)
  ├─ ⠼ researcher (Searching)
  └─ ⠧ coder (Editing file)
```

**~100 行**

### E-18: Reduced Motion 配置

`config.yaml` 中 `reduced_motion: true` 时关闭所有动画，用静态符号替代。

**~20 行**

### E-19: 上下文感知快捷键提示

底栏根据当前状态显示不同的快捷键提示：

- 空闲时：`Enter to chat · /help for commands · Ctrl+D to debug`
- 流式中：`Esc to stop · Ctrl+O toggle tools`
- 审批中：`Y accept · N deny · A always allow`

**~40 行**

### E-20: 时间格式增强

增强 `format_elapsed`：
- 短格式：`5s` / `1m12s` / `1h5m`（已有）
- 新增：超过 1 小时只显示最高单位 `1h`（hideTrailingZeros）
- 新增：亚秒精度 `0.5s`

**~15 行**

---

## 五、实施分组

### Phase 1: 立即可做（纯视觉，无功能依赖）

| 编号 | 内容 | 代码量 |
|------|------|--------|
| E-1 | Unicode 符号集 `figures.rs` | ~60 行 |
| E-2 | Stalled animation | ~30 行 |
| E-3 | Spinner verbs | ~50 行 |
| E-5 | 消息 ⎿ 指示符 | ~20 行 |
| E-6 | Effort indicator | ~15 行 |
| E-15 | Thinking shimmer | ~30 行 |
| E-16 | Byline middot 分隔 | ~10 行 |
| E-18 | Reduced motion | ~20 行 |
| E-19 | 快捷键提示 | ~40 行 |
| E-20 | 时间格式增强 | ~15 行 |
| **小计** | | **~290 行** |

### Phase 2: Shimmer 效果 + 快捷键增强

| 编号 | 内容 | 代码量 |
|------|------|--------|
| E-4 | Shimmer/Glimmer 效果 | ~80 行 |
| E-7 | Ctrl+R 历史搜索 | ~100 行 |
| E-9 | Shift+Tab 模式循环 | ~30 行 |
| E-11 | Meta+O 快速模式 | ~20 行 |
| E-12 | 外部编辑器 | ~50 行 |
| E-13 | 选区复制 | ~40 行 |
| E-14 | 权限请求 UI | ~60 行 |
| **小计** | | **~380 行** |

### Phase 3: 大功能

| 编号 | 内容 | 代码量 |
|------|------|--------|
| E-8 | Vim 模式 | ~200 行 |
| E-10 | 模型选择器 | ~80 行 |
| E-17 | 多 Session Spinner Tree | ~100 行 |
| **小计** | | **~380 行** |

### 总计: ~1050 行

---

## 六、与其他设计的关联

| 关联 | 影响 |
|------|------|
| P1 PermissionEngine | E-9 权限模式循环 + E-14 权限 UI 增强 |
| 多 Agent 编排 | E-17 多 Session Spinner Tree |
| P0 上下文管理 | E-2 Stalled animation 在压缩期间也需要状态显示 |
| Tool 接口 P0-6 (streaming progress) | E-2 stalled detection 配合工具进度流 |
