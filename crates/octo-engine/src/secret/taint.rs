//! Taint Tracking - Track sensitive data through the system
//!
//! Provides taint labels to mark sensitive data and track where
//! sensitive information flows to prevent accidental leakage.

#![allow(dead_code)]

use zeroize::{Zeroize, ZeroizeOnDrop};

/// Label indicating the sensitivity level of data
#[derive(Debug, Clone)]
pub enum TaintLabel {
    /// Public data, no restrictions
    Public,
    /// Internal data, not for external display
    Internal,
    /// Confidential data, restricted access
    Confidential,
    /// Secret data, highly restricted
    Secret,
}

/// A value with associated taint label
// Note: Clone intentionally not derived - contains sensitive data that should not be duplicated
pub struct TaintedValue {
    /// The actual value (should be zeroized on drop)
    value: Vec<u8>,
    /// The taint label
    label: TaintLabel,
}

impl Zeroize for TaintedValue {
    fn zeroize(&mut self) {
        self.value.zeroize();
    }
}

impl ZeroizeOnDrop for TaintedValue {}

/// Types of sinks where sensitive data should not flow
#[derive(Debug, Clone)]
pub enum TaintSink {
    /// Log output
    Log,
    /// Error messages
    Error,
    /// External API responses
    ExternalResponse,
    /// File output
    File,
}

/// Represents a taint violation when sensitive data flows to a sink
#[derive(Debug)]
pub struct TaintViolation {
    /// The sink where data should not have flowed
    pub sink: TaintSink,
    /// The taint label that was violated
    pub label: TaintLabel,
    /// Description of the violation
    pub description: String,
}
