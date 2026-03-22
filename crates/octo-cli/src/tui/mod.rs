//! Conversation-centric TUI mode based on Ratatui 0.29
//!
//! Vertical stack: conversation area -> progress panel -> input area -> status bar.
//! Overlays (approval dialog, debug panels) render on top.

pub mod app_state;
pub mod autocomplete;
pub mod event;
pub mod event_handler;
pub mod formatters;
pub mod key_handler;
pub mod managers;
pub mod overlays;
pub mod render;
pub mod theme;
pub mod widgets;

use std::io;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use ratatui::Terminal;

use crate::commands::AppState;

/// Run the conversation-centric TUI with full agent integration.
///
/// This creates a session, starts an AgentExecutor, subscribes to AgentEvent
/// broadcasts, and runs the EventHandler-based event loop.
pub async fn run_tui_conversation(state: &AppState) -> Result<()> {
    use octo_types::{SandboxId, UserId};

    // Setup terminal FIRST so user sees the TUI immediately
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Get model name from provider config
    let model_name = std::env::var("LLM_MODEL")
        .or_else(|_| std::env::var("OPENAI_MODEL_NAME"))
        .unwrap_or_else(|_| "agent".to_string());

    let user_id = UserId::from_string("cli-user");
    let session_store = state.agent_runtime.session_store();

    // Create a new session
    let session = session_store.create_session().await;
    let session_id = session.session_id.clone();
    let sandbox_id = session_store
        .get_session(&session_id)
        .await
        .map(|s| s.sandbox_id)
        .unwrap_or_else(|| SandboxId::from_string("default"));

    // Start the agent executor
    let handle = state
        .agent_runtime
        .start_primary(session_id.clone(), user_id, sandbox_id, vec![], None)
        .await;

    // Initialize state
    let mut tui_state = app_state::TuiState::new(session_id, handle.clone(), model_name);

    // Inject approval gate for Y/N/A key responses
    if let Some(gate) = state.agent_runtime.approval_gate() {
        tui_state.approval_gate = Some(gate.clone());
    }

    // Inject user-invocable skills as slash commands for autocomplete
    if let Some(registry) = state.agent_runtime.skill_registry() {
        let skill_commands: Vec<autocomplete::SlashCommand> = registry
            .list_all()
            .into_iter()
            .filter(|s| s.user_invocable)
            .map(|s| autocomplete::SlashCommand {
                name: s.name.clone(),
                description: format!("[skill] {}", s.description),
            })
            .collect();
        if !skill_commands.is_empty() {
            tui_state.autocomplete.add_commands(&skill_commands);
        }
    }

    // Get terminal size
    if let Ok(size) = crossterm::terminal::size() {
        tui_state.terminal_width = size.0;
        tui_state.terminal_height = size.1;
    }

    // Create event handler with agent broadcast subscription
    let agent_rx = handle.subscribe();
    let mut event_handler =
        event_handler::EventHandler::new(agent_rx, std::time::Duration::from_millis(100));

    // Main event loop
    let result = run_conversation_loop(&mut terminal, &mut tui_state, &mut event_handler).await;

    // Restore terminal (always, even on error)
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    result
}

/// The async event loop for the conversation-centric TUI.
async fn run_conversation_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut app_state::TuiState,
    event_handler: &mut event_handler::EventHandler,
) -> Result<()> {
    loop {
        // Rebuild line cache if content changed
        if state.lines_generation != state.message_generation {
            state.rebuild_cached_lines();
        }

        // Conditional redraw
        if state.dirty {
            terminal.draw(|frame| render::render(state, frame))?;
            state.dirty = false;
        }

        // Wait for next event
        if let Some(event) = event_handler.next().await {
            state.dirty = true; // assume dirty; render will check
            match event {
                event::AppEvent::Key(key) => {
                    key_handler::handle_key(state, key).await;
                }
                event::AppEvent::MouseScroll { up, .. } => {
                    // Mouse scroll always controls conversation area
                    if up {
                        state.scroll_offset = state.scroll_offset.saturating_add(3);
                        state.user_scrolled = true;
                    } else {
                        state.scroll_offset = state.scroll_offset.saturating_sub(3);
                        if state.scroll_offset == 0 {
                            state.user_scrolled = false;
                        }
                    }
                }
                event::AppEvent::Resize(w, h) => {
                    state.terminal_width = w;
                    state.terminal_height = h;
                    state.invalidate_cache(); // width change affects wrapping
                }
                event::AppEvent::Tick => {
                    state.spinner_service.stop(); // tick — just mark dirty for animation
                    // Drive welcome panel breathing animation when conversation is empty
                    if state.cached_lines.is_empty() && !state.welcome_state.fade_complete {
                        let w = state.terminal_width;
                        let h = state.terminal_height;
                        state.welcome_state.tick(w, h);
                    }
                    // Refresh git info every ~5 seconds
                    state.git_refresh_counter += 1;
                    if state.git_refresh_counter >= 83 {
                        state.git_refresh_counter = 0;
                        state.refresh_git_info();
                    }
                }
                event::AppEvent::Agent(agent_event) => {
                    handle_agent_event(state, agent_event);
                }
                event::AppEvent::Quit => {
                    state.running = false;
                }
                _ => {} // UserSubmit handled via key_handler Enter
            }
        }

        // Batch drain remaining events
        while let Some(event) = event_handler.try_next() {
            match event {
                event::AppEvent::Key(key) => {
                    key_handler::handle_key(state, key).await;
                }
                event::AppEvent::MouseScroll { up, .. } => {
                    if up {
                        state.scroll_offset = state.scroll_offset.saturating_add(3);
                        state.user_scrolled = true;
                    } else {
                        state.scroll_offset = state.scroll_offset.saturating_sub(3);
                        if state.scroll_offset == 0 {
                            state.user_scrolled = false;
                        }
                    }
                }
                event::AppEvent::Resize(w, h) => {
                    state.terminal_width = w;
                    state.terminal_height = h;
                }
                event::AppEvent::Agent(agent_event) => {
                    handle_agent_event(state, agent_event);
                }
                event::AppEvent::Quit => {
                    state.running = false;
                }
                _ => {}
            }
            state.dirty = true;
            if !state.running {
                break;
            }
        }

        if !state.running {
            break;
        }
    }

    Ok(())
}

/// Process an AgentEvent, updating TuiState accordingly.
fn handle_agent_event(state: &mut app_state::TuiState, event: octo_engine::agent::AgentEvent) {
    use octo_engine::agent::AgentEvent;
    use octo_types::message::{ChatMessage, ContentBlock, MessageRole};

    match event {
        AgentEvent::TextDelta { text } => {
            // Fade out welcome panel on first content
            if !state.welcome_state.fade_complete && !state.welcome_state.is_fading {
                state.welcome_state.start_fade();
            }
            state.streaming_text.push_str(&text);
            state.invalidate_cache();
            state.auto_scroll();
        }
        AgentEvent::TextComplete { text: _ } => {
            // Finalize streaming text into a message
            if !state.streaming_text.is_empty() {
                let final_text = std::mem::take(&mut state.streaming_text);
                state.messages.push(ChatMessage::assistant(&final_text));
            }
            state.is_streaming = false;
            state.invalidate_cache();
        }
        AgentEvent::ThinkingDelta { text } => {
            state.thinking_text.push_str(&text);
            state.is_thinking = true;
            state.invalidate_cache();
        }
        AgentEvent::ThinkingComplete { text: _ } => {
            state.thinking_text.clear();
            state.is_thinking = false;
            state.invalidate_cache();
        }
        AgentEvent::ToolStart {
            tool_id,
            tool_name,
            input,
        } => {
            // Flush any streaming text as an assistant message before the tool call
            if !state.streaming_text.is_empty() {
                let partial = std::mem::take(&mut state.streaming_text);
                state.messages.push(ChatMessage::assistant(&partial));
            }
            state
                .active_tools
                .push(widgets::conversation::ActiveTool {
                    tool_id,
                    name: tool_name,
                    args: input,
                    started_at: std::time::Instant::now(),
                });
            state.dirty = true;
        }
        AgentEvent::ToolResult {
            tool_id,
            output,
            success,
        } => {
            // Find and remove the matching active tool
            let tool_info = if let Some(idx) = state.active_tools.iter().position(|t| t.tool_id == tool_id) {
                Some(state.active_tools.remove(idx))
            } else {
                state.active_tools.pop().map(|t| t)
            };

            // Build inline messages: Assistant(ToolUse) + User(ToolResult)
            if let Some(tool) = tool_info {
                // Assistant message with ToolUse content block
                let tool_use_block = ContentBlock::ToolUse {
                    id: tool_id.clone(),
                    name: tool.name.clone(),
                    input: tool.args.clone(),
                };
                state.messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: vec![tool_use_block],
                });

                // User message with ToolResult content block (collapsed by default)
                let result_text = output;
                let tool_result_block = ContentBlock::ToolResult {
                    tool_use_id: tool_id,
                    content: result_text,
                    is_error: !success,
                };
                state.messages.push(ChatMessage {
                    role: MessageRole::User,
                    content: vec![tool_result_block],
                });
            }
            state.invalidate_cache();
        }
        AgentEvent::ToolProgress {
            tool_id: _,
            tool_name: _,
            progress: _,
        } => {
            // elapsed_secs is now computed dynamically from started_at
            state.dirty = true;
        }
        AgentEvent::ApprovalRequired {
            tool_name,
            tool_id,
            risk_level,
        } => {
            state.pending_approval = Some(app_state::PendingApproval {
                tool_id,
                tool_name,
                risk_level,
            });
            state.dirty = true;
        }
        AgentEvent::Completed(result) => {
            state.total_input_tokens += result.input_tokens;
            state.total_output_tokens += result.output_tokens;
            state.task_input_tokens += result.input_tokens;
            state.task_output_tokens += result.output_tokens;
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.active_tools.clear();

            // Replace messages with final_messages from agent loop if available.
            // These include full tool call/result content blocks (collapsed in UI).
            // When cancelled, keep the messages we already preserved (partial response).
            if state.cancelled {
                state.cancelled = false;
                // Don't replace — ESC handler already preserved partial messages
            } else if !result.final_messages.is_empty() {
                state.messages = result.final_messages;
                state.streaming_text.clear();
            } else if !state.streaming_text.is_empty() {
                let final_text = std::mem::take(&mut state.streaming_text);
                state.messages.push(ChatMessage::assistant(&final_text));
            }
            state.invalidate_cache();
        }
        AgentEvent::Done => {
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.invalidate_cache();
        }
        AgentEvent::Error { message: _ } => {
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.active_tools.clear();
            if !state.streaming_text.is_empty() {
                state.streaming_text.clear();
            }
            state.invalidate_cache();
        }
        AgentEvent::SecurityBlocked { reason: _ } => {
            state.invalidate_cache();
        }
        AgentEvent::EmergencyStopped(_reason) => {
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.active_tools.clear();
            state.streaming_text.clear();
            state.invalidate_cache();
        }
        AgentEvent::PlanUpdate { steps } => {
            state.plan_steps = steps;
            // Plan steps are rendered inline in conversation area
            state.invalidate_cache();
        }
        AgentEvent::TokenBudgetUpdate { budget } => {
            state.context_usage_pct = budget.usage_percent as f64;
            state.dirty = true;
        }
        AgentEvent::ContextDegraded { usage_pct, .. } => {
            state.context_usage_pct = usage_pct as f64;
            state.dirty = true;
        }
        AgentEvent::IterationEnd { input_tokens, output_tokens, .. } => {
            // Update task-level tokens from cumulative iteration data
            state.task_input_tokens = input_tokens;
            state.task_output_tokens = output_tokens;
            state.dirty = true;
        }
        _ => {
            // IterationStart, MemoryFlushed, ToolExecution, Typing
            state.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- AppEvent tests --

    #[test]
    fn app_event_debug_format() {
        let event = event::AppEvent::Quit;
        let debug = format!("{:?}", event);
        assert!(debug.contains("Quit"));
    }

    #[test]
    fn app_event_clone() {
        let event = event::AppEvent::UserSubmit("hello".to_string());
        let cloned = event.clone();
        if let event::AppEvent::UserSubmit(msg) = cloned {
            assert_eq!(msg, "hello");
        } else {
            panic!("Clone produced wrong variant");
        }
    }

    #[test]
    fn app_event_tick() {
        let event = event::AppEvent::Tick;
        let debug = format!("{:?}", event);
        assert!(debug.contains("Tick"));
    }

    // -- TuiTheme tests --

    #[test]
    fn theme_default_is_cyan() {
        let theme = theme::TuiTheme::default();
        assert_eq!(theme.accent, Color::Rgb(6, 182, 212));
    }

    #[test]
    fn theme_from_each_name() {
        let names = [
            crate::ui::theme::ThemeName::Cyan,
            crate::ui::theme::ThemeName::Sgcc,
            crate::ui::theme::ThemeName::Blue,
            crate::ui::theme::ThemeName::Indigo,
            crate::ui::theme::ThemeName::Violet,
            crate::ui::theme::ThemeName::Emerald,
            crate::ui::theme::ThemeName::Amber,
            crate::ui::theme::ThemeName::Coral,
            crate::ui::theme::ThemeName::Rose,
            crate::ui::theme::ThemeName::Teal,
            crate::ui::theme::ThemeName::Sunset,
            crate::ui::theme::ThemeName::Slate,
        ];
        for name in names {
            let _theme = theme::TuiTheme::from_cli_theme(name);
        }
    }
}
