use futures_util::StreamExt;
use octo_engine::agent::{run_agent_loop, AgentEvent, AgentLoopConfig};

#[tokio::test]
async fn test_harness_no_provider_returns_error_then_done() {
    let config = AgentLoopConfig::default();
    let messages = vec![];
    let events: Vec<AgentEvent> = run_agent_loop(config, messages).collect().await;
    // Should get Error (no provider) + Done
    assert!(events.len() >= 2);
    assert!(matches!(events[0], AgentEvent::Error { .. }));
    assert!(matches!(events.last().unwrap(), AgentEvent::Done));
}

#[tokio::test]
async fn test_harness_stream_is_static() {
    let config = AgentLoopConfig::default();
    let stream = run_agent_loop(config, vec![]);
    // Verify stream can be collected (it's 'static)
    let events: Vec<AgentEvent> = stream.collect().await;
    assert!(!events.is_empty());
}
