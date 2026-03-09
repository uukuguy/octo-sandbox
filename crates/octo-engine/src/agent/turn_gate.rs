use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Per-session mutual exclusion for agent turns.
/// Prevents TOCTOU race when HTTP requests and heartbeat runners
/// compete for the same session. (localgpt pattern)
pub struct TurnGate {
    semaphore: Arc<Semaphore>,
}

/// RAII guard -- held while an agent turn is in progress.
pub struct TurnGateGuard {
    _permit: OwnedSemaphorePermit,
}

impl TurnGate {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    /// Block until the gate is available, then acquire exclusive access.
    pub async fn acquire(&self) -> TurnGateGuard {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        TurnGateGuard { _permit: permit }
    }

    /// Try to acquire the gate without blocking. Returns None if already held.
    pub fn try_acquire(&self) -> Option<TurnGateGuard> {
        self.semaphore
            .clone()
            .try_acquire_owned()
            .ok()
            .map(|permit| TurnGateGuard { _permit: permit })
    }

    /// Check if the gate is currently held (a turn is in progress).
    pub fn is_busy(&self) -> bool {
        self.semaphore.available_permits() == 0
    }
}

impl Default for TurnGate {
    fn default() -> Self {
        Self::new()
    }
}
