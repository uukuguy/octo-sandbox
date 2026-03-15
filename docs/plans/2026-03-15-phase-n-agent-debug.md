# Phase N: Workbench Agent 调试面板 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the Dev-Agent debug panel with three-column layout (Sessions + Conversation + Inspector) and 4 Inspector sub-panels (Skill/MCP/Provider/Memory), completing the Workbench mode from AGENT_CLI_DESIGN.md §6.9.2.

**Architecture:** The Dev-Agent panel occupies the DevTask::Agent slot in the dual-view framework (built in M-b). It uses a three-column layout: left column shows active sessions, center column shows the selected session's conversation (messages + tool calls), right column is a switchable Inspector with 4 sub-panels (S:Skill, M:MCP, P:Provider, R:Memory). A context usage bar sits below the left column.

**Tech Stack:** Ratatui 0.29, crossterm, octo-engine (SessionStore, SkillRegistry, McpManager, ProviderChain, MemoryStore)

**Prerequisite:** Phase M-b complete (TUI dual-view framework with Dev view + DevTask selector)

---

### Task 1: Dev-Agent three-column layout skeleton

**Files:**
- Create: `crates/octo-cli/src/tui/screens/dev_agent.rs`
- Modify: `crates/octo-cli/src/tui/screens/mod.rs`

**Step 1: Create DevAgentScreen struct**

```rust
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;
use super::Screen;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFocus { Left, Center, Right }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorPanel { Skill, Mcp, Provider, Memory }

impl InspectorPanel {
    pub fn label(&self) -> &'static str {
        match self {
            InspectorPanel::Skill => "Skill",
            InspectorPanel::Mcp => "MCP",
            InspectorPanel::Provider => "Provider",
            InspectorPanel::Memory => "Memory",
        }
    }
    pub fn key_hint(&self) -> &'static str {
        match self {
            InspectorPanel::Skill => "S",
            InspectorPanel::Mcp => "M",
            InspectorPanel::Provider => "P",
            InspectorPanel::Memory => "R",
        }
    }
}

pub struct DevAgentScreen {
    focus: AgentFocus,
    inspector: InspectorPanel,
    selected_session: usize,
    conversation_scroll: usize,
    inspector_scroll: usize,
}

impl DevAgentScreen {
    pub fn new() -> Self {
        Self {
            focus: AgentFocus::Left,
            inspector: InspectorPanel::Skill,
            selected_session: 0,
            conversation_scroll: 0,
            inspector_scroll: 0,
        }
    }
}
```

**Step 2: Implement Screen trait with three-column layout**

```rust
impl Screen for DevAgentScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),  // Sessions
                Constraint::Percentage(45),  // Conversation
                Constraint::Percentage(35),  // Inspector
            ])
            .split(area);

        // Left column: Sessions + Context bar
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(cols[0]);

        self.render_sessions(frame, left_chunks[0], theme);
        self.render_context_bar(frame, left_chunks[1], theme);

        // Center: Conversation
        self.render_conversation(frame, cols[1], theme);

        // Right: Inspector
        self.render_inspector(frame, cols[2], theme);
    }

    fn title(&self) -> &str { "Agent Debug" }
}
```

**Step 3: Add placeholder render methods**

Each method renders a titled block with placeholder text:
- `render_sessions()`: "Sessions - loading..."
- `render_context_bar()`: "Context: --%" with a gauge
- `render_conversation()`: "Select a session to view conversation"
- `render_inspector()`: Inspector panel label + "[S]kill [M]cp [P]rovider [R]mem" hint bar

**Step 4: Register in ScreenManager**

In `screens/mod.rs`, add `pub mod dev_agent;` and add `dev_agent: dev_agent::DevAgentScreen` to ScreenManager (this will be connected to the Dev view framework from M-b).

**Step 5: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git add crates/octo-cli/src/tui/screens/dev_agent.rs crates/octo-cli/src/tui/screens/mod.rs
git commit -m "feat(tui): Dev-Agent three-column layout skeleton"
```

---

### Task 2: Sessions column — session list with live state

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_agent.rs`

**Step 1: Add session state**

```rust
pub struct DevAgentScreen {
    // ... existing fields ...
    sessions: Vec<SessionSummary>,
    loaded: bool,
}

struct SessionSummary {
    id: String,
    agent_name: String,
    message_count: usize,
    last_active: String,
    is_active: bool,
}
```

**Step 2: Load sessions from AppState**

```rust
fn ensure_loaded(&mut self, state: &AppState) {
    if !self.loaded {
        // Load from state.session_store or similar
        // For now, generate mock data if no real sessions available
        self.loaded = true;
    }
}
```

**Step 3: Render session list**

Each session row: `[*] session-id  agent-name  (N msgs)`
- Active sessions marked with `*`
- Selected session highlighted with accent color
- Inactive sessions dimmed

**Step 4: Handle j/k navigation in left column**

When `focus == Left`, j/k moves `selected_session` up/down. Enter selects a session and loads its conversation into the center column.

**Step 5: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Agent Sessions column with session list"
```

---

### Task 3: Conversation column — message + tool call timeline

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_agent.rs`

**Step 1: Add conversation state**

```rust
pub struct DevAgentScreen {
    // ... existing fields ...
    messages: Vec<ConversationEntry>,
}

enum ConversationEntry {
    UserMessage { content: String },
    AssistantThinking { content: String },
    AssistantMessage { content: String },
    ToolCall { name: String, status: ToolCallStatus, duration_ms: u64 },
}

enum ToolCallStatus { Ok, Error, Blocked }
```

**Step 2: Load conversation when session is selected**

When `selected_session` changes (via Enter), load the session's messages from SessionStore. Map LLM messages to ConversationEntry variants.

**Step 3: Render conversation timeline**

Format each entry as a styled line:
```
User: 帮我重构这段代码...
Agent: [thinking]
  需要先读取文件结构...
Agent: [tool_call]
  file_read -> OK  120ms
Agent: 好的，我来帮你重构...
```

Color coding:
- User messages: white
- Assistant thinking: dim italic
- Tool calls OK: green
- Tool calls Error: red
- Tool calls Blocked: yellow bold

**Step 4: Handle j/k scrolling in center column**

When `focus == Center`, j/k scrolls `conversation_scroll`.

**Step 5: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Agent Conversation column with message timeline"
```

---

### Task 4: Inspector sub-panel — Skill

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_agent.rs`

**Step 1: Add skill state**

```rust
struct SkillInspectorState {
    skills: Vec<SkillInfo>,
    trigger_log: Vec<String>,
    selected: usize,
}

struct SkillInfo {
    name: String,
    status: SkillStatus,  // Loaded, Error, Disabled
    trigger_count: usize,
}
```

**Step 2: Render Skill inspector**

Layout (vertical split within right column):
```
-- Skills --
debugger    loaded  (3 triggers)
git-helper  loaded  (1 trigger)
test-gen    error   (0 triggers)

-- Trigger Log --
[12:30] debugger matched pattern: "重构"
[12:31] git-helper triggered by tool_call
```

**Step 3: Load from SkillRegistry**

When inspector == Skill, load skill list from AppState's SkillRegistry.

**Step 4: Handle j/k scrolling + Enter to expand**

When focused on Inspector with Skill panel, j/k scrolls skill list, Enter shows skill manifest details.

**Step 5: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Agent Inspector Skill sub-panel"
```

---

### Task 5: Inspector sub-panel — MCP + Provider

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_agent.rs`

**Step 1: Add MCP inspector state**

```rust
struct McpInspectorState {
    servers: Vec<McpServerInfo>,
    tools: Vec<McpToolInfo>,
    log_lines: Vec<String>,
    selected: usize,
}

struct McpServerInfo {
    name: String,
    transport: String,  // "stdio" | "sse"
    status: String,     // "connected" | "disconnected" | "error"
    tool_count: usize,
}
```

**Step 2: Render MCP inspector**

Layout:
```
-- MCP Servers --
filesystem  stdio  connected  (5 tools)
github      sse    connected  (12 tools)
database    stdio  error

-- Tools --
file_read, file_write, file_list, ...

-- Logs --
[12:30] filesystem: tool file_read called
```

**Step 3: Add Provider inspector state**

```rust
struct ProviderInspectorState {
    providers: Vec<ProviderInfo>,
    recent_calls: Vec<LlmCallInfo>,
    selected: usize,
}

struct ProviderInfo {
    name: String,
    model: String,
    status: String,
    total_tokens: u64,
    estimated_cost: f64,
}

struct LlmCallInfo {
    timestamp: String,
    model: String,
    input_tokens: u32,
    output_tokens: u32,
    duration_ms: u64,
}
```

**Step 4: Render Provider inspector**

Layout:
```
-- Providers --
anthropic  claude-sonnet  active
  tokens: 12,400  cost: $0.037

openai     gpt-4o         standby
  tokens: 0       cost: $0.000

-- Recent Calls --
[12:30] claude-sonnet  in=2400 out=180  500ms
[12:31] claude-sonnet  in=3100 out=420  740ms
```

**Step 5: Wire S/M/P key switching**

In handle_event, when focus == Right:
- `S` → inspector = Skill
- `M` → inspector = Mcp
- `P` → inspector = Provider
- `R` → inspector = Memory

**Step 6: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Agent Inspector MCP + Provider sub-panels"
```

---

### Task 6: Inspector sub-panel — Memory + Context bar

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_agent.rs`

**Step 1: Add Memory inspector state**

```rust
struct MemoryInspectorState {
    layers: Vec<MemoryLayerInfo>,
    kg_entities: Vec<String>,
    search_results: Vec<String>,
    selected_layer: usize,
}

struct MemoryLayerInfo {
    name: String,        // "L0 Working", "L1 Session", "L2 Persistent"
    entry_count: usize,
    size_bytes: usize,
}
```

**Step 2: Render Memory inspector**

Layout:
```
-- Memory Layers --
L0 Working    12 entries   2.4KB
L1 Session    45 entries   18KB
L2 Persistent 230 entries  1.2MB

-- Knowledge Graph --
Entities: 15  Relations: 23
  "AgentRuntime" -> "ProviderChain"
  "McpManager" -> "McpClient"

-- Search --
[/] to search memory...
```

**Step 3: Implement context usage bar**

```rust
fn render_context_bar(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
    // Show context window usage as a gauge
    // Format: "Context: 62% ========--"
    let usage = 62; // TODO: get from AppState
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(if usage > 80 {
            Style::default().fg(Color::Red)
        } else if usage > 60 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Green)
        })
        .ratio(usage as f64 / 100.0)
        .label(format!("Context: {}%", usage));
    frame.render_widget(gauge, area);
}
```

**Step 4: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Agent Inspector Memory sub-panel + Context bar"
```

---

### Task 7: Three-column linked interaction + tests

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_agent.rs`
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: Implement focus-aware key handling**

```rust
fn handle_event(&mut self, event: &AppEvent) {
    if let AppEvent::Key(key) = event {
        match key.code {
            // Focus navigation
            KeyCode::Char('h') | KeyCode::Left => self.focus_prev(),
            KeyCode::Char('l') | KeyCode::Right => self.focus_next(),
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
            KeyCode::Enter => self.select_current(),
            KeyCode::Esc => self.back(),

            // Inspector sub-panel switching (only when focus == Right)
            KeyCode::Char('s') | KeyCode::Char('S') if self.focus == AgentFocus::Right => {
                self.inspector = InspectorPanel::Skill;
            }
            KeyCode::Char('m') | KeyCode::Char('M') if self.focus == AgentFocus::Right => {
                self.inspector = InspectorPanel::Mcp;
            }
            KeyCode::Char('p') | KeyCode::Char('P') if self.focus == AgentFocus::Right => {
                self.inspector = InspectorPanel::Provider;
            }
            KeyCode::Char('r') | KeyCode::Char('R') if self.focus == AgentFocus::Right => {
                self.inspector = InspectorPanel::Memory;
            }
            _ => {}
        }
    }
}
```

**Step 2: Implement cascading selection**

- `select_current()` on left: load session conversation → update center
- `select_current()` on center: expand message/tool call details → update right
- `scroll_down/up()` routes to correct column based on focus

**Step 3: Visual focus indicator**

Active column border uses `theme.accent`, inactive uses `theme.border_dim`.

**Step 4: Connect to Dev view framework**

In `mod.rs`, wire DevTask::Agent to render `screens.dev_agent.render(...)` and route events to `screens.dev_agent.handle_event(...)`.

**Step 5: Add unit tests**

```rust
#[test]
fn inspector_panel_count_is_4() {
    let panels = [InspectorPanel::Skill, InspectorPanel::Mcp,
                  InspectorPanel::Provider, InspectorPanel::Memory];
    assert_eq!(panels.len(), 4);
}

#[test]
fn inspector_key_hints_unique() {
    let hints: Vec<&str> = [InspectorPanel::Skill, InspectorPanel::Mcp,
                            InspectorPanel::Provider, InspectorPanel::Memory]
        .iter().map(|p| p.key_hint()).collect();
    assert_eq!(hints, vec!["S", "M", "P", "R"]);
}

#[test]
fn agent_focus_cycle() {
    let mut screen = DevAgentScreen::new();
    assert_eq!(screen.focus, AgentFocus::Left);
    screen.focus_next();
    assert_eq!(screen.focus, AgentFocus::Center);
    screen.focus_next();
    assert_eq!(screen.focus, AgentFocus::Right);
    screen.focus_next();
    assert_eq!(screen.focus, AgentFocus::Right); // stays at rightmost
}
```

**Step 6: Full test run and commit**

```bash
cargo test -p octo-cli -- --test-threads=1
cargo check --workspace
git add -A
git commit -m "feat(tui): Dev-Agent linked interaction, tests, Phase N complete"
```

---

## Execution Order

```
Task 1 (Three-column skeleton)
  |
Task 2 (Sessions column)
  |
Task 3 (Conversation column)
  |
Task 4 (Inspector: Skill)  -->  Task 5 (Inspector: MCP + Provider)  -->  Task 6 (Inspector: Memory + Context)
                                                                              |
                                                                        Task 7 (Linked interaction + tests)
```

Tasks 1-3 are sequential (each builds on previous). Tasks 4-6 are sequential (sub-panel by sub-panel). Task 7 is final integration.

---

## Deferred

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D1 | Session 实时数据流（WebSocket 推送会话更新） | octo-server WS 集成 | ✅ 已补 (Phase O G3-T10~T13) |
| D2 | Memory 搜索交互（/键触发搜索输入框） | TUI input widget | ✅ 已补 (Phase O G1-T5) |
| D3 | Provider failover 链路可视化（链式图） | ProviderChain 暴露状态 API | ✅ 已补 (Phase O G2-T7~T9) |
| D4 | 落地 AGENT_CLI_DESIGN.md §6.9.2 完整 Workbench 模式 | Phase N complete + octo-workbench 集成 | ✅ 已补 (Phase O G4-T14) |
