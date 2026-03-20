//! Session/Agent picker overlay (Ctrl+A).
//!
//! Shows current session info and available agents.
//! Future: full session switching via AgentRuntime.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::app_state::TuiState;

/// Render the session/agent picker overlay.
pub fn render(state: &TuiState, frame: &mut Frame, area: Rect) {
    let inner =
        super::render_overlay_frame("Session / Agent Picker (Ctrl+A)", frame, area, Color::Yellow);

    // Two-column layout
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    render_session_info(state, frame, columns[0]);
    render_agent_info(frame, columns[1]);
}

/// Left: current session info.
fn render_session_info(state: &TuiState, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Current Session ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("ID:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.session_id.as_str().to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Model:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state.model_name.clone(),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("Messages: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{}", state.messages.len()),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Tokens:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}",
                    state.total_input_tokens + state.total_output_tokens
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Session switching will be available",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "in a future update.",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

/// Right: available agents.
fn render_agent_info(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Available Agents ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let agents = [
        ("default", "General-purpose assistant"),
        ("coder", "Code generation specialist"),
        ("reviewer", "Code review expert"),
        ("researcher", "Research and analysis"),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (i, (name, desc)) in agents.iter().enumerate() {
        let marker = if i == 0 { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", marker),
                Style::default().fg(if i == 0 {
                    Color::Cyan
                } else {
                    Color::DarkGray
                }),
            ),
            Span::styled(
                format!("{:<12}", name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(if i == 0 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(desc.to_string(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Agent selection will be available",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "in a future update.",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_picker_functions_exist() {
        let _ = render as fn(&TuiState, &mut Frame, Rect);
    }
}
