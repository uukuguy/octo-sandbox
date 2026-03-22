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
    let input_lines = state.input_buffer.split('\n').count().max(1).min(8) as u16;
    let activity_height: u16 = if state.is_streaming || state.is_thinking { 3 } else { 0 };
    let status_height = 3u16; // always: border + row1 (brand/dir/git) + row2 (tokens/mcp/cost/context)

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),                    // conversation area
            Constraint::Length(activity_height),    // activity indicator (0 or 1)
            Constraint::Length(input_lines + 1),    // input area (top separator + text)
            Constraint::Length(status_height),      // status bar (border + info)
        ])
        .split(area);

    render_conversation(state, frame, chunks[0]);
    if activity_height > 0 {
        render_activity_indicator(state, frame, chunks[1]);
    }
    render_input(state, frame, chunks[2]);
    render_status_bar(state, frame, chunks[3]);

    // Autocomplete popup (rendered above input area)
    if state.autocomplete.is_visible() {
        render_autocomplete_popup(state, frame, chunks[2]);
    }

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

/// Render the activity indicator row (between conversation and input).
fn render_activity_indicator(state: &TuiState, frame: &mut Frame, area: Rect) {
    use super::widgets::status_bar::AgentStateDisplay;

    let agent_display = if state.is_thinking {
        AgentStateDisplay::Thinking
    } else {
        AgentStateDisplay::Streaming
    };
    let task_elapsed = state.task_start_time.map(|t| t.elapsed());

    let widget = super::widgets::status_bar::ActivityIndicatorWidget::new(
        agent_display,
        task_elapsed,
        state.task_input_tokens,
        state.task_output_tokens,
    )
    .tool_calls(state.task_tool_calls, state.task_rounds);
    frame.render_widget(widget, area);
}

/// Render the input area using the ported OpenDev InputWidget.
fn render_input(state: &TuiState, frame: &mut Frame, area: Rect) {
    // Input mode is always "NORMAL" — activity indicator is shown separately above
    let input_widget = super::widgets::input::InputWidget::new(
        &state.input_buffer,
        state.input_cursor,
        "NORMAL",
        0, // pending_count — future: message queue
    );
    let result = input_widget.render_with_cursor(area, frame.buffer_mut());

    // Set terminal cursor position for IME (Chinese input method) placement
    if let Some((cx, cy)) = result.cursor_position {
        frame.set_cursor_position((cx, cy));
    }
}

/// Render the status bar using the StatusBarWidget (always 2 rows: border + info).
fn render_status_bar(state: &TuiState, frame: &mut Frame, area: Rect) {
    let widget = super::widgets::status_bar::StatusBarWidget::new(
        &state.model_name,
        &state.working_dir,
        state.git_branch.as_deref(),
    )
    .git_status(state.git_staged, state.git_modified, state.git_untracked, state.git_unpushed)
    .context_usage_pct(state.context_usage_pct)
    .session_elapsed(Some(state.session_start_time.elapsed()))
    .tokens(state.total_input_tokens, state.total_output_tokens);

    frame.render_widget(widget, area);
}

/// Render the autocomplete popup above the input area.
fn render_autocomplete_popup(state: &TuiState, frame: &mut Frame, input_area: Rect) {
    let items = state.autocomplete.render_popup();
    if items.is_empty() {
        return;
    }

    let visible_count = items.len().min(10);
    let popup_height = visible_count as u16 + 2; // +2 for border
    let popup_width = 50u16.min(input_area.width);

    // Position above the input area
    let popup_y = input_area.y.saturating_sub(popup_height);
    let popup_x = input_area.x + 2; // indent slightly past the prompt

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Completions ")
        .title_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Scroll offset: keep selected item in view
    let selected_idx = state.autocomplete.selected_index();
    let scroll_offset = if selected_idx >= visible_count {
        selected_idx - visible_count + 1
    } else {
        0
    };

    // Render visible items with scroll
    for (vi, (label, desc, selected)) in items.iter().skip(scroll_offset).take(visible_count).enumerate() {
        if (vi as u16) >= inner.height {
            break;
        }
        let row = inner.y + vi as u16;
        let style = if *selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let desc_style = if *selected {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let label_width = label.len().min(inner.width as usize / 2);
        let desc_width = (inner.width as usize).saturating_sub(label_width + 3);
        let desc_display: String = if desc.len() > desc_width {
            desc.chars().take(desc_width.saturating_sub(1)).chain(std::iter::once('\u{2026}')).collect()
        } else {
            desc.clone()
        };

        let line = Line::from(vec![
            Span::styled(format!(" {:<width$}", label, width = label_width), style),
            Span::styled(format!("  {}", desc_display), desc_style),
        ]);
        frame.buffer_mut().set_line(inner.x, row, &line, inner.width);
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
