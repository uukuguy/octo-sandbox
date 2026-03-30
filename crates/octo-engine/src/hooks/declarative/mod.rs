//! Declarative hook system — hooks.yaml configuration, command executor, prompt renderer, and bridge handler.

mod bridge;
mod command_executor;
mod config;
pub mod loader;
pub mod prompt_executor;
pub mod prompt_renderer;
pub mod webhook_executor;

pub use bridge::DeclarativeHookBridge;
pub use command_executor::{execute_command, HookDecision};
pub use config::{FailureMode, HookActionConfig, HookEntry, HooksConfig};
pub use loader::load_hooks_config;
