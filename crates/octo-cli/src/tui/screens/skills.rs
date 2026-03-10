//! Skills screen — skill management overview

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

struct SkillTopic {
    label: &'static str,
    description: &'static str,
}

const TOPICS: &[SkillTopic] = &[
    SkillTopic {
        label: "YAML Manifests",
        description: "Skills are defined as YAML files with name, description, and tool bindings",
    },
    SkillTopic {
        label: "Skill Registry",
        description: "Loaded skills are indexed in a registry for fast lookup by name",
    },
    SkillTopic {
        label: "Skill Runtime",
        description: "Execution engine providing SkillContext and tool access to skills",
    },
];

pub struct SkillsScreen {
    selected: usize,
}

impl SkillsScreen {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Screen for SkillsScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Skills ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = vec![
            Line::from(Span::styled("Skills Management", theme.block_title())),
            Line::from(""),
        ];

        for (i, topic) in TOPICS.iter().enumerate() {
            let style = if i == self.selected {
                theme.list_selected()
            } else {
                theme.text_normal()
            };
            let marker = if i == self.selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", marker, topic.label),
                style,
            )));
            lines.push(Line::from(Span::styled(
                format!("     {}", topic.description),
                theme.text_dim(),
            )));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "Skills are loaded from YAML manifests at startup.",
            theme.text_dim(),
        )));
        lines.push(Line::from(Span::styled(
            "Place .yaml files in the configured skills directory.",
            theme.text_dim(),
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
                    if self.selected + 1 < TOPICS.len() {
                        self.selected += 1;
                    }
                }
                _ => {}
            }
        }
    }

    fn title(&self) -> &str {
        "Skills"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_default() {
        let screen = SkillsScreen::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.title(), "Skills");
    }

    #[test]
    fn topics_are_defined() {
        assert_eq!(TOPICS.len(), 3);
        assert!(TOPICS[0].label.contains("YAML"));
    }
}
