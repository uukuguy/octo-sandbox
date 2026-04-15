//! Stop hooks — claude-code-style hooks that run at agent loop termination.
//!
//! A `StopHook` runs on **every natural termination boundary** — the
//! `EndTurn`-with-no-tool-uses exit — regardless of whether any tools
//! were called this session (i.e. pure text-chat sessions also fire
//! Stop hooks). The hook can either let termination proceed (`Noop`)
//! or push fresh messages and re-enter the loop (`InjectAndContinue`).
//!
//! Stop hooks do NOT fire on error exits (provider failure, stream
//! consumption failure, SecurityBlocked, LoopGuard, EStop, cooperative
//! cancel). Those paths return before reaching the dispatch site — see
//! `run_agent_loop_inner` for the control-flow proof.
//!
//! ## Why a separate trait (vs. extending `HookHandler`)
//!
//! `HookHandler` returns `HookAction` whose variants (`Continue / Modify /
//! Abort / Block / Redirect / ModifyInput / InjectContext /
//! PermissionOverride`) cannot express "push these `ChatMessage`s and re-run
//! the loop". Stop hooks need typed access to `Vec<ChatMessage>`, so they
//! get their own narrow trait that can later be bridged from the existing
//! `HookPoint::Stop` registration in S3.T5 (scoped-hook executor).
//!
//! ## Re-entry safety
//!
//! Each `InjectAndContinue` decision counts against
//! [`MAX_STOP_HOOK_INJECTIONS`]. When the cap is hit, the harness logs a
//! warning and returns the final response unchanged. The cap is **per
//! loop invocation**, not global / per-process — see
//! `run_agent_loop_inner` for the counter wiring.
//!
//! See ADR-V2-006 (S3.T5) for the wire envelope between scoped bash hooks
//! and this trait.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use grid_types::ChatMessage;

use crate::hooks::HookContext;

/// Maximum number of times a Stop hook can inject messages and re-enter
/// the agent loop within a single session. Prevents infinite re-entry
/// from a buggy hook. Unrelated to `AgentLoopConfig::max_iterations`,
/// which bounds total LLM rounds.
pub const MAX_STOP_HOOK_INJECTIONS: u32 = 3;

/// Decision returned by a Stop hook after the agent loop completes.
#[derive(Debug, Clone)]
pub enum StopHookDecision {
    /// No action; agent loop returns the final response unchanged.
    Noop,
    /// Push these messages to the conversation history and re-run the loop.
    /// Bounded by [`MAX_STOP_HOOK_INJECTIONS`].
    InjectAndContinue(Vec<ChatMessage>),
}

/// A Stop hook runs when the agent loop would otherwise terminate.
///
/// Multiple hooks can be registered in a single `Vec`. The dispatcher
/// (`dispatch_stop_hooks`) executes them sequentially. The first hook
/// that returns `InjectAndContinue` wins; subsequent hooks still run for
/// observability side effects, but their injection decisions are dropped
/// (consistent with claude-code "first decisive verdict" semantics).
#[async_trait]
pub trait StopHook: Send + Sync {
    /// Hook identifier for logging / debugging.
    fn name(&self) -> &str;

    /// Execute the hook. Return `Noop` to let the loop terminate, or
    /// `InjectAndContinue(messages)` to push messages and re-enter.
    async fn execute(&self, ctx: &HookContext) -> Result<StopHookDecision>;
}

/// Default no-op implementation — agents without stop hooks use this.
pub struct NoOpStopHook;

#[async_trait]
impl StopHook for NoOpStopHook {
    fn name(&self) -> &str {
        "noop"
    }

    async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
        Ok(StopHookDecision::Noop)
    }
}

/// Dispatch a slice of Stop hooks sequentially.
///
/// Returns the first `InjectAndContinue` decision encountered (remaining
/// hooks still execute for observability but their injection is ignored).
/// Returns `Noop` if all hooks return `Noop` or if `hooks` is empty.
///
/// A hook that returns `Err` is logged at `warn` level and treated as
/// `Noop` so a single buggy hook cannot poison the dispatch chain.
pub async fn dispatch_stop_hooks(
    hooks: &[Arc<dyn StopHook>],
    ctx: &HookContext,
) -> StopHookDecision {
    let mut accepted: Option<StopHookDecision> = None;
    for h in hooks {
        match h.execute(ctx).await {
            Ok(StopHookDecision::InjectAndContinue(msgs)) => {
                if accepted.is_none() {
                    accepted = Some(StopHookDecision::InjectAndContinue(msgs));
                }
                // Continue the loop to fire remaining hooks for side effects.
            }
            Ok(StopHookDecision::Noop) => {}
            Err(e) => {
                tracing::warn!(
                    hook = h.name(),
                    error = %e,
                    "stop hook failed, treating as Noop"
                );
            }
        }
    }
    accepted.unwrap_or(StopHookDecision::Noop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn ctx() -> HookContext {
        HookContext::new().with_session("test-session")
    }

    fn inject_msg(text: &str) -> Vec<ChatMessage> {
        vec![ChatMessage::user(text)]
    }

    #[tokio::test]
    async fn test_noop_default() {
        let hook = NoOpStopHook;
        let decision = hook.execute(&ctx()).await.unwrap();
        assert!(matches!(decision, StopHookDecision::Noop));
        assert_eq!(hook.name(), "noop");
    }

    #[tokio::test]
    async fn test_dispatch_empty_returns_noop() {
        let hooks: Vec<Arc<dyn StopHook>> = vec![];
        let decision = dispatch_stop_hooks(&hooks, &ctx()).await;
        assert!(matches!(decision, StopHookDecision::Noop));
    }

    #[tokio::test]
    async fn test_dispatch_first_inject_wins() {
        // Hook A: Noop. Hook B: Inject("first"). Hook C: Inject("second")
        // — but C also asserts it actually ran (via call counter), proving
        // remaining hooks fire for side effects even after A wins.
        struct NoopHook;
        #[async_trait]
        impl StopHook for NoopHook {
            fn name(&self) -> &str {
                "a-noop"
            }
            async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
                Ok(StopHookDecision::Noop)
            }
        }

        struct InjectHook {
            label: &'static str,
            text: &'static str,
            ran: Arc<AtomicU32>,
        }
        #[async_trait]
        impl StopHook for InjectHook {
            fn name(&self) -> &str {
                self.label
            }
            async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
                self.ran.fetch_add(1, Ordering::SeqCst);
                Ok(StopHookDecision::InjectAndContinue(inject_msg(self.text)))
            }
        }

        let b_ran = Arc::new(AtomicU32::new(0));
        let c_ran = Arc::new(AtomicU32::new(0));

        let hooks: Vec<Arc<dyn StopHook>> = vec![
            Arc::new(NoopHook),
            Arc::new(InjectHook {
                label: "b-inject",
                text: "first",
                ran: b_ran.clone(),
            }),
            Arc::new(InjectHook {
                label: "c-inject",
                text: "second",
                ran: c_ran.clone(),
            }),
        ];

        let decision = dispatch_stop_hooks(&hooks, &ctx()).await;

        match decision {
            StopHookDecision::InjectAndContinue(msgs) => {
                assert_eq!(msgs.len(), 1);
                // First inject (B) wins, not the second (C).
                assert_eq!(msgs[0].text_content(), "first");
            }
            other => panic!("expected InjectAndContinue(first), got {:?}", other),
        }
        assert_eq!(b_ran.load(Ordering::SeqCst), 1, "B must have run");
        assert_eq!(
            c_ran.load(Ordering::SeqCst),
            1,
            "C must still run for observability even though its decision is dropped"
        );
    }

    #[tokio::test]
    async fn test_dispatch_error_logged_as_noop() {
        // Hook A errors, Hook B returns Noop. Final decision must be Noop
        // and the chain must continue to B (B's call counter proves it ran).
        struct ErrHook;
        #[async_trait]
        impl StopHook for ErrHook {
            fn name(&self) -> &str {
                "err"
            }
            async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
                anyhow::bail!("simulated hook failure")
            }
        }

        struct CountingNoop {
            ran: Arc<AtomicU32>,
        }
        #[async_trait]
        impl StopHook for CountingNoop {
            fn name(&self) -> &str {
                "counting-noop"
            }
            async fn execute(&self, _ctx: &HookContext) -> Result<StopHookDecision> {
                self.ran.fetch_add(1, Ordering::SeqCst);
                Ok(StopHookDecision::Noop)
            }
        }

        let ran = Arc::new(AtomicU32::new(0));
        let hooks: Vec<Arc<dyn StopHook>> = vec![
            Arc::new(ErrHook),
            Arc::new(CountingNoop { ran: ran.clone() }),
        ];

        let decision = dispatch_stop_hooks(&hooks, &ctx()).await;
        assert!(matches!(decision, StopHookDecision::Noop));
        assert_eq!(
            ran.load(Ordering::SeqCst),
            1,
            "trailing hook must fire even after a prior hook errored"
        );
    }
}
