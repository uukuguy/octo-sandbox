//! Main render function for the conversation-centric TUI layout.
//!
//! Vertical stack: conversation area → progress panel → input area → status bar.
//! Overlays (approval dialog, debug panels) render on top.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use super::app_state::{OverlayMode, PendingApproval, TuiState};

/// Render the entire TUI frame.
pub fn render(state: &TuiState, frame: &mut Frame) {
    let area = frame.area();

    // Dynamic panel heights
    let progress_height = if state.active_tools.is_empty() {
        0
    } else {
        (state.active_tools.len() as u16 + 2).min(8)
    };
    let input_lines = state.input_buffer.lines().count().max(1).min(8) as u16;
    let status_height = 2u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),                 // conversation area
            Constraint::Length(progress_height), // progress panel (hidden when empty)
            Constraint::Length(input_lines + 2), // input area (+border)
            Constraint::Length(status_height),   // status bar
        ])
        .split(area);

    render_conversation(state, frame, chunks[0]);
    if progress_height > 0 {
        render_progress(state, frame, chunks[1]);
    }
    render_input(state, frame, chunks[2]);
    render_status_bar(state, frame, chunks[3]);

    // Overlays (rendered on top)
    if let Some(ref approval) = state.pending_approval {
        render_approval_dialog(approval, frame, area);
    }
}

/// Render the conversation area with message history and scrollbar.
fn render_conversation(state: &TuiState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::NONE);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.cached_lines.is_empty() {
        // Welcome text when no messages
        let welcome = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Octo Agent — Conversation Mode",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Type a message and press Enter to start.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Ctrl+C to cancel | Double Ctrl+C to quit",
                Style::default().fg(Color::DarkGray),
            )),
        ]);
        frame.render_widget(welcome, inner);
        return;
    }

    let total_lines = state.cached_lines.len() as u16;
    let visible_height = inner.height;

    // Calculate scroll: we scroll from the bottom
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = max_scroll.saturating_sub(state.scroll_offset);

    let conversation = Paragraph::new(state.cached_lines.clone())
        .scroll((scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(conversation, inner);

    // Scrollbar
    if total_lines > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines as usize)
            .position(scroll as usize)
            .viewport_content_length(visible_height as usize);
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));
        frame.render_stateful_widget(
            scrollbar,
            inner.inner(Margin { vertical: 0, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }
}

/// Render the progress panel showing active tool executions.
fn render_progress(state: &TuiState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Active Tools ")
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let spinner_char = super::widgets::spinner::SPINNER_FRAMES
        [(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as usize / 100)
            % super::widgets::spinner::SPINNER_FRAMES.len()];

    let lines: Vec<Line> = state
        .active_tools
        .iter()
        .map(|tool| {
            Line::from(vec![
                Span::styled(
                    format!("{} ", spinner_char),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    tool.name.clone(),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    format!(" ({}s)", tool.elapsed_secs),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}

/// Render the input area with cursor.
fn render_input(state: &TuiState, frame: &mut Frame, area: Rect) {
    let title = if state.is_streaming {
        " Streaming... (Ctrl+C to cancel) "
    } else {
        " Message "
    };

    let border_color = if state.is_streaming {
        Color::Yellow
    } else {
        Color::Cyan
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_text = Paragraph::new(state.input_buffer.as_str())
        .style(Style::default().fg(Color::White));
    frame.render_widget(input_text, inner);

    // Position cursor
    let cursor_x = inner.x + state.input_cursor as u16;
    let cursor_y = inner.y;
    if cursor_x < inner.x + inner.width {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// Render the status bar with model, tokens, session info.
fn render_status_bar(state: &TuiState, frame: &mut Frame, area: Rect) {
    let left = format!(
        " {} | Session: {} ",
        state.model_name,
        state.session_id.as_str(),
    );
    let right = format!(
        "Tokens: {}↑ {}↓ ",
        state.total_input_tokens, state.total_output_tokens,
    );

    // Split status bar into left and right halves
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let left_para = Paragraph::new(Line::from(Span::styled(
        left,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(left_para, chunks[0]);

    let right_para = Paragraph::new(Line::from(Span::styled(
        right,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Right);
    frame.render_widget(right_para, chunks[1]);

    // Second row: hints
    if area.height >= 2 {
        let hints_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        };
        let hints = if state.overlay != OverlayMode::None {
            " Esc: close overlay "
        } else {
            " Ctrl+D: debug | Ctrl+C: cancel | ↑↓: scroll "
        };
        let hints_para = Paragraph::new(Line::from(Span::styled(
            hints,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(hints_para, hints_area);
    }
}

/// Render the tool approval dialog as a centered popup.
fn render_approval_dialog(approval: &PendingApproval, frame: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 8, area);
    frame.render_widget(Clear, popup);

    let risk_color = match approval.risk_level {
        octo_types::tool::RiskLevel::ReadOnly => Color::Green,
        octo_types::tool::RiskLevel::LowRisk => Color::Yellow,
        octo_types::tool::RiskLevel::HighRisk => Color::Red,
        octo_types::tool::RiskLevel::Destructive => Color::LightRed,
    };

    let block = Block::default()
        .title(format!(" Tool Approval: {} ", approval.tool_name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(risk_color));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let text = vec![
        Line::from(Span::styled(
            format!("Risk: {:?}", approval.risk_level),
            Style::default().fg(risk_color),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Y] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Approve  "),
            Span::styled("[N] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Deny  "),
            Span::styled("[A] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Always approve"),
        ]),
    ];
    let para = Paragraph::new(text);
    frame.render_widget(para, inner);
}

/// Create a centered rectangle with the given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height.min(100)) / 2),
            Constraint::Length(height),
            Constraint::Percentage((100 - height.min(100)) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_creates_valid_rect() {
        let area = Rect::new(0, 0, 100, 40);
        let popup = centered_rect(60, 8, area);
        assert!(popup.width > 0);
        assert!(popup.height > 0);
        assert!(popup.x >= area.x);
        assert!(popup.y >= area.y);
    }

    #[test]
    fn centered_rect_100_percent() {
        let area = Rect::new(0, 0, 80, 24);
        let popup = centered_rect(100, 24, area);
        // Should fill the area
        assert_eq!(popup.width, area.width);
    }

    #[test]
    fn centered_rect_small_area() {
        let area = Rect::new(0, 0, 20, 5);
        let popup = centered_rect(60, 3, area);
        assert!(popup.height <= area.height);
    }
}
