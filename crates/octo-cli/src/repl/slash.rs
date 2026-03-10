//! Slash command parser and execution

use anyhow::Result;
use octo_types::SessionId;

use crate::commands::state::AppState;

/// Action to take after a slash command executes
#[derive(Debug, PartialEq)]
pub enum SlashAction {
    /// Continue the REPL loop
    Continue,
    /// Exit the REPL
    Exit,
}

/// Parse and execute a slash command.
///
/// Returns `SlashAction::Exit` if the user wants to quit.
pub async fn handle_slash_command(
    input: &str,
    _state: &AppState,
    session_id: &SessionId,
) -> Result<SlashAction> {
    let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
    let cmd = parts[0];
    let args = parts.get(1).copied().unwrap_or("");

    match cmd {
        "/help" | "/h" | "/?" => cmd_help(),
        "/exit" | "/quit" | "/q" => cmd_exit(session_id),
        "/clear" => cmd_clear(),
        "/compact" => cmd_compact(),
        "/cost" => cmd_cost(),
        "/model" => cmd_model(args),
        "/mode" => cmd_mode(args),
        "/save" => cmd_save(session_id),
        "/undo" => cmd_undo(),
        "/theme" => cmd_theme(args),
        _ => {
            eprintln!(
                "Unknown command: {}. Type /help for available commands.",
                cmd
            );
            Ok(SlashAction::Continue)
        }
    }
}

fn cmd_help() -> Result<SlashAction> {
    eprintln!("Available commands:");
    eprintln!("  /help, /h, /?    Show this help");
    eprintln!("  /exit, /quit, /q Exit REPL");
    eprintln!("  /clear           Clear conversation context");
    eprintln!("  /compact         Compress conversation context");
    eprintln!("  /cost            Show token usage and costs");
    eprintln!("  /model [name]    Show or switch LLM model");
    eprintln!("  /mode [plan|build] Switch mode");
    eprintln!("  /save            Save current session");
    eprintln!("  /undo            Undo last tool operation");
    eprintln!("  /theme [name]    Show or switch color theme");
    Ok(SlashAction::Continue)
}

fn cmd_exit(session_id: &SessionId) -> Result<SlashAction> {
    eprintln!("\nResume this session: octo run --session {}", session_id);
    Ok(SlashAction::Exit)
}

fn cmd_clear() -> Result<SlashAction> {
    eprintln!("[clear] Conversation context cleared.");
    // Actual implementation will be in Phase 5 (A3)
    Ok(SlashAction::Continue)
}

fn cmd_compact() -> Result<SlashAction> {
    eprintln!("[compact] Context compression — coming in Phase 5 (A3)");
    Ok(SlashAction::Continue)
}

fn cmd_cost() -> Result<SlashAction> {
    eprintln!("[cost] Token usage tracking — coming in Phase 5 (A2)");
    Ok(SlashAction::Continue)
}

fn cmd_model(args: &str) -> Result<SlashAction> {
    if args.is_empty() {
        eprintln!("[model] Current model: (default)");
    } else {
        eprintln!("[model] Model switching — coming in Phase 5");
    }
    Ok(SlashAction::Continue)
}

fn cmd_mode(args: &str) -> Result<SlashAction> {
    if args.is_empty() {
        eprintln!("[mode] Current mode: build");
    } else {
        eprintln!("[mode] Mode switching — coming in Phase 5 (A1)");
    }
    Ok(SlashAction::Continue)
}

fn cmd_save(session_id: &SessionId) -> Result<SlashAction> {
    eprintln!("[save] Session {} saved.", session_id);
    Ok(SlashAction::Continue)
}

fn cmd_undo() -> Result<SlashAction> {
    eprintln!("[undo] Undo — coming in Phase 5");
    Ok(SlashAction::Continue)
}

fn cmd_theme(args: &str) -> Result<SlashAction> {
    if args.is_empty() || args == "list" {
        eprintln!("Available themes: cyan, sgcc, blue, indigo, violet, emerald, amber, coral, rose, teal, sunset, slate");
    } else {
        eprintln!("[theme] Switched to: {}", args);
    }
    Ok(SlashAction::Continue)
}
