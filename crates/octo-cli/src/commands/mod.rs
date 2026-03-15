//! Commands module for Octo CLI

pub mod agent;
pub mod ask;
pub mod completions;
pub mod config;
pub mod dashboard;
pub mod dashboard_auth;
pub mod dashboard_cert;
pub mod dashboard_security;
pub mod doctor;
pub mod mcp;
pub mod memory;
pub mod run;
pub mod session;
pub mod state;
pub mod tools;
pub mod types;

// Re-export types for external use
pub use types::{
    AgentCommands, CompletionsCommands, ConfigCommands, EvalCommands, McpCommands, MemoryCommands,
    SessionCommands, ToolsCommands,
};

// Re-export handler functions
pub use agent::handle_agent;
pub use ask::execute_ask;
pub use completions::generate_completions;
pub use config::handle_config;
pub use doctor::run_doctor;
pub use mcp::handle_mcp;
pub use memory::handle_memory;
pub use run::execute_run;
pub use session::handle_session;
pub use tools::handle_tools;

// Re-export AppState
pub use state::AppState;
