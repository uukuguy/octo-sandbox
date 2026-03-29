//! Declarative hook system — hooks.yaml configuration, command executor, and bridge handler.

mod bridge;
mod command_executor;
mod config;
mod loader;

pub use bridge::DeclarativeHookBridge;
pub use command_executor::{execute_command, HookDecision};
pub use config::{FailureMode, HookActionConfig, HookEntry, HooksConfig};
pub use loader::load_hooks_config;
