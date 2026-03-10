//! Settings screen — configuration management

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::theme::TuiTheme;

use super::Screen;

pub struct SettingsScreen;

impl SettingsScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Screen for SettingsScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState) {
        let block = theme.styled_block(" Settings ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let working_dir = state.working_dir.display().to_string();
        let db_path = state.db_path.display().to_string();

        let lines = vec![
            Line::from(Span::styled("Configuration", theme.block_title())),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Working Directory:  ", theme.text_dim()),
                Span::styled(&working_dir, theme.text_normal()),
            ]),
            Line::from(vec![
                Span::styled("  Database Path:      ", theme.text_dim()),
                Span::styled(&db_path, theme.text_normal()),
            ]),
            Line::from(vec![
                Span::styled("  Output Format:      ", theme.text_dim()),
                Span::styled(
                    format!("{:?}", state.output_config.format),
                    theme.text_normal(),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled("Configuration Sources (priority order):", theme.text_dim())),
            Line::from(Span::styled("  1. Environment variables (.env)", theme.text_normal())),
            Line::from(Span::styled("  2. CLI arguments", theme.text_normal())),
            Line::from(Span::styled("  3. config.yaml", theme.text_normal())),
            Line::from(""),
            Line::from(Span::styled("Manage via CLI:", theme.text_dim())),
            Line::from(Span::styled(
                "  octo config show       — Display current config",
                Style::default().fg(theme.info),
            )),
            Line::from(Span::styled(
                "  octo config validate   — Validate config file",
                Style::default().fg(theme.info),
            )),
        ];

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn title(&self) -> &str {
        "Settings"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = SettingsScreen::new();
        assert_eq!(screen.title(), "Settings");
    }
}
