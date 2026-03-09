//! Commands module for Octo CLI

pub mod agent;
pub mod config;
pub mod memory;
pub mod session;
pub mod state;
pub mod tools;
pub mod types;

// Re-export types for external use
pub use types::{AgentCommands, ConfigCommands, MemoryCommands, SessionCommands, ToolsCommands};

// Re-export handler functions
pub use agent::handle_agent;
pub use config::handle_config;
pub use memory::handle_memory;
pub use session::handle_session;
pub use tools::handle_tools;

// Re-export AppState
pub use state::AppState;
