//! Application state for the conversation-centric TUI.
//!
//! `TuiState` holds all mutable state for the TUI event loop:
//! conversation history, streaming buffer, active tool executions,
//! input buffer, scroll position, and metrics.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use octo_engine::agent::AgentExecutorHandle;
use octo_engine::tools::approval::ApprovalGate;
use octo_types::message::{ChatMessage, ContentBlock, MessageRole};
use octo_types::tool::RiskLevel;
use octo_types::SessionId;
use ratatui::text::Line;

use super::managers::interrupt::InterruptManager;
use super::managers::message_history::MessageHistory;
use super::managers::spinner::SpinnerService;
use super::widgets::conversation::ActiveTool;
use super::widgets::welcome_panel::WelcomePanelState;

/// Overlay mode — only one overlay can be active at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// No overlay — normal conversation view.
    None,
    /// Agent debug panel (Ctrl+D).
    AgentDebug,
    /// Eval results panel (Ctrl+E).
    Eval,
    /// Session/Agent picker (Ctrl+A).
    SessionPicker,
}

/// Agent operational state for status bar display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Streaming,
    Thinking,
}

/// A tool execution awaiting user approval.
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub tool_id: String,
    pub tool_name: String,
    pub risk_level: RiskLevel,
}

/// Main TUI application state.
pub struct TuiState {
    // ── Runtime ──
    /// Whether the TUI is still running.
    pub running: bool,
    /// Whether a redraw is needed.
    pub dirty: bool,
    /// Whether mouse capture is enabled (toggled via /mouse command).
    pub mouse_captured: bool,

    // ── Agent connection ──
    /// Handle to the agent executor for sending messages and subscribing.
    pub handle: AgentExecutorHandle,

    // ── Conversation ──
    /// Full conversation history (finalized messages only).
    pub messages: Vec<ChatMessage>,
    /// Buffer for the current streaming assistant response.
    pub streaming_text: String,
    /// Whether the agent is currently streaming a response.
    pub is_streaming: bool,
    /// Whether the current task was cancelled by user (ESC).
    /// When true, the Completed event preserves existing messages instead of replacing them.
    pub cancelled: bool,

    // ── Tool execution ──
    /// Currently executing tools (shown as inline spinners).
    pub active_tools: Vec<ActiveTool>,
    /// Tool awaiting user approval (shown as modal dialog).
    pub pending_approval: Option<PendingApproval>,

    // ── Input ──
    /// Current input buffer text.
    pub input_buffer: String,
    /// Cursor position within the input buffer (byte offset).
    pub input_cursor: usize,

    // ── Scroll ──
    /// Scroll offset from the bottom of the conversation.
    pub scroll_offset: u16,
    /// Whether the user has manually scrolled up.
    pub user_scrolled: bool,

    // ── Line cache ──
    /// Pre-rendered lines for the conversation area.
    pub cached_lines: Vec<Line<'static>>,
    /// Generation counter for the line cache.
    pub lines_generation: u64,
    /// Generation counter for message changes (bumped on any content change).
    pub message_generation: u64,

    // ── Metrics ──
    /// Cumulative input tokens across all agent loop completions.
    pub total_input_tokens: u64,
    /// Cumulative output tokens across all agent loop completions.
    pub total_output_tokens: u64,
    /// Current session ID.
    pub session_id: SessionId,
    /// Model name (from agent runtime config).
    pub model_name: String,

    // ── Terminal ──
    /// Current terminal width.
    pub terminal_width: u16,
    /// Current terminal height.
    pub terminal_height: u16,
    /// Whether the terminal window currently has focus.
    pub has_focus: bool,

    // ── Managers ──
    /// Ctrl+C interrupt manager.
    pub interrupt_manager: InterruptManager,
    /// Spinner animation service.
    pub spinner_service: SpinnerService,
    /// Input history for Up/Down navigation.
    pub message_history: MessageHistory,

    // ── Overlays ──
    /// Currently active overlay (if any).
    pub overlay: OverlayMode,

    // ── Thinking ──
    /// Buffer for thinking/reasoning text (rendered dimmed).
    pub thinking_text: String,
    /// Whether the agent is in a thinking phase.
    pub is_thinking: bool,

    // ── Welcome ──
    /// Animation state for the welcome panel (shown when conversation is empty).
    pub welcome_state: WelcomePanelState,

    // ── Scroll acceleration ──
    /// Last scroll direction: true=up, false=down.
    pub scroll_last_dir: Option<bool>,
    /// Timestamp of last scroll event.
    pub scroll_last_time: Option<Instant>,
    /// Current acceleration level (0=3 lines, 1=6, 2=12).
    pub scroll_accel: u8,

    // ── Approval ──
    /// Gate for responding to tool approval requests from the engine.
    pub approval_gate: Option<ApprovalGate>,

    // ── Per-message cache ──
    /// Cached rendered lines per message: (content_hash, rendered_lines).
    /// Indexed by message position in `self.messages`.
    pub per_message_cache: Vec<(u64, Vec<Line<'static>>)>,

    // ── Formatter registry ──
    /// Dynamic tool output formatter registry.
    pub tool_formatter_registry: super::formatters::formatter_registry::ToolFormatterRegistry,

    // ── Tool collapse ──
    /// Global default: tool results collapsed by default.
    pub tools_default_collapsed: bool,
    /// Per-tool override: tool_use_id -> expanded state. `true` = force expand.
    pub tool_expanded_overrides: HashMap<String, bool>,
    /// Cursor for Ctrl+O traversal: index into all_tool_use_ids() (from end).
    /// Increments each press, wraps around. Reset on new messages.
    pub tool_toggle_cursor: usize,
    /// When set, render will scroll to make this tool_use_id visible, then clear.
    pub scroll_to_tool: Option<String>,

    // ── StatusBar data ──
    /// Current working directory (shortened for display).
    pub working_dir: String,
    /// Current git branch name (if in a git repo).
    pub git_branch: Option<String>,
    /// Context window usage percentage (0.0–100.0).
    pub context_usage_pct: f64,
    /// Detailed git status counts.
    pub git_staged: usize,
    pub git_modified: usize,
    pub git_untracked: usize,
    pub git_unpushed: usize,
    /// When the TUI session started (for elapsed time display).
    pub session_start_time: Instant,
    /// Counter for periodic git info refresh (ticks between refreshes).
    pub git_refresh_counter: u32,

    // ── Plan steps (rendered inline in conversation) ──
    /// Plan steps from dual-mode agent (rendered as inline messages).
    pub plan_steps: Vec<octo_engine::agent::dual::PlanStep>,

    // ── Task timing ──
    /// When the current task (user message → completion) started.
    pub task_start_time: Option<Instant>,
    /// Input tokens for the current task only.
    pub task_input_tokens: u64,
    /// Output tokens for the current task only.
    pub task_output_tokens: u64,
    /// Tool calls in the current task.
    pub task_tool_calls: u32,
    /// Rounds (LLM calls) in the current task.
    pub task_rounds: u32,

    // ── Autocomplete ──
    /// Autocomplete engine for slash commands and file mentions.
    pub autocomplete: super::autocomplete::AutocompleteEngine,

    // ── Custom commands ──
    /// Loaded custom commands from ~/.octo/commands/ and .octo/commands/.
    pub custom_commands: Vec<octo_engine::commands::CustomCommand>,

    // ── SubAgent streaming ──
    /// Active sub-agent source ID (e.g. "skill-review"), or None if no sub-agent running.
    pub subagent_source_id: Option<String>,
    /// Sub-agent streaming text buffer (rendered separately from main streaming_text).
    pub subagent_streaming_text: String,
    /// Sub-agent thinking text buffer.
    pub subagent_thinking_text: String,
    /// Sub-agent active tool executions.
    pub subagent_active_tools: Vec<ActiveTool>,
    /// Sub-agent completion summary (rounds, tool_calls) shown after completion.
    pub subagent_completed: Option<(u32, u32)>,

    // ── Visual preferences ──
    /// Reduced motion: suppress shimmer, breathing, and stalled color transitions.
    pub reduced_motion: super::widgets::figures::ReducedMotion,
    /// Reasoning effort level (0=low, 1=med, 2=high, 3=max) for status bar display.
    pub effort_level: Option<u8>,

    // ── History search (Ctrl+R) ──
    /// Reverse incremental history search state.
    pub history_search: super::widgets::figures::HistorySearchState,

    // ── Permission mode (Shift+Tab) ──
    /// Current permission mode for tool execution.
    pub permission_mode: super::widgets::figures::PermissionMode,

    // ── Vim mode (E-8) ──
    /// Vim editing state.
    pub vim: super::widgets::figures::VimState,

    // ── Model selector (Meta+P, E-10) ──
    /// Model selector popup state.
    pub model_selector: super::widgets::figures::ModelSelectorState,

    // ── Sub-session tracking (E-17) ──
    /// Active sub-sessions for spinner tree display.
    pub sub_sessions: Vec<super::widgets::figures::SubSessionEntry>,
}

impl TuiState {
    /// Create a new TuiState for the given session and agent handle.
    pub fn new(session_id: SessionId, handle: AgentExecutorHandle, model_name: String) -> Self {
        let interrupt = InterruptManager::with_handle(handle.clone());

        // Use file-backed history so it persists across sessions
        let history_path = crate::repl::history::history_dir().join("tui_history.txt");
        let message_history = MessageHistory::with_file(100, history_path);

        let mut state = Self::with_history(session_id, handle, model_name, interrupt, message_history);
        state.refresh_git_info();
        state
    }

    /// Override the working directory (e.g. from --project flag).
    /// Also updates the autocomplete engine's base directory.
    pub fn set_working_dir(&mut self, dir: std::path::PathBuf) {
        self.working_dir = dir.display().to_string();
        self.autocomplete = super::autocomplete::AutocompleteEngine::new(dir);
    }

    /// Create TuiState with in-memory-only history (for tests).
    #[cfg(test)]
    pub fn new_for_test(
        session_id: SessionId,
        handle: AgentExecutorHandle,
        model_name: String,
    ) -> Self {
        let interrupt = InterruptManager::with_handle(handle.clone());
        let message_history = MessageHistory::new(100);
        Self::with_history(session_id, handle, model_name, interrupt, message_history)
    }

    fn with_history(
        session_id: SessionId,
        handle: AgentExecutorHandle,
        model_name: String,
        interrupt: InterruptManager,
        message_history: MessageHistory,
    ) -> Self {
        Self {
            running: true,
            mouse_captured: true,
            dirty: true,
            handle,
            messages: Vec::new(),
            streaming_text: String::new(),
            is_streaming: false,
            cancelled: false,
            active_tools: Vec::new(),
            pending_approval: None,
            input_buffer: String::new(),
            input_cursor: 0,
            scroll_offset: 0,
            user_scrolled: false,
            cached_lines: Vec::new(),
            lines_generation: 0,
            message_generation: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            session_id,
            model_name,
            terminal_width: 80,
            terminal_height: 24,
            has_focus: true,
            interrupt_manager: interrupt,
            spinner_service: SpinnerService::new(),
            message_history,
            overlay: OverlayMode::None,
            thinking_text: String::new(),
            is_thinking: false,
            welcome_state: WelcomePanelState::new(),
            scroll_last_dir: None,
            scroll_last_time: None,
            scroll_accel: 0,
            approval_gate: None,
            per_message_cache: Vec::new(),
            tool_formatter_registry:
                super::formatters::formatter_registry::ToolFormatterRegistry::new(),
            tools_default_collapsed: true,
            tool_expanded_overrides: HashMap::new(),
            tool_toggle_cursor: 0,
            scroll_to_tool: None,
            working_dir: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            git_branch: None,
            context_usage_pct: 0.0,
            session_start_time: Instant::now(),
            git_refresh_counter: 0,
            git_staged: 0,
            git_modified: 0,
            git_untracked: 0,
            git_unpushed: 0,
            plan_steps: Vec::new(),
            task_start_time: None,
            task_input_tokens: 0,
            task_output_tokens: 0,
            task_tool_calls: 0,
            task_rounds: 0,
            autocomplete: super::autocomplete::AutocompleteEngine::new(
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
            ),
            custom_commands: Vec::new(),
            subagent_source_id: None,
            subagent_streaming_text: String::new(),
            subagent_thinking_text: String::new(),
            subagent_active_tools: Vec::new(),
            subagent_completed: None,
            reduced_motion: super::widgets::figures::ReducedMotion::default(),
            effort_level: None,
            history_search: super::widgets::figures::HistorySearchState::default(),
            permission_mode: super::widgets::figures::PermissionMode::default(),
            vim: super::widgets::figures::VimState::default(),
            model_selector: super::widgets::figures::ModelSelectorState::default(),
            sub_sessions: Vec::new(),
        }
    }

    /// Compute a content hash for a single message (for per-message cache invalidation).
    fn hash_message(msg: &ChatMessage) -> u64 {
        let mut hasher = DefaultHasher::new();
        format!("{:?}", msg.role).hash(&mut hasher);
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => text.hash(&mut hasher),
                ContentBlock::ToolUse { id, name, input, .. } => {
                    id.hash(&mut hasher);
                    name.hash(&mut hasher);
                    input.to_string().hash(&mut hasher);
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                    ..
                } => {
                    tool_use_id.hash(&mut hasher);
                    content.hash(&mut hasher);
                    is_error.hash(&mut hasher);
                }
                _ => {}
            }
        }
        hasher.finish()
    }

    /// Render a single message into styled lines (used by per-message cache).
    fn render_single_message(msg: &ChatMessage) -> Vec<Line<'static>> {
        use super::formatters::markdown::MarkdownRenderer;

        let mut lines: Vec<Line<'static>> = Vec::new();

        // Role header
        let role_label = match msg.role {
            MessageRole::User => "You",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        };
        let role_style = match msg.role {
            MessageRole::User => ratatui::style::Style::default()
                .fg(ratatui::style::Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
            MessageRole::Assistant => ratatui::style::Style::default()
                .fg(ratatui::style::Color::Green)
                .add_modifier(ratatui::style::Modifier::BOLD),
            MessageRole::System => ratatui::style::Style::default()
                .fg(ratatui::style::Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        };
        lines.push(Line::from(ratatui::text::Span::styled(
            format!("─── {} ───", role_label),
            role_style,
        )));

        // Content blocks
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } => {
                    let md_lines = MarkdownRenderer::render(text);
                    lines.extend(md_lines);
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    lines.push(Line::from(ratatui::text::Span::styled(
                        format!("⚙ Tool: {}", name),
                        ratatui::style::Style::default().fg(ratatui::style::Color::Magenta),
                    )));
                    let input_str = input.to_string();
                    let summary: String = input_str.chars().take(80).collect();
                    if !summary.is_empty() {
                        lines.push(Line::from(ratatui::text::Span::styled(
                            format!("  {}", summary),
                            ratatui::style::Style::default()
                                .fg(ratatui::style::Color::DarkGray),
                        )));
                    }
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let style = if *is_error {
                        ratatui::style::Style::default().fg(ratatui::style::Color::Red)
                    } else {
                        ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)
                    };
                    let display: String =
                        content.lines().take(10).collect::<Vec<_>>().join("\n");
                    for line in display.lines() {
                        lines.push(Line::from(ratatui::text::Span::styled(
                            format!("  {}", line),
                            style,
                        )));
                    }
                }
                _ => {}
            }
        }

        lines.push(Line::from("")); // spacing
        lines
    }

    /// Rebuild cached lines from the current messages and streaming buffer.
    ///
    /// Uses per-message caching: only re-renders messages whose content hash
    /// has changed. Streaming and thinking text are always re-rendered.
    pub fn rebuild_cached_lines(&mut self) {
        use super::formatters::markdown::MarkdownRenderer;

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut new_cache = Vec::with_capacity(self.messages.len());

        for (i, msg) in self.messages.iter().enumerate() {
            let hash = Self::hash_message(msg);

            // Cache hit: reuse previously rendered lines
            if let Some((cached_hash, cached_lines)) = self.per_message_cache.get(i) {
                if *cached_hash == hash {
                    lines.extend(cached_lines.iter().cloned());
                    new_cache.push((hash, cached_lines.clone()));
                    continue;
                }
            }

            // Cache miss: render this message
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

        // Plan steps — rendered inline after messages
        if !self.plan_steps.is_empty() {
            let completed = self.plan_steps.iter().filter(|s| s.completed).count();
            let total = self.plan_steps.len();
            lines.push(Line::from(ratatui::text::Span::styled(
                format!("─── Plan ({}/{}) ───", completed, total),
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::Rgb(245, 158, 11)) // amber
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )));
            for (i, step) in self.plan_steps.iter().enumerate() {
                let (icon, color) = if step.completed {
                    ("\u{2705}", ratatui::style::Color::DarkGray) // ✅
                } else if i == completed {
                    ("\u{23F3}", ratatui::style::Color::White) // ⏳ current
                } else {
                    ("\u{25CB}", ratatui::style::Color::DarkGray) // ○ pending
                };
                lines.push(Line::from(ratatui::text::Span::styled(
                    format!(" {} {}. {}", icon, step.number, step.description),
                    ratatui::style::Style::default().fg(color),
                )));
            }
            lines.push(Line::from("")); // spacing
        }

        // Sub-agent streaming output — rendered in an indented block
        if self.subagent_source_id.is_some() {
            let source = self.subagent_source_id.as_deref().unwrap_or("sub-agent");
            let dim_style = ratatui::style::Style::default()
                .fg(ratatui::style::Color::DarkGray);
            let indent_style = ratatui::style::Style::default()
                .fg(ratatui::style::Color::Rgb(120, 120, 180)); // muted blue

            // Header
            lines.push(Line::from(ratatui::text::Span::styled(
                format!("  \u{2502} \u{2500}\u{2500}\u{2500} SubAgent: {} \u{2500}\u{2500}\u{2500}", source),
                indent_style,
            )));

            // Sub-agent thinking
            if !self.subagent_thinking_text.is_empty() {
                lines.push(Line::from(ratatui::text::Span::styled(
                    format!("  \u{2502} \u{1F4AD} Thinking..."),
                    dim_style.add_modifier(ratatui::style::Modifier::ITALIC),
                )));
                for line in self.subagent_thinking_text.lines().take(5) {
                    lines.push(Line::from(ratatui::text::Span::styled(
                        format!("  \u{2502}   {}", line),
                        dim_style.add_modifier(ratatui::style::Modifier::ITALIC),
                    )));
                }
            }

            // Sub-agent active tools (spinning)
            for tool in &self.subagent_active_tools {
                let elapsed = tool.started_at.elapsed().as_secs();
                lines.push(Line::from(ratatui::text::Span::styled(
                    format!("  \u{2502} \u{2699} {}: {}s", tool.name, elapsed),
                    indent_style,
                )));
            }

            // Sub-agent streaming text
            if !self.subagent_streaming_text.is_empty() {
                for line in self.subagent_streaming_text.lines() {
                    lines.push(Line::from(ratatui::text::Span::styled(
                        format!("  \u{2502} {}", line),
                        indent_style,
                    )));
                }
            }

            // Completion footer
            if let Some((rounds, tools)) = self.subagent_completed {
                lines.push(Line::from(ratatui::text::Span::styled(
                    format!("  \u{2570}\u{2500} Completed ({} rounds, {} tools)", rounds, tools),
                    dim_style,
                )));
            }
        }

        // Thinking text — always re-render (not cached)
        if !self.thinking_text.is_empty() {
            lines.push(Line::from(ratatui::text::Span::styled(
                "💭 Thinking...",
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::DarkGray)
                    .add_modifier(ratatui::style::Modifier::ITALIC),
            )));
            for line in self.thinking_text.lines() {
                lines.push(Line::from(ratatui::text::Span::styled(
                    line.to_string(),
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::DarkGray)
                        .add_modifier(ratatui::style::Modifier::ITALIC),
                )));
            }
        }

        self.per_message_cache = new_cache;
        self.cached_lines = lines;
        self.lines_generation = self.message_generation;
    }

    /// Mark content as changed, triggering a cache rebuild on next render.
    /// Per-message cache entries are preserved — only changed messages re-render.
    pub fn invalidate_cache(&mut self) {
        self.message_generation += 1;
        self.dirty = true;
    }

    /// Refresh git branch, detailed status counts, and unpushed commits.
    pub fn refresh_git_info(&mut self) {
        self.git_branch = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            });

        // Parse `git status --porcelain` for staged/modified/untracked
        let (mut staged, mut modified, mut untracked) = (0usize, 0usize, 0usize);
        if let Ok(o) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
        {
            if o.status.success() {
                for line in String::from_utf8_lossy(&o.stdout).lines() {
                    if line.len() < 2 {
                        continue;
                    }
                    let bytes = line.as_bytes();
                    let x = bytes[0]; // index (staged) status
                    let y = bytes[1]; // worktree status
                    if x == b'?' {
                        untracked += 1;
                    } else {
                        if x != b' ' && x != b'?' {
                            staged += 1;
                        }
                        if y != b' ' && y != b'?' {
                            modified += 1;
                        }
                    }
                }
            }
        }
        self.git_staged = staged;
        self.git_modified = modified;
        self.git_untracked = untracked;

        // Count unpushed commits: `git rev-list @{u}..HEAD --count`
        self.git_unpushed = std::process::Command::new("git")
            .args(["rev-list", "@{u}..HEAD", "--count"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout)
                        .ok()
                        .and_then(|s| s.trim().parse::<usize>().ok())
                } else {
                    None
                }
            })
            .unwrap_or(0);
    }

    /// Check if a tool result is collapsed.
    pub fn is_tool_collapsed(&self, tool_use_id: &str) -> bool {
        match self.tool_expanded_overrides.get(tool_use_id) {
            Some(expanded) => !expanded,
            None => self.tools_default_collapsed,
        }
    }

    /// Find the most recent completed tool_use_id in messages.
    pub fn find_last_tool_use_id(&self) -> Option<String> {
        self.messages
            .iter()
            .rev()
            .flat_map(|m| m.content.iter())
            .find_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
                _ => None,
            })
    }

    /// Collect all tool_use_ids from messages in order (oldest first).
    pub fn all_tool_use_ids(&self) -> Vec<String> {
        self.messages
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Find the next collapsed tool_use_id after the most recently toggled one.
    /// Returns the first collapsed tool if none was toggled yet, cycling through all.
    pub fn find_next_collapsed_tool_id(&self) -> Option<String> {
        let ids = self.all_tool_use_ids();
        if ids.is_empty() {
            return None;
        }
        // First try: find any collapsed tool (prefer last/most recent)
        ids.iter()
            .rev()
            .find(|id| self.is_tool_collapsed(id))
            .or_else(|| ids.last()) // all expanded — return last to allow re-collapse
            .cloned()
    }

    /// Extract the last assistant response text (final text block only, not intermediate monologue).
    pub fn last_assistant_response_text(&self) -> Option<String> {
        for msg in self.messages.iter().rev() {
            if msg.role != octo_types::message::MessageRole::Assistant {
                continue;
            }
            // Find the last Text block that is NOT followed by a ToolUse
            let blocks = &msg.content;
            for (i, block) in blocks.iter().enumerate().rev() {
                if let ContentBlock::Text { text } = block {
                    let has_tool_after = blocks[i + 1..]
                        .iter()
                        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
                    if !has_tool_after && !text.trim().is_empty() {
                        return Some(text.clone());
                    }
                }
            }
        }
        None
    }

    /// Copy text to system clipboard (macOS: pbcopy, Linux: xclip/xsel, fallback: no-op).
    pub fn copy_to_clipboard(text: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            use std::io::Write;
            if let Ok(mut child) = std::process::Command::new("pbcopy")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                return child.wait().map(|s| s.success()).unwrap_or(false);
            }
        }
        #[cfg(target_os = "linux")]
        {
            use std::io::Write;
            // Try xclip first, then xsel
            for cmd in &["xclip", "xsel"] {
                let args: &[&str] = if *cmd == "xclip" {
                    &["-selection", "clipboard"]
                } else {
                    &["--clipboard", "--input"]
                };
                if let Ok(mut child) = std::process::Command::new(cmd)
                    .args(args)
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(ref mut stdin) = child.stdin {
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    if child.wait().map(|s| s.success()).unwrap_or(false) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Invalidate all caches including per-message cache (e.g., on terminal resize).
    pub fn invalidate_all_cache(&mut self) {
        self.per_message_cache.clear();
        self.invalidate_cache();
    }

    /// Get the current agent state for status bar display.
    pub fn agent_state(&self) -> AgentState {
        if self.is_thinking {
            AgentState::Thinking
        } else if self.is_streaming {
            AgentState::Streaming
        } else {
            AgentState::Idle
        }
    }

    /// Auto-scroll to bottom unless user has manually scrolled up.
    pub fn auto_scroll(&mut self) {
        if !self.user_scrolled {
            self.scroll_offset = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::SessionId;
    use tokio::sync::{broadcast, mpsc};

    fn make_test_handle() -> AgentExecutorHandle {
        let (tx, _rx) = mpsc::channel(16);
        let (broadcast_tx, _) = broadcast::channel(16);
        AgentExecutorHandle {
            tx,
            broadcast_tx,
            session_id: SessionId::from_string("test-session"),
        }
    }

    #[test]
    fn tui_state_new_defaults() {
        let handle = make_test_handle();
        let state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        assert!(state.running);
        assert!(state.dirty);
        assert!(!state.is_streaming);
        assert!(state.messages.is_empty());
        assert!(state.active_tools.is_empty());
        assert!(state.pending_approval.is_none());
        assert_eq!(state.input_buffer, "");
        assert_eq!(state.scroll_offset, 0);
        assert!(!state.user_scrolled);
        assert_eq!(state.total_input_tokens, 0);
        assert_eq!(state.total_output_tokens, 0);
        assert_eq!(state.overlay, OverlayMode::None);
        assert!(!state.is_thinking);
    }

    #[test]
    fn tui_state_invalidate_cache_bumps_generation() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        assert_eq!(state.message_generation, 0);
        state.invalidate_cache();
        assert_eq!(state.message_generation, 1);
        assert!(state.dirty);
    }

    #[test]
    fn tui_state_rebuild_empty() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.rebuild_cached_lines();
        assert!(state.cached_lines.is_empty());
        assert_eq!(state.lines_generation, 0);
    }

    #[test]
    fn tui_state_rebuild_with_messages() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.messages.push(ChatMessage::user("Hello"));
        state.messages.push(ChatMessage::assistant("Hi there!"));
        state.message_generation = 1;
        state.rebuild_cached_lines();
        // Should have role headers + content + spacing
        assert!(!state.cached_lines.is_empty());
        assert_eq!(state.lines_generation, 1);
    }

    #[test]
    fn tui_state_rebuild_with_streaming() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.streaming_text = "partial response".to_string();
        state.rebuild_cached_lines();
        assert!(!state.cached_lines.is_empty());
    }

    #[test]
    fn tui_state_auto_scroll_resets_when_not_user_scrolled() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.scroll_offset = 10;
        state.user_scrolled = false;
        state.auto_scroll();
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn tui_state_auto_scroll_preserves_when_user_scrolled() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.scroll_offset = 10;
        state.user_scrolled = true;
        state.auto_scroll();
        assert_eq!(state.scroll_offset, 10);
    }

    #[test]
    fn overlay_mode_equality() {
        assert_eq!(OverlayMode::None, OverlayMode::None);
        assert_ne!(OverlayMode::None, OverlayMode::AgentDebug);
        assert_ne!(OverlayMode::Eval, OverlayMode::SessionPicker);
    }

    #[test]
    fn per_message_cache_reuse() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.messages.push(ChatMessage::user("Hello"));
        state.messages.push(ChatMessage::assistant("Hi"));
        state.message_generation = 1;
        state.rebuild_cached_lines();
        assert_eq!(state.per_message_cache.len(), 2);

        // Second rebuild without changes — should reuse cache (same hashes)
        let old_hashes: Vec<u64> = state.per_message_cache.iter().map(|(h, _)| *h).collect();
        state.message_generation = 2;
        state.rebuild_cached_lines();
        let new_hashes: Vec<u64> = state.per_message_cache.iter().map(|(h, _)| *h).collect();
        assert_eq!(old_hashes, new_hashes);
    }

    #[test]
    fn per_message_cache_invalidates_on_change() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.messages.push(ChatMessage::user("Hello"));
        state.message_generation = 1;
        state.rebuild_cached_lines();
        let old_hash = state.per_message_cache[0].0;

        // Modify the message — cache should miss
        state.messages[0] = ChatMessage::user("Goodbye");
        state.message_generation = 2;
        state.rebuild_cached_lines();
        let new_hash = state.per_message_cache[0].0;
        assert_ne!(old_hash, new_hash);
    }

    #[test]
    fn per_message_cache_grows_with_new_messages() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.messages.push(ChatMessage::user("Hello"));
        state.message_generation = 1;
        state.rebuild_cached_lines();
        assert_eq!(state.per_message_cache.len(), 1);

        // Add a second message
        state.messages.push(ChatMessage::assistant("Hi"));
        state.message_generation = 2;
        state.rebuild_cached_lines();
        assert_eq!(state.per_message_cache.len(), 2);
    }

    #[test]
    fn invalidate_all_cache_clears_per_message() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.messages.push(ChatMessage::user("Hello"));
        state.message_generation = 1;
        state.rebuild_cached_lines();
        assert!(!state.per_message_cache.is_empty());

        state.invalidate_all_cache();
        assert!(state.per_message_cache.is_empty());
    }

    #[test]
    fn pending_approval_clone() {
        let approval = PendingApproval {
            tool_id: "t1".to_string(),
            tool_name: "bash".to_string(),
            risk_level: RiskLevel::HighRisk,
        };
        let cloned = approval.clone();
        assert_eq!(cloned.tool_id, "t1");
        assert_eq!(cloned.tool_name, "bash");
    }

    #[test]
    fn agent_state_idle_by_default() {
        let handle = make_test_handle();
        let state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "model".to_string(),
        );
        assert_eq!(state.agent_state(), AgentState::Idle);
    }

    #[test]
    fn agent_state_streaming() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "model".to_string(),
        );
        state.is_streaming = true;
        assert_eq!(state.agent_state(), AgentState::Streaming);
    }

    #[test]
    fn agent_state_thinking_takes_priority() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "model".to_string(),
        );
        state.is_streaming = true;
        state.is_thinking = true;
        assert_eq!(state.agent_state(), AgentState::Thinking);
    }

    #[test]
    fn working_dir_and_git_branch_initialized() {
        let handle = make_test_handle();
        let state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "model".to_string(),
        );
        assert!(!state.working_dir.is_empty());
        assert_eq!(state.context_usage_pct, 0.0);
        // session_start_time is set to Instant::now() on creation
        assert!(state.session_start_time.elapsed().as_secs() < 2);
        assert!(state.plan_steps.is_empty());
        assert!(state.task_start_time.is_none());
        assert_eq!(state.task_input_tokens, 0);
    }

    #[test]
    fn tui_state_rebuild_with_thinking() {
        let handle = make_test_handle();
        let mut state = TuiState::new_for_test(
            SessionId::from_string("s1"),
            handle,
            "test-model".to_string(),
        );
        state.thinking_text = "let me consider...".to_string();
        state.is_thinking = true;
        state.rebuild_cached_lines();
        assert!(!state.cached_lines.is_empty());
    }
}
