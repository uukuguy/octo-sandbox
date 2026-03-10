//! Headless ask command — send a single message and print the response

use anyhow::Result;
use octo_engine::agent::{AgentEvent, AgentMessage};
use octo_engine::AgentId;
use octo_types::{SessionId, UserId};

use crate::commands::AppState;

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
    let quiet = state.output_config.quiet;
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
                        format!("{}...", &output[..100])
                    } else {
                        output
                    };
                    eprintln!("  [{}] {}", icon, preview);
                }
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
