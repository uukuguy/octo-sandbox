//! Agent Configuration - Configuration for agent loop behavior

use serde::{Deserialize, Serialize};

/// Agent loop configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Maximum conversation rounds (0 = unlimited)
    pub max_rounds: u32,
    /// Enable parallel tool execution
    pub enable_parallel: bool,
    /// Maximum parallel tools at once
    pub max_parallel_tools: u8,
    /// Tool execution timeout in seconds
    pub tool_timeout_secs: u64,
    /// Enable typing indicator signal
    pub enable_typing_signal: bool,
    /// Enable streaming tool execution (safe tools start during API stream).
    /// Default: false (gradual rollout).
    #[serde(default)]
    pub enable_streaming_execution: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_rounds: 50,
            enable_parallel: false,
            max_parallel_tools: 8,
            tool_timeout_secs: 60,
            enable_typing_signal: true,
            enable_streaming_execution: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.max_rounds, 50);
        assert!(!config.enable_parallel);
        assert_eq!(config.max_parallel_tools, 8);
        assert_eq!(config.tool_timeout_secs, 60);
        assert!(config.enable_typing_signal);
        assert!(!config.enable_streaming_execution);
    }

    #[test]
    fn test_config_serialization() {
        let config = AgentConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("max_rounds"));
        assert!(json.contains("enable_streaming_execution"));

        let decoded: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_rounds, 50);
        assert!(!decoded.enable_streaming_execution);
    }
}
