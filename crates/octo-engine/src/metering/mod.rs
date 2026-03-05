//! Metering module for tracking token usage and request metrics.
//!
//! This module provides atomic counters for tracking LLM usage including
//! input/output tokens, request counts, errors, and duration.

use std::sync::atomic::{AtomicU64, Ordering};

/// A snapshot of metering data at a point in time.
#[derive(Debug, Clone, Default)]
pub struct MeteringSnapshot {
    /// Total input tokens used.
    pub input_tokens: u64,
    /// Total output tokens generated.
    pub output_tokens: u64,
    /// Total number of requests made.
    pub requests: u64,
    /// Total number of errors encountered.
    pub errors: u64,
    /// Total duration of all requests in milliseconds.
    pub duration_ms: u64,
}

impl MeteringSnapshot {
    /// Calculate total tokens (input + output).
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Calculate average tokens per request.
    pub fn avg_tokens_per_request(&self) -> f64 {
        if self.requests == 0 {
            return 0.0;
        }
        self.total_tokens() as f64 / self.requests as f64
    }

    /// Calculate average duration per request in milliseconds.
    pub fn avg_duration_ms(&self) -> f64 {
        if self.requests == 0 {
            return 0.0;
        }
        self.duration_ms as f64 / self.requests as f64
    }
}

/// Metering struct for tracking LLM usage with atomic counters.
pub struct Metering {
    /// Total input tokens used.
    pub input_tokens: AtomicU64,
    /// Total output tokens generated.
    pub output_tokens: AtomicU64,
    /// Total number of requests made.
    pub requests: AtomicU64,
    /// Total number of errors encountered.
    pub errors: AtomicU64,
    /// Total duration of all requests in milliseconds.
    pub duration_ms: AtomicU64,
}

impl Default for Metering {
    fn default() -> Self {
        Self::new()
    }
}

impl Metering {
    /// Create a new Metering instance.
    pub fn new() -> Self {
        Self {
            input_tokens: AtomicU64::new(0),
            output_tokens: AtomicU64::new(0),
            requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
        }
    }

    /// Record a successful request with input/output tokens and duration.
    pub fn record_request(&self, input: usize, output: usize, duration_ms: u64) {
        self.input_tokens.fetch_add(input as u64, Ordering::Relaxed);
        self.output_tokens
            .fetch_add(output as u64, Ordering::Relaxed);
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.duration_ms.fetch_add(duration_ms, Ordering::Relaxed);
    }

    /// Record an error (increments error counter only).
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a snapshot of current metering values.
    pub fn snapshot(&self) -> MeteringSnapshot {
        MeteringSnapshot {
            input_tokens: self.input_tokens.load(Ordering::Relaxed),
            output_tokens: self.output_tokens.load(Ordering::Relaxed),
            requests: self.requests.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            duration_ms: self.duration_ms.load(Ordering::Relaxed),
        }
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.input_tokens.store(0, Ordering::Relaxed);
        self.output_tokens.store(0, Ordering::Relaxed);
        self.requests.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        self.duration_ms.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metering_record_request() {
        let metering = Metering::new();

        metering.record_request(100, 50, 1000);
        metering.record_request(200, 75, 500);

        let snapshot = metering.snapshot();
        assert_eq!(snapshot.input_tokens, 300);
        assert_eq!(snapshot.output_tokens, 125);
        assert_eq!(snapshot.requests, 2);
        assert_eq!(snapshot.duration_ms, 1500);
        assert_eq!(snapshot.errors, 0);
    }

    #[test]
    fn test_metering_record_error() {
        let metering = Metering::new();

        metering.record_request(100, 50, 1000);
        metering.record_error();

        let snapshot = metering.snapshot();
        assert_eq!(snapshot.requests, 1);
        assert_eq!(snapshot.errors, 1);
    }

    #[test]
    fn test_metering_snapshot_calculations() {
        let metering = Metering::new();

        // Record 3 requests with varying token counts
        metering.record_request(100, 50, 1000);
        metering.record_request(200, 100, 2000);
        metering.record_request(300, 150, 3000);

        let snapshot = metering.snapshot();

        // Total tokens: (100+200+300) + (50+100+150) = 600 + 300 = 900
        assert_eq!(snapshot.total_tokens(), 900);

        // Average: 900 / 3 = 300
        assert_eq!(snapshot.avg_tokens_per_request(), 300.0);

        // Average duration: 6000 / 3 = 2000
        assert_eq!(snapshot.avg_duration_ms(), 2000.0);
    }

    #[test]
    fn test_metering_reset() {
        let metering = Metering::new();

        metering.record_request(100, 50, 1000);
        metering.record_error();
        metering.reset();

        let snapshot = metering.snapshot();
        assert_eq!(snapshot.input_tokens, 0);
        assert_eq!(snapshot.output_tokens, 0);
        assert_eq!(snapshot.requests, 0);
        assert_eq!(snapshot.errors, 0);
        assert_eq!(snapshot.duration_ms, 0);
    }

    #[test]
    fn test_default_metering() {
        let metering = Metering::default();
        let snapshot = metering.snapshot();

        assert_eq!(snapshot.input_tokens, 0);
        assert_eq!(snapshot.output_tokens, 0);
        assert_eq!(snapshot.requests, 0);
        assert_eq!(snapshot.errors, 0);
        assert_eq!(snapshot.duration_ms, 0);
    }
}
