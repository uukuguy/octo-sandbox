//! Dev view — Eval panel screen

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::commands::AppState;
use crate::tui::event::AppEvent;
use crate::tui::theme::TuiTheme;

use super::Screen;

/// Eval panel for the Dev view
pub struct DevEvalScreen;

impl DevEvalScreen {
    pub fn new() -> Self {
        Self
    }
}

impl Screen for DevEvalScreen {
    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &TuiTheme, _state: &AppState) {
        let block = theme.styled_block(" Eval ");
        frame.render_widget(
            Paragraph::new("Eval panel - loading...").block(block),
            area,
        );
    }

    fn handle_event(&mut self, _event: &AppEvent) {}

    fn title(&self) -> &str {
        "Eval"
    }
}
