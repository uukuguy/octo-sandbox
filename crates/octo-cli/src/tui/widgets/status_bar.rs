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

/// Bottom status bar widget (always 2 rows: border + info).
#[allow(dead_code)]
pub struct StatusBarWidget<'a> {
    model: &'a str,
    working_dir: &'a str,
    git_branch: Option<&'a str>,
    git_staged: usize,
    git_modified: usize,
    git_untracked: usize,
    git_unpushed: usize,
    context_usage_pct: f64,
    session_elapsed: Option<std::time::Duration>,
    input_tokens: u64,
    output_tokens: u64,
    /// Sandbox profile name (e.g., "development", "staging", "production")
    sandbox_profile: Option<&'a str>,
    /// Reasoning effort level (0=low, 1=med, 2=high, 3=max)
    effort_level: Option<u8>,
}

/// Standalone activity indicator widget (1 row, shown between conversation and input).
pub struct ActivityIndicatorWidget {
    agent_state: AgentStateDisplay,
    task_elapsed: Option<std::time::Duration>,
    task_input_tokens: u64,
    task_output_tokens: u64,
    task_tool_calls: u32,
    task_rounds: u32,
}

impl ActivityIndicatorWidget {
    pub fn new(
        agent_state: AgentStateDisplay,
        task_elapsed: Option<std::time::Duration>,
        task_input_tokens: u64,
        task_output_tokens: u64,
    ) -> Self {
        Self {
            agent_state,
            task_elapsed,
            task_input_tokens,
            task_output_tokens,
            task_tool_calls: 0,
            task_rounds: 0,
        }
    }

    pub fn tool_calls(mut self, tool_calls: u32, rounds: u32) -> Self {
        self.task_tool_calls = tool_calls;
        self.task_rounds = rounds;
        self
    }
}

impl Widget for ActivityIndicatorWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use super::figures;

        if area.height == 0 {
            return;
        }

        let mut spans: Vec<Span> = Vec::new();

        let elapsed_ms = self
            .task_elapsed
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let elapsed_secs = elapsed_ms / 1000;

        // Stalled detection: shift color from amber → yellow → red
        let stalled = figures::StalledState::from_elapsed_secs(elapsed_secs);
        let (hue, sat, lightness_base) = stalled.breathing_params();

        // Breathing animation with stalled-aware color
        let phase = (elapsed_ms as f64 / 2000.0) * std::f64::consts::TAU;
        let lightness = lightness_base + 0.15 * phase.sin();
        let (r, g, b) = hsl_to_rgb(hue, sat, lightness);
        let breathing_color = ratatui::style::Color::Rgb(r, g, b);

        // Thinking shimmer color (gentle blue-purple wave)
        let shimmer_phase = (elapsed_ms as f64 / 3000.0) * std::f64::consts::TAU;
        let (sr, sg, sb) = figures::shimmer_color(shimmer_phase);
        let shimmer_color = ratatui::style::Color::Rgb(sr, sg, sb);

        // Spinner frame (braille dots)
        let spinner_frames = ['\u{28F7}', '\u{28EF}', '\u{28DF}', '\u{287F}', '\u{28BF}',
                              '\u{28FB}', '\u{28FD}', '\u{28FE}', '\u{28F7}', '\u{28EF}'];
        let frame_idx = (elapsed_ms / 100) as usize % spinner_frames.len();
        let spinner = spinner_frames[frame_idx];

        // Tick count for verb rotation (~10 ticks/sec at 100ms interval)
        let tick_count = elapsed_ms / 100;

        // Mode label: use spinner verbs for Thinking, static for Streaming
        let (mode_label, mode_color) = match self.agent_state {
            AgentStateDisplay::Streaming => (
                format!("{} Streaming", figures::arrow::RIGHT),
                style_tokens::GREEN_LIGHT,
            ),
            AgentStateDisplay::Thinking => {
                let verb = figures::spinner_verb(tick_count);
                (
                    format!("{} {}", figures::circle::EMPTY, verb),
                    shimmer_color,
                )
            }
            AgentStateDisplay::Idle => return,
        };

        spans.push(Span::styled(
            format!(" {} ", spinner),
            Style::default().fg(breathing_color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{} ", mode_label),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ));

        // Elapsed time with sub-second precision
        if let Some(elapsed) = self.task_elapsed {
            spans.push(Span::styled(
                figures::format_elapsed_precise(elapsed),
                Style::default().fg(breathing_color),
            ));
            spans.push(Span::styled("  ", Style::default()));
        }

        // Rounds and tool calls
        if self.task_rounds > 0 || self.task_tool_calls > 0 {
            spans.push(Span::styled(
                format!("{}r {}t", self.task_rounds, self.task_tool_calls),
                Style::default().fg(style_tokens::SUBTLE),
            ));
            spans.push(Span::styled("  ", Style::default()));
        }

        // Task tokens
        if self.task_input_tokens > 0 || self.task_output_tokens > 0 {
            let ti = StatusBarWidget::format_tokens(self.task_input_tokens);
            let to = StatusBarWidget::format_tokens(self.task_output_tokens);
            spans.push(Span::styled(
                format!("{}{ti} {}{to}", figures::arrow::RIGHT, figures::arrow::DOWN),
                Style::default().fg(style_tokens::SUBTLE),
            ));
        }

        // Single row: content only (no borders)
        let content_line = Line::from(spans);
        buf.set_line(area.left(), area.top(), &content_line, area.width);
    }
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(model: &'a str, working_dir: &'a str, git_branch: Option<&'a str>) -> Self {
        Self {
            model,
            working_dir,
            git_branch,
            git_staged: 0,
            git_modified: 0,
            git_untracked: 0,
            git_unpushed: 0,
            context_usage_pct: 0.0,
            session_elapsed: None,
            input_tokens: 0,
            output_tokens: 0,
            sandbox_profile: None,
            effort_level: None,
        }
    }

    /// Set the sandbox profile to display.
    pub fn sandbox_profile(mut self, profile: Option<&'a str>) -> Self {
        self.sandbox_profile = profile;
        self
    }

    /// Set the reasoning effort level (0=low, 1=med, 2=high, 3=max).
    pub fn effort_level(mut self, level: Option<u8>) -> Self {
        self.effort_level = level;
        self
    }

    pub fn git_status(mut self, staged: usize, modified: usize, untracked: usize, unpushed: usize) -> Self {
        self.git_staged = staged;
        self.git_modified = modified;
        self.git_untracked = untracked;
        self.git_unpushed = unpushed;
        self
    }

    pub fn context_usage_pct(mut self, pct: f64) -> Self {
        self.context_usage_pct = pct;
        self
    }

    pub fn session_elapsed(mut self, elapsed: Option<std::time::Duration>) -> Self {
        self.session_elapsed = elapsed;
        self
    }

    pub fn tokens(mut self, input: u64, output: u64) -> Self {
        self.input_tokens = input;
        self.output_tokens = output;
        self
    }

    pub fn format_tokens(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}k", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }
}

impl StatusBarWidget<'_> {
    /// Format elapsed duration as compact string: "5s", "1m12s", "3m"
    pub fn format_elapsed(d: std::time::Duration) -> String {
        let secs = d.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else {
            let m = secs / 60;
            let s = secs % 60;
            if s == 0 {
                format!("{}m", m)
            } else {
                format!("{}m{}s", m, s)
            }
        }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        let sep = Span::styled("  \u{2502}  ", Style::default().fg(style_tokens::GREY));

        // Row 0: border line (always)
        let border_line: String = "\u{2500}".repeat(area.width as usize);
        buf.set_string(
            area.left(),
            area.top(),
            &border_line,
            Style::default().fg(style_tokens::BORDER),
        );

        // Row 1: brand 🦑 Octo | model | tokens | mcp | cost | context%
        if area.height >= 2 {
            let mut spans: Vec<Span> = Vec::new();

            // Brand
            spans.push(Span::styled(
                " \u{1F991} Octo",
                Style::default()
                    .fg(style_tokens::AMBER)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(sep.clone());

            // Model name
            spans.push(Span::styled(
                self.model.to_string(),
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(sep.clone());

            // Session token usage
            if self.input_tokens > 0 || self.output_tokens > 0 {
                let input_str = Self::format_tokens(self.input_tokens);
                let output_str = Self::format_tokens(self.output_tokens);
                spans.push(Span::styled(
                    format!("\u{25B8}{input_str} \u{25BE}{output_str}"),
                    Style::default().fg(style_tokens::SUBTLE),
                ));
                spans.push(sep.clone());
            }

            // Session elapsed time
            if let Some(elapsed) = self.session_elapsed {
                spans.push(Span::styled(
                    format!("\u{23F1} {}", Self::format_elapsed(elapsed)),
                    Style::default().fg(style_tokens::SUBTLE),
                ));
                spans.push(sep.clone());
            }

            // Sandbox profile badge
            if let Some(profile) = self.sandbox_profile {
                let profile_color = match profile {
                    "development" => style_tokens::GREEN_LIGHT,
                    "staging" => style_tokens::GOLD,
                    "production" => style_tokens::ORANGE,
                    _ => style_tokens::SUBTLE,
                };
                spans.push(Span::styled(
                    format!("\u{25CF} {}", profile),
                    Style::default().fg(profile_color),
                ));
                spans.push(sep.clone());
            }

            // Effort indicator (○◐●◉)
            if let Some(level) = self.effort_level {
                let (symbol, label) = super::figures::effort_indicator(level);
                spans.push(Span::styled(
                    format!("{} {}", symbol, label),
                    Style::default().fg(style_tokens::SUBTLE),
                ));
                spans.push(sep.clone());
            }

            // Context remaining % with mini progress bar ▮▮▮▯▯
            let context_left = (100.0 - self.context_usage_pct).max(0.0);
            let pct_color = if context_left > 50.0 {
                style_tokens::GREEN_LIGHT
            } else if context_left > 25.0 {
                style_tokens::GOLD
            } else {
                style_tokens::ORANGE
            };

            let filled = ((context_left / 100.0) * 5.0).round() as usize;
            let bar: String = "\u{25AE}".repeat(filled)
                + &"\u{25AF}".repeat(5usize.saturating_sub(filled));
            spans.push(Span::styled(
                bar,
                Style::default().fg(pct_color),
            ));
            spans.push(Span::styled(
                format!(" {context_left:.0}%"),
                Style::default().fg(pct_color).add_modifier(Modifier::BOLD),
            ));

            buf.set_line(area.left(), area.top() + 1, &Line::from(spans), area.width);
        }

        // Row 2: directory | git
        if area.height >= 3 {
            let mut spans: Vec<Span> = Vec::new();
            spans.push(Span::raw(" "));

            // Working directory
            let short_dir = shorten_path(self.working_dir, 40);
            spans.push(Span::styled(
                short_dir,
                Style::default().fg(style_tokens::SUBTLE),
            ));

            // Git info: ⏇ branch +S ~M ?U ↑N — CC-style
            if let Some(branch) = self.git_branch {
                spans.push(sep.clone());
                let has_changes = self.git_staged + self.git_modified + self.git_untracked > 0;
                let branch_color = if has_changes {
                    ratatui::style::Color::Rgb(255, 255, 100) // bright yellow — dirty
                } else {
                    style_tokens::GREEN_LIGHT // green — clean
                };
                spans.push(Span::styled(
                    format!("\u{23C7} {}", branch),
                    Style::default().fg(branch_color),
                ));
                if self.git_staged > 0 {
                    spans.push(Span::styled(
                        format!(" +{}", self.git_staged),
                        Style::default().fg(style_tokens::GREEN_LIGHT),
                    ));
                }
                if self.git_modified > 0 {
                    spans.push(Span::styled(
                        format!(" ~{}", self.git_modified),
                        Style::default().fg(ratatui::style::Color::Rgb(255, 255, 100)),
                    ));
                }
                if self.git_untracked > 0 {
                    spans.push(Span::styled(
                        format!(" ?{}", self.git_untracked),
                        Style::default().fg(style_tokens::SUBTLE),
                    ));
                }
                if self.git_unpushed > 0 {
                    spans.push(Span::styled(
                        format!(" \u{2191}{}", self.git_unpushed),
                        Style::default().fg(style_tokens::AMBER),
                    ));
                }
            }

            buf.set_line(area.left(), area.top() + 2, &Line::from(spans), area.width);
        }
    }
}

/// HSL to RGB conversion (h in degrees, s and l in 0.0-1.0).
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Shorten a path to at most `max_len` characters, keeping the last components.
fn shorten_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    // Keep last path components that fit
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut result = String::new();
    for part in parts.iter().rev() {
        let candidate = if result.is_empty() {
            part.to_string()
        } else {
            format!("{}/{}", part, result)
        };
        if candidate.len() > max_len {
            break;
        }
        result = candidate;
    }
    if result.is_empty() {
        // Fallback: truncate from the end
        format!("…{}", &path[path.len().saturating_sub(max_len - 1)..])
    } else if result.len() < path.len() {
        format!("…/{}", result)
    } else {
        result
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
            .session_elapsed(Some(std::time::Duration::from_secs(120)))
            .tokens(5000, 1500);
    }

    #[test]
    fn test_status_bar_render_info_row() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("test-model", ".", None);
        widget.render(area, &mut buf);

        // Row 1 contains brand + model
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("test-model"));
    }

    #[test]
    fn test_status_bar_brand_symbol() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("test-model", "/home/user", Some("main"))
            .tokens(5000, 1500);
        widget.render(area, &mut buf);

        // Collect all symbols from the buffer
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Octo"), "Should contain brand name Octo");
        assert!(content.contains("test-model"), "Should contain model name");
    }

    #[test]
    fn test_activity_indicator_streaming() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = ActivityIndicatorWidget::new(
            AgentStateDisplay::Streaming,
            Some(std::time::Duration::from_secs(5)),
            100,
            50,
        )
        .tool_calls(3, 2);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Streaming"), "Should show Streaming label");
        assert!(content.contains("5.0s"), "Should show elapsed time with sub-second precision");
        assert!(content.contains("2r 3t"), "Should show rounds and tool calls");
    }

    #[test]
    fn test_activity_indicator_thinking() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = ActivityIndicatorWidget::new(
            AgentStateDisplay::Thinking,
            Some(std::time::Duration::from_secs(12)),
            0,
            0,
        );
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        // At 12s (120 ticks), spinner_verb selects VERBS[120/80 = 1] = "Reasoning"
        assert!(content.contains("Reasoning"), "Should show rotating spinner verb at 12s");
        assert!(content.contains("12s"), "Should show elapsed time");
    }

    #[test]
    fn test_activity_indicator_idle_renders_nothing() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = ActivityIndicatorWidget::new(
            AgentStateDisplay::Idle,
            None,
            0,
            0,
        );
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(!content.contains("Streaming"), "Should NOT show Streaming when idle");
        assert!(!content.contains("Thinking"), "Should NOT show Thinking when idle");
    }

    #[test]
    fn test_status_bar_with_elapsed() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .session_elapsed(Some(std::time::Duration::from_secs(125)));
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("2m5s"), "Should show session elapsed time");
    }

    #[test]
    fn test_shorten_path_short() {
        assert_eq!(shorten_path("/home/user", 30), "/home/user");
    }

    #[test]
    fn test_shorten_path_long() {
        let long = "/Users/someone/sandbox/LLM/speechless/Agents/octo-sandbox";
        let short = shorten_path(long, 25);
        assert!(short.len() <= 27); // 25 + "…/" prefix
        assert!(short.contains("octo-sandbox"), "Should keep last component");
    }

    #[test]
    fn test_status_bar_shows_working_dir() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", "/home/user/project", None);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("project"), "Should show working directory");
    }

    #[test]
    fn test_status_bar_git_symbol() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", Some("main"))
            .git_status(1, 2, 3, 0);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("\u{23C7}"), "Should use \u{23C7} (U+23C7) git symbol");
        assert!(content.contains("main"), "Should show branch name");
        assert!(content.contains("+1"), "Should show staged count");
        assert!(content.contains("~2"), "Should show modified count");
        assert!(content.contains("?3"), "Should show untracked count");
    }

    #[test]
    fn test_status_bar_context_progress_bar() {
        let area = Rect::new(0, 0, 120, 3);
        let mut buf = Buffer::empty(area);

        let widget = StatusBarWidget::new("model", ".", None)
            .context_usage_pct(40.0); // 60% remaining → 3 filled
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("60%"), "Should show context remaining percent");
        assert!(content.contains("\u{25AE}"), "Should contain filled bar segment ▮");
        assert!(content.contains("\u{25AF}"), "Should contain empty bar segment ▯");
    }
}
