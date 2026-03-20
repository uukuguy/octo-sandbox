//! Overlay panels rendered on top of the main conversation view.
//!
//! - Ctrl+D: Agent debug panel (session info, tool history, context metrics)
//! - Ctrl+E: Eval results panel (run history, task results)
//! - Ctrl+A: Session/Agent picker

pub mod agent_debug;
pub mod eval;
pub mod session_picker;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear};

use super::app_state::{OverlayMode, TuiState};

/// Render the currently active overlay, if any.
pub fn render_overlay(state: &TuiState, frame: &mut Frame, area: Rect) {
    match state.overlay {
        OverlayMode::None => {}
        OverlayMode::AgentDebug => agent_debug::render(state, frame, area),
        OverlayMode::Eval => eval::render(state, frame, area),
        OverlayMode::SessionPicker => session_picker::render(state, frame, area),
    }
}

/// Create a centered overlay rect (85% width, 80% height).
pub fn overlay_rect(area: Rect) -> Rect {
    let h_margin = area.width / 13; // ~7.5% each side
    let v_margin = area.height / 10; // ~10% each side
    Rect {
        x: area.x + h_margin,
        y: area.y + v_margin,
        width: area.width.saturating_sub(h_margin * 2),
        height: area.height.saturating_sub(v_margin * 2),
    }
}

/// Render a clear background and bordered frame for an overlay.
pub fn render_overlay_frame(
    title: &str,
    frame: &mut Frame,
    area: Rect,
    border_color: Color,
) -> Rect {
    let popup = overlay_rect(area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_rect_produces_valid_rect() {
        let area = Rect::new(0, 0, 120, 40);
        let popup = overlay_rect(area);
        assert!(popup.width > 0);
        assert!(popup.height > 0);
        assert!(popup.x >= area.x);
        assert!(popup.y >= area.y);
        assert!(popup.x + popup.width <= area.x + area.width);
        assert!(popup.y + popup.height <= area.y + area.height);
    }

    #[test]
    fn overlay_rect_small_terminal() {
        let area = Rect::new(0, 0, 40, 12);
        let popup = overlay_rect(area);
        assert!(popup.width > 0);
        assert!(popup.height > 0);
    }
}
