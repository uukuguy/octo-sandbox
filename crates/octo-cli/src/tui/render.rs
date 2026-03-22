//! Main render function for the conversation-centric TUI layout.
//!
//! Vertical stack: conversation area → progress panel → input area → status bar.
//! Overlays (approval dialog, debug panels) render on top.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

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
    if state.overlay != OverlayMode::None {
        super::overlays::render_overlay(state, frame, area);
    }

    // Approval dialog (highest priority overlay)
    if let Some(ref approval) = state.pending_approval {
        render_approval_dialog(approval, frame, area);
    }
}

/// Render the conversation area using ConversationWidget or welcome panel.
fn render_conversation(state: &TuiState, frame: &mut Frame, area: Rect) {
    if state.messages.is_empty() && state.streaming_text.is_empty() {
        // Welcome panel when no messages yet
        let welcome = super::widgets::welcome_panel::WelcomePanel::new(
            &state.welcome_state,
            &state.model_name,
        );
        frame.render_widget(welcome, area);
        return;
    }

    // Build messages list: finalized + streaming (as temporary assistant message)
    let mut messages = state.messages.clone();
    if !state.streaming_text.is_empty() {
        use octo_types::message::ChatMessage;
        messages.push(ChatMessage::assistant(&state.streaming_text));
    }
    if !state.thinking_text.is_empty() {
        use octo_types::message::{ChatMessage, ContentBlock};
        messages.push(ChatMessage {
            role: octo_types::message::MessageRole::System,
            content: vec![ContentBlock::Text {
                text: format!(
                    "{} Thinking...\n{}",
                    crate::tui::formatters::style_tokens::THINKING_ICON,
                    state.thinking_text,
                ),
            }],
        });
    }

    // Use the real ConversationWidget (with markdown, tool formatting, scrollbar)
    let spinner_char = super::widgets::spinner::SPINNER_FRAMES
        [(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as usize
            / 100)
            % super::widgets::spinner::SPINNER_FRAMES.len()];

    let collapse = super::widgets::conversation::ToolCollapseState {
        default_collapsed: state.tools_default_collapsed,
        overrides: &state.tool_expanded_overrides,
    };

    let conversation = super::widgets::conversation::ConversationWidget::new(
        &messages,
        state.scroll_offset,
    )
    .active_tools(&state.active_tools, spinner_char)
    .formatter_registry(&state.tool_formatter_registry)
    .collapse_state(collapse);

    frame.render_widget(conversation, area);
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
                    format!(" ({}s)", tool.elapsed_secs()),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}

/// Render the input area using the ported OpenDev InputWidget.
fn render_input(state: &TuiState, frame: &mut Frame, area: Rect) {
    let mode = if state.is_streaming {
        "Streaming"
    } else {
        "NORMAL"
    };
    let input_widget = super::widgets::input::InputWidget::new(
        &state.input_buffer,
        state.input_cursor,
        mode,
        0, // pending_count — future: message queue
    );
    let result = input_widget.render_with_cursor(area, frame.buffer_mut());

    // Set terminal cursor position for IME (Chinese input method) placement
    if let Some((cx, cy)) = result.cursor_position {
        frame.set_cursor_position((cx, cy));
    }
}

/// Render the status bar with model, tokens, and hints.
fn render_status_bar(state: &TuiState, frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }

    let dim = Style::default().fg(Color::DarkGray);

    // Row 0: model | tokens (compact, left + right aligned)
    let left = format!(
        " {} \u{00b7} {}",
        state.model_name,
        state.session_id.as_str(),
    );
    let right = format!(
        "{}↑ {}↓ ",
        state.total_input_tokens, state.total_output_tokens,
    );

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(Rect { height: 1, ..area });

    frame.render_widget(Paragraph::new(Line::from(Span::styled(left, dim))), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(right, dim)))
            .alignment(Alignment::Right),
        chunks[1],
    );

    // Row 1: hints
    if area.height >= 2 {
        let hints_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        };
        let hints = if state.overlay != OverlayMode::None {
            " Esc: close overlay"
        } else {
            " Ctrl+D: debug \u{00b7} Ctrl+C: cancel \u{00b7} \u{2191}\u{2193} scroll"
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hints, dim))),
            hints_area,
        );
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
