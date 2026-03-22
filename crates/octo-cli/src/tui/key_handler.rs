//! Keyboard event handler for the conversation-centric TUI.
//!
//! Maps key events to state mutations: text input, scrolling,
//! Ctrl+C cancellation, overlay toggles, and approval responses.

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use octo_engine::agent::AgentMessage;
use octo_types::message::ChatMessage;

use super::app_state::{OverlayMode, TuiState};

const SCROLL_AMOUNTS: [u16; 3] = [3, 6, 12];
const SCROLL_ACCEL_WINDOW_MS: u128 = 200;

/// Compute the scroll amount with 3-level acceleration.
///
/// Rapidly scrolling in the same direction within 200ms accelerates:
/// level 0 = 3 lines, level 1 = 6 lines, level 2 = 12 lines.
/// Changing direction or pausing resets to level 0.
fn compute_scroll_amount(state: &mut TuiState, direction_up: bool) -> u16 {
    let now = Instant::now();
    let same_dir = state.scroll_last_dir == Some(direction_up);
    let within_window = state
        .scroll_last_time
        .map(|t| now.duration_since(t).as_millis() < SCROLL_ACCEL_WINDOW_MS)
        .unwrap_or(false);

    if same_dir && within_window {
        state.scroll_accel = (state.scroll_accel + 1).min(2);
    } else {
        state.scroll_accel = 0;
    }

    state.scroll_last_dir = Some(direction_up);
    state.scroll_last_time = Some(now);
    SCROLL_AMOUNTS[state.scroll_accel as usize]
}

/// Execute a TUI-local slash command. Returns `true` if handled locally.
fn execute_slash_command(state: &mut TuiState, input: &str) {
    let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
    let cmd = parts[0];
    let _args = parts.get(1).copied().unwrap_or("");

    match cmd {
        "/help" | "/h" | "/?" => {
            let help_text = concat!(
                "Available commands:\n",
                "  /help       — Show this help\n",
                "  /clear      — Clear conversation history\n",
                "  /exit /quit — Exit the session\n",
                "  /mouse      — Toggle mouse capture (off = select text to copy)\n",
                "  /debug      — Toggle debug panel\n",
                "  /eval       — Toggle eval panel\n",
                "  /sessions   — Toggle session picker\n",
                "  /todo       — Toggle todo/plan panel\n",
                "  /compact    — Compact conversation context\n",
                "  /cost       — Show token usage and costs\n",
                "  /model      — Switch the LLM model\n",
                "  /mode       — Switch between plan/normal mode\n",
                "  /theme      — Change color theme\n",
                "\nKeyboard shortcuts:\n",
                "  Ctrl+Y      — Copy last response to clipboard\n",
                "  Ctrl+O      — Cycle through tool results (expand/collapse one by one)\n",
                "  Ctrl+Shift+O — Toggle ALL tool results expand/collapse\n",
                "\nText selection:\n",
                "  Most terminals (iTerm2, etc.) support native text selection & copy.\n",
                "  /mouse      — Toggle mouse capture off if native selection doesn't work.\n",
            );
            state
                .messages
                .push(ChatMessage::assistant(help_text));
            state.invalidate_cache();
            state.auto_scroll();
        }
        "/clear" => {
            state.messages.clear();
            state.streaming_text.clear();
            state.thinking_text.clear();
            state.per_message_cache.clear();
            state.active_tools.clear();
            state.plan_steps.clear();
            // Reset context and token counters
            state.context_usage_pct = 0.0;
            state.total_input_tokens = 0;
            state.total_output_tokens = 0;
            state.task_input_tokens = 0;
            state.task_output_tokens = 0;
            state.tool_expanded_overrides.clear();
            state.tool_toggle_cursor = 0;
            state.invalidate_cache();
            // Notify backend to clear conversation history
            let _ = state.handle.tx.try_send(AgentMessage::ClearHistory);
        }
        "/exit" | "/quit" | "/q" => {
            state.running = false;
        }
        "/debug" => {
            state.overlay = if state.overlay == OverlayMode::AgentDebug {
                OverlayMode::None
            } else {
                OverlayMode::AgentDebug
            };
        }
        "/eval" => {
            state.overlay = if state.overlay == OverlayMode::Eval {
                OverlayMode::None
            } else {
                OverlayMode::Eval
            };
        }
        "/sessions" => {
            state.overlay = if state.overlay == OverlayMode::SessionPicker {
                OverlayMode::None
            } else {
                OverlayMode::SessionPicker
            };
        }
        "/todo" => {
            // Plan steps are now shown inline in conversation — no separate panel
            let msg = "Plan steps are shown inline in the conversation area.";
            state.messages.push(ChatMessage::assistant(msg));
            state.invalidate_cache();
            state.auto_scroll();
        }
        "/mouse" => {
            state.mouse_captured = !state.mouse_captured;
            if state.mouse_captured {
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::event::EnableMouseCapture
                );
                let msg = "Mouse capture ON — scroll with mouse, Shift+drag to select text.";
                state.messages.push(ChatMessage::assistant(msg));
            } else {
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::event::DisableMouseCapture
                );
                let msg = "Mouse capture OFF — select text with mouse to copy. Use keyboard (↑↓/PgUp/PgDn) to scroll. /mouse to re-enable.";
                state.messages.push(ChatMessage::assistant(msg));
            }
            state.invalidate_cache();
            state.auto_scroll();
        }
        _ => {
            // Unknown slash command — show error message locally
            let msg = format!("Unknown command: {}. Type /help for available commands.", cmd);
            state.messages.push(ChatMessage::assistant(&msg));
            state.invalidate_cache();
            state.auto_scroll();
        }
    }
}

/// Handle a keyboard event, mutating TuiState accordingly.
pub async fn handle_key(state: &mut TuiState, key: KeyEvent) {
    // If an overlay is active, route to overlay key handler
    if state.overlay != OverlayMode::None {
        handle_overlay_key(state, key).await;
        return;
    }

    // If approval dialog is showing, route to approval handler
    if state.pending_approval.is_some() {
        handle_approval_key(state, key).await;
        return;
    }

    match (key.modifiers, key.code) {
        // ── Ctrl shortcuts ──
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            if state.interrupt_manager.handle_ctrl_c().await {
                state.running = false;
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
            state.overlay = if state.overlay == OverlayMode::AgentDebug {
                OverlayMode::None
            } else {
                OverlayMode::AgentDebug
            };
        }
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            // Emacs: move cursor to end of line; when input empty: toggle eval overlay
            if !state.input_buffer.is_empty() {
                state.input_cursor = state.input_buffer.len();
            } else {
                state.overlay = if state.overlay == OverlayMode::Eval {
                    OverlayMode::None
                } else {
                    OverlayMode::Eval
                };
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            // Emacs: move cursor to start of line; when input empty: toggle session picker
            if !state.input_buffer.is_empty() {
                state.input_cursor = 0;
            } else {
                state.overlay = if state.overlay == OverlayMode::SessionPicker {
                    OverlayMode::None
                } else {
                    OverlayMode::SessionPicker
                };
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            // Emacs: move cursor down one line in multiline input
            if state.input_buffer.contains('\n') {
                let lines: Vec<&str> = state.input_buffer.split('\n').collect();
                let mut pos = 0;
                let mut cursor_line = 0;
                let mut cursor_col = 0;
                for (i, line) in lines.iter().enumerate() {
                    if state.input_cursor <= pos + line.len() {
                        cursor_line = i;
                        cursor_col = state.input_cursor - pos;
                        break;
                    }
                    pos += line.len() + 1;
                    if i == lines.len() - 1 {
                        cursor_line = i;
                        cursor_col = line.len();
                    }
                }
                if cursor_line + 1 < lines.len() {
                    let next_line = lines[cursor_line + 1];
                    let new_col = cursor_col.min(next_line.len());
                    let mut new_pos = 0;
                    for line in &lines[..=cursor_line] {
                        new_pos += line.len() + 1;
                    }
                    state.input_cursor = new_pos + new_col;
                }
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            // Emacs: move cursor up one line in multiline input; when input empty: toggle todo
            if state.input_buffer.contains('\n') {
                let lines: Vec<&str> = state.input_buffer.split('\n').collect();
                let mut pos = 0;
                let mut cursor_line = 0;
                let mut cursor_col = 0;
                for (i, line) in lines.iter().enumerate() {
                    if state.input_cursor <= pos + line.len() {
                        cursor_line = i;
                        cursor_col = state.input_cursor - pos;
                        break;
                    }
                    pos += line.len() + 1;
                    if i == lines.len() - 1 {
                        cursor_line = i;
                        cursor_col = line.len();
                    }
                }
                if cursor_line > 0 {
                    let prev_line = lines[cursor_line - 1];
                    let new_col = cursor_col.min(prev_line.len());
                    let mut new_pos = 0;
                    for line in &lines[..cursor_line - 1] {
                        new_pos += line.len() + 1;
                    }
                    state.input_cursor = new_pos + new_col;
                }
            }
            // When input is single-line, Ctrl+P is a no-op (todo panel removed)
        }

        // ── Ctrl+O: cycle tool results — open one at a time, then close all ──
        // Cycle: open last → open second-to-last → ... → open first → close all → repeat
        // Only one tool is expanded at a time (previous one closes when next opens).
        // When opening: scroll to make the tool call visible.
        // When closing all: scroll back to bottom.
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
            let ids = state.all_tool_use_ids();
            if !ids.is_empty() {
                let n = ids.len();
                let cursor = state.tool_toggle_cursor % (n + 1); // extra slot = "close all"

                // Clear all overrides first (close previous)
                state.tool_expanded_overrides.clear();

                if cursor < n {
                    // Open one tool: cursor 0 = last, 1 = second-to-last, etc.
                    let target_idx = n - 1 - cursor;
                    let tool_id = ids[target_idx].clone();
                    state.tool_expanded_overrides.insert(tool_id.clone(), true);

                    // Scroll to make the tool visible: estimate lines below this tool
                    // by counting messages after the ToolUse message containing this id.
                    let msgs_after = state
                        .messages
                        .iter()
                        .rev()
                        .take_while(|m| {
                            !m.content.iter().any(|b| matches!(b,
                                octo_types::message::ContentBlock::ToolUse { id, .. } if id == &tool_id
                            ))
                        })
                        .count();
                    // ~2 lines per message (collapsed tool results, blank lines)
                    let estimated_offset = (msgs_after * 2).min(u16::MAX as usize) as u16;
                    state.scroll_offset = estimated_offset;
                    state.user_scrolled = true;
                } else {
                    // cursor == n → all closed — scroll back to bottom
                    state.scroll_offset = 0;
                    state.user_scrolled = false;
                }

                state.tool_toggle_cursor += 1;
                state.invalidate_cache();
            }
        }

        // ── Alt+O: toggle ALL tool results expand/collapse ──
        (KeyModifiers::ALT, KeyCode::Char('o')) => {
            state.tools_default_collapsed = !state.tools_default_collapsed;
            state.tool_expanded_overrides.clear();
            state.tool_toggle_cursor = 0;
            // Scroll to bottom when collapsing all
            if state.tools_default_collapsed {
                state.scroll_offset = 0;
                state.user_scrolled = false;
            }
            state.invalidate_cache();
        }
        // ── Ctrl+Shift+O: same as Alt+O (Alt may not work on macOS) ──
        (_, KeyCode::Char('O')) if key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::SHIFT) => {
            state.tools_default_collapsed = !state.tools_default_collapsed;
            state.tool_expanded_overrides.clear();
            state.tool_toggle_cursor = 0;
            if state.tools_default_collapsed {
                state.scroll_offset = 0;
                state.user_scrolled = false;
            }
            state.invalidate_cache();
        }

        // ── Ctrl+Y: copy last assistant response to clipboard ──
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            if let Some(text) = state.last_assistant_response_text() {
                if super::app_state::TuiState::copy_to_clipboard(&text) {
                    // Brief visual feedback — could add a toast/notification later
                    state.dirty = true;
                }
            }
        }

        // ── Tab: accept autocomplete suggestion ──
        (KeyModifiers::NONE, KeyCode::Tab) => {
            if state.autocomplete.is_visible() {
                if let Some((insert, delete_count)) = state.autocomplete.accept() {
                    // Delete trigger + partial text, then insert completion
                    let start = state.input_cursor.saturating_sub(delete_count);
                    state.input_buffer.replace_range(start..state.input_cursor, &insert);
                    state.input_cursor = start + insert.len();
                    state.dirty = true;
                }
            }
        }

        // ── Enter: accept autocomplete OR execute slash command OR submit input ──
        (KeyModifiers::NONE, KeyCode::Enter) => {
            // If autocomplete popup is visible, accept the selection
            if state.autocomplete.is_visible() {
                if let Some((insert, delete_count)) = state.autocomplete.accept() {
                    let start = state.input_cursor.saturating_sub(delete_count);
                    state.input_buffer.replace_range(start..state.input_cursor, &insert);
                    state.input_cursor = start + insert.len();
                    state.dirty = true;
                }
            } else if !state.input_buffer.trim().is_empty() && !state.is_streaming {
                let text = std::mem::take(&mut state.input_buffer);
                state.input_cursor = 0;

                // Check for slash commands
                if text.starts_with('/') {
                    execute_slash_command(state, &text);
                } else {
                    // Save to message history
                    state.message_history.push(text.clone());

                    // Add user message to conversation
                    state.messages.push(ChatMessage::user(&text));
                    state.invalidate_cache();
                    state.auto_scroll();

                    // Start task timing
                    state.task_start_time = Some(std::time::Instant::now());
                    state.task_input_tokens = 0;
                    state.task_output_tokens = 0;
                    state.task_tool_calls = 0;
                    state.task_rounds = 0;

                    // Send to agent
                    let _ = state
                        .handle
                        .send(AgentMessage::UserMessage {
                            content: text,
                            channel_id: "tui".into(),
                        })
                        .await;
                    state.is_streaming = true;
                    state.cancelled = false;
                    state.interrupt_manager.reset();
                }
            }
        }

        // ── Shift+Enter / Alt+Enter / Ctrl+J: newline in input ──
        (KeyModifiers::SHIFT, KeyCode::Enter)
        | (KeyModifiers::ALT, KeyCode::Enter)
        | (KeyModifiers::CONTROL, KeyCode::Char('j')) => {
            state.input_buffer.insert(state.input_cursor, '\n');
            state.input_cursor += 1;
            state.dirty = true;
        }

        // ── Arrow keys: autocomplete navigation / history / scroll ──
        (KeyModifiers::NONE, KeyCode::Up) => {
            if state.autocomplete.is_visible() {
                state.autocomplete.select_prev();
                return;
            }
            // Try history navigation first (when input is empty and history exists)
            if state.input_buffer.is_empty() && !state.message_history.is_empty() {
                if let Some(prev) = state.message_history.up() {
                    state.input_buffer = prev.to_string();
                    state.input_cursor = state.input_buffer.len();
                }
            } else if state.input_buffer.is_empty() {
                // No history — scroll up with acceleration
                let amount = compute_scroll_amount(state, true);
                state.scroll_offset = state.scroll_offset.saturating_add(amount);
                state.user_scrolled = true;
            } else {
                // Input has content — navigate history
                if let Some(prev) = state.message_history.up() {
                    state.input_buffer = prev.to_string();
                    state.input_cursor = state.input_buffer.len();
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if state.autocomplete.is_visible() {
                state.autocomplete.select_next();
                return;
            }
            if state.message_history.is_navigating() {
                // Currently browsing history — navigate forward
                if let Some(next) = state.message_history.down() {
                    state.input_buffer = next.to_string();
                    state.input_cursor = state.input_buffer.len();
                } else {
                    // Reached end of history — clear input
                    state.input_buffer.clear();
                    state.input_cursor = 0;
                }
            } else if state.user_scrolled {
                // Scroll down with acceleration
                let amount = compute_scroll_amount(state, false);
                state.scroll_offset = state.scroll_offset.saturating_sub(amount);
                if state.scroll_offset == 0 {
                    state.user_scrolled = false;
                }
            }
        }

        // ── Home/End: jump scroll ──
        (KeyModifiers::NONE, KeyCode::Home) => {
            state.scroll_offset = u16::MAX; // scroll to top
            state.user_scrolled = true;
        }
        (KeyModifiers::NONE, KeyCode::End) => {
            state.scroll_offset = 0;
            state.user_scrolled = false;
        }

        // ── PageUp/PageDown ──
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            state.scroll_offset = state
                .scroll_offset
                .saturating_add(state.terminal_height.saturating_sub(4));
            state.user_scrolled = true;
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            state.scroll_offset = state
                .scroll_offset
                .saturating_sub(state.terminal_height.saturating_sub(4));
            if state.scroll_offset == 0 {
                state.user_scrolled = false;
            }
        }

        // ── Text input ──
        (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            state.input_buffer.insert(state.input_cursor, c);
            state.input_cursor += c.len_utf8();
            state.interrupt_manager.reset();
            state.dirty = true;
            // Update autocomplete on every keystroke
            let text_before = state.input_buffer[..state.input_cursor].to_string();
            state.autocomplete.update(&text_before);
        }

        // ── Backspace ──
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            if state.input_cursor > 0 {
                // Find the previous char boundary
                let prev = state.input_buffer[..state.input_cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                state.input_buffer.remove(prev);
                state.input_cursor = prev;
                state.dirty = true;
                let text_before = state.input_buffer[..state.input_cursor].to_string();
                state.autocomplete.update(&text_before);
            }
        }

        // ── Delete ──
        (KeyModifiers::NONE, KeyCode::Delete) => {
            if state.input_cursor < state.input_buffer.len() {
                state.input_buffer.remove(state.input_cursor);
                state.dirty = true;
                let text_before = state.input_buffer[..state.input_cursor].to_string();
                state.autocomplete.update(&text_before);
            }
        }

        // ── Left/Right cursor ──
        (KeyModifiers::NONE, KeyCode::Left) => {
            if state.input_cursor > 0 {
                let prev = state.input_buffer[..state.input_cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                state.input_cursor = prev;
            }
        }
        (KeyModifiers::NONE, KeyCode::Right) => {
            if state.input_cursor < state.input_buffer.len() {
                let next = state.input_buffer[state.input_cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| state.input_cursor + i)
                    .unwrap_or(state.input_buffer.len());
                state.input_cursor = next;
            }
        }

        // ── Escape: dismiss autocomplete → cancel streaming → clear input → reset scroll ──
        (KeyModifiers::NONE, KeyCode::Esc) => {
            if state.autocomplete.is_visible() {
                state.autocomplete.dismiss();
                return;
            }
            if state.is_streaming || !state.active_tools.is_empty() {
                // Cancel current agent operation — highest priority
                let _ = state
                    .handle
                    .send(AgentMessage::Cancel)
                    .await;
                state.is_streaming = false;
                state.active_tools.clear();
                // Preserve partial streaming text as a message before clearing
                if !state.streaming_text.is_empty() {
                    let partial = std::mem::take(&mut state.streaming_text);
                    state
                        .messages
                        .push(octo_types::message::ChatMessage::assistant(&partial));
                    state.invalidate_cache();
                }
                // Mark as cancelled so Completed event won't overwrite preserved messages
                state.cancelled = true;
            } else if !state.input_buffer.is_empty() {
                state.input_buffer.clear();
                state.input_cursor = 0;
            } else if state.user_scrolled {
                state.scroll_offset = 0;
                state.user_scrolled = false;
            }
        }

        _ => {}
    }
}

/// Handle keys when an overlay is active.
async fn handle_overlay_key(state: &mut TuiState, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            state.overlay = OverlayMode::None;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
            state.overlay = if state.overlay == OverlayMode::AgentDebug {
                OverlayMode::None
            } else {
                OverlayMode::AgentDebug
            };
        }
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            state.overlay = if state.overlay == OverlayMode::Eval {
                OverlayMode::None
            } else {
                OverlayMode::Eval
            };
        }
        (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            state.overlay = if state.overlay == OverlayMode::SessionPicker {
                OverlayMode::None
            } else {
                OverlayMode::SessionPicker
            };
        }
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            if state.interrupt_manager.handle_ctrl_c().await {
                state.running = false;
            }
        }
        _ => {} // Overlays handle their own keys in T3
    }
}

/// Handle keys when the approval dialog is showing.
async fn handle_approval_key(state: &mut TuiState, key: KeyEvent) {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('y') | KeyCode::Char('Y')) => {
            // Approve
            if let Some(ref approval) = state.pending_approval {
                if let Some(ref gate) = state.approval_gate {
                    gate.respond(&approval.tool_id, true).await;
                }
            }
            state.pending_approval = None;
        }
        (KeyModifiers::NONE, KeyCode::Char('a') | KeyCode::Char('A')) => {
            // Always approve (respond true; future: persist preference)
            if let Some(ref approval) = state.pending_approval {
                if let Some(ref gate) = state.approval_gate {
                    gate.respond(&approval.tool_id, true).await;
                }
            }
            state.pending_approval = None;
        }
        (KeyModifiers::NONE, KeyCode::Char('n') | KeyCode::Char('N'))
        | (KeyModifiers::NONE, KeyCode::Esc) => {
            // Deny
            if let Some(ref approval) = state.pending_approval {
                if let Some(ref gate) = state.approval_gate {
                    gate.respond(&approval.tool_id, false).await;
                }
            }
            state.pending_approval = None;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
            if state.interrupt_manager.handle_ctrl_c().await {
                state.running = false;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use octo_types::message::ContentBlock;
    use octo_types::SessionId;
    use tokio::sync::{broadcast, mpsc};

    use crate::tui::app_state::TuiState;

    fn make_test_state() -> TuiState {
        let (tx, _rx) = mpsc::channel(16);
        let (broadcast_tx, _) = broadcast::channel(16);
        let handle = octo_engine::agent::AgentExecutorHandle {
            tx,
            broadcast_tx,
            session_id: SessionId::from_string("test"),
        };
        TuiState::new_for_test(SessionId::from_string("test"), handle, "test-model".to_string())
    }

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_ctrl_key(c: char) -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[tokio::test]
    async fn test_char_input() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('h'))).await;
        handle_key(&mut state, make_key(KeyCode::Char('i'))).await;
        assert_eq!(state.input_buffer, "hi");
        assert_eq!(state.input_cursor, 2);
    }

    #[tokio::test]
    async fn test_backspace() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('a'))).await;
        handle_key(&mut state, make_key(KeyCode::Char('b'))).await;
        handle_key(&mut state, make_key(KeyCode::Backspace)).await;
        assert_eq!(state.input_buffer, "a");
        assert_eq!(state.input_cursor, 1);
    }

    #[tokio::test]
    async fn test_backspace_empty() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Backspace)).await;
        assert_eq!(state.input_buffer, "");
        assert_eq!(state.input_cursor, 0);
    }

    #[tokio::test]
    async fn test_esc_clears_input() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('x'))).await;
        handle_key(&mut state, make_key(KeyCode::Esc)).await;
        assert_eq!(state.input_buffer, "");
        assert_eq!(state.input_cursor, 0);
    }

    #[tokio::test]
    async fn test_ctrl_c_first_does_not_exit() {
        let mut state = make_test_state();
        handle_key(&mut state, make_ctrl_key('c')).await;
        assert!(state.running);
    }

    #[tokio::test]
    async fn test_ctrl_c_double_exits() {
        let mut state = make_test_state();
        handle_key(&mut state, make_ctrl_key('c')).await;
        handle_key(&mut state, make_ctrl_key('c')).await;
        assert!(!state.running);
    }

    #[tokio::test]
    async fn test_ctrl_d_toggles_debug() {
        let mut state = make_test_state();
        handle_key(&mut state, make_ctrl_key('d')).await;
        assert_eq!(state.overlay, OverlayMode::AgentDebug);
        handle_key(&mut state, make_ctrl_key('d')).await;
        assert_eq!(state.overlay, OverlayMode::None);
    }

    #[tokio::test]
    async fn test_scroll_up_down() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Up)).await;
        assert_eq!(state.scroll_offset, 3);
        assert!(state.user_scrolled);
        handle_key(&mut state, make_key(KeyCode::Down)).await;
        assert_eq!(state.scroll_offset, 0);
        assert!(!state.user_scrolled);
    }

    #[tokio::test]
    async fn test_enter_sends_message() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('h'))).await;
        handle_key(&mut state, make_key(KeyCode::Char('i'))).await;
        handle_key(&mut state, make_key(KeyCode::Enter)).await;
        assert_eq!(state.input_buffer, "");
        assert!(state.is_streaming);
        assert_eq!(state.messages.len(), 1);
    }

    #[tokio::test]
    async fn test_enter_on_empty_does_nothing() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Enter)).await;
        assert!(!state.is_streaming);
        assert!(state.messages.is_empty());
    }

    #[tokio::test]
    async fn test_left_right_cursor() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('a'))).await;
        handle_key(&mut state, make_key(KeyCode::Char('b'))).await;
        assert_eq!(state.input_cursor, 2);
        handle_key(&mut state, make_key(KeyCode::Left)).await;
        assert_eq!(state.input_cursor, 1);
        handle_key(&mut state, make_key(KeyCode::Right)).await;
        assert_eq!(state.input_cursor, 2);
    }

    #[tokio::test]
    async fn test_overlay_esc_closes() {
        let mut state = make_test_state();
        state.overlay = OverlayMode::AgentDebug;
        handle_key(&mut state, make_key(KeyCode::Esc)).await;
        assert_eq!(state.overlay, OverlayMode::None);
    }

    #[tokio::test]
    async fn test_delete_key() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('a'))).await;
        handle_key(&mut state, make_key(KeyCode::Char('b'))).await;
        handle_key(&mut state, make_key(KeyCode::Left)).await;
        handle_key(&mut state, make_key(KeyCode::Delete)).await;
        assert_eq!(state.input_buffer, "a");
    }

    #[tokio::test]
    async fn test_home_end_scroll() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Home)).await;
        assert_eq!(state.scroll_offset, u16::MAX);
        assert!(state.user_scrolled);
        handle_key(&mut state, make_key(KeyCode::End)).await;
        assert_eq!(state.scroll_offset, 0);
        assert!(!state.user_scrolled);
    }

    #[tokio::test]
    async fn test_typing_resets_ctrl_c_count() {
        let mut state = make_test_state();
        handle_key(&mut state, make_ctrl_key('c')).await; // first ctrl+c
        assert_eq!(state.interrupt_manager.press_count(), 1);
        handle_key(&mut state, make_key(KeyCode::Char('a'))).await; // type something
        assert_eq!(state.interrupt_manager.press_count(), 0); // reset
    }

    #[tokio::test]
    async fn test_history_recall_after_submit() {
        let mut state = make_test_state();
        // Type "hello" and submit
        for c in "hello".chars() {
            handle_key(&mut state, make_key(KeyCode::Char(c))).await;
        }
        handle_key(&mut state, make_key(KeyCode::Enter)).await;
        assert_eq!(state.input_buffer, "");
        assert!(state.is_streaming);
        assert_eq!(state.message_history.len(), 1);

        // Simulate agent completion so is_streaming = false
        state.is_streaming = false;

        // Now press Up — should recall "hello"
        handle_key(&mut state, make_key(KeyCode::Up)).await;
        assert_eq!(state.input_buffer, "hello");
    }

    #[tokio::test]
    async fn test_history_recall_blocked_during_streaming() {
        let mut state = make_test_state();
        // Manually add history
        state.message_history.push("previous".into());
        state.is_streaming = true;

        // Press Up during streaming — ESC priority means streaming blocks history?
        // Actually Up key has no streaming check, so it should still work
        handle_key(&mut state, make_key(KeyCode::Up)).await;
        assert_eq!(state.input_buffer, "previous");
    }

    #[tokio::test]
    async fn test_approval_y_with_gate_clears_pending() {
        use octo_engine::tools::approval::ApprovalGate;
        let mut state = make_test_state();
        let gate = ApprovalGate::new();
        state.approval_gate = Some(gate.clone());
        // Register a pending approval in the gate and get the receiver
        let rx = gate.register("t1").await;
        state.pending_approval = Some(crate::tui::app_state::PendingApproval {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            risk_level: octo_types::tool::RiskLevel::HighRisk,
        });
        handle_key(&mut state, make_key(KeyCode::Char('y'))).await;
        assert!(state.pending_approval.is_none());
        // The receiver should get `true` (approved)
        assert_eq!(rx.await.unwrap(), true);
    }

    #[tokio::test]
    async fn test_approval_n_with_gate_denies() {
        use octo_engine::tools::approval::ApprovalGate;
        let mut state = make_test_state();
        let gate = ApprovalGate::new();
        state.approval_gate = Some(gate.clone());
        let rx = gate.register("t2").await;
        state.pending_approval = Some(crate::tui::app_state::PendingApproval {
            tool_id: "t2".into(),
            tool_name: "bash".into(),
            risk_level: octo_types::tool::RiskLevel::HighRisk,
        });
        handle_key(&mut state, make_key(KeyCode::Char('n'))).await;
        assert!(state.pending_approval.is_none());
        assert_eq!(rx.await.unwrap(), false);
    }

    #[tokio::test]
    async fn test_approval_a_with_gate_approves() {
        use octo_engine::tools::approval::ApprovalGate;
        let mut state = make_test_state();
        let gate = ApprovalGate::new();
        state.approval_gate = Some(gate.clone());
        let rx = gate.register("t3").await;
        state.pending_approval = Some(crate::tui::app_state::PendingApproval {
            tool_id: "t3".into(),
            tool_name: "bash".into(),
            risk_level: octo_types::tool::RiskLevel::HighRisk,
        });
        handle_key(&mut state, make_key(KeyCode::Char('a'))).await;
        assert!(state.pending_approval.is_none());
        assert_eq!(rx.await.unwrap(), true);
    }

    #[tokio::test]
    async fn test_approval_without_gate_still_clears() {
        let mut state = make_test_state();
        // No gate set — approval_gate is None
        state.pending_approval = Some(crate::tui::app_state::PendingApproval {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            risk_level: octo_types::tool::RiskLevel::HighRisk,
        });
        handle_key(&mut state, make_key(KeyCode::Char('y'))).await;
        assert!(state.pending_approval.is_none());
    }

    #[test]
    fn test_tool_collapsed_by_default() {
        let state = make_test_state();
        assert!(state.is_tool_collapsed("any-tool-id"));
        assert!(state.tools_default_collapsed);
    }

    #[test]
    fn test_tool_expand_override() {
        let mut state = make_test_state();
        state.tool_expanded_overrides.insert("t1".into(), true);
        assert!(!state.is_tool_collapsed("t1")); // expanded
        assert!(state.is_tool_collapsed("t2")); // others still collapsed
    }

    #[test]
    fn test_global_toggle_clears_overrides() {
        let mut state = make_test_state();
        state.tool_expanded_overrides.insert("t1".into(), true);
        // Simulate Alt+O: toggle global + clear overrides
        state.tools_default_collapsed = !state.tools_default_collapsed;
        state.tool_expanded_overrides.clear();
        assert!(!state.is_tool_collapsed("t1")); // follows global (now false)
    }

    #[tokio::test]
    async fn test_ctrl_o_toggles_last_tool() {
        let mut state = make_test_state();
        // Add a message with a tool result
        state.messages.push(ChatMessage {
            role: octo_types::message::MessageRole::Assistant,
            content: vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "bash".into(),
                    input: serde_json::json!({"command": "ls"}),
                },
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "file1\nfile2".into(),
                    is_error: false,
                },
            ],
        });
        assert!(state.is_tool_collapsed("t1")); // collapsed by default

        // Ctrl+O should toggle the last tool
        handle_key(&mut state, make_ctrl_key('o')).await;
        assert!(!state.is_tool_collapsed("t1")); // now expanded

        // Ctrl+O again should collapse it
        handle_key(&mut state, make_ctrl_key('o')).await;
        assert!(state.is_tool_collapsed("t1")); // back to collapsed
    }

    #[tokio::test]
    async fn test_alt_o_toggles_global() {
        let mut state = make_test_state();
        assert!(state.tools_default_collapsed);

        let alt_o = KeyEvent {
            code: KeyCode::Char('o'),
            modifiers: KeyModifiers::ALT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        handle_key(&mut state, alt_o).await;
        assert!(!state.tools_default_collapsed);
        assert!(state.tool_expanded_overrides.is_empty());
    }

    #[tokio::test]
    async fn test_ctrl_shift_o_toggles_global() {
        let mut state = make_test_state();
        assert!(state.tools_default_collapsed);

        let ctrl_shift_o = KeyEvent {
            code: KeyCode::Char('O'),
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        handle_key(&mut state, ctrl_shift_o).await;
        assert!(!state.tools_default_collapsed);
        assert!(state.tool_expanded_overrides.is_empty());
    }

    #[test]
    fn test_scroll_acceleration_levels() {
        let mut state = make_test_state();
        // First scroll: level 0 = 3 lines
        let amount = compute_scroll_amount(&mut state, true);
        assert_eq!(amount, 3);
        // Immediately again (same direction): level 1 = 6 lines
        let amount = compute_scroll_amount(&mut state, true);
        assert_eq!(amount, 6);
        // Again: level 2 = 12 lines
        let amount = compute_scroll_amount(&mut state, true);
        assert_eq!(amount, 12);
        // Caps at 12
        let amount = compute_scroll_amount(&mut state, true);
        assert_eq!(amount, 12);
    }

    #[test]
    fn test_scroll_direction_change_resets() {
        let mut state = make_test_state();
        compute_scroll_amount(&mut state, true);  // level 0
        compute_scroll_amount(&mut state, true);  // level 1
        // Direction change → reset to level 0
        let amount = compute_scroll_amount(&mut state, false);
        assert_eq!(amount, 3);
    }

    #[tokio::test]
    async fn test_ctrl_p_noop_when_single_line() {
        let mut state = make_test_state();
        // Ctrl+P with empty input is a no-op (todo panel removed)
        handle_key(&mut state, make_ctrl_key('p')).await;
        // No crash, no state change
        assert!(state.messages.is_empty());
    }

    #[test]
    fn test_scroll_accel_state_fields() {
        let state = make_test_state();
        assert!(state.scroll_last_dir.is_none());
        assert!(state.scroll_last_time.is_none());
        assert_eq!(state.scroll_accel, 0);
    }

    #[test]
    fn test_slash_command_help() {
        let mut state = make_test_state();
        execute_slash_command(&mut state, "/help");
        assert_eq!(state.messages.len(), 1);
        let text = match &state.messages[0].content[0] {
            ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        };
        assert!(text.contains("/help"), "Help output should list /help command");
        assert!(text.contains("/debug"), "Help output should list /debug command");
    }

    #[test]
    fn test_slash_command_clear() {
        let mut state = make_test_state();
        state.messages.push(ChatMessage::user("hello"));
        state.messages.push(ChatMessage::assistant("hi"));
        execute_slash_command(&mut state, "/clear");
        assert!(state.messages.is_empty());
    }

    #[test]
    fn test_slash_command_exit() {
        let mut state = make_test_state();
        assert!(state.running);
        execute_slash_command(&mut state, "/exit");
        assert!(!state.running);
    }

    #[test]
    fn test_slash_command_quit() {
        let mut state = make_test_state();
        execute_slash_command(&mut state, "/quit");
        assert!(!state.running);
    }

    #[test]
    fn test_slash_command_debug_toggle() {
        let mut state = make_test_state();
        assert_eq!(state.overlay, OverlayMode::None);
        execute_slash_command(&mut state, "/debug");
        assert_eq!(state.overlay, OverlayMode::AgentDebug);
        execute_slash_command(&mut state, "/debug");
        assert_eq!(state.overlay, OverlayMode::None);
    }

    #[test]
    fn test_slash_command_eval_toggle() {
        let mut state = make_test_state();
        execute_slash_command(&mut state, "/eval");
        assert_eq!(state.overlay, OverlayMode::Eval);
        execute_slash_command(&mut state, "/eval");
        assert_eq!(state.overlay, OverlayMode::None);
    }

    #[test]
    fn test_slash_command_sessions_toggle() {
        let mut state = make_test_state();
        execute_slash_command(&mut state, "/sessions");
        assert_eq!(state.overlay, OverlayMode::SessionPicker);
    }

    #[test]
    fn test_slash_command_todo_shows_message() {
        let mut state = make_test_state();
        execute_slash_command(&mut state, "/todo");
        // /todo now shows an informational message instead of toggling a panel
        assert_eq!(state.messages.len(), 1);
    }

    #[test]
    fn test_slash_command_unknown() {
        let mut state = make_test_state();
        execute_slash_command(&mut state, "/foobar");
        assert_eq!(state.messages.len(), 1);
        let text = match &state.messages[0].content[0] {
            ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        };
        assert!(text.contains("Unknown command"));
    }

    #[tokio::test]
    async fn test_slash_command_via_enter() {
        let mut state = make_test_state();
        // Type "/help" — autocomplete will show
        for c in "/help".chars() {
            handle_key(&mut state, make_key(KeyCode::Char(c))).await;
        }
        // First Enter: accepts autocomplete (inserts "/help")
        handle_key(&mut state, make_key(KeyCode::Enter)).await;
        assert_eq!(state.input_buffer, "/help");
        // Second Enter: executes the slash command
        handle_key(&mut state, make_key(KeyCode::Enter)).await;
        // Should NOT be streaming (local command, not sent to agent)
        assert!(!state.is_streaming);
        // Should have help message
        assert_eq!(state.messages.len(), 1);
    }

    #[tokio::test]
    async fn test_autocomplete_triggers_on_slash() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('/'))).await;
        // Autocomplete should be visible with all slash commands
        assert!(state.autocomplete.is_visible());
        assert!(!state.autocomplete.items().is_empty());
    }

    #[tokio::test]
    async fn test_autocomplete_dismiss_on_esc() {
        let mut state = make_test_state();
        handle_key(&mut state, make_key(KeyCode::Char('/'))).await;
        assert!(state.autocomplete.is_visible());
        handle_key(&mut state, make_key(KeyCode::Esc)).await;
        assert!(!state.autocomplete.is_visible());
    }

    #[tokio::test]
    async fn test_autocomplete_accept_on_tab() {
        let mut state = make_test_state();
        // Type "/hel" to trigger autocomplete
        for c in "/hel".chars() {
            handle_key(&mut state, make_key(KeyCode::Char(c))).await;
        }
        assert!(state.autocomplete.is_visible());
        // Tab to accept
        handle_key(&mut state, make_key(KeyCode::Tab)).await;
        assert!(!state.autocomplete.is_visible());
        assert_eq!(state.input_buffer, "/help");
    }
}
