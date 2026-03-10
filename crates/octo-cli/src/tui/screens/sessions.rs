//! Sessions screen — session list with keyboard navigation

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Paragraph, Row, Table};

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;
use super::Screen;

struct SessionRow {
    id: String,
    created_at: String,
    message_count: usize,
}

pub struct SessionsScreen {
    selected: usize,
    sessions: Vec<SessionRow>,
    loaded: bool,
}

impl SessionsScreen {
    pub fn new() -> Self {
        Self { selected: 0, sessions: vec![], loaded: false }
    }
}

impl Screen for SessionsScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Sessions ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.sessions.is_empty() {
            let msg = if self.loaded {
                "No sessions found. Start a chat to create one."
            } else {
                "Session data loads asynchronously.\nUse `octo session list` for full listing."
            };
            frame.render_widget(Paragraph::new(msg).style(theme.text_dim()), inner);
            return;
        }
        if self.selected >= self.sessions.len() {
            self.selected = self.sessions.len().saturating_sub(1);
        }
        let hs = Style::default().add_modifier(Modifier::BOLD).fg(theme.accent);
        let header = Row::new(vec![
            Cell::from("Session ID").style(hs),
            Cell::from("Created").style(hs),
            Cell::from("Messages").style(hs),
        ]);
        let rows: Vec<Row> = self.sessions.iter().enumerate().map(|(i, s)| {
            let st = if i == self.selected { theme.list_selected() } else { theme.text_normal() };
            Row::new(vec![
                Cell::from(truncate_id(&s.id)),
                Cell::from(s.created_at.clone()),
                Cell::from(s.message_count.to_string()),
            ]).style(st)
        }).collect();
        let widths = [Constraint::Percentage(40), Constraint::Percentage(35), Constraint::Percentage(25)];
        frame.render_widget(Table::new(rows, widths).header(header), inner);
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::Key(key) = event {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.selected = self.selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !self.sessions.is_empty() && self.selected < self.sessions.len() - 1 {
                        self.selected += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str { "Sessions" }
}

fn truncate_id(id: &str) -> String {
    if id.len() > 12 { format!("{}...", &id[..12]) } else { id.to_string() }
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
        assert_eq!(truncate_id("1234567890123456"), "123456789012...");
    }

    #[test]
    fn screen_defaults_and_title() {
        let s = SessionsScreen::new();
        assert_eq!(s.selected, 0);
        assert!(s.sessions.is_empty());
        assert!(!s.loaded);
        assert_eq!(s.title(), "Sessions");
    }

    #[test]
    fn key_nav_empty() {
        let mut s = SessionsScreen::new();
        s.handle_event(&key(KeyCode::Up));
        assert_eq!(s.selected, 0);
        s.handle_event(&key(KeyCode::Down));
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn key_nav_with_data() {
        let mut s = SessionsScreen::new();
        for i in 0..3 {
            s.sessions.push(SessionRow {
                id: format!("s-{i}"), created_at: "2024".into(), message_count: i,
            });
        }
        s.handle_event(&key(KeyCode::Down));
        assert_eq!(s.selected, 1);
        s.handle_event(&key(KeyCode::Down));
        assert_eq!(s.selected, 2);
        s.handle_event(&key(KeyCode::Down));
        assert_eq!(s.selected, 2); // clamped
        s.handle_event(&key(KeyCode::Char('k')));
        assert_eq!(s.selected, 1);
    }
}
