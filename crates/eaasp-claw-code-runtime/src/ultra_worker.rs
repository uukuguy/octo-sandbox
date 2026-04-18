//! UltraWorker event types for claw-code subprocess communication.
//!
//! claw-code emits newline-delimited JSON events over stdout via its
//! UltraWorkers channel. This module parses those events into typed
//! variants for the service layer.

use serde::Deserialize;

/// Parsed event from claw-code's UltraWorkers stdout stream.
#[derive(Debug, Clone)]
pub enum UltraWorkerEvent {
    /// Text chunk from the assistant.
    Chunk { text: String, session_id: String },
    /// Tool call requested by the model.
    ToolCall {
        tool_name: String,
        tool_id: String,
        input_json: String,
        session_id: String,
    },
    /// Session completed normally.
    Stop { reason: String, session_id: String },
    /// Error from the worker.
    Error { message: String, session_id: String },
    /// Unrecognized event; raw line preserved for logging.
    Unknown { raw: String },
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    session_id: Option<String>,
    text: Option<String>,
    tool_name: Option<String>,
    tool_id: Option<String>,
    input: Option<serde_json::Value>,
    reason: Option<String>,
    message: Option<String>,
}

impl TryFrom<&str> for UltraWorkerEvent {
    type Error = serde_json::Error;

    fn try_from(raw: &str) -> std::result::Result<Self, serde_json::Error> {
        let ev: RawEvent = serde_json::from_str(raw)?;
        let sid = ev.session_id.unwrap_or_default();

        let parsed = match ev.event_type.as_deref() {
            Some("chunk") => UltraWorkerEvent::Chunk {
                text: ev.text.unwrap_or_default(),
                session_id: sid,
            },
            Some("tool_call") => UltraWorkerEvent::ToolCall {
                tool_name: ev.tool_name.unwrap_or_default(),
                tool_id: ev.tool_id.unwrap_or_default(),
                input_json: ev
                    .input
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "{}".to_string()),
                session_id: sid,
            },
            Some("stop") | Some("finish") => UltraWorkerEvent::Stop {
                reason: ev.reason.unwrap_or_else(|| "end_turn".to_string()),
                session_id: sid,
            },
            Some("error") => UltraWorkerEvent::Error {
                message: ev.message.unwrap_or_default(),
                session_id: sid,
            },
            _ => UltraWorkerEvent::Unknown { raw: raw.to_string() },
        };
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chunk() {
        let raw = r#"{"type":"chunk","text":"Hello","session_id":"s1"}"#;
        let ev = UltraWorkerEvent::try_from(raw).unwrap();
        assert!(matches!(ev, UltraWorkerEvent::Chunk { .. }));
    }

    #[test]
    fn parses_stop() {
        let raw = r#"{"type":"stop","reason":"end_turn","session_id":"s1"}"#;
        let ev = UltraWorkerEvent::try_from(raw).unwrap();
        assert!(matches!(ev, UltraWorkerEvent::Stop { .. }));
    }

    #[test]
    fn parses_error() {
        let raw = r#"{"type":"error","message":"oops","session_id":"s1"}"#;
        let ev = UltraWorkerEvent::try_from(raw).unwrap();
        assert!(matches!(ev, UltraWorkerEvent::Error { .. }));
    }

    #[test]
    fn unknown_type_gives_unknown_variant() {
        let raw = r#"{"type":"heartbeat","session_id":"s1"}"#;
        let ev = UltraWorkerEvent::try_from(raw).unwrap();
        assert!(matches!(ev, UltraWorkerEvent::Unknown { .. }));
    }

    #[test]
    fn malformed_json_returns_err() {
        let raw = "not json";
        assert!(UltraWorkerEvent::try_from(raw).is_err());
    }
}
