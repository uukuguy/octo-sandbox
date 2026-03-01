use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// Gauge metric - can go up or down
pub struct Gauge(Arc<AtomicI64>);

impl Gauge {
    pub fn new() -> Self {
        Self(Arc::new(AtomicI64::new(0)))
    }

    pub fn from_arc(arc: Arc<AtomicI64>) -> Self {
        Self(arc)
    }

    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_by(&self, value: i64) {
        self.0.fetch_add(value, Ordering::Relaxed);
    }

    pub fn dec(&self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn dec_by(&self, value: i64) {
        self.0.fetch_sub(value, Ordering::Relaxed);
    }

    pub fn set(&self, value: i64) {
        self.0.store(value, Ordering::Relaxed);
    }

    pub fn get(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Gauge {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl std::fmt::Debug for Gauge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gauge")
            .field("value", &self.get())
            .finish()
    }
}
