use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::ModelInfo;

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

/// Multi-model comparison configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiModelConfig {
    /// Models to evaluate (each with its own engine config + metadata)
    pub models: Vec<ModelEntry>,
    /// Shared evaluation settings
    pub concurrency: usize,
    pub timeout_secs: u64,
    pub record_traces: bool,
    pub output_dir: PathBuf,
    /// Fall back to MockProvider if API key is missing
    pub fallback_to_mock: bool,
}

/// A single model entry in multi-model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub engine: EngineConfig,
    pub info: ModelInfo,
}

impl Default for MultiModelConfig {
    fn default() -> Self {
        Self {
            models: vec![],
            concurrency: 1,
            timeout_secs: 120,
            record_traces: false,
            output_dir: PathBuf::from("eval_output"),
            fallback_to_mock: true,
        }
    }
}

impl MultiModelConfig {
    /// Convert a single model entry into an EvalConfig for the runner.
    pub fn to_eval_config(&self, entry: &ModelEntry) -> EvalConfig {
        EvalConfig {
            target: EvalTarget::Engine(entry.engine.clone()),
            concurrency: self.concurrency,
            timeout_secs: self.timeout_secs,
            record_traces: self.record_traces,
            output_dir: self.output_dir.clone(),
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
