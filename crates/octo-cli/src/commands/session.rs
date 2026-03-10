//! Session commands implementation

use crate::commands::{AppState, SessionCommands};
use crate::ui::table::Table;
use anyhow::Result;
use octo_types::SessionId;

/// Handle session commands
pub async fn handle_session(action: SessionCommands, state: &AppState) -> Result<()> {
    match action {
        SessionCommands::List { limit } => list_sessions(limit, state).await?,
        SessionCommands::Create { name } => create_session(name, state).await?,
        SessionCommands::Show { session_id } => show_session(session_id, state).await?,
        SessionCommands::Delete { session_id } => delete_session(session_id, state).await?,
        SessionCommands::Export {
            session_id,
            format,
            output,
        } => {
            export_session(session_id, format, output, state).await?;
        }
    }
    Ok(())
}

/// List all sessions with table-formatted output
async fn list_sessions(limit: usize, state: &AppState) -> Result<()> {
    let session_store = state.agent_runtime.session_store();
    let sessions = session_store.list_sessions(limit, 0).await;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    let mut table = Table::new(vec!["Session ID", "Created", "Messages"]);
    for session in &sessions {
        let created = chrono::DateTime::from_timestamp(session.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| session.created_at.to_string());
        table.add_row(vec![
            session.session_id.clone(),
            created,
            session.message_count.to_string(),
        ]);
    }
    table.print();
    println!("\n{} session(s) total", sessions.len());
    Ok(())
}

/// Create a new session
async fn create_session(name: Option<String>, state: &AppState) -> Result<()> {
    let session_store = state.agent_runtime.session_store();
    let session = session_store.create_session().await;

    println!("Created session: {}", session.session_id);
    if let Some(n) = name {
        println!("  Name: {}", n);
    }
    Ok(())
}

/// Show session details
async fn show_session(session_id: String, state: &AppState) -> Result<()> {
    let session_store = state.agent_runtime.session_store();
    let sid = SessionId::from_string(&session_id);

    match session_store.get_session(&sid).await {
        Some(session) => {
            let msg_count = session_store
                .get_messages(&sid)
                .await
                .map(|msgs| msgs.len())
                .unwrap_or(0);
            let created = chrono::DateTime::from_timestamp(session.created_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| session.created_at.to_string());

            println!("Session: {}", session.session_id);
            println!("  User ID:  {}", session.user_id);
            println!("  Sandbox:  {}", session.sandbox_id);
            println!("  Created:  {}", created);
            println!("  Messages: {}", msg_count);
        }
        None => {
            eprintln!("Session not found: {}", session_id);
        }
    }
    Ok(())
}

/// Delete a session
async fn delete_session(session_id: String, state: &AppState) -> Result<()> {
    let session_store = state.agent_runtime.session_store();
    let sid = SessionId::from_string(&session_id);

    if session_store.delete_session(&sid).await {
        println!("Deleted session: {}", session_id);
    } else {
        eprintln!("Session not found: {}", session_id);
    }
    Ok(())
}

/// Export a session
async fn export_session(
    session_id: String,
    format: String,
    output: Option<String>,
    _state: &AppState,
) -> Result<()> {
    println!("Exporting session: {} (format: {})", session_id, format);
    if let Some(out) = &output {
        println!("  Output: {}", out);
    }
    println!("Session export — coming in Phase 5 (A4)");
    Ok(())
}
