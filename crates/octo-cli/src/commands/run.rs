//! Run command — start interactive REPL session

use anyhow::Result;

use crate::commands::AppState;

/// Options for the run command
pub struct RunOptions {
    pub resume: bool,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub theme: String,
}

/// Execute the run command: start an interactive REPL session
pub async fn execute_run(opts: RunOptions, state: &AppState) -> Result<()> {
    crate::repl::run_repl(state, &opts).await
}
