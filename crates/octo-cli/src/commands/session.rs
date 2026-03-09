//! Session commands implementation

use crate::commands::{AppState, SessionCommands};
use anyhow::Result;

/// Handle session commands
pub async fn handle_session(action: SessionCommands, state: &AppState) -> Result<()> {
    match action {
        SessionCommands::List => list_sessions(state).await?,
        SessionCommands::Create { name } => create_session(name, state).await?,
        SessionCommands::Show { session_id } => show_session(session_id, state).await?,
        SessionCommands::Delete { .. } => {
            println!("Session deletion not supported yet.");
        }
    }
    Ok(())
}

/// List all sessions
async fn list_sessions(state: &AppState) -> Result<()> {
    let session_store = state.agent_runtime.session_store();
    let sessions = session_store.list_sessions(50, 0).await;

    if sessions.is_empty() {
        println!("No sessions found.");
    } else {
        println!("Sessions:");
        for session in sessions {
            println!(
                "  - {} (created: {})",
                session.session_id, session.created_at
            );
        }
    }
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
    use octo_types::SessionId;

    let session_store = state.agent_runtime.session_store();
    let sid = SessionId::from_string(&session_id);

    match session_store.get_session(&sid).await {
        Some(session) => {
            println!("Session: {}", session.session_id);
            println!("  User ID: {}", session.user_id);
            println!("  Created: {}", session.created_at);
        }
        None => {
            println!("Session not found: {}", session_id);
        }
    }
    Ok(())
}
