//! Welcome screen — first-time setup and branding

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
#[allow(unused_imports)]
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

const LOGO: &str = r"
   ___       _
  / _ \  ___| |_ ___
 | | | |/ __| __/ _ \
 | |_| | (__| || (_) |
  \___/ \___|\__\___/
";

pub struct WelcomeScreen;

impl WelcomeScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Screen for WelcomeScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState) {
        let block = theme.styled_block(" Welcome ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let version = env!("CARGO_PKG_VERSION");
        let cwd = state.working_dir.display();

        let logo_style = Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD);
        let heading_style = Style::default()
            .fg(theme.text)
            .add_modifier(Modifier::BOLD);

        let mut lines: Vec<Line> = Vec::new();

        // ASCII art logo — one Line per row
        for row in LOGO.trim_matches('\n').lines() {
            lines.push(Line::from(Span::styled(row.to_string(), logo_style)));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("Octo Agent Workbench v{version}"),
            Style::default().fg(theme.accent),
        )));
        lines.push(Line::from(Span::styled(
            format!("Working directory: {cwd}"),
            theme.text_dim(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Quick Start:", heading_style)));
        lines.push(Line::from(Span::styled(
            "  Tab / Shift+Tab   Navigate between screens",
            theme.text_normal(),
        )));
        lines.push(Line::from(Span::styled(
            "  1-9               Jump to screen by number",
            theme.text_normal(),
        )));
        lines.push(Line::from(Span::styled(
            "  q / Ctrl+C        Quit",
            theme.text_normal(),
        )));
        lines.push(Line::from(Span::styled(
            "  ?                 Show help",
            theme.text_normal(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Navigate to the Chat tab to start a conversation.",
            theme.text_dim(),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn title(&self) -> &str {
        "Welcome"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_has_expected_lines() {
        let lines: Vec<&str> = LOGO.trim_matches('\n').lines().collect();
        assert!(lines.len() >= 5, "Logo should have at least 5 lines");
        // ASCII art spells "Octo" — check for recognizable fragments
        assert!(lines.iter().any(|l| l.contains("___")));
    }

    #[test]
    fn welcome_screen_title() {
        let screen = WelcomeScreen::new();
        assert_eq!(screen.title(), "Welcome");
    }
}
