//! Main render function for the conversation-centric TUI layout.
//!
//! Vertical stack: conversation area → progress panel → input area → status bar.
//! Overlays (approval dialog, debug panels) render on top.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::app_state::{OverlayMode, PendingApproval, TuiState};

/// Render the entire TUI frame.
pub fn render(state: &mut TuiState, frame: &mut Frame) {
    let area = frame.area();

    // Dynamic panel heights
    let input_lines = state.input_buffer.split('\n').count().max(1).min(8) as u16;
    let base_activity: u16 = if state.is_streaming || state.is_thinking { 1 } else { 0 };
    let sub_session_height = if !state.sub_sessions.is_empty() { state.sub_sessions.len() as u16 } else { 0 };
    let activity_height: u16 = base_activity + sub_session_height;
    let status_height = 4u16; // always: border + row1 (brand/dir/git) + row2 (tokens/mcp/cost/context) + empty

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),                    // conversation area
            Constraint::Length(activity_height),    // activity indicator (0 or 1)
            Constraint::Length(input_lines + 1),    // input area (top separator + text)
            Constraint::Length(status_height),      // status bar (border + info)
        ])
        .split(area);

    // Process deferred scroll-to-tool before rendering conversation
    if let Some(tool_id) = state.scroll_to_tool.take() {
        let conv_area = chunks[0];
        let messages = state.messages.clone();
        let collapse = super::widgets::conversation::ToolCollapseState {
            default_collapsed: state.tools_default_collapsed,
            overrides: &state.tool_expanded_overrides,
        };
        let widget = super::widgets::conversation::ConversationWidget::new(&messages, 0)
            .formatter_registry(&state.tool_formatter_registry)
            .collapse_state(collapse);
        if let Some(offset) = widget.scroll_offset_for_tool(&tool_id, conv_area.width, conv_area.height) {
            state.scroll_offset = offset;
        }
    }

    render_conversation(state, frame, chunks[0]);
    if activity_height > 0 {
        if base_activity > 0 {
            let indicator_area = Rect { height: 1, ..chunks[1] };
            render_activity_indicator(state, frame, indicator_area);
        }
        if sub_session_height > 0 {
            let tree_area = Rect {
                y: chunks[1].y + base_activity,
                height: sub_session_height,
                ..chunks[1]
            };
            let spinner_char = super::widgets::spinner::SPINNER_FRAMES
                [(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as usize
                    / 100)
                    % super::widgets::spinner::SPINNER_FRAMES.len()];
            render_sub_session_tree(&state.sub_sessions, frame, tree_area, spinner_char);
        }
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

    // Model selector popup (Meta+P)
    if state.model_selector.visible {
        render_model_selector(state, frame, area);
    }

    // Approval dialog (highest priority overlay)
    if let Some(ref approval) = state.pending_approval {
        render_approval_dialog(approval, frame, area, &state.theme);
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
        use grid_types::message::ChatMessage;
        messages.push(ChatMessage::assistant(&state.streaming_text));
    }
    if !state.thinking_text.is_empty() {
        use grid_types::message::{ChatMessage, ContentBlock};
        messages.push(ChatMessage {
            role: grid_types::message::MessageRole::System,
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
    // If history search is active, show search prompt instead of normal input
    if state.history_search.active {
        let search_line = state.history_search.prompt_line();
        let input_widget = super::widgets::input::InputWidget::new(
            &search_line,
            search_line.len(),
            "NORMAL",
            0,
        )
        .accent(state.theme.accent)
        .has_focus(state.has_focus)
        .hint_context(false, false, false);
        let result = input_widget.render_with_cursor(area, frame.buffer_mut());
        if let Some((cx, cy)) = result.cursor_position {
            frame.set_cursor_position((cx, cy));
        }
        return;
    }

    // Input mode is always "NORMAL" — activity indicator is shown separately above
    let input_widget = super::widgets::input::InputWidget::new(
        &state.input_buffer,
        state.input_cursor,
        "NORMAL",
        0, // pending_count — future: message queue
    )
    .accent(state.theme.accent)
    .has_focus(state.has_focus)
    .hint_context(
        state.is_streaming || state.is_thinking,
        state.overlay != super::app_state::OverlayMode::None,
        state.pending_approval.is_some(),
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
    .brand_color(state.theme.accent)
    .git_status(state.git_staged, state.git_modified, state.git_untracked, state.git_unpushed)
    .context_usage_pct(state.context_usage_pct)
    .session_elapsed(Some(state.session_start_time.elapsed()))
    .tokens(state.total_input_tokens, state.total_output_tokens)
    .effort_level(state.effort_level)
    .extended_thinking(state.extended_thinking);

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

    let theme = &state.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Completions ")
        .title_style(Style::default().fg(theme.accent));
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
                .fg(theme.surface)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let desc_style = if *selected {
            Style::default().fg(theme.surface).bg(theme.accent)
        } else {
            Style::default().fg(theme.text_secondary)
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

/// Render the tool approval dialog as a centered popup with enhanced risk UI.
fn render_approval_dialog(approval: &PendingApproval, frame: &mut Frame, area: Rect, theme: &crate::tui::theme::TuiTheme) {
    // E-14: Dynamic height — taller if we have args preview
    let has_args = approval.args_preview.is_some();
    let popup_height = if has_args { 14 } else { 10 };
    let popup = centered_rect(70, popup_height, area);
    frame.render_widget(Clear, popup);

    // Risk colors: semantic meaning preserved, mapped through theme where appropriate
    let (risk_color, risk_label) = match approval.risk_level {
        grid_types::tool::RiskLevel::ReadOnly => (theme.success, "Low Risk (Read-Only)"),
        grid_types::tool::RiskLevel::LowRisk => (Color::Yellow, "Low Risk"),
        grid_types::tool::RiskLevel::HighRisk => (theme.error, "High Risk"),
        grid_types::tool::RiskLevel::Destructive => (Color::LightRed, "Destructive"),
    };

    // Risk-colored border with tool name
    let block = Block::default()
        .title(format!(" {} ", approval.tool_name))
        .title_style(Style::default().fg(risk_color).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(risk_color));

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // Risk badge with middot separator
    let middot = super::widgets::figures::separator::MIDDOT;
    let mut text = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", super::widgets::figures::status::WARNING),
                Style::default().fg(risk_color),
            ),
            Span::styled(
                risk_label.to_string(),
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {} ", middot), Style::default().fg(theme.text_faint)),
            Span::styled(
                format!("tool_id: {}", &approval.tool_id[..approval.tool_id.len().min(16)]),
                Style::default().fg(theme.text_faint),
            ),
        ]),
    ];

    // E-14: Args preview (truncated, dimmed)
    if let Some(ref args) = approval.args_preview {
        text.push(Line::from(""));
        // Truncate to fit popup width, show up to 3 lines
        let max_width = inner.width.saturating_sub(2) as usize;
        for (i, line) in args.lines().take(3).enumerate() {
            let display: String = if line.len() > max_width {
                format!("{}\u{2026}", &line[..max_width.saturating_sub(1)])
            } else {
                line.to_string()
            };
            let prefix = if i == 0 { "\u{25B8} " } else { "  " };
            text.push(Line::from(Span::styled(
                format!("{}{}", prefix, display),
                Style::default().fg(theme.text_secondary),
            )));
        }
        if args.lines().count() > 3 {
            text.push(Line::from(Span::styled(
                format!("  \u{2026} ({} more lines)", args.lines().count() - 3),
                Style::default().fg(theme.text_faint),
            )));
        }
    }

    text.push(Line::from(""));
    // Keybinding hints (YNA)
    text.push(Line::from(vec![
        Span::styled("[Y] ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
        Span::raw("Allow"),
        Span::styled(format!("  {}  ", middot), Style::default().fg(theme.text_faint)),
        Span::styled("[N] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
        Span::raw("Deny"),
        Span::styled(format!("  {}  ", middot), Style::default().fg(theme.text_faint)),
        Span::styled("[A] ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("Allow session"),
    ]));

    let para = Paragraph::new(text);
    frame.render_widget(para, inner);
}

/// Render the model selector popup (Meta+P).
fn render_model_selector(state: &TuiState, frame: &mut Frame, area: Rect) {
    let theme = &state.theme;
    let models = &state.model_selector.models;
    let popup_height = (models.len() as u16 + 2).min(area.height); // +2 for border
    let popup_width = 35u16.min(area.width);

    // Position near the top-right area
    let popup_x = area.width.saturating_sub(popup_width + 2);
    let popup_y = 2;

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(" Select Model ")
        .title_style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    for (i, model) in models.iter().enumerate() {
        if (i as u16) >= inner.height {
            break;
        }
        let row = inner.y + i as u16;
        let is_active = i == state.model_selector.active_index;
        let is_selected = i == state.model_selector.selected;

        let marker = if is_active {
            super::widgets::figures::circle::FILLED
        } else {
            super::widgets::figures::circle::EMPTY
        };

        let style = if is_selected {
            Style::default()
                .fg(theme.surface)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let line = Line::from(vec![
            Span::styled(format!(" {} ", marker), style),
            Span::styled(model.clone(), style),
        ]);
        frame.buffer_mut().set_line(inner.x, row, &line, inner.width);
    }
}

/// Render multi-session spinner tree (E-17) in the activity indicator area.
///
/// Shows tree-structured parallel progress for sub-agent sessions.
/// Called from render_activity_indicator when sub-sessions exist.
fn render_sub_session_tree(
    sessions: &[super::widgets::figures::SubSessionEntry],
    frame: &mut Frame,
    area: Rect,
    spinner_char: char,
) {
    if sessions.is_empty() || area.height == 0 {
        return;
    }

    for (i, session) in sessions.iter().enumerate() {
        if (i as u16) >= area.height {
            break;
        }
        let row = area.y + i as u16;
        let is_last = i == sessions.len() - 1;
        let connector = if is_last {
            super::widgets::figures::tree::LAST
        } else {
            super::widgets::figures::tree::BRANCH
        };

        let status_color = if session.active {
            Color::Rgb(137, 209, 133) // green
        } else {
            Color::Rgb(122, 126, 134) // grey
        };

        let spinner = if session.active {
            format!("{} ", spinner_char)
        } else {
            format!("{} ", super::widgets::figures::status::SUCCESS)
        };

        let elapsed = super::widgets::figures::format_elapsed_precise(
            std::time::Duration::from_secs(session.elapsed_secs),
        );

        let line = Line::from(vec![
            Span::styled(format!("  {} ", connector), Style::default().fg(Color::DarkGray)),
            Span::styled(spinner, Style::default().fg(status_color)),
            Span::styled(
                session.name.clone(),
                Style::default().fg(status_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" — {}", session.status),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!(" ({})", elapsed),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        frame.buffer_mut().set_line(area.x, row, &line, area.width);
    }
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
