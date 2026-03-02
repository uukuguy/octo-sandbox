//! Cancellation Token - Cooperative cancellation for async operations
//!
//! Provides cancellation support for agent operations with parent/child token support

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;

/// Cancellation token for cooperative cancellation
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notifier: Option<watch::Sender<()>>,
}

impl CancellationToken {
    /// Create a new cancellation token
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notifier: None,
        }
    }

    /// Create a new token with notifier for async wait
    pub fn with_notifier() -> (Self, watch::Receiver<()>) {
        let (tx, rx) = watch::channel(());
        let token = Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notifier: Some(tx),
        };
        (token, rx)
    }

    /// Check if cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Request cancellation
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(ref tx) = self.notifier {
            let _ = tx.send(());
        }
    }

    /// Create a child token that inherits parent cancellation
    pub fn child(&self) -> ChildCancellationToken {
        ChildCancellationToken {
            parent: self.cancelled.clone(),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Child cancellation token that inherits parent cancellation
pub struct ChildCancellationToken {
    parent: Arc<AtomicBool>,
}

impl ChildCancellationToken {
    /// Check if cancelled (including parent cancellation)
    pub fn is_cancelled(&self) -> bool {
        self.parent.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancellation_token_new() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancellation_token_cancel() {
        let token = CancellationToken::new();

        assert!(!token.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancellation_token_with_notifier() {
        let (token, mut rx) = CancellationToken::with_notifier();

        assert!(!token.is_cancelled());

        token.cancel();

        assert!(token.is_cancelled());

        // Receiver should be notified
        rx.changed().await.unwrap();
    }

    #[test]
    fn test_child_token_inherits_parent() {
        let parent = CancellationToken::new();
        let child = parent.child();

        assert!(!child.is_cancelled());

        parent.cancel();

        assert!(child.is_cancelled());
    }
}
