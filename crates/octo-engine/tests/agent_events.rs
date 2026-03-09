use octo_engine::agent::{AgentEvent, AgentLoopResult, NormalizedStopReason};

#[test]
fn test_agent_event_new_variants() {
    let event = AgentEvent::ContextDegraded {
        level: "warning".into(),
        usage_pct: 85.0,
    };
    assert!(matches!(event, AgentEvent::ContextDegraded { .. }));

    let event = AgentEvent::SecurityBlocked {
        reason: "injection detected".into(),
    };
    assert!(matches!(event, AgentEvent::SecurityBlocked { .. }));

    let event = AgentEvent::IterationStart { round: 1 };
    assert!(matches!(event, AgentEvent::IterationStart { round: 1 }));
}

#[test]
fn test_agent_loop_result_default() {
    let result = AgentLoopResult::default();
    assert_eq!(result.rounds, 0);
    assert_eq!(result.tool_calls, 0);
    assert_eq!(result.stop_reason, NormalizedStopReason::EndTurn);
}

#[test]
fn test_completed_event() {
    let result = AgentLoopResult {
        rounds: 3,
        tool_calls: 5,
        stop_reason: NormalizedStopReason::MaxIterations,
    };
    let event = AgentEvent::Completed(result);
    assert!(matches!(event, AgentEvent::Completed(_)));
}

#[test]
fn test_normalized_stop_reason_variants() {
    assert_eq!(NormalizedStopReason::default(), NormalizedStopReason::EndTurn);
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
    assert_eq!(reasons.len(), 8);
}
