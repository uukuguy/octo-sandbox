//! Dev view — Eval panel screen (three-column layout)
//!
//! Left:   Run history list from RunStore
//! Center: Task results + failure summary for the selected run
//! Right:  Timeline events + score dimensions for the selected task

use std::path::PathBuf;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use octo_eval::recorder::EvalTrace;
use octo_eval::reporter::TaskResultSummary;
use octo_eval::run_store::{RunData, RunFilter, RunManifest, RunStore};
use octo_eval::trace::TraceEvent;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;
use crate::tui::widgets::TextInput;

use super::Screen;

/// Input mode for the eval screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalInputMode {
    Normal,
    Search,
    RunSuite,
    DiffInput,
    Filter,
}

/// Filter target type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTarget {
    Suite,
    Tag,
}

/// Which column is focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalFocus {
    Left,
    Center,
    Right,
}

impl EvalFocus {
    /// Move focus to the right, wrapping around
    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Center,
            Self::Center => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// Move focus to the left, wrapping around
    pub fn prev(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Center => Self::Left,
            Self::Right => Self::Center,
        }
    }
}

/// Three-column panel for the Dev view
pub struct DevEvalScreen {
    /// Loaded run manifests
    runs: Vec<RunManifest>,
    /// Selected index in the left column
    selected_run: usize,
    /// Which column has keyboard focus
    pub focus: EvalFocus,
    /// Whether initial load has been done
    pub loaded: bool,
    /// Task results from the selected run's report
    tasks: Vec<TaskResultSummary>,
    /// Selected index in the center column
    selected_task: usize,
    /// Full run data for the currently selected run
    current_run_data: Option<Box<RunData>>,
    /// Trace for the currently selected task
    current_trace: Option<EvalTrace>,
    /// Scroll offset for the timeline in the right column
    timeline_scroll: usize,
    /// Status message for shortcut feedback
    status_msg: Option<String>,
    /// Current input mode
    input_mode: EvalInputMode,
    /// Search input
    search_input: TextInput,
    /// Command input (for run/diff)
    command_input: TextInput,
    /// Active filter target
    filter_target: Option<FilterTarget>,
    /// Filter input
    filter_input: TextInput,
    /// Current applied filter text
    active_filter: Option<String>,
}

impl DevEvalScreen {
    pub fn new() -> Self {
        Self {
            runs: Vec::new(),
            selected_run: 0,
            focus: EvalFocus::Left,
            loaded: false,
            tasks: Vec::new(),
            selected_task: 0,
            current_run_data: None,
            current_trace: None,
            timeline_scroll: 0,
            status_msg: None,
            input_mode: EvalInputMode::Normal,
            search_input: TextInput::new("Search runs..."),
            command_input: TextInput::new(""),
            filter_target: None,
            filter_input: TextInput::new("Filter..."),
            active_filter: None,
        }
    }

    /// Try to load runs from the default RunStore directory.
    fn load_runs(&mut self) {
        let base = PathBuf::from("eval_output/runs");
        if !base.exists() {
            self.runs = Vec::new();
            self.loaded = true;
            return;
        }
        match RunStore::new(base) {
            Ok(store) => {
                self.runs = store
                    .list_runs(&RunFilter::default())
                    .unwrap_or_default();
            }
            Err(_) => {
                self.runs = Vec::new();
            }
        }
        self.loaded = true;
    }

    /// Load full run data for the currently selected run.
    fn load_selected_run(&mut self) {
        if self.runs.is_empty() {
            self.tasks.clear();
            self.current_run_data = None;
            self.current_trace = None;
            return;
        }

        let run_id = &self.runs[self.selected_run].run_id;
        let base = PathBuf::from("eval_output/runs");
        let store = match RunStore::new(base) {
            Ok(s) => s,
            Err(_) => {
                self.tasks.clear();
                self.current_run_data = None;
                return;
            }
        };

        match store.load_run(run_id) {
            Ok(data) => {
                self.tasks = data
                    .report
                    .as_ref()
                    .map(|r| r.task_results.clone())
                    .unwrap_or_default();
                self.selected_task = 0;
                self.current_trace = None;
                self.timeline_scroll = 0;
                self.current_run_data = Some(Box::new(data));
            }
            Err(_) => {
                self.tasks.clear();
                self.current_run_data = None;
                self.current_trace = None;
            }
        }
    }

    /// Load the trace for the currently selected task.
    fn load_selected_trace(&mut self) {
        if self.tasks.is_empty() {
            self.current_trace = None;
            self.timeline_scroll = 0;
            return;
        }

        let task_id = &self.tasks[self.selected_task].task_id;
        if let Some(ref data) = self.current_run_data {
            self.current_trace = data
                .traces
                .iter()
                .find(|t| t.task_id == *task_id)
                .cloned();
            self.timeline_scroll = 0;
        }
    }

    // -- Rendering helpers --

    fn render_columns(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ])
            .split(area);

        self.render_left(frame, columns[0], theme);
        self.render_center(frame, columns[1], theme);
        self.render_right(frame, columns[2], theme);
    }

    /// Return visible runs after applying the active filter.
    fn visible_runs(&self) -> Vec<(usize, &RunManifest)> {
        self.runs
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                if let Some(ref filter) = self.active_filter {
                    if let Some(suite_filter) = filter.strip_prefix("suite:") {
                        return m.suite.to_lowercase().contains(&suite_filter.to_lowercase());
                    }
                    if let Some(tag_filter) = filter.strip_prefix("tag:") {
                        return m
                            .tag
                            .as_ref()
                            .map(|t| t.to_lowercase().contains(&tag_filter.to_lowercase()))
                            .unwrap_or(false);
                    }
                }
                true
            })
            .collect()
    }

    fn render_left(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let visible = self.visible_runs();
        let title = if let Some(ref filter) = self.active_filter {
            format!(" Runs ({}) [{}] ", visible.len(), filter)
        } else {
            format!(" Runs ({}) ", self.runs.len())
        };
        let block = if self.focus == EvalFocus::Left {
            theme.styled_block_active(&title)
        } else {
            theme.styled_block(&title)
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.runs.is_empty() {
            let msg = Paragraph::new("No runs found.\nRun `octo eval run` first.")
                .style(theme.text_dim())
                .wrap(Wrap { trim: false });
            frame.render_widget(msg, inner);
            return;
        }

        let items: Vec<ListItem> = self
            .runs
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let tag_str = m
                    .tag
                    .as_ref()
                    .map(|t| format!(" [{}]", t))
                    .unwrap_or_default();
                let line = format!(
                    "{}{} {} {:.0}% {}/{}",
                    m.run_id,
                    tag_str,
                    truncate_str(&m.suite, 12),
                    m.pass_rate * 100.0,
                    m.passed,
                    m.task_count,
                );
                let style = if i == self.selected_run {
                    theme.list_selected()
                } else {
                    theme.text_normal()
                };
                ListItem::new(line).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }

    fn render_center(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let block = if self.focus == EvalFocus::Center {
            theme.styled_block_active(" Tasks ")
        } else {
            theme.styled_block(" Tasks ")
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.tasks.is_empty() {
            let msg = if self.runs.is_empty() {
                "Select a run"
            } else {
                "No task results"
            };
            frame.render_widget(
                Paragraph::new(msg).style(theme.text_dim()),
                inner,
            );
            return;
        }

        // Split: task list (top 70%) + failure summary (bottom 30%)
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(inner);

        // Task list
        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let marker = if t.passed { "OK" } else { "NG" };
                let line = format!(
                    "[{}] {} {:.2} {}ms",
                    marker,
                    truncate_str(&t.task_id, 20),
                    t.score,
                    t.duration_ms,
                );
                let base_style = if i == self.selected_task {
                    theme.list_selected()
                } else if t.passed {
                    theme.status_ok()
                } else {
                    theme.status_error()
                };
                ListItem::new(line).style(base_style)
            })
            .collect();
        let list = List::new(items);
        frame.render_widget(list, sections[0]);

        // Failure summary
        let failure_block = Block::default()
            .title("Failures")
            .title_style(theme.block_title())
            .borders(Borders::TOP)
            .border_style(theme.block_border());
        let failure_inner = failure_block.inner(sections[1]);
        frame.render_widget(failure_block, sections[1]);

        if let Some(ref data) = self.current_run_data {
            let summary = &data.manifest.failure_summary;
            if summary.by_class.is_empty() && summary.total_classified == 0 {
                frame.render_widget(
                    Paragraph::new("No failures classified")
                        .style(theme.text_dim()),
                    failure_inner,
                );
            } else {
                let mut lines: Vec<Line> = summary
                    .by_class
                    .iter()
                    .map(|(class, count)| {
                        Line::from(format!("  {}: {}", class, count))
                    })
                    .collect();
                if summary.total_unclassified > 0 {
                    lines.push(Line::from(format!(
                        "  unclassified: {}",
                        summary.total_unclassified
                    )));
                }
                let text = Text::from(lines);
                frame.render_widget(
                    Paragraph::new(text)
                        .style(theme.text_normal())
                        .wrap(Wrap { trim: false }),
                    failure_inner,
                );
            }
        }
    }

    fn render_right(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let block = if self.focus == EvalFocus::Right {
            theme.styled_block_active(" Timeline ")
        } else {
            theme.styled_block(" Timeline ")
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let trace = match &self.current_trace {
            Some(t) => t,
            None => {
                frame.render_widget(
                    Paragraph::new("Select a task to view trace")
                        .style(theme.text_dim()),
                    inner,
                );
                return;
            }
        };

        // Split: timeline (top 70%) + score details (bottom 30%)
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(inner);

        // Timeline events
        let timeline_lines: Vec<Line> = trace
            .timeline
            .iter()
            .skip(self.timeline_scroll)
            .map(|evt| format_trace_event(evt, theme))
            .collect();

        let timeline_text = Text::from(timeline_lines);
        frame.render_widget(
            Paragraph::new(timeline_text).wrap(Wrap { trim: false }),
            sections[0],
        );

        // Score details
        let score_block = Block::default()
            .title("Score")
            .title_style(theme.block_title())
            .borders(Borders::TOP)
            .border_style(theme.block_border());
        let score_inner = score_block.inner(sections[1]);
        frame.render_widget(score_block, sections[1]);

        let score = &trace.score;
        let mut lines: Vec<Line> = Vec::new();

        // Failure class
        if let Some(ref fc) = score.failure_class {
            lines.push(Line::styled(
                format!("  Failure: {:?}", fc),
                theme.status_error(),
            ));
        }

        // Dimensions
        if !score.dimensions.is_empty() {
            lines.push(Line::styled("  Dimensions:", theme.text_normal()));
            for (key, val) in &score.dimensions {
                lines.push(Line::from(format!("    {}: {:.3}", key, val)));
            }
        }

        // Basic score info
        let pass_style = if score.passed {
            theme.status_ok()
        } else {
            theme.status_error()
        };
        lines.push(Line::styled(
            format!(
                "  {} score={:.3}",
                if score.passed { "PASS" } else { "FAIL" },
                score.score
            ),
            pass_style,
        ));

        let score_text = Text::from(lines);
        frame.render_widget(
            Paragraph::new(score_text).wrap(Wrap { trim: false }),
            score_inner,
        );
    }
}

impl Screen for DevEvalScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        if !self.loaded {
            self.load_runs();
            if !self.runs.is_empty() {
                self.load_selected_run();
            }
        }

        // Calculate main area and optional input bar at bottom
        let (main_area, input_area) = if self.input_mode != EvalInputMode::Normal {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        // Show status message if present
        if let Some(ref msg) = self.status_msg {
            // Reserve 1 line at top for status
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(main_area);
            frame.render_widget(
                Paragraph::new(msg.clone()).style(theme.status_warn()),
                chunks[0],
            );
            self.render_columns(frame, chunks[1], theme);
        } else {
            self.render_columns(frame, main_area, theme);
        }

        // Render input bar when in an input mode
        if let Some(input_area) = input_area {
            let (label, input) = match self.input_mode {
                EvalInputMode::Search => ("Search", &self.search_input),
                EvalInputMode::RunSuite => ("Run Suite", &self.command_input),
                EvalInputMode::DiffInput => ("Diff (A B)", &self.command_input),
                EvalInputMode::Filter => {
                    let target_label = match self.filter_target {
                        Some(FilterTarget::Tag) => "Filter [Tag]",
                        _ => "Filter [Suite]",
                    };
                    (target_label, &self.filter_input)
                }
                EvalInputMode::Normal => unreachable!(),
            };
            let title = format!(" {} ", label);
            let block = theme.styled_block_active(&title);
            input.render(frame, input_area, theme, Some(block));
        }
    }

    fn handle_event(&mut self, event: &AppEvent) {
        // Clear status on any key press
        if matches!(event, AppEvent::Key(_)) {
            self.status_msg = None;
        }

        if let AppEvent::Key(key) = event {
            // When in input mode, delegate to the active TextInput first
            match self.input_mode {
                EvalInputMode::Search => {
                    match key.code {
                        KeyCode::Enter => {
                            // Apply search filter
                            self.input_mode = EvalInputMode::Normal;
                            self.search_input.deactivate();
                        }
                        KeyCode::Esc => {
                            self.input_mode = EvalInputMode::Normal;
                            self.search_input.clear();
                        }
                        _ => {
                            self.search_input.handle_key(key.code);
                        }
                    }
                    return;
                }
                EvalInputMode::RunSuite => {
                    match key.code {
                        KeyCode::Enter => {
                            let suite = self.command_input.value().to_string();
                            self.status_msg =
                                Some(format!("Running suite: {} (pending)", suite));
                            self.input_mode = EvalInputMode::Normal;
                            self.command_input.clear();
                        }
                        KeyCode::Esc => {
                            self.input_mode = EvalInputMode::Normal;
                            self.command_input.clear();
                        }
                        _ => {
                            self.command_input.handle_key(key.code);
                        }
                    }
                    return;
                }
                EvalInputMode::DiffInput => {
                    match key.code {
                        KeyCode::Enter => {
                            let ids = self.command_input.value().to_string();
                            self.status_msg =
                                Some(format!("Diff: {} (pending)", ids));
                            self.input_mode = EvalInputMode::Normal;
                            self.command_input.clear();
                        }
                        KeyCode::Esc => {
                            self.input_mode = EvalInputMode::Normal;
                            self.command_input.clear();
                        }
                        _ => {
                            self.command_input.handle_key(key.code);
                        }
                    }
                    return;
                }
                EvalInputMode::Filter => {
                    match key.code {
                        KeyCode::Enter => {
                            let value = self.filter_input.value().to_string();
                            if !value.is_empty() {
                                let prefix = match self.filter_target {
                                    Some(FilterTarget::Tag) => "tag:",
                                    _ => "suite:",
                                };
                                self.active_filter =
                                    Some(format!("{}{}", prefix, value));
                            }
                            self.input_mode = EvalInputMode::Normal;
                            self.filter_input.clear();
                            self.filter_target = None;
                        }
                        KeyCode::Esc => {
                            self.input_mode = EvalInputMode::Normal;
                            self.filter_input.clear();
                            self.filter_target = None;
                        }
                        KeyCode::Tab => {
                            // Cycle filter target between Suite and Tag
                            self.filter_target = match self.filter_target {
                                Some(FilterTarget::Suite) | None => {
                                    Some(FilterTarget::Tag)
                                }
                                Some(FilterTarget::Tag) => {
                                    Some(FilterTarget::Suite)
                                }
                            };
                        }
                        _ => {
                            self.filter_input.handle_key(key.code);
                        }
                    }
                    return;
                }
                EvalInputMode::Normal => {}
            }

            // Normal mode key handling
            match key.code {
                // Navigation within focused column
                KeyCode::Char('j') | KeyCode::Down => match self.focus {
                    EvalFocus::Left => {
                        if !self.runs.is_empty() {
                            let new = (self.selected_run + 1).min(self.runs.len() - 1);
                            if new != self.selected_run {
                                self.selected_run = new;
                                self.load_selected_run();
                            }
                        }
                    }
                    EvalFocus::Center => {
                        if !self.tasks.is_empty() {
                            let new = (self.selected_task + 1).min(self.tasks.len() - 1);
                            if new != self.selected_task {
                                self.selected_task = new;
                                self.load_selected_trace();
                            }
                        }
                    }
                    EvalFocus::Right => {
                        if let Some(ref trace) = self.current_trace {
                            if self.timeline_scroll + 1 < trace.timeline.len() {
                                self.timeline_scroll += 1;
                            }
                        }
                    }
                },
                KeyCode::Char('k') | KeyCode::Up => match self.focus {
                    EvalFocus::Left => {
                        if self.selected_run > 0 {
                            self.selected_run -= 1;
                            self.load_selected_run();
                        }
                    }
                    EvalFocus::Center => {
                        if self.selected_task > 0 {
                            self.selected_task -= 1;
                            self.load_selected_trace();
                        }
                    }
                    EvalFocus::Right => {
                        self.timeline_scroll = self.timeline_scroll.saturating_sub(1);
                    }
                },
                // Focus switching
                KeyCode::Char('l') | KeyCode::Right => {
                    self.focus = self.focus.next();
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.focus = self.focus.prev();
                }
                // Enter: drill down
                KeyCode::Enter => match self.focus {
                    EvalFocus::Left => {
                        self.load_selected_run();
                        self.focus = EvalFocus::Center;
                    }
                    EvalFocus::Center => {
                        self.load_selected_trace();
                        self.focus = EvalFocus::Right;
                    }
                    EvalFocus::Right => {}
                },
                // Shortcut keys
                KeyCode::Char('r') => {
                    self.input_mode = EvalInputMode::RunSuite;
                    self.command_input.set_value("");
                    self.command_input.activate();
                }
                KeyCode::Char('d') => {
                    self.input_mode = EvalInputMode::DiffInput;
                    self.command_input.set_value("");
                    self.command_input.activate();
                }
                KeyCode::Char('/') => {
                    self.input_mode = EvalInputMode::Search;
                    self.search_input.clear();
                    self.search_input.activate();
                }
                KeyCode::Char('f') => {
                    self.input_mode = EvalInputMode::Filter;
                    self.filter_target = Some(FilterTarget::Suite);
                    self.filter_input.clear();
                    self.filter_input.activate();
                }
                KeyCode::Char('F') => {
                    // Clear active filter
                    self.active_filter = None;
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Eval"
    }
}

// -- Helpers --

/// Truncate string to max_len, appending ".." if truncated
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let boundary = s.floor_char_boundary(max_len.saturating_sub(2));
        format!("{}..", &s[..boundary])
    }
}

/// Format a TraceEvent as a styled Line for the timeline view.
fn format_trace_event<'a>(event: &TraceEvent, theme: &TuiTheme) -> Line<'a> {
    match event {
        TraceEvent::RoundStart {
            round,
            timestamp_ms,
        } => Line::styled(
            format!("[{}ms] Round {} start", timestamp_ms, round),
            theme.text_dim(),
        ),
        TraceEvent::LlmCall {
            round,
            input_tokens,
            output_tokens,
            duration_ms,
            model,
        } => Line::styled(
            format!(
                "[R{}] LLM {} in={} out={} {}ms",
                round, model, input_tokens, output_tokens, duration_ms
            ),
            Style::default().fg(theme.info),
        ),
        TraceEvent::Thinking { round, content } => {
            let preview = truncate_str(content, 60);
            Line::styled(
                format!("[R{}] Think: {}", round, preview),
                theme.text_dim(),
            )
        }
        TraceEvent::ToolCall {
            round,
            tool_name,
            success,
            duration_ms,
            ..
        } => {
            let style = if *success {
                Style::default().fg(theme.success)
            } else {
                Style::default().fg(theme.error)
            };
            let marker = if *success { "OK" } else { "ERR" };
            Line::styled(
                format!(
                    "[R{}] Tool {} [{}] {}ms",
                    round, tool_name, marker, duration_ms
                ),
                style,
            )
        }
        TraceEvent::Error {
            round,
            source,
            message,
        } => Line::styled(
            format!(
                "[R{}] ERROR {}: {}",
                round,
                source,
                truncate_str(message, 50)
            ),
            Style::default()
                .fg(theme.error)
                .add_modifier(Modifier::BOLD),
        ),
        TraceEvent::SecurityBlocked {
            round,
            tool,
            risk_level,
            reason,
        } => Line::styled(
            format!(
                "[R{}] BLOCKED {} ({}) {}",
                round,
                tool,
                risk_level,
                truncate_str(reason, 40)
            ),
            Style::default().fg(theme.warning),
        ),
        TraceEvent::ContextDegraded {
            round,
            stage,
            usage_pct,
        } => Line::styled(
            format!("[R{}] CtxDegraded {} {:.0}%", round, stage, usage_pct),
            Style::default().fg(theme.warning),
        ),
        TraceEvent::BudgetSnapshot {
            round,
            input_used,
            output_used,
            limit,
        } => Line::styled(
            format!(
                "[R{}] Budget in={} out={} limit={}",
                round, input_used, output_used, limit
            ),
            theme.text_dim(),
        ),
        TraceEvent::LoopGuardVerdict {
            round,
            verdict,
            reason,
        } => Line::styled(
            format!(
                "[R{}] LoopGuard {} {}",
                round,
                verdict,
                truncate_str(reason, 40)
            ),
            theme.text_dim(),
        ),
        TraceEvent::Completed {
            rounds,
            stop_reason,
            total_duration_ms,
        } => Line::styled(
            format!(
                "Completed {} rounds, {} {}ms",
                rounds, stop_reason, total_duration_ms
            ),
            Style::default().fg(Color::Cyan),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> AppEvent {
        AppEvent::Key(crossterm::event::KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ))
    }

    #[test]
    fn eval_focus_cycle() {
        let f = EvalFocus::Left;
        assert_eq!(f.next(), EvalFocus::Center);
        assert_eq!(f.next().next(), EvalFocus::Right);
        assert_eq!(f.next().next().next(), EvalFocus::Left);
    }

    #[test]
    fn eval_focus_prev_cycle() {
        let f = EvalFocus::Right;
        assert_eq!(f.prev(), EvalFocus::Center);
        assert_eq!(f.prev().prev(), EvalFocus::Left);
        assert_eq!(f.prev().prev().prev(), EvalFocus::Right);
    }

    #[test]
    fn dev_eval_screen_new() {
        let screen = DevEvalScreen::new();
        assert!(!screen.loaded);
        assert_eq!(screen.focus, EvalFocus::Left);
        assert_eq!(screen.selected_run, 0);
        assert_eq!(screen.selected_task, 0);
        assert!(screen.runs.is_empty());
        assert!(screen.tasks.is_empty());
        assert!(screen.current_run_data.is_none());
        assert!(screen.current_trace.is_none());
        assert_eq!(screen.timeline_scroll, 0);
    }

    #[test]
    fn eval_focus_equality() {
        assert_eq!(EvalFocus::Left, EvalFocus::Left);
        assert_ne!(EvalFocus::Left, EvalFocus::Right);
        assert_ne!(EvalFocus::Center, EvalFocus::Right);
    }

    #[test]
    fn focus_switch_with_h_l() {
        let mut screen = DevEvalScreen::new();
        assert_eq!(screen.focus, EvalFocus::Left);

        screen.handle_event(&key(KeyCode::Char('l')));
        assert_eq!(screen.focus, EvalFocus::Center);

        screen.handle_event(&key(KeyCode::Char('l')));
        assert_eq!(screen.focus, EvalFocus::Right);

        screen.handle_event(&key(KeyCode::Char('h')));
        assert_eq!(screen.focus, EvalFocus::Center);

        screen.handle_event(&key(KeyCode::Char('h')));
        assert_eq!(screen.focus, EvalFocus::Left);
    }

    #[test]
    fn focus_switch_with_arrows() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Right));
        assert_eq!(screen.focus, EvalFocus::Center);
        screen.handle_event(&key(KeyCode::Left));
        assert_eq!(screen.focus, EvalFocus::Left);
    }

    #[test]
    fn shortcut_r_enters_run_mode() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('r')));
        assert_eq!(screen.input_mode, EvalInputMode::RunSuite);
        assert!(screen.command_input.is_active());
    }

    #[test]
    fn shortcut_slash_enters_search_mode() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('/')));
        assert_eq!(screen.input_mode, EvalInputMode::Search);
        assert!(screen.search_input.is_active());
    }

    #[test]
    fn status_cleared_on_next_key() {
        let mut screen = DevEvalScreen::new();
        // Trigger status via run-suite Enter
        screen.input_mode = EvalInputMode::RunSuite;
        screen.command_input.activate();
        screen.command_input.set_value("basic");
        screen.handle_event(&key(KeyCode::Enter));
        assert!(screen.status_msg.is_some());
        // Next key press clears status
        screen.handle_event(&key(KeyCode::Char('j')));
        assert!(screen.status_msg.is_none());
    }

    #[test]
    fn d_enters_diff_mode() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('d')));
        assert_eq!(screen.input_mode, EvalInputMode::DiffInput);
        assert!(screen.command_input.is_active());
    }

    #[test]
    fn esc_in_search_returns_to_normal() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('/')));
        assert_eq!(screen.input_mode, EvalInputMode::Search);
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
    }

    #[test]
    fn esc_in_run_suite_returns_to_normal() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('r')));
        assert_eq!(screen.input_mode, EvalInputMode::RunSuite);
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
    }

    #[test]
    fn esc_in_diff_returns_to_normal() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('d')));
        assert_eq!(screen.input_mode, EvalInputMode::DiffInput);
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
    }

    #[test]
    fn enter_in_run_suite_sets_status() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('r')));
        // Type "basic"
        for c in "basic".chars() {
            screen.handle_event(&key(KeyCode::Char(c)));
        }
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
        assert!(screen.status_msg.is_some());
        assert!(screen.status_msg.as_ref().unwrap().contains("basic"));
    }

    #[test]
    fn enter_in_diff_sets_status() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('d')));
        for c in "A B".chars() {
            screen.handle_event(&key(KeyCode::Char(c)));
        }
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
        assert!(screen.status_msg.as_ref().unwrap().contains("A B"));
    }

    #[test]
    fn f_enters_filter_mode() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('f')));
        assert_eq!(screen.input_mode, EvalInputMode::Filter);
        assert!(screen.filter_input.is_active());
        assert_eq!(screen.filter_target, Some(FilterTarget::Suite));
    }

    #[test]
    fn shift_f_clears_filter() {
        let mut screen = DevEvalScreen::new();
        screen.active_filter = Some("suite:test".to_string());
        screen.handle_event(&key(KeyCode::Char('F')));
        assert!(screen.active_filter.is_none());
    }

    #[test]
    fn filter_tab_cycles_target() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('f')));
        assert_eq!(screen.filter_target, Some(FilterTarget::Suite));
        screen.handle_event(&key(KeyCode::Tab));
        assert_eq!(screen.filter_target, Some(FilterTarget::Tag));
        screen.handle_event(&key(KeyCode::Tab));
        assert_eq!(screen.filter_target, Some(FilterTarget::Suite));
    }

    #[test]
    fn filter_enter_applies_suite_filter() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('f')));
        for c in "basic".chars() {
            screen.handle_event(&key(KeyCode::Char(c)));
        }
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
        assert_eq!(screen.active_filter, Some("suite:basic".to_string()));
    }

    #[test]
    fn filter_enter_applies_tag_filter() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('f')));
        screen.handle_event(&key(KeyCode::Tab)); // Switch to Tag
        for c in "v1".chars() {
            screen.handle_event(&key(KeyCode::Char(c)));
        }
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.active_filter, Some("tag:v1".to_string()));
    }

    #[test]
    fn filter_esc_cancels() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('f')));
        for c in "test".chars() {
            screen.handle_event(&key(KeyCode::Char(c)));
        }
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
        assert!(screen.active_filter.is_none());
    }

    #[test]
    fn filter_empty_enter_no_filter() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('f')));
        // Enter without typing
        screen.handle_event(&key(KeyCode::Enter));
        assert!(screen.active_filter.is_none());
    }

    #[test]
    fn input_mode_default_is_normal() {
        let screen = DevEvalScreen::new();
        assert_eq!(screen.input_mode, EvalInputMode::Normal);
    }

    #[test]
    fn j_k_on_empty_runs_no_panic() {
        let mut screen = DevEvalScreen::new();
        screen.handle_event(&key(KeyCode::Char('j')));
        assert_eq!(screen.selected_run, 0);
        screen.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(screen.selected_run, 0);
    }

    #[test]
    fn enter_on_left_moves_focus_to_center() {
        let mut screen = DevEvalScreen::new();
        screen.focus = EvalFocus::Left;
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.focus, EvalFocus::Center);
    }

    #[test]
    fn enter_on_center_moves_focus_to_right() {
        let mut screen = DevEvalScreen::new();
        screen.focus = EvalFocus::Center;
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.focus, EvalFocus::Right);
    }

    #[test]
    fn title_is_eval() {
        let screen = DevEvalScreen::new();
        assert_eq!(screen.title(), "Eval");
    }

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_long() {
        let result = truncate_str("hello world this is long", 10);
        assert!(result.len() <= 12);
        assert!(result.ends_with(".."));
    }
}
