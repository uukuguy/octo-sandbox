//! Logs screen — structured log viewer

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::theme::TuiTheme;

use super::Screen;

pub struct LogsScreen;

impl LogsScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Screen for LogsScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Logs ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = vec![
            Line::from(Span::styled("Structured Log Viewer", theme.block_title())),
            Line::from(""),
            Line::from(Span::styled(
                "Log output is controlled by the RUST_LOG environment variable.",
                theme.text_normal(),
            )),
            Line::from(""),
            Line::from(Span::styled("Common configurations:", theme.text_dim())),
            Line::from(Span::styled(
                "  RUST_LOG=octo_server=debug,octo_engine=debug",
                Style::default().fg(theme.info),
            )),
            Line::from(Span::styled(
                "  RUST_LOG=octo_cli=trace",
                Style::default().fg(theme.info),
            )),
            Line::from(Span::styled(
                "  RUST_LOG=warn",
                Style::default().fg(theme.info),
            )),
            Line::from(""),
            Line::from(Span::styled("Log formats:", theme.text_dim())),
            Line::from(vec![
                Span::styled("  Pretty  ", Style::default().fg(theme.success)),
                Span::styled("— Human-readable, colored (default for TTY)", theme.text_normal()),
            ]),
            Line::from(vec![
                Span::styled("  JSON    ", Style::default().fg(theme.warning)),
                Span::styled("— Structured JSON lines (default for piped output)", theme.text_normal()),
            ]),
            Line::from(""),
            Line::from(Span::styled("Diagnostics:", theme.text_dim())),
            Line::from(Span::styled(
                "  octo doctor    — Run system health checks",
                Style::default().fg(theme.info),
            )),
        ];

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn title(&self) -> &str {
        "Logs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = LogsScreen::new();
        assert_eq!(screen.title(), "Logs");
    }
}
