//! Headless ask command — send a single message and print the response

use std::collections::HashMap;

use anyhow::Result;
use octo_engine::agent::{AgentEvent, AgentMessage};
use octo_engine::AgentId;
use octo_types::{SessionId, UserId};

use crate::commands::AppState;
use crate::output::OutputFormat;

/// Aggregated JSON output for `octo ask --output json`
#[derive(serde::Serialize)]
struct AskJsonOutput {
    text: String,
    tool_calls: Vec<AskToolCall>,
    rounds: u32,
    input_tokens: u64,
    output_tokens: u64,
    duration_ms: u64,
    stop_reason: String,
}

/// A single tool invocation record within the JSON output
#[derive(serde::Serialize)]
struct AskToolCall {
    name: String,
    args: serde_json::Value,
    result: String,
    success: bool,
}

/// Options for the ask command
pub struct AskOptions {
    pub message: String,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
}

/// Execute the ask command: create/resume a session, send one message,
/// stream the response to stdout, then exit.
pub async fn execute_ask(opts: AskOptions, state: &AppState) -> Result<()> {
    let user_id = UserId::from_string("cli-user");
    let session_store = state.agent_runtime.session_store();

    // Resolve session: use provided ID, or create a new one
    let (session_id, history) = if let Some(ref sid_str) = opts.session_id {
        let sid = SessionId::from_string(sid_str);
        let history = session_store.get_messages(&sid).await.unwrap_or_default();
        (sid, history)
    } else {
        let session = session_store.create_session().await;
        (session.session_id.clone(), vec![])
    };

    // Look up sandbox_id from the session (needed by start_primary)
    let sandbox_id = session_store
        .get_session(&session_id)
        .await
        .map(|s| s.sandbox_id)
        .unwrap_or_else(|| octo_types::SandboxId::from_string("default"));

    let agent_id = opts.agent_id.as_ref().map(|id| AgentId(id.clone()));

    // Start the agent executor (or reuse if already running)
    let handle = state
        .agent_runtime
        .start_primary(
            session_id.clone(),
            user_id,
            sandbox_id,
            history,
            agent_id.as_ref(),
        )
        .await;

    // Subscribe to events BEFORE sending the message so we don't miss any
    let mut rx = handle.subscribe();

    // Send the user message
    handle
        .send(AgentMessage::UserMessage {
            content: opts.message.clone(),
            channel_id: "cli".to_string(),
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;

    // Stream events until Done/Completed
    let started = std::time::Instant::now();

    if matches!(state.output_config.format, OutputFormat::Json) {
        collect_json_output(&mut rx, started).await
    } else {
        stream_text_output(&mut rx, state.output_config.quiet).await
    }
}

/// Text output mode — stream events to stdout/stderr as they arrive (existing behaviour).
async fn stream_text_output(
    rx: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
    quiet: bool,
) -> Result<()> {
    loop {
        match rx.recv().await {
            Ok(AgentEvent::TextDelta { text }) => {
                print!("{}", text);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
            Ok(AgentEvent::ThinkingDelta { text }) => {
                if !quiet {
                    eprint!("{}", text);
                }
            }
            Ok(AgentEvent::ToolStart { tool_name, .. }) => {
                if !quiet {
                    eprintln!("  [tool] {}...", tool_name);
                }
            }
            Ok(AgentEvent::ToolResult {
                output, success, ..
            }) => {
                if !quiet {
                    let icon = if success { "+" } else { "!" };
                    let preview = if output.len() > 100 {
                        // Find a valid UTF-8 char boundary at or before 100
                        let end = output
                            .char_indices()
                            .take_while(|&(i, _)| i <= 100)
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        format!("{}...", &output[..end])
                    } else {
                        output
                    };
                    eprintln!("  [{}] {}", icon, preview);
                }
            }
            Ok(AgentEvent::RetryingMalformedToolCall { attempt, max_attempts, reason }) => {
                eprintln!(
                    "[Retry {}/{}] Malformed tool call detected, retrying: {}",
                    attempt, max_attempts, reason
                );
            }
            Ok(AgentEvent::Error { message }) => {
                eprintln!("Error: {}", message);
                break;
            }
            Ok(AgentEvent::Done) => {
                println!();
                break;
            }
            Ok(AgentEvent::Completed(_)) => {
                println!();
                break;
            }
            Ok(_) => {} // ignore other event variants
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("Skipped {} events", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
    Ok(())
}

/// JSON output mode — collect all events and emit a single JSON blob at the end.
async fn collect_json_output(
    rx: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
    started: std::time::Instant,
) -> Result<()> {
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<AskToolCall> = Vec::new();
    // Map tool_id → (name, args) for pending tool starts
    let mut pending_tools: HashMap<String, (String, serde_json::Value)> = HashMap::new();
    let mut rounds: u32 = 0;
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut stop_reason = String::from("unknown");
    let mut error_message: Option<String> = None;

    loop {
        match rx.recv().await {
            Ok(AgentEvent::TextDelta { text }) => {
                text_parts.push(text);
            }
            Ok(AgentEvent::ToolStart {
                tool_id,
                tool_name,
                input,
            }) => {
                pending_tools.insert(tool_id, (tool_name, input));
            }
            Ok(AgentEvent::ToolResult {
                tool_id,
                output,
                success,
            }) => {
                let (name, args) = pending_tools
                    .remove(&tool_id)
                    .unwrap_or_else(|| (String::from("unknown"), serde_json::Value::Null));
                tool_calls.push(AskToolCall {
                    name,
                    args,
                    result: output,
                    success,
                });
            }
            Ok(AgentEvent::Completed(result)) => {
                rounds = result.rounds;
                input_tokens = result.input_tokens;
                output_tokens = result.output_tokens;
                stop_reason = format!("{:?}", result.stop_reason);
                break;
            }
            Ok(AgentEvent::RetryingMalformedToolCall { attempt, max_attempts, reason }) => {
                eprintln!(
                    "[Retry {}/{}] Malformed tool call detected, retrying: {}",
                    attempt, max_attempts, reason
                );
            }
            Ok(AgentEvent::Error { message }) => {
                error_message = Some(message);
                stop_reason = String::from("Error");
                break;
            }
            Ok(AgentEvent::Done) => {
                stop_reason = String::from("Done");
                break;
            }
            Ok(_) => {} // ignore other event variants
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("Skipped {} events", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }

    let duration_ms = started.elapsed().as_millis() as u64;

    let mut text = text_parts.join("");
    if let Some(err) = error_message {
        if text.is_empty() {
            text = format!("Error: {}", err);
        }
    }

    let output = AskJsonOutput {
        text,
        tool_calls,
        rounds,
        input_tokens,
        output_tokens,
        duration_ms,
        stop_reason,
    };

    crate::output::json::print_json(&output);
    Ok(())
}
