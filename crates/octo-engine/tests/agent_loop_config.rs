use octo_engine::agent::AgentLoopConfig;

#[test]
fn test_agent_loop_config_builder() {
    let config = AgentLoopConfig::builder()
        .max_iterations(30)
        .max_concurrent_tools(8)
        .tool_timeout_secs(120)
        .build();
    assert_eq!(config.max_iterations, 30);
    assert_eq!(config.max_concurrent_tools, 8);
    assert_eq!(config.tool_timeout_secs, 120);
}

#[test]
fn test_agent_loop_config_defaults() {
    let config = AgentLoopConfig::default();
    assert_eq!(config.max_iterations, 30);
    assert_eq!(config.max_concurrent_tools, 8);
    assert_eq!(config.tool_timeout_secs, 120);
    assert!(config.force_text_at_last);
    assert_eq!(config.max_tokens_continuation, 3);
}

#[test]
fn test_agent_loop_config_builder_custom_values() {
    let config = AgentLoopConfig::builder()
        .max_iterations(50)
        .max_concurrent_tools(4)
        .tool_timeout_secs(60)
        .force_text_at_last(false)
        .max_tokens_continuation(5)
        .build();
    assert_eq!(config.max_iterations, 50);
    assert_eq!(config.max_concurrent_tools, 4);
    assert_eq!(config.tool_timeout_secs, 60);
    assert!(!config.force_text_at_last);
    assert_eq!(config.max_tokens_continuation, 5);
}
