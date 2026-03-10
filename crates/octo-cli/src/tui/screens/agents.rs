//! Agents screen — agent catalog table with keyboard navigation

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

pub struct AgentsScreen {
    selected: usize,
}

impl AgentsScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Screen for AgentsScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, state: &AppState) {
        let block = theme.styled_block(" Agents ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let entries = state.agent_catalog.list_all();

        if entries.is_empty() {
            let msg = Paragraph::new("No agents registered. Use `octo agent create` to add one.")
                .style(theme.text_dim());
            frame.render_widget(msg, inner);
            return;
        }

        // Clamp selection to valid range
        if self.selected >= entries.len() {
            self.selected = entries.len().saturating_sub(1);
        }

        let header_style = Style::default().add_modifier(Modifier::BOLD).fg(theme.accent);
        let header = Row::new(vec![
            Cell::from("ID").style(header_style),
            Cell::from("Name").style(header_style),
            Cell::from("Role").style(header_style),
            Cell::from("Status").style(header_style),
        ]);

        let rows: Vec<Row> = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let style = if i == self.selected {
                    theme.list_selected()
                } else {
                    theme.text_normal()
                };
                let status_style = match &entry.state {
                    octo_engine::agent::entry::AgentStatus::Running => theme.status_ok(),
                    octo_engine::agent::entry::AgentStatus::Error(_) => theme.status_error(),
                    octo_engine::agent::entry::AgentStatus::Paused => theme.status_warn(),
                    _ => style,
                };
                Row::new(vec![
                    Cell::from(truncate_id(&entry.id.to_string())),
                    Cell::from(entry.manifest.name.clone()),
                    Cell::from(entry.manifest.role.clone().unwrap_or_default()),
                    Cell::from(entry.state.to_string()).style(status_style),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ];
        let table = Table::new(rows, widths).header(header);
        frame.render_widget(table, inner);
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::Key(key) = event {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.selected = self.selected.saturating_add(1);
                    // clamped during render
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Agents"
    }
}

/// Truncate a UUID-style ID to first 12 characters for display.
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> AppEvent {
        AppEvent::Key(crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))
    }

    #[test]
    fn truncate_id_cases() {
        assert_eq!(truncate_id("abc"), "abc");
        assert_eq!(truncate_id("123456789012"), "123456789012");
        assert_eq!(truncate_id("1234567890123"), "123456789012...");
    }

    #[test]
    fn screen_defaults_and_title() {
        let screen = AgentsScreen::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.title(), "Agents");
    }

    #[test]
    fn key_navigation() {
        let mut s = AgentsScreen::new();
        s.handle_event(&key(KeyCode::Up));
        assert_eq!(s.selected, 0); // saturates at zero
        s.handle_event(&key(KeyCode::Down));
        assert_eq!(s.selected, 1);
        s.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(s.selected, 0);
        s.handle_event(&key(KeyCode::Char('j')));
        assert_eq!(s.selected, 1);
    }
}
