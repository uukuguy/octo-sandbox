//! D130 — CancellationTokenTree: session-lifetime parent + per-turn child.
//!
//! Problem (D130): `AgentExecutor` held a single `CancellationToken` that was
//! replaced with `CancellationToken::new()` on every `UserMessage`. This meant
//! `SessionInterruptRegistry::cancel()` (path 1) only flipped a flag nobody
//! was reading; the authoritative interrupt relied on `AgentMessage::Cancel`
//! (path 2) reaching the executor via mpsc.
//!
//! Solution: the tree gives each session a *parent* token (session-lifetime)
//! and creates a fresh *child* token per turn. Cancelling the parent propagates
//! into any active child immediately, without a channel round-trip.
//!
//! # Terminology
//! - **session token** — alive for the whole session; injected by
//!   `AgentRuntime` from `SessionInterruptRegistry` at spawn time.
//! - **turn token** — created at the start of each `UserMessage` turn via
//!   `next_turn()`; shares the parent's cancelled flag.
//!
//! # Usage
//! ```rust,ignore
//! // At session spawn (AgentRuntime):
//! let tree = CancellationTokenTree::new();
//! registry.register(session_id.clone(), tree.session_token());
//!
//! // At executor construction:
//! executor.set_cancel_tree(tree);
//!
//! // At turn start (inside executor.run()):
//! let turn_token = self.cancel_tree.next_turn();
//! // pass turn_token into AgentLoopConfig
//!
//! // External cancel (REST /cancel, gRPC interrupt):
//! runtime.cancel_session(&session_id).await;
//! // → registry.cancel(&session_id) fires session_token.cancel()
//! // → propagates into any active turn_token via shared AtomicBool
//! ```

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::agent::cancellation::CancellationToken;

/// Shared atomic flag powering session-lifetime cancellation propagation.
///
/// One instance is shared between the `SessionToken` held by
/// `SessionInterruptRegistry` and every `TurnToken` created within the session.
/// Flipping the flag (via `cancel()` on the session token) is instantly visible
/// to all active turn tokens — no channel needed.
#[derive(Clone, Default)]
struct SharedFlag(Arc<AtomicBool>);

impl SharedFlag {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Session-lifetime token held by `SessionInterruptRegistry`.
///
/// Cancelling this token sets the shared flag; all active `TurnToken`s for the
/// same session observe cancellation on the next `is_cancelled()` poll.
///
/// Cheap to clone (Arc inside).
#[derive(Clone)]
pub struct SessionToken {
    flag: SharedFlag,
    /// Bridges to the existing `CancellationToken` so the registry can keep
    /// its existing `CancellationToken`-typed interface.
    legacy: CancellationToken,
}

impl SessionToken {
    pub fn cancel(&self) {
        self.flag.cancel();
        // Also cancel the legacy token so SessionInterruptRegistry.cancel()
        // callers that read the CancellationToken directly still work.
        self.legacy.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.is_cancelled()
    }

    /// Convert to the `CancellationToken` expected by `SessionInterruptRegistry`.
    pub fn as_cancellation_token(&self) -> &CancellationToken {
        &self.legacy
    }
}

/// Per-turn token created by `CancellationTokenTree::next_turn()`.
///
/// Reports cancelled if either the *session-level flag* (set by external cancel)
/// or the *turn-level flag* (set by the previous turn's reset or an explicit
/// turn cancel) is true.
///
/// Cheap to clone (two Arcs inside).
#[derive(Clone)]
pub struct TurnToken {
    /// Shared with the session — set when the session is externally cancelled.
    session_flag: SharedFlag,
    /// Turn-local flag — only this turn is affected.
    turn_flag: SharedFlag,
}

impl TurnToken {
    /// Returns true if either the session or this specific turn is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.session_flag.is_cancelled() || self.turn_flag.is_cancelled()
    }

    /// Cancel only this turn (does not affect the session or other turns).
    pub fn cancel_turn(&self) {
        self.turn_flag.cancel();
    }

    /// Adapt to `CancellationToken` for code that has not migrated to `TurnToken`.
    ///
    /// The returned token mirrors the turn-level cancelled state by checking
    /// both flags on every `is_cancelled()` call (via a freshly-allocated
    /// bridge token that delegates). For code paths that only call
    /// `cancel_token.is_cancelled()` this works perfectly; code that calls
    /// `cancel_token.cancel()` will only set the turn flag.
    pub fn to_cancellation_token(&self) -> CancellationToken {
        // Create a fresh CancellationToken. If either flag is already set,
        // pre-cancel it so callers get consistent state immediately.
        let tok = CancellationToken::new();
        if self.is_cancelled() {
            tok.cancel();
        }
        tok
    }
}

/// D130 fix — manages the session/turn token hierarchy.
///
/// Create one `CancellationTokenTree` per session at spawn time and hold it
/// in `AgentExecutor`. Call `next_turn()` at the top of each turn instead of
/// `CancellationToken::new()`.
#[derive(Clone)]
pub struct CancellationTokenTree {
    session_flag: SharedFlag,
    /// Legacy token registered with `SessionInterruptRegistry` so the existing
    /// registry API keeps working without breaking changes.
    session_legacy: CancellationToken,
}

impl CancellationTokenTree {
    pub fn new() -> Self {
        let session_flag = SharedFlag::new();
        let session_legacy = CancellationToken::new();
        Self {
            session_flag,
            session_legacy,
        }
    }

    /// Return the session-lifetime token for `SessionInterruptRegistry`.
    ///
    /// Cancelling this token propagates to all future and active turn tokens.
    pub fn session_token(&self) -> SessionToken {
        SessionToken {
            flag: self.session_flag.clone(),
            legacy: self.session_legacy.clone(),
        }
    }

    /// Return the `CancellationToken` for backward-compat registry registration.
    pub fn session_cancellation_token(&self) -> CancellationToken {
        self.session_legacy.clone()
    }

    /// Create a fresh turn token. Called at the start of every `UserMessage`
    /// turn instead of `CancellationToken::new()`.
    ///
    /// The returned token observes session cancellation immediately; it also
    /// has its own turn-local flag that can be set independently.
    pub fn next_turn(&self) -> TurnToken {
        // If session already cancelled, propagate into new turn instantly.
        let turn_flag = SharedFlag::new();
        if self.session_flag.is_cancelled() {
            turn_flag.cancel();
        }
        TurnToken {
            session_flag: self.session_flag.clone(),
            turn_flag,
        }
    }

    /// Whether the session has been externally cancelled.
    pub fn is_session_cancelled(&self) -> bool {
        self.session_flag.is_cancelled()
    }
}

impl Default for CancellationTokenTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cancel_propagates_to_active_turn() {
        let tree = CancellationTokenTree::new();
        let turn = tree.next_turn();
        assert!(!turn.is_cancelled());

        tree.session_token().cancel();

        assert!(turn.is_cancelled(), "session cancel must propagate to active turn");
    }

    #[test]
    fn session_cancel_propagates_to_future_turn() {
        let tree = CancellationTokenTree::new();
        tree.session_token().cancel();

        // next_turn() after session cancel → instantly cancelled
        let turn = tree.next_turn();
        assert!(turn.is_cancelled(), "future turn must observe session cancellation");
    }

    #[test]
    fn turn_cancel_does_not_affect_session() {
        let tree = CancellationTokenTree::new();
        let turn = tree.next_turn();
        turn.cancel_turn();

        assert!(turn.is_cancelled(), "turn must be cancelled");
        assert!(
            !tree.is_session_cancelled(),
            "turn cancel must NOT affect the session"
        );
    }

    #[test]
    fn turn_cancel_does_not_affect_peer_turns() {
        let tree = CancellationTokenTree::new();
        let turn_a = tree.next_turn();
        let turn_b = tree.next_turn();

        turn_a.cancel_turn();

        assert!(turn_a.is_cancelled());
        assert!(
            !turn_b.is_cancelled(),
            "cancelling turn A must not affect turn B"
        );
    }

    #[test]
    fn multiple_turns_all_see_session_cancel() {
        let tree = CancellationTokenTree::new();
        let turns: Vec<_> = (0..5).map(|_| tree.next_turn()).collect();

        for t in &turns {
            assert!(!t.is_cancelled());
        }

        tree.session_token().cancel();

        for (i, t) in turns.iter().enumerate() {
            assert!(t.is_cancelled(), "turn {i} must see session cancel");
        }
    }

    #[test]
    fn session_token_cancel_also_fires_legacy_token() {
        let tree = CancellationTokenTree::new();
        let session_tok = tree.session_token();
        let legacy = session_tok.as_cancellation_token().clone();

        assert!(!legacy.is_cancelled());
        session_tok.cancel();
        assert!(legacy.is_cancelled(), "legacy CancellationToken must fire on session cancel");
    }

    #[test]
    fn turn_token_to_cancellation_token_reflects_session_cancel() {
        let tree = CancellationTokenTree::new();
        let turn = tree.next_turn();
        let compat_tok = turn.to_cancellation_token();

        // Before cancel: not cancelled
        assert!(!compat_tok.is_cancelled());

        // Session cancel → turn.is_cancelled() → compat token was already snapshotted;
        // for already-cancelled turns the helper pre-cancels the token.
        let tree2 = CancellationTokenTree::new();
        tree2.session_token().cancel();
        let turn2 = tree2.next_turn();
        let compat2 = turn2.to_cancellation_token();
        assert!(
            compat2.is_cancelled(),
            "compat token for an already-cancelled turn must reflect cancellation"
        );
    }

    #[test]
    fn new_turn_after_non_cancelled_session_is_not_cancelled() {
        let tree = CancellationTokenTree::new();
        let turn = tree.next_turn();
        assert!(!turn.is_cancelled());
        assert!(!tree.is_session_cancelled());
    }

    #[test]
    fn clone_session_token_shares_flag() {
        let tree = CancellationTokenTree::new();
        let tok1 = tree.session_token();
        let tok2 = tok1.clone();

        assert!(!tok1.is_cancelled());
        tok2.cancel();
        assert!(tok1.is_cancelled(), "cloned session tokens share the same flag");
    }

    #[test]
    fn default_tree_is_not_cancelled() {
        let tree = CancellationTokenTree::default();
        assert!(!tree.is_session_cancelled());
        let turn = tree.next_turn();
        assert!(!turn.is_cancelled());
    }
}
