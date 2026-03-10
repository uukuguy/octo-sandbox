//! Memory screen — multi-layer memory explorer

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

/// Memory layer information
struct MemoryLayer {
    label: &'static str,
    icon: &'static str,
    description: &'static str,
}

const LAYERS: &[MemoryLayer] = &[
    MemoryLayer {
        label: "L0: Working Memory",
        icon: "[*]",
        description: "Current conversation context (ephemeral, per-turn)",
    },
    MemoryLayer {
        label: "L1: Session Memory",
        icon: "[S]",
        description: "Per-session storage (persists across turns within a session)",
    },
    MemoryLayer {
        label: "L2: Persistent Memory",
        icon: "[P]",
        description: "Long-term storage (survives across sessions, SQLite-backed)",
    },
    MemoryLayer {
        label: "Knowledge Graph",
        icon: "[G]",
        description: "Entity-relation graph with full-text search (FTS5)",
    },
];

pub struct MemoryScreen {
    selected: usize,
}

impl MemoryScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Screen for MemoryScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Memory ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![
            Line::from(Span::styled(
                "Multi-Layer Memory Architecture",
                theme.block_title(),
            )),
            Line::from(""),
        ];

        for (i, layer) in LAYERS.iter().enumerate() {
            let style = if i == self.selected {
                theme.list_selected()
            } else {
                theme.text_normal()
            };
            let marker = if i == self.selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{} {}", marker, layer.icon, layer.label),
                style,
            )));
            lines.push(Line::from(Span::styled(
                format!("     {}", layer.description),
                theme.text_dim(),
            )));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "Use CLI for full access:",
            theme.text_dim(),
        )));
        lines.push(Line::from(Span::styled(
            "  octo memory search <query>",
            Style::default().fg(theme.info),
        )));
        lines.push(Line::from(Span::styled(
            "  octo memory list",
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
                    if self.selected + 1 < LAYERS.len() {
                        self.selected += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = MemoryScreen::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.title(), "Memory");
    }

    #[test]
    fn layers_are_defined() {
        assert_eq!(LAYERS.len(), 4);
        assert!(LAYERS[0].label.contains("L0"));
        assert!(LAYERS[3].label.contains("Graph"));
    }
}
