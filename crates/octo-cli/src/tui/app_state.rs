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
}

impl TuiState {
    /// Create a new TuiState for the given session and agent handle.
    pub fn new(session_id: SessionId, handle: AgentExecutorHandle, model_name: String) -> Self {
        let interrupt = InterruptManager::with_handle(handle.clone());

        // Use file-backed history so it persists across sessions
        let history_path = crate::repl::history::history_dir().join("tui_history.txt");
        let message_history = MessageHistory::with_file(100, history_path);

        Self::with_history(session_id, handle, model_name, interrupt, message_history)
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
            dirty: true,
            handle,
            messages: Vec::new(),
            streaming_text: String::new(),
            is_streaming: false,
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

    /// Invalidate all caches including per-message cache (e.g., on terminal resize).
    pub fn invalidate_all_cache(&mut self) {
        self.per_message_cache.clear();
        self.invalidate_cache();
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
