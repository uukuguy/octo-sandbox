//! Dev view — Agent debug panel screen (three-column layout)
//!
//! Left:   Session list with context usage bar
//! Center: Conversation timeline (messages + tool calls)
//! Right:  Switchable Inspector (Skill / MCP / Provider / Memory)

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap};

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;
use crate::tui::widgets::TextInput;

use super::Screen;

/// Which column is focused
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFocus {
    Left,
    Center,
    Right,
}

impl AgentFocus {
    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Center,
            Self::Center => Self::Right,
            Self::Right => Self::Right, // stay at rightmost
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Left => Self::Left, // stay at leftmost
            Self::Center => Self::Left,
            Self::Right => Self::Center,
        }
    }
}

/// Inspector sub-panel selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorPanel {
    Skill,
    Mcp,
    Provider,
    Memory,
}

impl InspectorPanel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Mcp => "MCP",
            Self::Provider => "Provider",
            Self::Memory => "Memory",
        }
    }

    pub fn key_hint(&self) -> &'static str {
        match self {
            Self::Skill => "S",
            Self::Mcp => "M",
            Self::Provider => "P",
            Self::Memory => "R",
        }
    }

    pub fn all() -> &'static [InspectorPanel] {
        &[
            InspectorPanel::Skill,
            InspectorPanel::Mcp,
            InspectorPanel::Provider,
            InspectorPanel::Memory,
        ]
    }
}

/// Status of a tool call in the conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallStatus {
    Ok,
    Error,
    Blocked,
}

/// A single entry in the conversation timeline
#[derive(Debug, Clone)]
pub enum ConversationEntry {
    UserMessage { content: String },
    AssistantThinking { content: String },
    AssistantMessage { content: String },
    ToolCall {
        name: String,
        status: ToolCallStatus,
        duration_ms: u64,
    },
}

/// Summary of a session for the left column
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub id: String,
    pub agent_name: String,
    pub message_count: usize,
    pub last_active: String,
    pub is_active: bool,
}

/// Info about a loaded skill
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub status: String,
    pub trigger_count: usize,
}

/// Info about an MCP server
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub transport: String,
    pub status: String,
    pub tool_count: usize,
}

/// Info about a provider
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
    pub status: String,
    pub total_tokens: u64,
    pub estimated_cost: f64,
}

/// Info about a recent LLM call
#[derive(Debug, Clone)]
pub struct LlmCallInfo {
    pub timestamp: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub duration_ms: u64,
}

/// Info about a failover attempt for display
#[derive(Debug, Clone)]
pub struct FailoverAttemptInfo {
    pub instance_id: String,
    pub duration_ms: u64,
    pub result: String, // "ok", "failed", "rate_limited", "no_instance"
}

/// Info about a failover trace for display
#[derive(Debug, Clone)]
pub struct FailoverTraceInfo {
    pub request_id: u64,
    pub timestamp: String,
    pub total_duration_ms: u64,
    pub attempts: Vec<FailoverAttemptInfo>,
}

/// Info about a memory layer
#[derive(Debug, Clone)]
pub struct MemoryLayerInfo {
    pub name: String,
    pub entry_count: usize,
    pub size_bytes: usize,
}

/// Three-column Agent Debug panel for the Dev view
pub struct DevAgentScreen {
    /// Which column has keyboard focus
    pub focus: AgentFocus,
    /// Active inspector sub-panel
    pub inspector: InspectorPanel,
    /// Selected index in the sessions list
    pub selected_session: usize,
    /// Conversation scroll offset
    pub conversation_scroll: usize,
    /// Inspector scroll offset
    pub inspector_scroll: usize,
    /// Whether initial load has been done
    pub loaded: bool,
    /// Session summaries
    sessions: Vec<SessionSummary>,
    /// Conversation entries for the selected session
    messages: Vec<ConversationEntry>,
    /// Context usage percentage (0-100)
    context_usage: u16,
    /// Skill inspector data
    skills: Vec<SkillInfo>,
    /// MCP inspector data
    mcp_servers: Vec<McpServerInfo>,
    /// Provider inspector data
    providers: Vec<ProviderInfo>,
    /// Recent LLM calls
    recent_calls: Vec<LlmCallInfo>,
    /// Recent failover traces
    failover_traces: Vec<FailoverTraceInfo>,
    /// Memory layer info
    memory_layers: Vec<MemoryLayerInfo>,
    /// Status message for shortcut feedback
    status_msg: Option<String>,
    /// Memory search input
    memory_search: TextInput,
    /// Whether memory search is active
    memory_search_active: bool,
}

impl DevAgentScreen {
    pub fn new() -> Self {
        Self {
            focus: AgentFocus::Left,
            inspector: InspectorPanel::Skill,
            selected_session: 0,
            conversation_scroll: 0,
            inspector_scroll: 0,
            loaded: false,
            sessions: Vec::new(),
            messages: Vec::new(),
            context_usage: 0,
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            providers: Vec::new(),
            recent_calls: Vec::new(),
            failover_traces: Vec::new(),
            memory_layers: Vec::new(),
            status_msg: None,
            memory_search: TextInput::new("Search memory..."),
            memory_search_active: false,
        }
    }

    /// Load session data from AppState (lazy, called on first render)
    fn load_sessions(&mut self, _state: &AppState) {
        // In a real implementation, this would query state.agent_runtime's SessionStore.
        // For now, show empty state with helpful message.
        self.sessions = Vec::new();
        self.loaded = true;
    }

    /// Load conversation for the selected session
    fn load_conversation(&mut self) {
        if self.sessions.is_empty() {
            self.messages.clear();
            return;
        }
        // Placeholder: real implementation loads from SessionStore
        self.messages = Vec::new();
        self.conversation_scroll = 0;
    }

    /// Load inspector data based on current panel
    fn load_inspector_data(&mut self, _state: &AppState) {
        // Placeholder: real implementation loads from respective registries
        self.skills = Vec::new();
        self.mcp_servers = Vec::new();
        self.providers = Vec::new();
        self.recent_calls = Vec::new();
        self.memory_layers = Vec::new();
        self.inspector_scroll = 0;
    }

    // -- Focus navigation --

    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    // -- Rendering helpers --

    fn render_columns(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20), // Sessions
                Constraint::Percentage(45), // Conversation
                Constraint::Percentage(35), // Inspector
            ])
            .split(area);

        // Left: Sessions + Context bar
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

    fn render_sessions(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let title = format!(" Sessions ({}) ", self.sessions.len());
        let block = if self.focus == AgentFocus::Left {
            theme.styled_block_active(&title)
        } else {
            theme.styled_block(&title)
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.sessions.is_empty() {
            let msg = Paragraph::new("No active sessions.\nStart an agent to debug.")
                .style(theme.text_dim())
                .wrap(Wrap { trim: false });
            frame.render_widget(msg, inner);
            return;
        }

        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let active_marker = if s.is_active { "*" } else { " " };
                let line = format!(
                    "[{}] {} {} ({} msgs)",
                    active_marker,
                    truncate_str(&s.id, 8),
                    truncate_str(&s.agent_name, 12),
                    s.message_count,
                );
                let style = if i == self.selected_session {
                    theme.list_selected()
                } else if s.is_active {
                    theme.text_normal()
                } else {
                    theme.text_dim()
                };
                ListItem::new(line).style(style)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }

    fn render_context_bar(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let style = if self.context_usage > 80 {
            Style::default().fg(theme.error)
        } else if self.context_usage > 60 {
            Style::default().fg(theme.warning)
        } else {
            Style::default().fg(theme.success)
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(style)
            .ratio(self.context_usage as f64 / 100.0)
            .label(format!("Context: {}%", self.context_usage));
        frame.render_widget(gauge, area);
    }

    fn render_conversation(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let block = if self.focus == AgentFocus::Center {
            theme.styled_block_active(" Conversation ")
        } else {
            theme.styled_block(" Conversation ")
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.messages.is_empty() {
            let msg = if self.sessions.is_empty() {
                "Select a session to view conversation"
            } else {
                "No messages in this session"
            };
            frame.render_widget(
                Paragraph::new(msg).style(theme.text_dim()),
                inner,
            );
            return;
        }

        let lines: Vec<Line> = self
            .messages
            .iter()
            .skip(self.conversation_scroll)
            .map(|entry| format_conversation_entry(entry, theme))
            .collect();

        let text = Text::from(lines);
        frame.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: false }),
            inner,
        );
    }

    fn render_inspector(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        let title = format!(" Inspector: {} ", self.inspector.label());
        let block = if self.focus == AgentFocus::Right {
            theme.styled_block_active(&title)
        } else {
            theme.styled_block(&title)
        };
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split: content (top 85%) + key hints bar (bottom 15%)
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        // Render active sub-panel content
        match self.inspector {
            InspectorPanel::Skill => self.render_skill_panel(frame, sections[0], theme),
            InspectorPanel::Mcp => self.render_mcp_panel(frame, sections[0], theme),
            InspectorPanel::Provider => self.render_provider_panel(frame, sections[0], theme),
            InspectorPanel::Memory => self.render_memory_panel(frame, sections[0], theme),
        }

        // Key hints bar
        let hints: Vec<Span> = InspectorPanel::all()
            .iter()
            .flat_map(|p| {
                let style = if *p == self.inspector {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    theme.text_dim()
                };
                vec![
                    Span::styled(format!("[{}]", p.key_hint()), style),
                    Span::styled(
                        format!("{} ", p.label()),
                        if *p == self.inspector {
                            theme.text_normal()
                        } else {
                            theme.text_dim()
                        },
                    ),
                ]
            })
            .collect();

        let hint_line = Paragraph::new(Line::from(hints));
        frame.render_widget(hint_line, sections[1]);
    }

    fn render_skill_panel(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        if self.skills.is_empty() {
            frame.render_widget(
                Paragraph::new("No skills loaded.\nSkills will appear when an agent runs.")
                    .style(theme.text_dim())
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }

        let items: Vec<ListItem> = self
            .skills
            .iter()
            .skip(self.inspector_scroll)
            .map(|s| {
                let line = format!(
                    "  {}  {}  ({} triggers)",
                    truncate_str(&s.name, 16),
                    s.status,
                    s.trigger_count,
                );
                ListItem::new(line).style(theme.text_normal())
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, area);
    }

    fn render_mcp_panel(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        if self.mcp_servers.is_empty() {
            frame.render_widget(
                Paragraph::new("No MCP servers connected.\nServers will appear when configured.")
                    .style(theme.text_dim())
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }

        let items: Vec<ListItem> = self
            .mcp_servers
            .iter()
            .skip(self.inspector_scroll)
            .map(|s| {
                let status_style = match s.status.as_str() {
                    "connected" => theme.status_ok(),
                    "error" => theme.status_error(),
                    _ => theme.text_dim(),
                };
                let line = Line::from(vec![
                    Span::styled(
                        format!("  {} ", truncate_str(&s.name, 14)),
                        theme.text_normal(),
                    ),
                    Span::styled(format!("{} ", s.transport), theme.text_dim()),
                    Span::styled(format!("{}", s.status), status_style),
                    Span::styled(
                        format!("  ({} tools)", s.tool_count),
                        theme.text_dim(),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, area);
    }

    fn render_provider_panel(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        if self.providers.is_empty()
            && self.recent_calls.is_empty()
            && self.failover_traces.is_empty()
        {
            frame.render_widget(
                Paragraph::new(
                    "No providers configured.\nProviders will appear when an agent runs.",
                )
                .style(theme.text_dim())
                .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }

        // Three sections: providers (35%), recent calls (30%), failover traces (35%)
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(35),
                Constraint::Percentage(30),
                Constraint::Percentage(35),
            ])
            .split(area);

        // Section 1: Providers
        let provider_lines: Vec<Line> = self
            .providers
            .iter()
            .flat_map(|p| {
                vec![
                    Line::styled(
                        format!("  {}  {}  {}", p.name, p.model, p.status),
                        theme.text_normal(),
                    ),
                    Line::styled(
                        format!(
                            "    tokens: {}  cost: ${:.3}",
                            p.total_tokens, p.estimated_cost
                        ),
                        theme.text_dim(),
                    ),
                ]
            })
            .collect();
        frame.render_widget(
            Paragraph::new(Text::from(provider_lines)).wrap(Wrap { trim: false }),
            sections[0],
        );

        // Section 2: Recent Calls
        let call_block = Block::default()
            .title("Recent Calls")
            .title_style(theme.block_title())
            .borders(Borders::TOP)
            .border_style(theme.block_border());
        let call_inner = call_block.inner(sections[1]);
        frame.render_widget(call_block, sections[1]);

        let call_lines: Vec<Line> = self
            .recent_calls
            .iter()
            .map(|c| {
                Line::styled(
                    format!(
                        "  [{}] {}  in={} out={}  {}ms",
                        c.timestamp, c.model, c.input_tokens, c.output_tokens, c.duration_ms
                    ),
                    theme.text_dim(),
                )
            })
            .collect();
        frame.render_widget(
            Paragraph::new(Text::from(call_lines)).wrap(Wrap { trim: false }),
            call_inner,
        );

        // Section 3: Failover Traces
        let trace_block = Block::default()
            .title("Failover Traces")
            .title_style(theme.block_title())
            .borders(Borders::TOP)
            .border_style(theme.block_border());
        let trace_inner = trace_block.inner(sections[2]);
        frame.render_widget(trace_block, sections[2]);

        if self.failover_traces.is_empty() {
            frame.render_widget(
                Paragraph::new("  No failover traces yet").style(theme.text_dim()),
                trace_inner,
            );
        } else {
            let mut trace_lines: Vec<Line> = Vec::new();
            for trace in self.failover_traces.iter().rev().take(5) {
                // Header line: req-ID [timestamp] total_ms
                trace_lines.push(Line::styled(
                    format!(
                        "  req-{} [{}] {}ms",
                        trace.request_id, trace.timestamp, trace.total_duration_ms
                    ),
                    theme.text_normal(),
                ));
                // Attempt lines with tree-like prefix
                for (i, attempt) in trace.attempts.iter().enumerate() {
                    let connector = if i == trace.attempts.len() - 1 {
                        "\u{2514}\u{2500}"
                    } else {
                        "\u{251c}\u{2500}"
                    };
                    let (marker, style) = match attempt.result.as_str() {
                        "ok" => ("\u{2713}", Style::default().fg(theme.success)),
                        "failed" => ("\u{2717}", Style::default().fg(theme.error)),
                        "rate_limited" => ("\u{26a0}", Style::default().fg(theme.warning)),
                        _ => ("?", theme.text_dim()),
                    };
                    trace_lines.push(Line::styled(
                        format!(
                            "    {} {} {} {}ms",
                            connector, attempt.instance_id, marker, attempt.duration_ms
                        ),
                        style,
                    ));
                }
            }
            frame.render_widget(
                Paragraph::new(Text::from(trace_lines)).wrap(Wrap { trim: false }),
                trace_inner,
            );
        }
    }

    fn render_memory_panel(&self, frame: &mut Frame, area: Rect, theme: &TuiTheme) {
        // Split area: content + search bar (if active or has search text)
        let show_search = self.memory_search_active || !self.memory_search.is_empty();

        let (content_area, search_area) = if show_search {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(area);
            (chunks[0], Some(chunks[1]))
        } else {
            (area, None)
        };

        if self.memory_layers.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "No memory data available.\nMemory layers will populate during agent execution.",
                )
                .style(theme.text_dim())
                .wrap(Wrap { trim: false }),
                content_area,
            );
        } else {
            let search_text = self.memory_search.value().to_lowercase();
            let mut lines: Vec<Line> = Vec::new();
            for layer in &self.memory_layers {
                // Filter by search text if present
                if !search_text.is_empty() && !layer.name.to_lowercase().contains(&search_text) {
                    continue;
                }
                let size_str = if layer.size_bytes >= 1_048_576 {
                    format!("{:.1}MB", layer.size_bytes as f64 / 1_048_576.0)
                } else if layer.size_bytes >= 1024 {
                    format!("{:.1}KB", layer.size_bytes as f64 / 1024.0)
                } else {
                    format!("{}B", layer.size_bytes)
                };
                lines.push(Line::styled(
                    format!(
                        "  {}  {} entries  {}",
                        layer.name, layer.entry_count, size_str
                    ),
                    theme.text_normal(),
                ));
            }

            if !show_search {
                lines.push(Line::raw(""));
                lines.push(Line::styled("  [/] to search memory...", theme.text_dim()));
            }

            frame.render_widget(
                Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
                content_area,
            );
        }

        // Render search bar
        if let Some(search_area) = search_area {
            let block = if self.memory_search_active {
                theme.styled_block_active(" Search ")
            } else {
                theme.styled_block(" Search ")
            };
            self.memory_search
                .render(frame, search_area, theme, Some(block));
        }
    }
}

impl Screen for DevAgentScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState) {
        if !self.loaded {
            self.load_sessions(state);
            self.load_inspector_data(state);
        }

        // Show status message if present
        if let Some(ref msg) = self.status_msg {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(area);
            frame.render_widget(
                Paragraph::new(msg.clone()).style(theme.status_warn()),
                chunks[0],
            );
            self.render_columns(frame, chunks[1], theme);
        } else {
            self.render_columns(frame, area, theme);
        }
    }

    fn handle_event(&mut self, event: &AppEvent) {
        // Clear status on any key press
        if matches!(event, AppEvent::Key(_)) {
            self.status_msg = None;
        }

        if let AppEvent::Key(key) = event {
            // Memory search input handling
            if self.memory_search_active {
                match key.code {
                    KeyCode::Enter => {
                        // Apply search (placeholder for now)
                        self.memory_search_active = false;
                        self.memory_search.deactivate();
                        self.status_msg = if self.memory_search.is_empty() {
                            None
                        } else {
                            Some(format!("Memory search: {}", self.memory_search.value()))
                        };
                    }
                    KeyCode::Esc => {
                        self.memory_search_active = false;
                        self.memory_search.clear();
                    }
                    _ => {
                        self.memory_search.handle_key(key.code);
                    }
                }
                return;
            }

            match key.code {
                // Navigation within focused column
                KeyCode::Char('j') | KeyCode::Down => match self.focus {
                    AgentFocus::Left => {
                        if !self.sessions.is_empty() {
                            let new = (self.selected_session + 1).min(self.sessions.len() - 1);
                            if new != self.selected_session {
                                self.selected_session = new;
                                self.load_conversation();
                            }
                        }
                    }
                    AgentFocus::Center => {
                        if !self.messages.is_empty() {
                            if self.conversation_scroll + 1 < self.messages.len() {
                                self.conversation_scroll += 1;
                            }
                        }
                    }
                    AgentFocus::Right => {
                        self.inspector_scroll += 1;
                    }
                },
                KeyCode::Char('k') | KeyCode::Up => match self.focus {
                    AgentFocus::Left => {
                        if self.selected_session > 0 {
                            self.selected_session -= 1;
                            self.load_conversation();
                        }
                    }
                    AgentFocus::Center => {
                        self.conversation_scroll = self.conversation_scroll.saturating_sub(1);
                    }
                    AgentFocus::Right => {
                        self.inspector_scroll = self.inspector_scroll.saturating_sub(1);
                    }
                },
                // Focus switching
                KeyCode::Char('l') | KeyCode::Right => {
                    self.focus_next();
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.focus_prev();
                }
                // Enter: drill down
                KeyCode::Enter => match self.focus {
                    AgentFocus::Left => {
                        self.load_conversation();
                        self.focus = AgentFocus::Center;
                    }
                    AgentFocus::Center => {
                        self.focus = AgentFocus::Right;
                    }
                    AgentFocus::Right => {}
                },
                // Esc: back
                KeyCode::Esc => match self.focus {
                    AgentFocus::Right => {
                        self.focus = AgentFocus::Center;
                    }
                    AgentFocus::Center => {
                        self.focus = AgentFocus::Left;
                    }
                    AgentFocus::Left => {}
                },
                // Inspector sub-panel switching (only when focus == Right)
                KeyCode::Char('s') | KeyCode::Char('S')
                    if self.focus == AgentFocus::Right =>
                {
                    self.inspector = InspectorPanel::Skill;
                    self.inspector_scroll = 0;
                }
                KeyCode::Char('m') | KeyCode::Char('M')
                    if self.focus == AgentFocus::Right =>
                {
                    self.inspector = InspectorPanel::Mcp;
                    self.inspector_scroll = 0;
                }
                KeyCode::Char('p') | KeyCode::Char('P')
                    if self.focus == AgentFocus::Right =>
                {
                    self.inspector = InspectorPanel::Provider;
                    self.inspector_scroll = 0;
                }
                KeyCode::Char('r') | KeyCode::Char('R')
                    if self.focus == AgentFocus::Right =>
                {
                    self.inspector = InspectorPanel::Memory;
                    self.inspector_scroll = 0;
                }
                // Shortcut keys
                KeyCode::Char('/') => {
                    if self.focus == AgentFocus::Right
                        && self.inspector == InspectorPanel::Memory
                    {
                        self.memory_search_active = true;
                        self.memory_search.clear();
                        self.memory_search.activate();
                    } else {
                        self.status_msg =
                            Some("Press / in Memory panel to search".to_string());
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Agent Debug"
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

/// Format a conversation entry as a styled Line
fn format_conversation_entry<'a>(entry: &ConversationEntry, theme: &TuiTheme) -> Line<'a> {
    match entry {
        ConversationEntry::UserMessage { content } => Line::styled(
            format!("User: {}", truncate_str(content, 60)),
            theme.text_normal(),
        ),
        ConversationEntry::AssistantThinking { content } => Line::styled(
            format!("  [thinking] {}", truncate_str(content, 55)),
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::ITALIC),
        ),
        ConversationEntry::AssistantMessage { content } => Line::styled(
            format!("Agent: {}", truncate_str(content, 58)),
            theme.text_normal(),
        ),
        ConversationEntry::ToolCall {
            name,
            status,
            duration_ms,
        } => {
            let (marker, style) = match status {
                ToolCallStatus::Ok => ("OK", Style::default().fg(theme.success)),
                ToolCallStatus::Error => ("ERR", Style::default().fg(theme.error)),
                ToolCallStatus::Blocked => (
                    "BLOCKED",
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
            };
            Line::styled(
                format!("  [tool] {} -> {} {}ms", name, marker, duration_ms),
                style,
            )
        }
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
    fn agent_focus_next() {
        assert_eq!(AgentFocus::Left.next(), AgentFocus::Center);
        assert_eq!(AgentFocus::Center.next(), AgentFocus::Right);
        assert_eq!(AgentFocus::Right.next(), AgentFocus::Right);
    }

    #[test]
    fn agent_focus_prev() {
        assert_eq!(AgentFocus::Left.prev(), AgentFocus::Left);
        assert_eq!(AgentFocus::Center.prev(), AgentFocus::Left);
        assert_eq!(AgentFocus::Right.prev(), AgentFocus::Center);
    }

    #[test]
    fn agent_focus_equality() {
        assert_eq!(AgentFocus::Left, AgentFocus::Left);
        assert_ne!(AgentFocus::Left, AgentFocus::Right);
        assert_ne!(AgentFocus::Center, AgentFocus::Right);
    }

    #[test]
    fn inspector_panel_count_is_4() {
        assert_eq!(InspectorPanel::all().len(), 4);
    }

    #[test]
    fn inspector_labels_nonempty() {
        for p in InspectorPanel::all() {
            assert!(!p.label().is_empty());
        }
    }

    #[test]
    fn inspector_key_hints_unique() {
        let hints: Vec<&str> = InspectorPanel::all()
            .iter()
            .map(|p| p.key_hint())
            .collect();
        assert_eq!(hints, vec!["S", "M", "P", "R"]);
    }

    #[test]
    fn dev_agent_screen_new() {
        let screen = DevAgentScreen::new();
        assert!(!screen.loaded);
        assert_eq!(screen.focus, AgentFocus::Left);
        assert_eq!(screen.inspector, InspectorPanel::Skill);
        assert_eq!(screen.selected_session, 0);
        assert_eq!(screen.conversation_scroll, 0);
        assert_eq!(screen.inspector_scroll, 0);
        assert!(screen.sessions.is_empty());
        assert!(screen.messages.is_empty());
        assert_eq!(screen.context_usage, 0);
    }

    #[test]
    fn focus_switch_with_h_l() {
        let mut screen = DevAgentScreen::new();
        assert_eq!(screen.focus, AgentFocus::Left);

        screen.handle_event(&key(KeyCode::Char('l')));
        assert_eq!(screen.focus, AgentFocus::Center);

        screen.handle_event(&key(KeyCode::Char('l')));
        assert_eq!(screen.focus, AgentFocus::Right);

        // stays at right
        screen.handle_event(&key(KeyCode::Char('l')));
        assert_eq!(screen.focus, AgentFocus::Right);

        screen.handle_event(&key(KeyCode::Char('h')));
        assert_eq!(screen.focus, AgentFocus::Center);

        screen.handle_event(&key(KeyCode::Char('h')));
        assert_eq!(screen.focus, AgentFocus::Left);

        // stays at left
        screen.handle_event(&key(KeyCode::Char('h')));
        assert_eq!(screen.focus, AgentFocus::Left);
    }

    #[test]
    fn focus_switch_with_arrows() {
        let mut screen = DevAgentScreen::new();
        screen.handle_event(&key(KeyCode::Right));
        assert_eq!(screen.focus, AgentFocus::Center);
        screen.handle_event(&key(KeyCode::Left));
        assert_eq!(screen.focus, AgentFocus::Left);
    }

    #[test]
    fn enter_on_left_moves_to_center() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Left;
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.focus, AgentFocus::Center);
    }

    #[test]
    fn enter_on_center_moves_to_right() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Center;
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.focus, AgentFocus::Right);
    }

    #[test]
    fn enter_on_right_stays() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.handle_event(&key(KeyCode::Enter));
        assert_eq!(screen.focus, AgentFocus::Right);
    }

    #[test]
    fn esc_navigates_back() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.focus, AgentFocus::Center);
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.focus, AgentFocus::Left);
        screen.handle_event(&key(KeyCode::Esc));
        assert_eq!(screen.focus, AgentFocus::Left); // stays
    }

    #[test]
    fn inspector_switch_only_when_right_focused() {
        let mut screen = DevAgentScreen::new();

        // When not on Right, 's' should not switch inspector
        screen.focus = AgentFocus::Left;
        screen.handle_event(&key(KeyCode::Char('s')));
        assert_eq!(screen.inspector, InspectorPanel::Skill); // unchanged (was default)

        // When on Right, 'm' should switch to Mcp
        screen.focus = AgentFocus::Right;
        screen.handle_event(&key(KeyCode::Char('m')));
        assert_eq!(screen.inspector, InspectorPanel::Mcp);

        screen.handle_event(&key(KeyCode::Char('p')));
        assert_eq!(screen.inspector, InspectorPanel::Provider);

        screen.handle_event(&key(KeyCode::Char('r')));
        assert_eq!(screen.inspector, InspectorPanel::Memory);

        screen.handle_event(&key(KeyCode::Char('s')));
        assert_eq!(screen.inspector, InspectorPanel::Skill);
    }

    #[test]
    fn inspector_switch_resets_scroll() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.inspector_scroll = 5;
        screen.handle_event(&key(KeyCode::Char('m')));
        assert_eq!(screen.inspector_scroll, 0);
    }

    #[test]
    fn j_k_on_empty_sessions_no_panic() {
        let mut screen = DevAgentScreen::new();
        screen.handle_event(&key(KeyCode::Char('j')));
        assert_eq!(screen.selected_session, 0);
        screen.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(screen.selected_session, 0);
    }

    #[test]
    fn j_k_scroll_center_on_empty_no_panic() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Center;
        screen.handle_event(&key(KeyCode::Char('j')));
        assert_eq!(screen.conversation_scroll, 0);
        screen.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(screen.conversation_scroll, 0);
    }

    #[test]
    fn j_k_scroll_inspector() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.handle_event(&key(KeyCode::Char('j')));
        assert_eq!(screen.inspector_scroll, 1);
        screen.handle_event(&key(KeyCode::Char('j')));
        assert_eq!(screen.inspector_scroll, 2);
        screen.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(screen.inspector_scroll, 1);
        screen.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(screen.inspector_scroll, 0);
        screen.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(screen.inspector_scroll, 0); // saturating
    }

    #[test]
    fn shortcut_slash_shows_hint() {
        let mut screen = DevAgentScreen::new();
        screen.handle_event(&key(KeyCode::Char('/')));
        assert!(screen.status_msg.is_some());
        assert!(screen.status_msg.as_ref().unwrap().contains("Memory"));
    }

    #[test]
    fn status_cleared_on_next_key() {
        let mut screen = DevAgentScreen::new();
        screen.handle_event(&key(KeyCode::Char('/')));
        assert!(screen.status_msg.is_some());
        screen.handle_event(&key(KeyCode::Char('j')));
        assert!(screen.status_msg.is_none());
    }

    #[test]
    fn title_is_agent_debug() {
        let screen = DevAgentScreen::new();
        assert_eq!(screen.title(), "Agent Debug");
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

    #[test]
    fn tool_call_status_variants() {
        assert_eq!(ToolCallStatus::Ok, ToolCallStatus::Ok);
        assert_ne!(ToolCallStatus::Ok, ToolCallStatus::Error);
        assert_ne!(ToolCallStatus::Error, ToolCallStatus::Blocked);
    }

    #[test]
    fn conversation_entry_user_message() {
        let entry = ConversationEntry::UserMessage {
            content: "test".to_string(),
        };
        let theme = TuiTheme::default();
        let line = format_conversation_entry(&entry, &theme);
        let text = line.to_string();
        assert!(text.contains("User:"));
    }

    #[test]
    fn conversation_entry_tool_call_ok() {
        let entry = ConversationEntry::ToolCall {
            name: "file_read".to_string(),
            status: ToolCallStatus::Ok,
            duration_ms: 120,
        };
        let theme = TuiTheme::default();
        let line = format_conversation_entry(&entry, &theme);
        let text = line.to_string();
        assert!(text.contains("file_read"));
        assert!(text.contains("OK"));
    }

    #[test]
    fn conversation_entry_tool_call_blocked() {
        let entry = ConversationEntry::ToolCall {
            name: "bash".to_string(),
            status: ToolCallStatus::Blocked,
            duration_ms: 0,
        };
        let theme = TuiTheme::default();
        let line = format_conversation_entry(&entry, &theme);
        let text = line.to_string();
        assert!(text.contains("BLOCKED"));
    }

    #[test]
    fn memory_layer_info_fields() {
        let layer = MemoryLayerInfo {
            name: "L0 Working".to_string(),
            entry_count: 12,
            size_bytes: 2400,
        };
        assert_eq!(layer.name, "L0 Working");
        assert_eq!(layer.entry_count, 12);
        assert_eq!(layer.size_bytes, 2400);
    }

    #[test]
    fn session_summary_fields() {
        let session = SessionSummary {
            id: "sess-001".to_string(),
            agent_name: "default".to_string(),
            message_count: 5,
            last_active: "12:30".to_string(),
            is_active: true,
        };
        assert_eq!(session.id, "sess-001");
        assert!(session.is_active);
    }

    #[test]
    fn failover_trace_info_fields() {
        let trace = FailoverTraceInfo {
            request_id: 42,
            timestamp: "14:30:05".to_string(),
            total_duration_ms: 5120,
            attempts: vec![
                FailoverAttemptInfo {
                    instance_id: "claude-opus".to_string(),
                    duration_ms: 5000,
                    result: "rate_limited".to_string(),
                },
                FailoverAttemptInfo {
                    instance_id: "claude-sonnet".to_string(),
                    duration_ms: 120,
                    result: "ok".to_string(),
                },
            ],
        };
        assert_eq!(trace.request_id, 42);
        assert_eq!(trace.attempts.len(), 2);
        assert_eq!(trace.attempts[0].result, "rate_limited");
        assert_eq!(trace.attempts[1].result, "ok");
    }

    #[test]
    fn failover_attempt_info_construction() {
        let attempt = FailoverAttemptInfo {
            instance_id: "test-1".to_string(),
            duration_ms: 100,
            result: "ok".to_string(),
        };
        assert_eq!(attempt.instance_id, "test-1");
        assert_eq!(attempt.duration_ms, 100);
    }

    #[test]
    fn failover_traces_initialized_empty() {
        let screen = DevAgentScreen::new();
        assert!(screen.failover_traces.is_empty());
    }

    #[test]
    fn slash_in_memory_panel_activates_search() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.inspector = InspectorPanel::Memory;
        screen.handle_event(&key(KeyCode::Char('/')));
        assert!(screen.memory_search_active);
        assert!(screen.memory_search.is_active());
    }

    #[test]
    fn slash_outside_memory_shows_hint() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Left;
        screen.handle_event(&key(KeyCode::Char('/')));
        assert!(!screen.memory_search_active);
        assert!(screen.status_msg.is_some());
        assert!(screen.status_msg.as_ref().unwrap().contains("Memory"));
    }

    #[test]
    fn esc_in_memory_search_deactivates() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.inspector = InspectorPanel::Memory;
        screen.handle_event(&key(KeyCode::Char('/')));
        assert!(screen.memory_search_active);
        screen.handle_event(&key(KeyCode::Esc));
        assert!(!screen.memory_search_active);
    }

    #[test]
    fn memory_search_input_captured() {
        let mut screen = DevAgentScreen::new();
        screen.focus = AgentFocus::Right;
        screen.inspector = InspectorPanel::Memory;
        screen.handle_event(&key(KeyCode::Char('/')));
        screen.handle_event(&key(KeyCode::Char('t')));
        screen.handle_event(&key(KeyCode::Char('e')));
        assert_eq!(screen.memory_search.value(), "te");
    }
}
