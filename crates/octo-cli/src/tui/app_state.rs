//! Application state for the conversation-centric TUI.
//!
//! `TuiState` holds all mutable state for the TUI event loop:
//! conversation history, streaming buffer, active tool executions,
//! input buffer, scroll position, and metrics.

use octo_engine::agent::AgentExecutorHandle;
use octo_types::message::{ChatMessage, ContentBlock, MessageRole};
use octo_types::tool::RiskLevel;
use octo_types::SessionId;
use ratatui::text::Line;

use super::managers::interrupt::InterruptManager;
use super::managers::message_history::MessageHistory;
use super::managers::spinner::SpinnerService;
use super::widgets::conversation::ActiveTool;

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
}

impl TuiState {
    /// Create a new TuiState for the given session and agent handle.
    pub fn new(session_id: SessionId, handle: AgentExecutorHandle, model_name: String) -> Self {
        let interrupt = InterruptManager::with_handle(handle.clone());
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
            message_history: MessageHistory::new(100),
            overlay: OverlayMode::None,
            thinking_text: String::new(),
            is_thinking: false,
        }
    }

    /// Rebuild cached lines from the current messages and streaming buffer.
    ///
    /// Called when `message_generation` has advanced past `lines_generation`.
    pub fn rebuild_cached_lines(&mut self) {
        use super::formatters::markdown::MarkdownRenderer;

        let mut lines: Vec<Line<'static>> = Vec::new();

        for msg in &self.messages {
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
                            ratatui::style::Style::default()
                                .fg(ratatui::style::Color::Magenta),
                        )));
                        // Show first line of input as summary
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
                        // Truncate long tool results
                        let display: String = content.lines().take(10).collect::<Vec<_>>().join("\n");
                        for line in display.lines() {
                            lines.push(Line::from(ratatui::text::Span::styled(
                                format!("  {}", line),
                                style,
                            )));
                        }
                    }
                    _ => {} // Image, Document — future
                }
            }

            lines.push(Line::from("")); // spacing between messages
        }

        // Append streaming text (if any)
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

        // Append thinking text (if any)
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

        self.cached_lines = lines;
        self.lines_generation = self.message_generation;
    }

    /// Mark content as changed, triggering a cache rebuild on next render.
    pub fn invalidate_cache(&mut self) {
        self.message_generation += 1;
        self.dirty = true;
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
        let state = TuiState::new(
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
        let mut state = TuiState::new(
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
        let mut state = TuiState::new(
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
        let mut state = TuiState::new(
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
        let mut state = TuiState::new(
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
        let mut state = TuiState::new(
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
        let mut state = TuiState::new(
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
        let mut state = TuiState::new(
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
