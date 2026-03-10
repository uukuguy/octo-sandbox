//! Dashboard screen — system overview

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::commands::AppState;
#[allow(unused_imports)]
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

pub struct DashboardScreen;

impl DashboardScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Screen for DashboardScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState) {
        let outer_block = theme.styled_block(" Dashboard ");
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // 2x2 grid layout
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);

        let top_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);

        let bot_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);

        // Agents panel
        let agent_count = state.agent_catalog.list_all().len();
        let agent_text = format!("{agent_count} registered");
        render_info_panel(frame, top_cols[0], theme, "Agents", &agent_text);

        // Environment panel
        let cwd = state.working_dir.display().to_string();
        let env_text = format!("Dir: {}", truncate(&cwd, 40));
        render_info_panel(frame, top_cols[1], theme, "Environment", &env_text);

        // Provider panel
        render_info_panel(frame, bot_cols[0], theme, "Provider", "Anthropic (default)");

        // Status panel — green dot for OK
        render_status_panel(frame, bot_cols[1], theme);
    }

    fn title(&self) -> &str {
        "Dashboard"
    }
}

fn render_info_panel(
    frame: &mut Frame,
    area: Rect,
    theme: &TuiTheme,
    title: &str,
    content: &str,
) {
    let block = Block::default()
        .title(title)
        .title_style(theme.block_title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let paragraph = Paragraph::new(content.to_string())
        .block(block)
        .style(theme.text_normal());
    frame.render_widget(paragraph, area);
}

fn render_status_panel(frame: &mut Frame, area: Rect, theme: &TuiTheme) {
    let block = Block::default()
        .title("Status")
        .title_style(theme.block_title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let line = Line::from(vec![
        Span::styled("● ", theme.status_ok()),
        Span::styled("System OK", theme.text_normal()),
    ]);
    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

/// Truncate a string to `max` characters, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = max.saturating_sub(3);
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("hello world!", 8);
        assert_eq!(result, "hello...");
        assert!(result.len() <= 8);
    }

    #[test]
    fn truncate_very_short_max() {
        let result = truncate("hello", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn dashboard_screen_title() {
        let screen = DashboardScreen::new();
        assert_eq!(screen.title(), "Dashboard");
    }
}
