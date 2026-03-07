//! Hook Engine -- Lifecycle hook system for octo-engine
//!
//! Provides extensible hook points across the agent lifecycle.
//! Hooks can observe, modify, or abort operations.

mod context;
mod handler;
mod registry;

pub use context::HookContext;
pub use handler::{BoxHookHandler, HookAction, HookHandler};
pub use registry::HookRegistry;

/// Hook points in the agent lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPoint {
    /// Before a tool is executed
    PreToolUse,
    /// After a tool completes
    PostToolUse,
    /// Before a task/turn starts
    PreTask,
    /// After a task/turn completes
    PostTask,
    /// Session starts
    SessionStart,
    /// Session ends
    SessionEnd,
    /// Context degradation detected (replaces PreCompact)
    ContextDegraded,
    /// Loop turn starts
    LoopTurnStart,
    /// Loop turn ends
    LoopTurnEnd,
    /// Agent is being routed
    AgentRoute,
}
