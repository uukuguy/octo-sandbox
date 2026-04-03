//! Commands module for Grid CLI

pub mod agent;
pub mod ask;
pub mod auth;
pub mod completions;
pub mod config;
#[cfg(feature = "studio")]
pub mod dashboard;
#[cfg(feature = "studio")]
pub mod dashboard_auth;
#[cfg(feature = "studio")]
pub mod dashboard_cert;
#[cfg(feature = "studio")]
pub mod dashboard_security;
pub mod doctor;
pub mod eval_cmd;
pub mod init;
pub mod mcp;
pub mod memory;
pub mod root;
pub mod run;
pub mod sandbox;
pub mod session;
pub mod skill;
pub mod state;
pub mod tools;
pub mod types;

// Re-export types for external use
pub use types::{
    AgentCommands, AuthCommands, CompletionsCommands, ConfigCommands, EvalCommands, McpCommands,
    MemoryCommands, RootCommands, SandboxCommands, SessionCommands, SkillCommands, ToolsCommands,
};

// Re-export handler functions
pub use agent::handle_agent;
pub use ask::execute_ask;
pub use auth::handle_auth;
pub use completions::generate_completions;
pub use config::handle_config;
pub use doctor::run_doctor;
pub use eval_cmd::handle_eval;
pub use init::execute_init;
pub use mcp::handle_mcp;
pub use memory::handle_memory;
pub use root::handle_root;
pub use run::execute_run;
pub use sandbox::handle_sandbox;
pub use session::handle_session;
pub use skill::handle_skill;
pub use tools::handle_tools;

// Re-export AppState
pub use state::AppState;
