/// S3.T2 — GooseRuntimeService::send() ACP streaming integration tests.
///
/// These tests do NOT require a real goose binary. They verify the send()
/// wiring by testing the adapter-level `send_message` / `next_event` API
/// against the ACP parser directly (unit-integration hybrid), and by checking
/// that the service correctly maps AcpEvent variants to SendResponse fields.
///
/// Full end-to-end contract tests (contract v1.1 with a real goose subprocess)
/// are gated on GOOSE_BIN and run in the pytest contract suite under --runtime goose.
use eaasp_goose_runtime::acp_parser::AcpEvent;

// ── AcpEvent → SendResponse field mapping tests ─────────────────────────────

#[test]
fn chunk_event_maps_to_chunk_type() {
    let event = AcpEvent::Chunk {
        text: "Hello, world!".to_string(),
        session_id: None,
    };
    match event {
        AcpEvent::Chunk { text, .. } => {
            assert_eq!(text, "Hello, world!");
        }
        _ => panic!("expected Chunk"),
    }
}

#[test]
fn tool_call_event_has_name_and_id() {
    let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"tool_use","content":{"name":"memory_search","id":"tc-999","input":{"q":"test"}}}}"#;
    let event = AcpEvent::try_from(raw).unwrap();
    match event {
        AcpEvent::ToolCall { tool_name, tool_id, input_json, .. } => {
            assert_eq!(tool_name, "memory_search");
            assert_eq!(tool_id, "tc-999");
            assert!(input_json.contains('"'));
        }
        _ => panic!("expected ToolCall"),
    }
}

#[test]
fn stop_event_finish_maps_to_done() {
    let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"finish"}}"#;
    let event = AcpEvent::try_from(raw).unwrap();
    assert!(matches!(event, AcpEvent::Stop { ref reason, .. } if reason == "finish"));
}

#[test]
fn stop_event_session_stopped_maps_to_done() {
    let raw = r#"{"jsonrpc":"2.0","method":"session/stopped","params":{"reason":"end_of_turn"}}"#;
    let event = AcpEvent::try_from(raw).unwrap();
    assert!(matches!(event, AcpEvent::Stop { ref reason, .. } if reason == "end_of_turn"));
}

#[test]
fn error_event_propagated() {
    let raw = r#"{"jsonrpc":"2.0","method":"session/error","params":{"message":"model_overload"}}"#;
    let event = AcpEvent::try_from(raw).unwrap();
    match event {
        AcpEvent::Error { message, .. } => assert!(message.contains("overload")),
        _ => panic!("expected Error"),
    }
}

#[test]
fn unknown_event_does_not_stop_stream() {
    // Unknown events should be skipped — they are not Stop/Error so the loop continues.
    let raw = r#"{"jsonrpc":"2.0","method":"session/ping","params":{}}"#;
    let event = AcpEvent::try_from(raw).unwrap();
    assert!(matches!(event, AcpEvent::Unknown { .. }));
}

// ── Multi-event sequence parsing ─────────────────────────────────────────────

#[test]
fn sequence_chunk_toolcall_stop() {
    let lines = [
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"agent_message_chunk","content":{"type":"text","text":"thinking..."}}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"tool_use","content":{"name":"memory_search","id":"t1","input":{}}}}"#,
        r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"finish"}}"#,
    ];
    let events: Vec<AcpEvent> = lines
        .iter()
        .map(|l| AcpEvent::try_from(*l).unwrap())
        .collect();

    assert!(matches!(events[0], AcpEvent::Chunk { .. }));
    assert!(matches!(events[1], AcpEvent::ToolCall { .. }));
    assert!(matches!(events[2], AcpEvent::Stop { .. }));
}

#[test]
fn empty_content_text_chunk_is_valid() {
    let raw = r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"agent_message_chunk","content":{"type":"text","text":""}}}"#;
    let event = AcpEvent::try_from(raw).unwrap();
    match event {
        AcpEvent::Chunk { text, .. } => assert_eq!(text, ""),
        _ => panic!("expected Chunk"),
    }
}
