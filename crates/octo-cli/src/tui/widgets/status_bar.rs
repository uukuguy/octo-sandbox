//! Status bar widget showing model, tokens, git branch, MCP, cost, context %.
//!
//! Adapted from opendev-tui. The opendev AutonomyLevel/OperationMode/ReasoningLevel
//! types are replaced with octo-specific simplified versions.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::formatters::style_tokens;

/// Agent operational state for status bar display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStateDisplay {
    Idle,
    Streaming,
    Thinking,
}

/// Bottom status bar widget.
pub struct StatusBarWidget<'a> {
    model: &'a str,
    working_dir: &'a str,
    git_branch: Option<&'a str>,
    context_usage_pct: f64,
    session_cost: f64,
    mcp_status: Option<(usize, usize)>,
    mcp_has_errors: bool,
    spinner_char: Option<char>,
    input_tokens: u64,
    output_tokens: u64,
    agent_state: AgentStateDisplay,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(model: &'a str, working_dir: &'a str, git_branch: Option<&'a str>) -> Self {
        Self {
            model,
            working_dir,
            git_branch,
            context_usage_pct: 0.0,
            session_cost: 0.0,
            mcp_status: None,
            mcp_has_errors: false,
            spinner_char: None,
            input_tokens: 0,
            output_tokens: 0,
            agent_state: AgentStateDisplay::Idle,
        }
    }

    pub fn context_usage_pct(mut self, pct: f64) -> Self {
        self.context_usage_pct = pct;
        self
    }

    pub fn session_cost(mut self, cost: f64) -> Self {
        self.session_cost = cost;
        self
    }

    pub fn mcp_status(mut self, status: Option<(usize, usize)>, has_errors: bool) -> Self {
        self.mcp_status = status;
        self.mcp_has_errors = has_errors;
        self
    }

    pub fn spinner_char(mut self, ch: Option<char>) -> Self {
        self.spinner_char = ch;
        self
    }

    pub fn tokens(mut self, input: u64, output: u64) -> Self {
        self.input_tokens = input;
        self.output_tokens = output;
        self
    }

    pub fn agent_state(mut self, state: AgentStateDisplay) -> Self {
        self.agent_state = state;
        self
    }

    fn format_tokens(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}k", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        let mut spans: Vec<Span> = Vec::new();

        // Brand + Model name (✳ = U+2733 Eight Spoked Asterisk)
        spans.push(Span::styled(
            format!("\u{2733} {}", self.model),
            Style::default()
                .fg(style_tokens::AMBER)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "  \u{2502}  ",
            Style::default().fg(style_tokens::GREY),
        ));

        // Agent state indicator
        let (state_symbol, state_color) = match self.agent_state {
            AgentStateDisplay::Streaming => ("\u{25B8} streaming", style_tokens::GREEN_LIGHT),
            AgentStateDisplay::Thinking => ("\u{25E6} thinking", style_tokens::MAGENTA),
            AgentStateDisplay::Idle => ("\u{00B7} idle", style_tokens::DIM_GREY),
        };
        spans.push(Span::styled(
            state_symbol,
            Style::default().fg(state_color),
        ));
        spans.push(Span::styled(
            "  \u{2502}  ",
            Style::default().fg(style_tokens::GREY),
        ));

        // Token usage
        if self.input_tokens > 0 || self.output_tokens > 0 {
            let input_str = Self::format_tokens(self.input_tokens);
            let output_str = Self::format_tokens(self.output_tokens);
            spans.push(Span::styled(
                format!("\u{25B8}{input_str} \u{25BE}{output_str}"),
                Style::default().fg(style_tokens::SUBTLE),
            ));
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        // Repo info (path + git branch)
        let repo_display = self.build_repo_display();
        if !repo_display.is_empty() {
            spans.push(Span::styled(
                repo_display,
                Style::default().fg(style_tokens::BLUE_PATH),
            ));
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        // MCP status
        if let Some((connected, total)) = self.mcp_status {
            let mcp_label = format!("MCP: {connected}/{total}");
            let mcp_color = if self.mcp_has_errors {
                style_tokens::ORANGE
            } else if connected < total {
                style_tokens::GOLD
            } else {
                style_tokens::GREEN_LIGHT
            };
            spans.push(Span::styled(
                mcp_label,
                Style::default().fg(mcp_color).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        // Spinner when agent is active
        if let Some(ch) = self.spinner_char {
            spans.push(Span::styled(
                format!("{ch} "),
                Style::default()
                    .fg(style_tokens::AMBER)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        // Right-aligned: cost + context remaining
        let context_left = (100.0 - self.context_usage_pct).max(0.0);
        let pct_str = format!("{context_left:.0}");
        let pct_color = if context_left > 50.0 {
            style_tokens::GREEN_LIGHT
        } else if context_left > 25.0 {
            style_tokens::GOLD
        } else {
            style_tokens::ORANGE
        };

        let cost_str = if self.session_cost > 0.0 {
            if self.session_cost < 0.01 {
                format!("${:.4}", self.session_cost)
            } else {
                format!("${:.2}", self.session_cost)
            }
        } else {
            String::new()
        };

        let left_len: usize = spans.iter().map(|s| s.content.len()).sum();
        let right_text = if cost_str.is_empty() {
            format!("Context {pct_str}%")
        } else {
            format!("{cost_str}  \u{2502}  Context {pct_str}%")
        };
        let right_len = right_text.len();

        let available_width = area.width as usize;
        let gap = available_width.saturating_sub(left_len + right_len);
        if gap >= 2 {
            spans.push(Span::raw(" ".repeat(gap)));
        } else {
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        if !cost_str.is_empty() {
            spans.push(Span::styled(
                cost_str,
                Style::default()
                    .fg(style_tokens::AMBER)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                "  \u{2502}  ",
                Style::default().fg(style_tokens::GREY),
            ));
        }

        spans.push(Span::styled(
            format!("Context {pct_str}%"),
            Style::default().fg(pct_color).add_modifier(Modifier::BOLD),
        ));

        // Render border line + text
        let line = Line::from(spans);
        if area.height >= 2 {
            let border_line: String = "\u{2500}".repeat(area.width as usize);
            buf.set_string(
                area.left(),
                area.top(),
                &border_line,
                Style::default().fg(style_tokens::BORDER),
            );
            buf.set_line(area.left(), area.top() + 1, &line, area.width);
        } else {
            buf.set_line(area.left(), area.top(), &line, area.width);
        }
    }
}

impl StatusBarWidget<'_> {
    fn build_repo_display(&self) -> String {
        if self.working_dir.is_empty() || self.working_dir == "." {
            return String::new();
        }

        let shortener = crate::tui::formatters::path_shortener::PathShortener::default();
        let dir_display = shortener.shorten_display(self.working_dir);
        match self.git_branch {
            Some(branch) => format!("{dir_display} ({branch})"),
            None => dir_display,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(StatusBarWidget::format_tokens(500), "500");
        assert_eq!(StatusBarWidget::format_tokens(1_500), "1.5k");
        assert_eq!(StatusBarWidget::format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_status_bar_creation() {
        let _widget = StatusBarWidget::new("claude-sonnet-4", "/home/user/project", Some("main"))
            .context_usage_pct(25.0)
            .session_cost(0.05)
            .mcp_status(Some((2, 3)), false)
            .tokens(5000, 1500);
    }

    #[test]
    fn test_status_bar_render_single_row() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("test-model", ".", None);
        widget.render(area, &mut buf);

        let rendered: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(rendered.contains("test-model"));
    }

    #[test]
    fn test_status_bar_brand_symbol() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("test-model", "/home/user", Some("main"))
            .tokens(5000, 1500);
        widget.render(area, &mut buf);

        // Collect all symbols from the buffer
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("\u{2733}"), "Should contain brand symbol ✳");
        assert!(content.contains("test-model"), "Should contain model name");
    }

    #[test]
    fn test_status_bar_agent_state_streaming() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .agent_state(AgentStateDisplay::Streaming);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("streaming"), "Should show streaming state");
    }

    #[test]
    fn test_status_bar_agent_state_thinking() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .agent_state(AgentStateDisplay::Thinking);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("thinking"), "Should show thinking state");
    }

    #[test]
    fn test_status_bar_agent_state_idle() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .agent_state(AgentStateDisplay::Idle);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("idle"), "Should show idle state");
    }

    #[test]
    fn test_status_bar_with_cost() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .session_cost(0.05);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("$0.05"), "Should show session cost");
    }

    #[test]
    fn test_status_bar_with_mcp() {
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .mcp_status(Some((2, 3)), false);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("MCP"), "Should show MCP status");
        assert!(content.contains("2/3"), "Should show connected/total");
    }
}
