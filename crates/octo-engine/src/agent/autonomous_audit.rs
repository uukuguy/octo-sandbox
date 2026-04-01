//! Autonomous mode audit logging (Phase AQ-T6).
//!
//! Provides structured audit entries for autonomous agent actions.
//! Entries are self-contained and can be serialized/stored independently
//! of the generic AuditRecord system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Autonomous audit event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum AutonomousAuditEvent {
    Started { config_summary: String },
    TickCompleted { round: u32, tokens_used: u64, cost_usd: f64 },
    Paused { reason: String },
    Resumed,
    BudgetExhausted { limit: String, value: String },
    UserPresenceChanged { online: bool },
    Completed { total_rounds: u32, total_tokens: u64, total_cost_usd: f64 },
    Failed { error: String },
}

impl AutonomousAuditEvent {
    /// Event type name for categorization.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::TickCompleted { .. } => "tick_completed",
            Self::Paused { .. } => "paused",
            Self::Resumed => "resumed",
            Self::BudgetExhausted { .. } => "budget_exhausted",
            Self::UserPresenceChanged { .. } => "user_presence_changed",
            Self::Completed { .. } => "completed",
            Self::Failed { .. } => "failed",
        }
    }
}

/// A single autonomous audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub event: AutonomousAuditEvent,
}

impl AutonomousAuditEntry {
    pub fn new(session_id: &str, event: AutonomousAuditEvent) -> Self {
        Self {
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            event,
        }
    }

    /// Format as a generic audit event type string.
    pub fn event_type(&self) -> String {
        format!("autonomous.{}", self.event.name())
    }

    /// Convert to JSON for generic storage.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// In-memory audit log for autonomous mode sessions.
///
/// Collects all audit entries for a single autonomous session run.
/// Can be queried or serialized after the run completes.
#[derive(Debug, Default)]
pub struct AutonomousAuditLog {
    entries: Vec<AutonomousAuditEntry>,
}

impl AutonomousAuditLog {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn record(&mut self, session_id: &str, event: AutonomousAuditEvent) {
        self.entries.push(AutonomousAuditEntry::new(session_id, event));
    }

    pub fn entries(&self) -> &[AutonomousAuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Export all entries as a JSON array.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.entries).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_names() {
        assert_eq!(AutonomousAuditEvent::Started { config_summary: "test".into() }.name(), "started");
        assert_eq!(AutonomousAuditEvent::TickCompleted { round: 1, tokens_used: 100, cost_usd: 0.01 }.name(), "tick_completed");
        assert_eq!(AutonomousAuditEvent::Paused { reason: "user".into() }.name(), "paused");
        assert_eq!(AutonomousAuditEvent::Resumed.name(), "resumed");
        assert_eq!(AutonomousAuditEvent::BudgetExhausted { limit: "rounds".into(), value: "10".into() }.name(), "budget_exhausted");
        assert_eq!(AutonomousAuditEvent::UserPresenceChanged { online: true }.name(), "user_presence_changed");
        assert_eq!(AutonomousAuditEvent::Completed { total_rounds: 5, total_tokens: 500, total_cost_usd: 0.05 }.name(), "completed");
        assert_eq!(AutonomousAuditEvent::Failed { error: "timeout".into() }.name(), "failed");
    }

    #[test]
    fn test_audit_entry_creation() {
        let entry = AutonomousAuditEntry::new("sess-1", AutonomousAuditEvent::Started {
            config_summary: "max_rounds=10, idle_sleep=30s".into(),
        });
        assert_eq!(entry.session_id, "sess-1");
        assert!(matches!(entry.event, AutonomousAuditEvent::Started { .. }));
        assert_eq!(entry.event_type(), "autonomous.started");
    }

    #[test]
    fn test_audit_entry_json() {
        let entry = AutonomousAuditEntry::new("sess-2", AutonomousAuditEvent::TickCompleted {
            round: 3,
            tokens_used: 1500,
            cost_usd: 0.015,
        });
        let json = entry.to_json();
        assert_eq!(json["session_id"], "sess-2");
        assert!(json["event"]["round"].is_number());
    }

    #[test]
    fn test_audit_event_serialization() {
        let event = AutonomousAuditEvent::Completed {
            total_rounds: 10,
            total_tokens: 5000,
            total_cost_usd: 0.50,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Completed"));
        assert!(json.contains("5000"));
    }

    #[test]
    fn test_audit_log_collect_and_export() {
        let mut log = AutonomousAuditLog::new();
        assert!(log.is_empty());

        log.record("sess-1", AutonomousAuditEvent::Started { config_summary: "test".into() });
        log.record("sess-1", AutonomousAuditEvent::TickCompleted { round: 1, tokens_used: 100, cost_usd: 0.01 });
        log.record("sess-1", AutonomousAuditEvent::Completed { total_rounds: 1, total_tokens: 100, total_cost_usd: 0.01 });

        assert_eq!(log.len(), 3);
        assert!(!log.is_empty());

        let json = log.to_json();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_user_presence_audit() {
        let entry = AutonomousAuditEntry::new("sess-3", AutonomousAuditEvent::UserPresenceChanged {
            online: false,
        });
        assert_eq!(entry.event_type(), "autonomous.user_presence_changed");
        let json = entry.to_json();
        assert_eq!(json["event"]["online"], false);
    }
}
