//! Security screen — security policy and audit overview

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

struct SecurityItem {
    label: &'static str,
    status_color: fn(&TuiTheme) -> Style,
    description: &'static str,
}

const ITEMS: &[SecurityItem] = &[
    SecurityItem {
        label: "Autonomy Level",
        status_color: TuiTheme::status_ok,
        description: "Controls how much freedom the agent has (supervised / semi / full)",
    },
    SecurityItem {
        label: "Command Risk Assessment",
        status_color: TuiTheme::status_warn,
        description: "Commands are classified as Low / Medium / High / Critical risk",
    },
    SecurityItem {
        label: "Action Tracker",
        status_color: TuiTheme::status_ok,
        description: "Tracks and rate-limits tool invocations per session",
    },
    SecurityItem {
        label: "Audit Logging",
        status_color: TuiTheme::status_ok,
        description: "All agent actions recorded with timestamps and metadata",
    },
    SecurityItem {
        label: "Secret Manager",
        status_color: TuiTheme::status_ok,
        description: "AES-GCM encrypted secret storage with optional keyring",
    },
];

pub struct SecurityScreen {
    selected: usize,
}

impl SecurityScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Screen for SecurityScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Security & Audit ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![
            Line::from(Span::styled("Security Policy Overview", theme.block_title())),
            Line::from(""),
        ];

        for (i, item) in ITEMS.iter().enumerate() {
            let label_style = if i == self.selected {
                theme.list_selected()
            } else {
                (item.status_color)(theme)
            };
            let marker = if i == self.selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", marker, item.label),
                label_style,
            )));
            lines.push(Line::from(Span::styled(
                format!("     {}", item.description),
                theme.text_dim(),
            )));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "Configure via:",
            theme.text_dim(),
        )));
        lines.push(Line::from(Span::styled(
            "  octo config set security.autonomy_level <level>",
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
                    if self.selected + 1 < ITEMS.len() {
                        self.selected += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Security"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = SecurityScreen::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.title(), "Security");
    }

    #[test]
    fn items_are_defined() {
        assert_eq!(ITEMS.len(), 5);
        assert!(ITEMS[0].label.contains("Autonomy"));
    }
}
