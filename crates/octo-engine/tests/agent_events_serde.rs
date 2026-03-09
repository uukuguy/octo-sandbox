use octo_engine::agent::{AgentEvent, AgentLoopResult, NormalizedStopReason};

#[test]
fn test_all_agent_event_variants_serialize() {
    let events: Vec<AgentEvent> = vec![
        AgentEvent::TextDelta { text: "hi".into() },
        AgentEvent::TextComplete {
            text: "hello".into(),
        },
        AgentEvent::ThinkingDelta { text: "hmm".into() },
        AgentEvent::ThinkingComplete {
            text: "done".into(),
        },
        AgentEvent::ToolStart {
            tool_id: "t1".into(),
            tool_name: "bash".into(),
            input: serde_json::json!({"cmd": "ls"}),
        },
        AgentEvent::ToolResult {
            tool_id: "t1".into(),
            output: "ok".into(),
            success: true,
        },
        AgentEvent::Typing { state: true },
        AgentEvent::Error {
            message: "err".into(),
        },
        AgentEvent::Done,
        AgentEvent::ContextDegraded {
            level: "warn".into(),
            usage_pct: 80.0,
        },
        AgentEvent::MemoryFlushed { facts_count: 5 },
        AgentEvent::ApprovalRequired {
            tool_name: "rm".into(),
        },
        AgentEvent::SecurityBlocked {
            reason: "blocked".into(),
        },
        AgentEvent::IterationStart { round: 0 },
        AgentEvent::IterationEnd { round: 0 },
        AgentEvent::Completed(AgentLoopResult {
            rounds: 3,
            tool_calls: 5,
            stop_reason: NormalizedStopReason::EndTurn,
        }),
    ];

    for event in &events {
        let json = serde_json::to_string(event);
        assert!(json.is_ok(), "Failed to serialize: {:?}", event);
        let json_str = json.unwrap();
        assert!(
            json_str.contains("\"type\""),
            "Missing type tag in: {}",
            json_str
        );
    }
}

#[test]
fn test_agent_loop_result_serialize() {
    let result = AgentLoopResult {
        rounds: 5,
        tool_calls: 10,
        stop_reason: NormalizedStopReason::MaxIterations,
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"rounds\":5"));
    assert!(json.contains("\"tool_calls\":10"));
    assert!(json.contains("MaxIterations"));
}

#[test]
fn test_normalized_stop_reason_serialize_all() {
    let reasons = vec![
        NormalizedStopReason::EndTurn,
        NormalizedStopReason::ToolCall,
        NormalizedStopReason::MaxTokens,
        NormalizedStopReason::MaxIterations,
        NormalizedStopReason::ContextOverflow,
        NormalizedStopReason::SafetyBlocked,
        NormalizedStopReason::Cancelled,
        NormalizedStopReason::Error,
    ];
    for reason in &reasons {
        let json = serde_json::to_string(reason);
        assert!(json.is_ok(), "Failed to serialize: {:?}", reason);
    }
}
