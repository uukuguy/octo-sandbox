//! REPL main loop — interactive chat with an Octo agent via rustyline

pub mod file_ref;
pub mod helper;
pub mod history;
pub mod slash;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use octo_engine::agent::AgentMessage;
use octo_engine::AgentId;
use octo_types::{SandboxId, SessionId, UserId};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;

use crate::commands::run::RunOptions;
use crate::commands::state::AppState;

/// Run the interactive REPL loop.
///
/// 1. Resolves or creates a session
/// 2. Starts an AgentExecutor via `start_primary`
/// 3. Reads user input via rustyline (in a blocking task)
/// 4. Sends messages to the agent and renders streaming responses
/// 5. Saves history on exit
pub async fn run_repl(state: &AppState, opts: &RunOptions) -> Result<()> {
    let user_id = UserId::from_string("cli-user");
    let session_store = state.agent_runtime.session_store();

    // ── 1. Resolve session ──────────────────────────────────────────────
    let (session_id, history) = if let Some(ref sid_str) = opts.session_id {
        let sid = SessionId::from_string(sid_str);
        let history = session_store.get_messages(&sid).await.unwrap_or_default();
        (sid, history)
    } else if opts.resume {
        match session_store.most_recent_session().await {
            Some(session) => {
                let history = session_store
                    .get_messages(&session.session_id)
                    .await
                    .unwrap_or_default();
                (session.session_id, history)
            }
            None => {
                let session = session_store.create_session().await;
                (session.session_id, vec![])
            }
        }
    } else {
        let session = session_store.create_session().await;
        (session.session_id, vec![])
    };

    // Look up sandbox_id from the session
    let sandbox_id = session_store
        .get_session(&session_id)
        .await
        .map(|s| s.sandbox_id)
        .unwrap_or_else(|| SandboxId::from_string("default"));

    let agent_id = opts.agent_id.as_ref().map(|id| AgentId(id.clone()));

    // ── 2. Start the agent executor ─────────────────────────────────────
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

    // ── 3. Initialize rustyline ─────────────────────────────────────────
    let is_streaming = Arc::new(AtomicBool::new(false));
    let repl_helper = helper::ReplHelper {
        is_streaming: is_streaming.clone(),
    };

    let config = rustyline::Config::builder()
        .max_history_size(1000)
        .expect("valid history size")
        .auto_add_history(false)
        .build();

    let mut rl =
        rustyline::Editor::<helper::ReplHelper, DefaultHistory>::with_config(config)?;
    rl.set_helper(Some(repl_helper));

    // History file in XDG data directory
    let history_path = history::history_file_path();
    let _ = rl.load_history(&history_path);

    // ── 4. Welcome banner ───────────────────────────────────────────────
    eprintln!("Octo REPL — Session: {}", session_id);
    eprintln!("Type a message to chat. /help for commands. Ctrl+D to exit.\n");

    // ── 5. Main loop ────────────────────────────────────────────────────
    let prompt = "octo> ".to_string();

    loop {
        // rustyline::Editor is !Send, so we move it into spawn_blocking
        // and get it back after the readline call completes.
        let prompt_clone = prompt.clone();
        let readline_result = tokio::task::spawn_blocking(move || {
            let result = rl.readline(&prompt_clone);
            (rl, result)
        })
        .await?;

        rl = readline_result.0;
        let result = readline_result.1;

        match result {
            Ok(line) if line.trim().is_empty() => continue,

            Ok(line) if line.starts_with('/') => {
                let action = slash::handle_slash_command(
                    line.trim(),
                    state,
                    &session_id,
                )
                .await?;
                if action == slash::SlashAction::Exit {
                    break;
                }
            }

            Ok(line) => {
                let _ = rl.add_history_entry(&line);

                // Expand @file references
                let expanded = file_ref::expand_file_refs(&line, &state.working_dir);

                // Subscribe BEFORE sending so we don't miss events
                let mut rx = handle.subscribe();

                handle
                    .send(AgentMessage::UserMessage {
                        content: expanded,
                        channel_id: "cli".to_string(),
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;

                // Render the streaming response
                is_streaming.store(true, Ordering::Relaxed);
                crate::ui::streaming::render_streaming_response(
                    &mut rx,
                    &state.output_config,
                )
                .await?;
                is_streaming.store(false, Ordering::Relaxed);
            }

            Err(ReadlineError::Interrupted) => {
                // Ctrl+C — cancel the current agent operation
                handle.send(AgentMessage::Cancel).await.ok();
                eprintln!("^C");
            }

            Err(ReadlineError::Eof) => {
                // Ctrl+D — exit gracefully
                eprintln!(
                    "\nResume this session: octo run --session {}",
                    session_id
                );
                break;
            }

            Err(e) => return Err(e.into()),
        }
    }

    // ── 6. Save history ─────────────────────────────────────────────────
    let _ = rl.save_history(&history_path);

    Ok(())
}
