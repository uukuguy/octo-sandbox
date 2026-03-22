//! Todo panel widget showing plan steps from dual-mode agent.
//!
//! Replaces the Active Tools panel with a structured plan view.
//! Steps are shown with status indicators (✅/⏳/○) and a progress bar.

use octo_engine::agent::dual::PlanStep;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::formatters::style_tokens;

/// Widget displaying plan steps as a todo list.
pub struct TodoPanelWidget<'a> {
    steps: &'a [PlanStep],
}

impl<'a> TodoPanelWidget<'a> {
    pub fn new(steps: &'a [PlanStep]) -> Self {
        Self { steps }
    }
}

impl Widget for TodoPanelWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || self.steps.is_empty() {
            return;
        }

        let completed = self.steps.iter().filter(|s| s.completed).count();
        let total = self.steps.len();

        // Row 0: header with plan progress and separator
        let header_text = format!(
            "\u{2500}\u{2500} Plan ({}/{}) ",
            completed, total
        );
        let hint = "Ctrl+P \u{2500}";
        let remaining = (area.width as usize)
            .saturating_sub(header_text.len() + hint.len());

        let header = Line::from(vec![
            Span::styled(
                header_text,
                Style::default()
                    .fg(style_tokens::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "\u{2500}".repeat(remaining),
                Style::default().fg(style_tokens::BORDER),
            ),
            Span::styled(hint, Style::default().fg(style_tokens::DIM_GREY)),
        ]);
        buf.set_line(area.left(), area.top(), &header, area.width);

        // Rows 1..N: plan steps
        let max_steps = (area.height.saturating_sub(2)) as usize; // -1 header, -1 progress
        for (i, step) in self.steps.iter().take(max_steps).enumerate() {
            let row = area.y + 1 + i as u16;
            if row >= area.y + area.height.saturating_sub(1) {
                break;
            }

            let (icon, icon_color) = if step.completed {
                ("\u{2705}", style_tokens::SUCCESS)  // ✅ green checkmark
            } else if i == completed {
                ("\u{23F3}", style_tokens::AMBER)    // ⏳ in progress (first incomplete)
            } else {
                ("\u{25CB}", style_tokens::DIM_GREY) // ○ pending
            };

            let desc_style = if step.completed {
                Style::default().fg(style_tokens::DIM_GREY)
            } else if i == completed {
                Style::default().fg(style_tokens::PRIMARY)
            } else {
                Style::default().fg(style_tokens::SUBTLE)
            };

            let line = Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(icon_color)),
                Span::styled(
                    format!("{}. ", step.number),
                    Style::default().fg(style_tokens::GREY),
                ),
                Span::styled(
                    truncate_str(&step.description, area.width.saturating_sub(8) as usize),
                    desc_style,
                ),
            ]);
            buf.set_line(area.left(), row, &line, area.width);
        }

        // Last row: progress bar
        let bar_row = area.y + area.height.saturating_sub(1);
        if bar_row > area.top() && total > 0 {
            let bar_width = (area.width.saturating_sub(2)) as usize;
            let filled = (bar_width * completed) / total;
            let empty = bar_width.saturating_sub(filled);

            let bar = Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    "\u{2501}".repeat(filled),
                    Style::default().fg(style_tokens::AMBER),
                ),
                Span::styled(
                    "\u{2501}".repeat(empty),
                    Style::default().fg(style_tokens::BORDER),
                ),
                Span::styled(
                    format!(" {}%", (completed * 100) / total),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
            ]);
            buf.set_line(area.left(), bar_row, &bar, area.width);
        }
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_steps() -> Vec<PlanStep> {
        vec![
            PlanStep { number: 1, description: "Setup project".into(), completed: true },
            PlanStep { number: 2, description: "Implement feature".into(), completed: false },
            PlanStep { number: 3, description: "Write tests".into(), completed: false },
        ]
    }

    #[test]
    fn todo_panel_renders_steps() {
        let steps = make_steps();
        let widget = TodoPanelWidget::new(&steps);
        let area = Rect::new(0, 0, 60, 6);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Plan"), "Should show Plan header");
        assert!(content.contains("1/3"), "Should show progress count");
    }

    #[test]
    fn todo_panel_empty_steps() {
        let steps: Vec<PlanStep> = vec![];
        let widget = TodoPanelWidget::new(&steps);
        let area = Rect::new(0, 0, 60, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Should render nothing
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(!content.contains("Plan"));
    }

    #[test]
    fn todo_panel_progress_bar() {
        let steps = vec![
            PlanStep { number: 1, description: "A".into(), completed: true },
            PlanStep { number: 2, description: "B".into(), completed: true },
            PlanStep { number: 3, description: "C".into(), completed: false },
        ];
        let widget = TodoPanelWidget::new(&steps);
        let area = Rect::new(0, 0, 80, 6);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("2/3"), "Should show 2/3 progress in header");
    }

    #[test]
    fn todo_panel_all_complete() {
        let steps = vec![
            PlanStep { number: 1, description: "Done".into(), completed: true },
            PlanStep { number: 2, description: "Also done".into(), completed: true },
        ];
        let widget = TodoPanelWidget::new(&steps);
        let area = Rect::new(0, 0, 80, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("2/2"), "Should show all complete in header");
    }

    #[test]
    fn truncate_str_works() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
        assert_eq!(truncate_str("ab", 2), "ab");
    }
}
