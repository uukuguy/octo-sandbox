//! Taint Tracking - Track sensitive data through the system
//!
//! Provides taint labels to mark sensitive data and track where
//! sensitive information flows to prevent accidental leakage.

#![allow(dead_code)]

use zeroize::{Zeroize, ZeroizeOnDrop};

/// Label indicating the sensitivity level of data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Source of the value (e.g., "config", "user_input", "env")
    source: String,
}

impl Zeroize for TaintedValue {
    fn zeroize(&mut self) {
        self.value.zeroize();
    }
}

impl ZeroizeOnDrop for TaintedValue {}

impl TaintedValue {
    /// Create a new tainted value with Secret label
    pub fn new_secret(value: String, source: String) -> Self {
        Self {
            value: value.into_bytes(),
            label: TaintLabel::Secret,
            source,
        }
    }

    /// Create a new tainted value with Internal label
    pub fn new_internal(value: String, source: String) -> Self {
        Self {
            value: value.into_bytes(),
            label: TaintLabel::Internal,
            source,
        }
    }

    /// Create a new tainted value with Public label
    pub fn new_public(value: String, source: String) -> Self {
        Self {
            value: value.into_bytes(),
            label: TaintLabel::Public,
            source,
        }
    }

    /// Create a new tainted value with Confidential label
    pub fn new_confidential(value: String, source: String) -> Self {
        Self {
            value: value.into_bytes(),
            label: TaintLabel::Confidential,
            source,
        }
    }

    /// Get the label of this tainted value
    pub fn label(&self) -> &TaintLabel {
        &self.label
    }

    /// Get the source of this tainted value
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the value as a string
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.value).unwrap_or("")
    }

    /// Check if this value can flow to the given sink
    ///
    /// Returns Ok(()) if the flow is allowed, Err(TaintViolation) if blocked
    pub fn check_sink(&self, sink: &TaintSink) -> Result<(), TaintViolation> {
        let blocked = match (self.label, sink) {
            // Secret and Confidential data cannot flow to any sink
            (TaintLabel::Secret, _) => true,
            (TaintLabel::Confidential, _) => true,
            // Internal data cannot flow to external response
            (TaintLabel::Internal, TaintSink::ExternalResponse) => true,
            // Internal can flow to Log, Error, File
            (TaintLabel::Internal, TaintSink::Log) => false,
            (TaintLabel::Internal, TaintSink::Error) => false,
            (TaintLabel::Internal, TaintSink::File) => false,
            // Public data can flow anywhere
            (TaintLabel::Public, _) => false,
        };

        if blocked {
            Err(TaintViolation {
                sink: *sink,
                label: self.label,
                description: format!(
                    "Tainted value with label {:?} cannot flow to sink {:?}",
                    self.label, sink
                ),
            })
        } else {
            Ok(())
        }
    }
}

/// Types of sinks where sensitive data should not flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tainted_value_new_secret() {
        let value = TaintedValue::new_secret("sk-12345".to_string(), "config".to_string());
        assert_eq!(value.label(), &TaintLabel::Secret);
        assert_eq!(value.source(), "config");
        assert_eq!(value.as_str(), "sk-12345");
    }

    #[test]
    fn test_tainted_value_new_internal() {
        let value = TaintedValue::new_internal("internal_info".to_string(), "system".to_string());
        assert_eq!(value.label(), &TaintLabel::Internal);
        assert_eq!(value.source(), "system");
    }

    #[test]
    fn test_tainted_value_new_public() {
        let value = TaintedValue::new_public("public_data".to_string(), "user".to_string());
        assert_eq!(value.label(), &TaintLabel::Public);
    }

    #[test]
    fn test_tainted_value_new_confidential() {
        let value = TaintedValue::new_confidential("confidential".to_string(), "db".to_string());
        assert_eq!(value.label(), &TaintLabel::Confidential);
    }

    #[test]
    fn test_secret_blocks_all_sinks() {
        let value = TaintedValue::new_secret("secret".to_string(), "config".to_string());

        // Secret should be blocked from all sinks
        assert!(value.check_sink(&TaintSink::Log).is_err());
        assert!(value.check_sink(&TaintSink::Error).is_err());
        assert!(value.check_sink(&TaintSink::ExternalResponse).is_err());
        assert!(value.check_sink(&TaintSink::File).is_err());
    }

    #[test]
    fn test_confidential_blocks_all_sinks() {
        let value = TaintedValue::new_confidential("confidential".to_string(), "db".to_string());

        // Confidential should be blocked from all sinks
        assert!(value.check_sink(&TaintSink::Log).is_err());
        assert!(value.check_sink(&TaintSink::ExternalResponse).is_err());
    }

    #[test]
    fn test_internal_allows_internal_sinks() {
        let value = TaintedValue::new_internal("internal".to_string(), "system".to_string());

        // Internal should be allowed for internal sinks
        assert!(value.check_sink(&TaintSink::Log).is_ok());
        assert!(value.check_sink(&TaintSink::Error).is_ok());
        assert!(value.check_sink(&TaintSink::File).is_ok());
        // But blocked for external response
        assert!(value.check_sink(&TaintSink::ExternalResponse).is_err());
    }

    #[test]
    fn test_public_allows_all_sinks() {
        let value = TaintedValue::new_public("public".to_string(), "user".to_string());

        // Public should be allowed for all sinks
        assert!(value.check_sink(&TaintSink::Log).is_ok());
        assert!(value.check_sink(&TaintSink::Error).is_ok());
        assert!(value.check_sink(&TaintSink::ExternalResponse).is_ok());
        assert!(value.check_sink(&TaintSink::File).is_ok());
    }

    #[test]
    fn test_taint_violation_contains_info() {
        let value = TaintedValue::new_secret("secret".to_string(), "config".to_string());
        let result = value.check_sink(&TaintSink::Log);

        assert!(result.is_err());
        let violation = result.unwrap_err();
        assert_eq!(violation.label, TaintLabel::Secret);
        assert_eq!(violation.sink, TaintSink::Log);
        assert!(!violation.description.is_empty());
    }
}
