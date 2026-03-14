use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::{ModelInfo, ModelTier};

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
    /// Track B-1: CLI subprocess — runs `octo ask --output json` as a child process
    Cli(CliConfig),
    /// Track B-2: HTTP calls to running octo-server instance
    Server(ServerConfig),
}

/// CLI subprocess evaluation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Path to the octo-cli binary (default: "target/debug/octo-cli")
    pub binary_path: PathBuf,
    /// Extra CLI arguments passed before the prompt
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Per-subprocess timeout in seconds (default: 120)
    pub timeout_secs: u64,
    /// Environment variables injected into the subprocess
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("target/debug/octo-cli"),
            extra_args: vec![],
            timeout_secs: 120,
            env: HashMap::new(),
        }
    }
}

/// Server HTTP evaluation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Base URL of the running octo-server (default: "http://127.0.0.1:3001")
    pub base_url: String,
    /// Per-request timeout in seconds (default: 120)
    pub timeout_secs: u64,
    /// Optional API key for authentication
    #[serde(default)]
    pub api_key: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:3001".into(),
            timeout_secs: 120,
            api_key: None,
        }
    }
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

// ─── TOML configuration file support ───────────────────────────────────────

/// TOML configuration file structure (all fields optional for partial override)
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EvalTomlConfig {
    #[serde(default)]
    pub default: TomlDefaultSection,
    #[serde(default)]
    pub models: Vec<TomlModelEntry>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TomlDefaultSection {
    pub timeout_secs: Option<u64>,
    pub concurrency: Option<usize>,
    pub record_traces: Option<bool>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TomlModelEntry {
    pub name: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub cost_per_1m_input: Option<f64>,
    #[serde(default)]
    pub cost_per_1m_output: Option<f64>,
}

impl EvalTomlConfig {
    /// Load from a TOML file path. Returns None if file doesn't exist.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(Some(config))
    }

    /// Apply TOML defaults to an EvalConfig (lower priority than CLI overrides).
    pub fn apply_to_eval_config(&self, config: &mut EvalConfig) {
        if let Some(t) = self.default.timeout_secs {
            config.timeout_secs = t;
        }
        if let Some(c) = self.default.concurrency {
            config.concurrency = c;
        }
        if let Some(r) = self.default.record_traces {
            config.record_traces = r;
        }
        if let Some(ref d) = self.default.output_dir {
            config.output_dir = PathBuf::from(d);
        }
    }

    /// Convert TOML model entries to ModelEntry list (resolving API keys from env).
    pub fn to_model_entries(&self) -> Vec<ModelEntry> {
        self.models
            .iter()
            .map(|m| {
                let tier = m
                    .tier
                    .as_deref()
                    .and_then(|t| match t.to_lowercase().as_str() {
                        "free" | "t0" => Some(ModelTier::Free),
                        "economy" | "t1" => Some(ModelTier::Economy),
                        "standard" | "t2" => Some(ModelTier::Standard),
                        "high" | "t3" => Some(ModelTier::HighPerformance),
                        "flagship" | "t4" => Some(ModelTier::Flagship),
                        "top" | "t5" => Some(ModelTier::TopTier),
                        _ => None,
                    })
                    .unwrap_or(ModelTier::Standard);

                // Resolve API key from environment
                let api_key = std::env::var("OPENAI_API_KEY")
                    .ok()
                    .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

                ModelEntry {
                    engine: EngineConfig {
                        provider_name: m.provider.clone(),
                        api_key,
                        base_url: m.base_url.clone(),
                        model: m.model.clone(),
                        ..EngineConfig::default()
                    },
                    info: ModelInfo {
                        name: m.name.clone(),
                        model_id: m.model.clone(),
                        provider: m.provider.clone(),
                        tier,
                        cost_per_1m_input: m.cost_per_1m_input.unwrap_or(0.0),
                        cost_per_1m_output: m.cost_per_1m_output.unwrap_or(0.0),
                    },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_config_parse() {
        let toml_str = r#"
[default]
timeout_secs = 60
concurrency = 4
record_traces = true
output_dir = "my_output"

[[models]]
name = "TestModel"
provider = "openai"
model = "gpt-4o"
tier = "flagship"
base_url = "https://api.openai.com/v1"
"#;
        let config: EvalTomlConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default.timeout_secs, Some(60));
        assert_eq!(config.default.concurrency, Some(4));
        assert_eq!(config.default.record_traces, Some(true));
        assert_eq!(config.default.output_dir, Some("my_output".into()));
        assert_eq!(config.models.len(), 1);
        assert_eq!(config.models[0].name, "TestModel");
        assert_eq!(config.models[0].provider, "openai");
        assert_eq!(config.models[0].model, "gpt-4o");
        assert_eq!(config.models[0].tier, Some("flagship".into()));
    }

    #[test]
    fn test_toml_apply_to_eval_config() {
        let toml_str = r#"
[default]
timeout_secs = 30
concurrency = 8
"#;
        let toml_config: EvalTomlConfig = toml::from_str(toml_str).unwrap();
        let mut config = EvalConfig::default();
        toml_config.apply_to_eval_config(&mut config);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.concurrency, 8);
        // Unchanged defaults
        assert!(!config.record_traces);
        assert_eq!(config.output_dir, PathBuf::from("eval_output"));
    }

    #[test]
    fn test_toml_load_nonexistent_returns_none() {
        let result = EvalTomlConfig::load(std::path::Path::new("/tmp/nonexistent_eval.toml"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_toml_to_model_entries() {
        let toml_str = r#"
[[models]]
name = "ModelA"
provider = "openai"
model = "gpt-4o"
tier = "flagship"
cost_per_1m_input = 2.5
cost_per_1m_output = 10.0

[[models]]
name = "ModelB"
provider = "anthropic"
model = "claude-3"
"#;
        let config: EvalTomlConfig = toml::from_str(toml_str).unwrap();
        let entries = config.to_model_entries();
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].info.name, "ModelA");
        assert_eq!(entries[0].info.tier, ModelTier::Flagship);
        assert!((entries[0].info.cost_per_1m_input - 2.5).abs() < f64::EPSILON);

        assert_eq!(entries[1].info.name, "ModelB");
        assert_eq!(entries[1].info.tier, ModelTier::Standard); // default
        assert!((entries[1].info.cost_per_1m_output - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_toml_empty_config() {
        let toml_str = "";
        let config: EvalTomlConfig = toml::from_str(toml_str).unwrap();
        assert!(config.default.timeout_secs.is_none());
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_toml_tier_parsing_variants() {
        let toml_str = r#"
[[models]]
name = "Free"
provider = "test"
model = "m1"
tier = "t0"

[[models]]
name = "Economy"
provider = "test"
model = "m2"
tier = "economy"

[[models]]
name = "High"
provider = "test"
model = "m3"
tier = "T3"
"#;
        let config: EvalTomlConfig = toml::from_str(toml_str).unwrap();
        let entries = config.to_model_entries();
        assert_eq!(entries[0].info.tier, ModelTier::Free);
        assert_eq!(entries[1].info.tier, ModelTier::Economy);
        assert_eq!(entries[2].info.tier, ModelTier::HighPerformance);
    }
}
