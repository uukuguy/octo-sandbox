//! MCP screen — MCP server management

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

struct McpSection {
    title: &'static str,
    status: &'static str,
    description: &'static str,
}

const SECTIONS: &[McpSection] = &[
    McpSection {
        title: "Server Lifecycle",
        status: "[--]",
        description: "Start, stop, and restart MCP servers (stdio/SSE)",
    },
    McpSection {
        title: "Tool Bridge",
        status: "[--]",
        description: "Unified tool interface wrapping MCP server tools",
    },
    McpSection {
        title: "Server Storage",
        status: "[--]",
        description: "SQLite-persisted server configurations",
    },
];

pub struct McpScreen {
    selected: usize,
}

impl McpScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Screen for McpScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" MCP Servers ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![
            Line::from(Span::styled(
                "Model Context Protocol Management",
                theme.block_title(),
            )),
            Line::from(""),
        ];

        for (i, section) in SECTIONS.iter().enumerate() {
            let style = if i == self.selected {
                theme.list_selected()
            } else {
                theme.text_normal()
            };
            let marker = if i == self.selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{} {}", marker, section.status, section.title),
                style,
            )));
            lines.push(Line::from(Span::styled(
                format!("     {}", section.description),
                theme.text_dim(),
            )));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "Use CLI for server management:",
            theme.text_dim(),
        )));
        lines.push(Line::from(Span::styled(
            "  octo mcp list              — List configured servers",
            Style::default().fg(theme.info),
        )));
        lines.push(Line::from(Span::styled(
            "  octo mcp add <name> <cmd>  — Add a new MCP server",
            Style::default().fg(theme.info),
        )));
        lines.push(Line::from(Span::styled(
            "  octo mcp remove <name>     — Remove an MCP server",
            Style::default().fg(theme.info),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::Key(key) = event {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected + 1 < SECTIONS.len() {
                        self.selected += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "MCP"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = McpScreen::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.title(), "MCP");
    }

    #[test]
    fn sections_are_defined() {
        assert_eq!(SECTIONS.len(), 3);
        assert!(SECTIONS[0].title.contains("Lifecycle"));
    }
}
