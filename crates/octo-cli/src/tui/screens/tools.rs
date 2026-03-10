//! Tools screen — registered tool browser

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

pub struct ToolsScreen {
    selected: usize,
}

impl ToolsScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Screen for ToolsScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState) {
        let block = theme.styled_block(" Tools ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Try to read tools from the registry (sync access via std::sync::Mutex)
        let specs = state
            .agent_runtime
            .tools()
            .lock()
            .ok()
            .map(|guard| guard.specs())
            .unwrap_or_default();

        if specs.is_empty() {
            let lines = vec![
                Line::from(Span::styled(
                    "No tools registered",
                    theme.text_dim(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Tools are loaded from built-in modules and MCP servers.",
                    theme.text_dim(),
                )),
                Line::from(Span::styled(
                    "  octo tools list    — List available tools",
                    Style::default().fg(theme.info),
                )),
            ];
            let paragraph = Paragraph::new(lines);
            frame.render_widget(paragraph, inner);
            return;
        }

        // Clamp selection
        let max_idx = specs.len().saturating_sub(1);
        if self.selected > max_idx {
            self.selected = max_idx;
        }

        // Build table rows
        let header = Row::new(vec![
            Cell::from("Name").style(theme.block_title()),
            Cell::from("Description").style(theme.block_title()),
        ]);

        let rows: Vec<Row> = specs
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let style = if i == self.selected {
                    theme.list_selected()
                } else {
                    theme.text_normal()
                };
                let desc = if spec.description.len() > 60 {
                    format!("{}...", &spec.description[..57])
                } else {
                    spec.description.clone()
                };
                Row::new(vec![
                    Cell::from(spec.name.clone()),
                    Cell::from(desc),
                ])
                .style(style)
            })
            .collect();

        let widths = [Constraint::Length(24), Constraint::Fill(1)];
        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(theme.list_selected());

        // Render table and a footer hint
        let chunks = Layout::vertical([Constraint::Fill(1), Constraint::Length(2)]).split(inner);

        frame.render_widget(table, chunks[0]);

        let footer = Paragraph::new(Line::from(Span::styled(
            format!(" {} tools registered | Up/Down to navigate", specs.len()),
            theme.text_dim(),
        )));
        frame.render_widget(footer, chunks[1]);
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::Key(key) = event {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.selected += 1;
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Tools"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = ToolsScreen::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.title(), "Tools");
    }

    #[test]
    fn truncation_logic() {
        let long = "a".repeat(80);
        let truncated = if long.len() > 60 {
            format!("{}...", &long[..57])
        } else {
            long.clone()
        };
        assert_eq!(truncated.len(), 60);
        assert!(truncated.ends_with("..."));
    }
}
