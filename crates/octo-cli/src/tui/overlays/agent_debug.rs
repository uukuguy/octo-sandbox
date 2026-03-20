//! Agent debug overlay (Ctrl+D).
//!
//! Three-column layout:
//! - Left: Session info and context metrics
//! - Center: Recent conversation messages
//! - Right: Active tools and tool history

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::app_state::TuiState;

/// Render the agent debug overlay.
pub fn render(state: &TuiState, frame: &mut Frame, area: Rect) {
    let inner = super::render_overlay_frame("Agent Debug (Ctrl+D)", frame, area, Color::Cyan);

    // Three-column layout
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Session info
            Constraint::Percentage(40), // Conversation
            Constraint::Percentage(30), // Tools
        ])
        .split(inner);

    render_session_info(state, frame, columns[0]);
    render_conversation_preview(state, frame, columns[1]);
    render_tool_info(state, frame, columns[2]);
}

/// Left column: session info and metrics.
fn render_session_info(state: &TuiState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Session ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("Session: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.session_id.as_str().to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Model: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.model_name.clone(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "── Token Usage ──",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Input:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.total_input_tokens),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Output: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.total_output_tokens),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Total:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.total_input_tokens + state.total_output_tokens),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "── State ──",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Streaming: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.is_streaming { "Yes" } else { "No" },
                Style::default().fg(if state.is_streaming {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Thinking:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if state.is_thinking { "Yes" } else { "No" },
                Style::default().fg(if state.is_thinking {
                    Color::Magenta
                } else {
                    Color::DarkGray
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Messages:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.messages.len()),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Scroll:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.scroll_offset),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

/// Center column: recent conversation messages.
fn render_conversation_preview(state: &TuiState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(format!(" Conversation ({}) ", state.messages.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Show last N messages that fit
    let max_messages = (inner.height as usize).max(1);
    let start = state.messages.len().saturating_sub(max_messages);

    let mut lines: Vec<Line> = Vec::new();
    for msg in state.messages.iter().skip(start) {
        let role_label = match msg.role {
            octo_types::message::MessageRole::User => ("You", Color::Cyan),
            octo_types::message::MessageRole::Assistant => ("AI", Color::Green),
            octo_types::message::MessageRole::System => ("Sys", Color::Yellow),
        };
        // Get first text block as summary
        let summary: String = msg
            .content
            .iter()
            .find_map(|b| match b {
                octo_types::message::ContentBlock::Text { text } => {
                    Some(text.chars().take(60).collect())
                }
                _ => None,
            })
            .unwrap_or_else(|| "[non-text]".to_string());

        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", role_label.0),
                Style::default().fg(role_label.1).add_modifier(Modifier::BOLD),
            ),
            Span::styled(summary, Style::default().fg(Color::White)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No messages yet",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

/// Right column: active tools and tool state.
fn render_tool_info(state: &TuiState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Tools ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    // Active tools
    lines.push(Line::from(Span::styled(
        "── Active ──",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    if state.active_tools.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for tool in &state.active_tools {
            lines.push(Line::from(vec![
                Span::styled("  > ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    tool.name.clone(),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    format!(" ({}s)", tool.elapsed_secs),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Pending approval
    lines.push(Line::from(Span::styled(
        "── Approval ──",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    if let Some(ref approval) = state.pending_approval {
        lines.push(Line::from(vec![
            Span::styled("  Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                approval.tool_name.clone(),
                Style::default().fg(Color::Red),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  Risk: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:?}", approval.risk_level),
                Style::default().fg(Color::Red),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "  (none pending)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));

    // Terminal info
    lines.push(Line::from(Span::styled(
        "── Terminal ──",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled("  Size: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}x{}", state.terminal_width, state.terminal_height),
            Style::default().fg(Color::White),
        ),
    ]));

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_debug_functions_exist() {
        // Verify module compiles and functions are accessible
        let _ = render as fn(&TuiState, &mut Frame, Rect);
    }
}
