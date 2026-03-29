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

    // Install a panic hook that restores the terminal before printing the panic.
    // Without this, a panic leaves the terminal in raw/alternate-screen mode,
    // producing garbled output (as seen when Ctrl+C races with rendering).
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

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

    // Initialize state with project working directory
    let mut tui_state = app_state::TuiState::new(session_id, handle.clone(), model_name);
    tui_state.set_working_dir(state.working_dir.clone());

    // Inject approval gate for Y/N/A key responses
    if let Some(gate) = state.agent_runtime.approval_gate() {
        tui_state.approval_gate = Some(gate.clone());
    }

    // Sync builtin commands to ~/.octo/commands/ (never overwrites existing)
    if let Err(e) = octo_engine::commands::sync_builtin_commands(&state.octo_root.global_commands_dir()) {
        tracing::warn!(error = %e, "Failed to sync builtin commands");
    }

    // Load custom commands from ~/.octo/commands/ and .octo/commands/
    {
        let custom_cmds = octo_engine::commands::load_commands(&state.octo_root.commands_dirs());
        if !custom_cmds.is_empty() {
            let slash_cmds: Vec<autocomplete::SlashCommand> = custom_cmds
                .iter()
                .map(|c| autocomplete::SlashCommand {
                    name: c.name.clone(),
                    description: format!("[cmd] {}", c.description),
                })
                .collect();
            tui_state.autocomplete.add_commands(&slash_cmds);
        }
        tui_state.custom_commands = custom_cmds;
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

    // Restore terminal (always, even on error).
    // Use let _ to ignore errors — each step must run regardless of prior failures.
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
    let _ = terminal.show_cursor();

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
            state.task_tool_calls += 1;
            // Clear sub-agent state when execute_skill tool completes
            if state.subagent_source_id.is_some() {
                state.subagent_source_id = None;
                state.subagent_streaming_text.clear();
                state.subagent_thinking_text.clear();
                state.subagent_active_tools.clear();
                state.subagent_completed = None;
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
            // Re-enable raw mode in case a child process inadvertently disabled it.
            // This is a defensive measure — without raw mode, terminal escape sequences
            // from mouse capture leak into the input area as garbled characters.
            let _ = crossterm::terminal::enable_raw_mode();

            // Capture elapsed before clearing task_start_time
            let task_elapsed = state.task_start_time.map(|t| t.elapsed());

            state.total_input_tokens += result.input_tokens;
            state.total_output_tokens += result.output_tokens;
            state.task_input_tokens += result.input_tokens;
            state.task_output_tokens += result.output_tokens;
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.active_tools.clear();
            state.task_start_time = None;
            // Clear sub-agent state on parent completion
            state.subagent_source_id = None;
            state.subagent_streaming_text.clear();
            state.subagent_thinking_text.clear();
            state.subagent_active_tools.clear();
            state.subagent_completed = None;

            // Replace messages with final_messages from agent loop if available.
            // These include full tool call/result content blocks (collapsed in UI).
            // When cancelled, keep the messages we already preserved (partial response).
            if state.cancelled {
                state.cancelled = false;
                // Don't replace — ESC handler already preserved partial messages
            } else if !result.final_messages.is_empty() {
                // Preserve completion summary messages from previous rounds.
                // These are TUI-only messages (not in agent history) starting with '─'.
                let summaries: Vec<ChatMessage> = state
                    .messages
                    .iter()
                    .filter(|m| {
                        m.role == MessageRole::Assistant
                            && m.content.iter().any(|c| matches!(c, ContentBlock::Text { text } if text.starts_with('\u{2500}')))
                    })
                    .cloned()
                    .collect();

                state.messages = result.final_messages;
                state.streaming_text.clear();

                // Re-insert previous summaries before the last user message of this round.
                // Find the position of the last user message to insert summaries before it.
                if !summaries.is_empty() {
                    if let Some(last_user_pos) = state.messages.iter().rposition(|m| m.role == MessageRole::User) {
                        for (i, s) in summaries.into_iter().enumerate() {
                            state.messages.insert(last_user_pos + i, s);
                        }
                    }
                }
            } else if !state.streaming_text.is_empty() {
                let final_text = std::mem::take(&mut state.streaming_text);
                state.messages.push(ChatMessage::assistant(&final_text));
            }

            // Append completion summary as a system-like message
            if result.tool_calls > 0 || result.rounds > 1 {
                use widgets::status_bar::StatusBarWidget;
                let elapsed_str = task_elapsed
                    .map(|d| StatusBarWidget::format_elapsed(d))
                    .unwrap_or_default();
                let ti = StatusBarWidget::format_tokens(result.input_tokens);
                let to = StatusBarWidget::format_tokens(result.output_tokens);
                let summary = format!(
                    "\u{2500} {elapsed_str} | {rounds}r {tools}t | \u{25B8}{ti} \u{25BE}{to}",
                    rounds = result.rounds,
                    tools = result.tool_calls,
                );
                state.messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: vec![ContentBlock::Text { text: summary }],
                });
            }

            state.invalidate_cache();
        }
        AgentEvent::Done => {
            let _ = crossterm::terminal::enable_raw_mode();
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.task_start_time = None;
            state.invalidate_cache();
        }
        AgentEvent::Error { message } => {
            let _ = crossterm::terminal::enable_raw_mode();
            // Flush any partial streaming text first
            if !state.streaming_text.is_empty() {
                let partial = std::mem::take(&mut state.streaming_text);
                state.messages.push(ChatMessage::assistant(&partial));
            }
            // Show error in conversation area as a visible message
            state.messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text {
                    text: format!("[Error] {}", message),
                }],
            });
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.active_tools.clear();
            state.task_start_time = None;
            state.invalidate_cache();
            state.auto_scroll();
        }
        AgentEvent::SecurityBlocked { reason } => {
            state.messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text {
                    text: format!("[Security] {}", reason),
                }],
            });
            state.invalidate_cache();
            state.auto_scroll();
        }
        AgentEvent::EmergencyStopped(reason) => {
            if !state.streaming_text.is_empty() {
                let partial = std::mem::take(&mut state.streaming_text);
                state.messages.push(ChatMessage::assistant(&partial));
            }
            state.messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text {
                    text: format!("[Emergency Stop] {}", reason.as_deref().unwrap_or("unknown")),
                }],
            });
            state.is_streaming = false;
            state.is_thinking = false;
            state.thinking_text.clear();
            state.active_tools.clear();
            state.streaming_text.clear();
            state.task_start_time = None;
            state.invalidate_cache();
            state.auto_scroll();
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
        AgentEvent::IterationStart { .. } => {
            state.task_rounds += 1;
            state.dirty = true;
        }
        AgentEvent::RetryingMalformedToolCall { attempt, max_attempts, reason } => {
            // Show retry notification inline so the user knows what's happening
            state.messages.push(ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text {
                    text: format!(
                        "[Retry {}/{}] LLM 输出了格式错误的工具调用，正在重试。原因：{}",
                        attempt, max_attempts, reason
                    ),
                }],
            });
            state.dirty = true;
            state.invalidate_cache();
            state.auto_scroll();
        }
        AgentEvent::SubAgentEvent { source_id, inner } => {
            handle_subagent_event(state, source_id, *inner);
        }
        _ => {
            // MemoryFlushed, ToolExecution, Typing
            state.dirty = true;
        }
    }
}

/// Process a SubAgentEvent, updating isolated sub-agent state in TuiState.
fn handle_subagent_event(
    state: &mut app_state::TuiState,
    source_id: String,
    event: octo_engine::agent::AgentEvent,
) {
    use octo_engine::agent::AgentEvent;

    // Track which sub-agent is active
    if state.subagent_source_id.is_none() {
        state.subagent_source_id = Some(source_id.clone());
    }

    match event {
        AgentEvent::TextDelta { text } => {
            state.subagent_streaming_text.push_str(&text);
            state.invalidate_cache();
            state.auto_scroll();
        }
        AgentEvent::TextComplete { .. } => {
            // Finalize sub-agent streaming text (keep accumulated text for display)
            state.invalidate_cache();
        }
        AgentEvent::ThinkingDelta { text } => {
            state.subagent_thinking_text.push_str(&text);
            state.invalidate_cache();
        }
        AgentEvent::ThinkingComplete { .. } => {
            state.subagent_thinking_text.clear();
            state.invalidate_cache();
        }
        AgentEvent::ToolStart { tool_id, tool_name, input } => {
            state.subagent_active_tools.push(
                widgets::conversation::ActiveTool {
                    tool_id,
                    name: tool_name,
                    args: input,
                    started_at: std::time::Instant::now(),
                },
            );
            state.dirty = true;
        }
        AgentEvent::ToolResult { tool_id, .. } => {
            if let Some(idx) = state.subagent_active_tools.iter().position(|t| t.tool_id == tool_id) {
                state.subagent_active_tools.remove(idx);
            } else {
                state.subagent_active_tools.pop();
            }
            state.invalidate_cache();
        }
        AgentEvent::Completed(result) => {
            state.subagent_completed = Some((result.rounds, result.tool_calls));
            state.subagent_active_tools.clear();
            state.subagent_thinking_text.clear();
            state.invalidate_cache();
        }
        AgentEvent::Error { message } => {
            // Show error inline in sub-agent text
            state.subagent_streaming_text.push_str(&format!("\n[Error] {}", message));
            state.subagent_active_tools.clear();
            state.subagent_thinking_text.clear();
            state.invalidate_cache();
        }
        _ => {
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
