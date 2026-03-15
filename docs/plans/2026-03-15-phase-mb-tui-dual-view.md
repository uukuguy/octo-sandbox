# Phase M-b: TUI 双视图 + Eval 面板 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor the TUI from 12-tab flat layout into Ops/Dev dual-view architecture, with a three-column Eval panel in Dev view.

**Architecture:** Introduce ViewMode enum (Ops/Dev) into the TUI App struct. Ops view keeps 6 tabs with existing Tab-switching UX. Dev view replaces tabs with a 2-task panel selector (Agent placeholder + Eval). The Eval panel is a three-column linked layout (RunHistory -> TaskResults -> DetailInspector) reading from RunStore (built in M-a).

**Tech Stack:** Ratatui 0.29, crossterm, octo-eval (RunStore, EvalTrace, FailureClass)

**Prerequisite:** Phase M-a complete (RunStore + `octo eval` commands)

---

### Task 1: ViewMode enum + Ctrl+O/D switching

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs`
- Modify: `crates/octo-cli/src/tui/event.rs`

**Step 1: Add ViewMode and events**

In `event.rs`, add new event variants:

```rust
pub enum AppEvent {
    // ... existing variants ...
    /// Switch to Ops view
    SwitchToOps,
    /// Switch to Dev view
    SwitchToDev,
}
```

In `mod.rs`, add ViewMode:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Ops,
    Dev,
}
```

Add `view_mode: ViewMode` field to `App` struct, default to `ViewMode::Dev`.

**Step 2: Wire Ctrl+O / Ctrl+D in event loop**

In `run_event_loop()`, add key handlers:

```rust
(KeyCode::Char('o'), KeyModifiers::CONTROL) => {
    app.handle_event(AppEvent::SwitchToOps);
}
(KeyCode::Char('d'), KeyModifiers::CONTROL) => {
    app.handle_event(AppEvent::SwitchToDev);
}
```

In `App::handle_event()`:

```rust
AppEvent::SwitchToOps => {
    self.view_mode = ViewMode::Ops;
    self.status_message = Some("Switched to Ops view".to_string());
}
AppEvent::SwitchToDev => {
    self.view_mode = ViewMode::Dev;
    self.status_message = Some("Switched to Dev view".to_string());
}
```

**Step 3: Update render to branch on ViewMode**

In `App::render()`, branch on `self.view_mode`:

```rust
match self.view_mode {
    ViewMode::Ops => self.render_ops_view(frame, chunks[1]),
    ViewMode::Dev => self.render_dev_view(frame, chunks[1]),
}
```

Tab bar rendering also changes per mode (Ops shows tabs, Dev shows task selector).

**Step 4: Update status bar to show current mode**

```rust
let mode_label = match self.view_mode {
    ViewMode::Ops => "[Ops]",
    ViewMode::Dev => "[Dev]",
};
let default_msg = format!("{} Press ? for help | Ctrl+O/D switch view | q quit", mode_label);
```

**Step 5: Run tests and commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git add crates/octo-cli/src/tui/
git commit -m "feat(tui): ViewMode Ops/Dev with Ctrl+O/D switching"
```

---

### Task 2: Ops view — 6 Tab subset

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs`

**Step 1: Define OpsTab enum**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpsTab {
    Dashboard,
    Agents,
    Sessions,
    Mcp,
    Security,
    Logs,
}

impl OpsTab {
    pub fn all() -> &'static [OpsTab] {
        &[OpsTab::Dashboard, OpsTab::Agents, OpsTab::Sessions,
          OpsTab::Mcp, OpsTab::Security, OpsTab::Logs]
    }
    pub fn label(&self) -> &'static str { /* ... */ }
    pub fn index(&self) -> usize { /* ... */ }
    pub fn from_index(idx: usize) -> Self { /* ... */ }
}
```

**Step 2: Add `ops_tab: OpsTab` to App**

Replace Ops-mode tab navigation to use OpsTab instead of full Tab enum.

**Step 3: Implement `render_ops_view()`**

Route to existing screens (dashboard, agents, sessions, mcp, security, logs). Render tab bar with only 6 items. Digits 1-6 select tabs.

**Step 4: Verify existing Ops screens still render correctly**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Ops view with 6-tab subset"
```

---

### Task 3: Dev view framework — 2 task selector

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs`
- Create: `crates/octo-cli/src/tui/screens/dev_eval.rs`

**Step 1: Define DevTask enum**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevTask {
    Agent,  // placeholder for Phase N
    Eval,
}

impl DevTask {
    pub fn all() -> &'static [DevTask] { &[DevTask::Agent, DevTask::Eval] }
    pub fn label(&self) -> &'static str {
        match self { DevTask::Agent => "Agent Debug", DevTask::Eval => "Eval" }
    }
}
```

**Step 2: Add `dev_task: DevTask` to App**

**Step 3: Implement `render_dev_view()`**

```rust
fn render_dev_view(&mut self, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    // Task selector bar: "1:Agent Debug  2:Eval"
    self.render_dev_task_bar(frame, chunks[0]);

    match self.dev_task {
        DevTask::Agent => self.render_agent_placeholder(frame, chunks[1]),
        DevTask::Eval => self.screens.dev_eval.render(frame, chunks[1], &self.theme, &self.state),
    }
}
```

**Step 4: Wire digit keys 1-2 in Dev mode**

In event loop, when `view_mode == Dev`, digits 1-2 switch `dev_task`.

**Step 5: Create dev_eval.rs with placeholder**

```rust
pub struct DevEvalScreen;
impl DevEvalScreen {
    pub fn new() -> Self { Self }
}
impl Screen for DevEvalScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Eval ");
        frame.render_widget(Paragraph::new("Eval panel - loading...").block(block), area);
    }
    fn title(&self) -> &str { "Eval" }
}
```

Register in ScreenManager.

**Step 6: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev view framework with Agent/Eval task selector"
```

---

### Task 4: Dev-Eval left column — Run History

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_eval.rs`

**Step 1: Add state to DevEvalScreen**

```rust
pub struct DevEvalScreen {
    runs: Vec<RunManifest>,
    selected_run: usize,
    focus: EvalFocus,    // Left | Center | Right
    loaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EvalFocus { Left, Center, Right }
```

**Step 2: Load runs from RunStore on first render**

```rust
fn ensure_loaded(&mut self, state: &AppState) {
    if !self.loaded {
        let store = RunStore::new(PathBuf::from("eval_output/runs"));
        if let Ok(store) = store {
            let filter = RunFilter { limit: 50, ..Default::default() };
            self.runs = store.list_runs(&filter).unwrap_or_default();
        }
        self.loaded = true;
    }
}
```

**Step 3: Render three-column layout**

```rust
let cols = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([
        Constraint::Percentage(25),  // Run History
        Constraint::Percentage(35),  // Task Results
        Constraint::Percentage(40),  // Detail
    ])
    .split(area);
```

**Step 4: Render Run History list**

Render each run as: `run_id  suite  pass_rate%  passed/total`
Highlight selected run. Show tag if present.

**Step 5: Handle j/k navigation in left column**

When `focus == Left`, j/k moves `selected_run` up/down.

**Step 6: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Eval Run History column with RunStore loading"
```

---

### Task 5: Dev-Eval center column — Task Results + Dimensions

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_eval.rs`

**Step 1: Add task results state**

```rust
pub struct DevEvalScreen {
    // ... existing fields ...
    tasks: Vec<TaskResultSummary>,    // from loaded run's report
    selected_task: usize,
    current_run_data: Option<RunData>,
}
```

**Step 2: Load run data when selected_run changes**

When user selects a run (Enter or changes selection), load full `RunData` from RunStore. Extract `task_results` from the report.

**Step 3: Render task list**

Each task row: `[OK/NG] task_id  score  duration`
Highlight selected task. Color OK green, NG red.

**Step 4: Render Dimensions section below task list**

When a task is selected, show its `dimensions` HashMap as key-value pairs.

**Step 5: Render Failure Summary section**

Show FailureSummary from the run manifest (WrongTool: N, WrongArgs: N, etc.)

**Step 6: Handle j/k in center column, h/l to switch focus**

**Step 7: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Eval Task Results column with dimensions"
```

---

### Task 6: Dev-Eval right column — Timeline + Failure detail

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_eval.rs`

**Step 1: Add trace state**

```rust
pub struct DevEvalScreen {
    // ... existing fields ...
    current_trace: Option<EvalTrace>,
    timeline_scroll: usize,
}
```

**Step 2: Load trace when selected_task changes**

Find the matching trace from `current_run_data.traces` by task_id.

**Step 3: Render Timeline section**

Format each TraceEvent as a single line:
```
[{timestamp}ms] {event_type}  {summary}
```

Color-code by event type:
- RoundStart: dim
- LlmCall: blue
- ToolCall success: green
- ToolCall failure: red
- SecurityBlocked: yellow bold
- Error: red bold
- Completed: cyan

**Step 4: Render Failure detail section**

If the task's score has `failure_class`, render it:
```
Failure: {class_name}
  {field}: {value}
```

**Step 5: Handle j/k scrolling in right column**

**Step 6: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Eval Detail Inspector with timeline and failure"
```

---

### Task 7: Three-column linked interaction + shortcuts

**Files:**
- Modify: `crates/octo-cli/src/tui/screens/dev_eval.rs`

**Step 1: Implement focus-aware key handling**

```rust
fn handle_event(&mut self, event: &AppEvent) {
    if let AppEvent::Key(key) = event {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => self.focus_prev(),
            KeyCode::Char('l') | KeyCode::Right => self.focus_next(),
            KeyCode::Char('j') | KeyCode::Down => self.scroll_down(),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_up(),
            KeyCode::Enter => self.select_current(),
            KeyCode::Esc => self.back(),
            KeyCode::Char('r') => self.trigger_run(),
            KeyCode::Char('d') => self.trigger_diff(),
            KeyCode::Char('/') => self.toggle_filter(),
            KeyCode::Char('t') => self.trigger_tag(),
            _ => {}
        }
    }
}
```

**Step 2: Implement cascading selection**

- `select_current()` on left column: load run data -> update center
- `select_current()` on center column: load trace -> update right
- `scroll_down/up()` routes to correct column based on focus

**Step 3: Visual focus indicator**

Active column border is `theme.accent`, inactive is `theme.border_dim`.

**Step 4: Implement shortcut actions**

- `r`: set status "Run which suite? (not yet implemented)"
- `d`: set status "Select second run for diff (not yet implemented)"
- `t`: set status "Enter tag (not yet implemented)"

These are placeholders — full implementation requires input dialogs (future work).

**Step 5: Commit**

```bash
cargo test -p octo-cli -- --test-threads=1
git commit -m "feat(tui): Dev-Eval three-column linked interaction"
```

---

### Task 8: Welcome/Settings overlays + tests

**Files:**
- Modify: `crates/octo-cli/src/tui/mod.rs`
- Modify: `crates/octo-cli/src/tui/screens/mod.rs`

**Step 1: Convert Welcome to overlay**

Add `show_help: bool` to App. `?` key toggles it. Render welcome content as a centered popup over current view (both Ops and Dev).

**Step 2: Convert Settings to overlay**

Add `show_settings: bool` to App. `,` key toggles it. Render settings as centered popup.

**Step 3: Add unit tests**

```rust
#[test]
fn view_mode_default_is_dev() {
    assert_eq!(ViewMode::default(), ViewMode::Dev);
}

#[test]
fn ops_tab_count_is_6() {
    assert_eq!(OpsTab::all().len(), 6);
}

#[test]
fn dev_task_count_is_2() {
    assert_eq!(DevTask::all().len(), 2);
}

#[test]
fn ops_tab_roundtrip() {
    for tab in OpsTab::all() {
        assert_eq!(OpsTab::from_index(tab.index()), *tab);
    }
}

#[test]
fn eval_focus_cycle() {
    let mut focus = EvalFocus::Left;
    focus = match focus { EvalFocus::Left => EvalFocus::Center, _ => EvalFocus::Left };
    assert_eq!(focus, EvalFocus::Center);
}
```

**Step 4: Full test run and commit**

```bash
cargo test -p octo-cli -- --test-threads=1
cargo check --workspace
git add -A
git commit -m "feat(tui): Welcome/Settings overlays, dual-view tests, Phase M-b complete"
```

---

## Execution Order

```
Task 1 (ViewMode + switching)
  |
Task 2 (Ops 6-tab)
  |
Task 3 (Dev framework + task selector)
  |
Task 4 (Eval left: Run History)  -->  Task 5 (Eval center: Tasks)  -->  Task 6 (Eval right: Detail)
                                                                              |
                                                                        Task 7 (Linked interaction)
                                                                              |
                                                                        Task 8 (Overlays + tests)
```

Tasks 1-3 are sequential (each builds on previous). Tasks 4-6 are sequential (column by column). Task 7 depends on 4-6. Task 8 is final integration.

---

## Deferred

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D1 | Eval shortcut dialogs (run/diff/tag input) | TUI input widget | ✅ 已补 (Phase O G1-T3) |
| D2 | Eval filter popup (suite/date/tag) | TUI input widget | ✅ 已补 (Phase O G1-T4) |
| D3 | Dev-Agent panel (Phase N) | M-b complete | ✅ 已补 (Phase N) |
