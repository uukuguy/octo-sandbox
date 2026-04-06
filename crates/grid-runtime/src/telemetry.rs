//! EAASP Telemetry — standardized event collection and conversion.
//!
//! Provides typed event construction and conversion from grid-engine's
//! internal `TelemetryEvent` (EventBus) and `MeteringSnapshot` to the
//! EAASP `contract::TelemetryEvent` schema defined in §8.4.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::contract::{ResourceUsage, TelemetryEvent};

// ── EAASP Event Types ──

/// Standardized EAASP telemetry event types (§8.4).
///
/// Replaces free-form strings with a typed enum for compile-time safety.
/// `as_str()` returns the wire-format string used in proto and JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EaaspEventType {
    /// Metering snapshot (token usage, compute time).
    MeteringSnapshot,
    /// Tool call started.
    ToolCallStarted,
    /// Tool call completed with duration.
    ToolCallCompleted,
    /// Agent loop turn started.
    LoopTurnStarted,
    /// Context degradation triggered.
    ContextDegraded,
    /// Loop guard triggered (max turns, token limit).
    LoopGuardTriggered,
    /// Token budget updated.
    TokenBudgetUpdated,
    /// Session initialized.
    SessionStart,
    /// Session terminated.
    SessionEnd,
    /// Hook decision made.
    HookDecision,
}

impl EaaspEventType {
    /// Wire-format string for proto/JSON serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MeteringSnapshot => "metering_snapshot",
            Self::ToolCallStarted => "tool_call_started",
            Self::ToolCallCompleted => "tool_call_completed",
            Self::LoopTurnStarted => "loop_turn_started",
            Self::ContextDegraded => "context_degraded",
            Self::LoopGuardTriggered => "loop_guard_triggered",
            Self::TokenBudgetUpdated => "token_budget_updated",
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::HookDecision => "hook_decision",
        }
    }
}

impl std::fmt::Display for EaaspEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Telemetry Event Builder ──

/// Builder for constructing standardized EAASP TelemetryEvent instances.
pub struct TelemetryEventBuilder {
    session_id: String,
    runtime_id: String,
    user_id: Option<String>,
}

impl TelemetryEventBuilder {
    pub fn new(session_id: impl Into<String>, runtime_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            runtime_id: runtime_id.into(),
            user_id: None,
        }
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Build a TelemetryEvent with the given type, payload, and resource usage.
    pub fn build(
        &self,
        event_type: EaaspEventType,
        payload: serde_json::Value,
        resource_usage: ResourceUsage,
    ) -> TelemetryEvent {
        TelemetryEvent {
            session_id: self.session_id.clone(),
            runtime_id: self.runtime_id.clone(),
            user_id: self.user_id.clone(),
            event_type: event_type.as_str().to_string(),
            timestamp: Utc::now(),
            payload,
            resource_usage,
        }
    }

    /// Convenience: build an event with default (zero) resource usage.
    pub fn build_simple(
        &self,
        event_type: EaaspEventType,
        payload: serde_json::Value,
    ) -> TelemetryEvent {
        self.build(event_type, payload, ResourceUsage::default())
    }
}

// ── Telemetry Collector ──

/// Session-level telemetry event collector.
///
/// Accumulates events from grid-engine's internal bus and metering,
/// converting them to EAASP-standardized `TelemetryEvent` format.
pub struct TelemetryCollector {
    runtime_id: String,
}

impl TelemetryCollector {
    pub fn new(runtime_id: impl Into<String>) -> Self {
        Self {
            runtime_id: runtime_id.into(),
        }
    }

    /// Convert a grid-engine `MeteringSnapshot` to an EAASP telemetry event.
    pub fn from_metering(
        &self,
        session_id: &str,
        snapshot: &grid_engine::metering::MeteringSnapshot,
    ) -> TelemetryEvent {
        let builder = TelemetryEventBuilder::new(session_id, &self.runtime_id);
        builder.build(
            EaaspEventType::MeteringSnapshot,
            serde_json::json!({
                "input_tokens": snapshot.input_tokens,
                "output_tokens": snapshot.output_tokens,
                "requests": snapshot.requests,
                "duration_ms": snapshot.duration_ms,
            }),
            ResourceUsage {
                input_tokens: snapshot.input_tokens,
                output_tokens: snapshot.output_tokens,
                compute_ms: snapshot.duration_ms,
            },
        )
    }

    /// Convert a grid-engine `TelemetryEvent` (EventBus) to EAASP format.
    pub fn from_bus_event(
        &self,
        bus_event: &grid_engine::event::TelemetryEvent,
    ) -> TelemetryEvent {
        let session_id = bus_event.session_id();
        let builder = TelemetryEventBuilder::new(session_id, &self.runtime_id);

        match bus_event {
            grid_engine::event::TelemetryEvent::LoopTurnStarted { turn, .. } => {
                builder.build_simple(
                    EaaspEventType::LoopTurnStarted,
                    serde_json::json!({ "turn": turn }),
                )
            }
            grid_engine::event::TelemetryEvent::ToolCallStarted { tool_name, .. } => {
                builder.build_simple(
                    EaaspEventType::ToolCallStarted,
                    serde_json::json!({ "tool_name": tool_name }),
                )
            }
            grid_engine::event::TelemetryEvent::ToolCallCompleted {
                tool_name,
                duration_ms,
                ..
            } => builder.build(
                EaaspEventType::ToolCallCompleted,
                serde_json::json!({
                    "tool_name": tool_name,
                    "duration_ms": duration_ms,
                }),
                ResourceUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    compute_ms: *duration_ms,
                },
            ),
            grid_engine::event::TelemetryEvent::ContextDegraded { level, .. } => {
                builder.build_simple(
                    EaaspEventType::ContextDegraded,
                    serde_json::json!({ "level": level }),
                )
            }
            grid_engine::event::TelemetryEvent::LoopGuardTriggered { reason, .. } => {
                builder.build_simple(
                    EaaspEventType::LoopGuardTriggered,
                    serde_json::json!({ "reason": reason }),
                )
            }
            grid_engine::event::TelemetryEvent::TokenBudgetUpdated {
                used,
                total,
                ratio,
                ..
            } => builder.build_simple(
                EaaspEventType::TokenBudgetUpdated,
                serde_json::json!({
                    "used": used,
                    "total": total,
                    "ratio": ratio,
                }),
            ),
        }
    }

    /// Collect all telemetry for a session: metering + bus events.
    ///
    /// This is the main entry point used by `GridHarness::emit_telemetry`.
    pub async fn collect(
        &self,
        session_id: &str,
        runtime: &grid_engine::AgentRuntime,
    ) -> Vec<TelemetryEvent> {
        let mut events = Vec::new();

        // 1. Metering snapshot
        let snapshot = runtime.metering();
        events.push(self.from_metering(session_id, &snapshot));

        // 2. EventBus recent events
        if let Some(bus) = runtime.event_bus() {
            let bus_events = bus.recent_events(50).await;
            for bus_event in &bus_events {
                events.push(self.from_bus_event(bus_event));
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eaasp_event_type_as_str() {
        assert_eq!(EaaspEventType::MeteringSnapshot.as_str(), "metering_snapshot");
        assert_eq!(EaaspEventType::ToolCallStarted.as_str(), "tool_call_started");
        assert_eq!(EaaspEventType::ToolCallCompleted.as_str(), "tool_call_completed");
        assert_eq!(EaaspEventType::SessionEnd.as_str(), "session_end");
        assert_eq!(EaaspEventType::HookDecision.as_str(), "hook_decision");
    }

    #[test]
    fn eaasp_event_type_display() {
        assert_eq!(format!("{}", EaaspEventType::LoopTurnStarted), "loop_turn_started");
    }

    #[test]
    fn builder_creates_event_with_resource_usage() {
        let builder = TelemetryEventBuilder::new("s-1", "grid-harness");
        let event = builder.build(
            EaaspEventType::MeteringSnapshot,
            serde_json::json!({"tokens": 500}),
            ResourceUsage {
                input_tokens: 300,
                output_tokens: 200,
                compute_ms: 1500,
            },
        );
        assert_eq!(event.session_id, "s-1");
        assert_eq!(event.runtime_id, "grid-harness");
        assert_eq!(event.event_type, "metering_snapshot");
        assert_eq!(event.resource_usage.input_tokens, 300);
        assert_eq!(event.resource_usage.output_tokens, 200);
    }

    #[test]
    fn builder_creates_simple_event() {
        let builder = TelemetryEventBuilder::new("s-2", "test-runtime")
            .with_user_id("user-1");
        let event = builder.build_simple(
            EaaspEventType::SessionStart,
            serde_json::json!({}),
        );
        assert_eq!(event.user_id, Some("user-1".into()));
        assert_eq!(event.event_type, "session_start");
        assert_eq!(event.resource_usage.input_tokens, 0);
    }

    #[test]
    fn collector_from_bus_event_tool_call_started() {
        let collector = TelemetryCollector::new("grid-harness");
        let bus_event = grid_engine::event::TelemetryEvent::ToolCallStarted {
            session_id: "s-1".into(),
            tool_name: "bash".into(),
        };
        let event = collector.from_bus_event(&bus_event);
        assert_eq!(event.event_type, "tool_call_started");
        assert_eq!(event.session_id, "s-1");
        assert!(event.payload.get("tool_name").is_some());
    }

    #[test]
    fn collector_from_bus_event_tool_call_completed() {
        let collector = TelemetryCollector::new("grid-harness");
        let bus_event = grid_engine::event::TelemetryEvent::ToolCallCompleted {
            session_id: "s-1".into(),
            tool_name: "read_file".into(),
            duration_ms: 42,
        };
        let event = collector.from_bus_event(&bus_event);
        assert_eq!(event.event_type, "tool_call_completed");
        assert_eq!(event.resource_usage.compute_ms, 42);
    }

    #[test]
    fn collector_from_bus_event_context_degraded() {
        let collector = TelemetryCollector::new("rt-1");
        let bus_event = grid_engine::event::TelemetryEvent::ContextDegraded {
            session_id: "s-2".into(),
            level: "warning".into(),
        };
        let event = collector.from_bus_event(&bus_event);
        assert_eq!(event.event_type, "context_degraded");
        assert_eq!(event.payload["level"], "warning");
    }

    #[test]
    fn collector_from_metering_snapshot() {
        let collector = TelemetryCollector::new("grid-harness");
        let snapshot = grid_engine::metering::MeteringSnapshot {
            input_tokens: 1000,
            output_tokens: 500,
            requests: 3,
            errors: 0,
            duration_ms: 2500,
        };
        let event = collector.from_metering("s-1", &snapshot);
        assert_eq!(event.event_type, "metering_snapshot");
        assert_eq!(event.resource_usage.input_tokens, 1000);
        assert_eq!(event.resource_usage.output_tokens, 500);
        assert_eq!(event.resource_usage.compute_ms, 2500);
        assert_eq!(event.payload["requests"], 3);
    }

    #[test]
    fn eaasp_event_type_serialization() {
        let json = serde_json::to_string(&EaaspEventType::ToolCallCompleted).unwrap();
        assert!(json.contains("ToolCallCompleted"));
        let restored: EaaspEventType = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, EaaspEventType::ToolCallCompleted);
    }
}
