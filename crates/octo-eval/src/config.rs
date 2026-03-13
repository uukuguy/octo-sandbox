use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level evaluation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    pub target: EvalTarget,
    pub concurrency: usize,
    pub timeout_secs: u64,
    pub record_traces: bool,
    pub output_dir: PathBuf,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            target: EvalTarget::Engine(EngineConfig::default()),
            concurrency: 1,
            timeout_secs: 120,
            record_traces: false,
            output_dir: PathBuf::from("eval_output"),
        }
    }
}

/// Evaluation target (which layer to test)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum EvalTarget {
    /// Track A: Direct engine API calls (fastest, supports mock/replay)
    Engine(EngineConfig),
    // Track B-1: CLI subprocess (Phase D)
    // Cli(CliConfig),
    // Track B-2: HTTP calls to server (Phase D)
    // Server(ServerConfig),
}

/// Engine-level evaluation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub provider_name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub max_iterations: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            provider_name: "openai".into(),
            api_key: None,
            base_url: None,
            model: "mock".into(),
            max_tokens: 4096,
            max_iterations: 10,
        }
    }
}
