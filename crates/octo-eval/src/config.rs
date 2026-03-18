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
        // Each model gets its own sub-directory so traces don't overwrite each other.
        let model_slug = entry.info.name.to_lowercase().replace([' ', '/', '.'], "_");
        EvalConfig {
            target: EvalTarget::Engine(entry.engine.clone()),
            concurrency: self.concurrency,
            timeout_secs: self.timeout_secs,
            record_traces: self.record_traces,
            output_dir: self.output_dir.join(&model_slug),
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
    /// Optional path to an agent manifest YAML for persona injection
    #[serde(default)]
    pub agent_manifest: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            provider_name: "openai".into(),
            api_key: std::env::var("OPENAI_API_KEY")
                .ok()
                .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok()),
            base_url: std::env::var("OPENAI_BASE_URL").ok(),
            model: "mock".into(),
            max_tokens: 4096,
            max_iterations: 10,
            agent_manifest: None,
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
    #[serde(default)]
    pub swe_bench: Option<TomlSweBenchSection>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TomlSweBenchSection {
    #[serde(default)]
    pub dataset: Option<String>,
    #[serde(default)]
    pub scorer: Option<String>,
    #[serde(default)]
    pub docker_required: Option<bool>,
    #[serde(default)]
    pub harness: Option<TomlSweBenchHarness>,
    #[serde(default)]
    pub sampling: Option<toml::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TomlSweBenchHarness {
    pub dataset_name: Option<String>,
    pub max_workers: Option<usize>,
    pub cache_level: Option<String>,
    pub namespace: Option<String>,
    pub python_bin: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TomlDefaultSection {
    pub timeout_secs: Option<u64>,
    pub concurrency: Option<usize>,
    pub record_traces: Option<bool>,
    pub output_dir: Option<String>,
    pub max_iterations: Option<u32>,
    /// Path to agent manifest YAML for persona injection
    pub agent_manifest: Option<String>,
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
        let default_max_iterations = self.default.max_iterations;
        let default_agent_manifest = self.default.agent_manifest.clone();
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

                // Resolve base_url: TOML field > OPENAI_BASE_URL env var
                let base_url = m
                    .base_url
                    .clone()
                    .or_else(|| std::env::var("OPENAI_BASE_URL").ok());

                let max_iterations = default_max_iterations
                    .unwrap_or(EngineConfig::default().max_iterations);

                ModelEntry {
                    engine: EngineConfig {
                        provider_name: m.provider.clone(),
                        api_key,
                        base_url,
                        model: m.model.clone(),
                        max_iterations,
                        agent_manifest: default_agent_manifest.clone(),
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

// ─── Benchmark sampling configuration ─────────────────────────────────────

/// Sampling preset for benchmark runs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingPreset {
    Quick,
    Standard,
    Full,
}

impl SamplingPreset {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "quick" => Some(Self::Quick),
            "standard" => Some(Self::Standard),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// Benchmark-specific sampling configuration
#[derive(Debug, Clone)]
pub struct BenchmarkSamplingConfig {
    pub preset: SamplingPreset,
    pub count: usize,
    /// Stratified ratio for GAIA levels [L1, L2, L3]
    pub stratified_ratio: Option<[f64; 3]>,
}

impl BenchmarkSamplingConfig {
    /// Create GAIA sampling config
    pub fn gaia(preset: SamplingPreset) -> Self {
        let count = match preset {
            SamplingPreset::Quick => 10,
            SamplingPreset::Standard => 30,
            SamplingPreset::Full => 165,
        };
        Self {
            preset,
            count,
            stratified_ratio: Some([0.3, 0.5, 0.2]),
        }
    }

    /// Create SWE-bench sampling config
    pub fn swe_bench(preset: SamplingPreset) -> Self {
        let count = match preset {
            SamplingPreset::Quick => 10,
            SamplingPreset::Standard => 30,
            SamplingPreset::Full => 300,
        };
        Self {
            preset,
            count,
            stratified_ratio: None,
        }
    }

    /// Perform stratified sampling on GAIA tasks.
    /// Tasks are grouped by level (extracted from metadata category "gaia-L{n}"),
    /// then sampled according to stratified_ratio.
    pub fn sample_tasks(
        &self,
        tasks: Vec<Box<dyn crate::task::EvalTask>>,
    ) -> Vec<Box<dyn crate::task::EvalTask>> {
        use rand::seq::SliceRandom;

        let total = tasks.len();
        if self.count >= total {
            return tasks;
        }

        let mut rng = rand::rng();

        if let Some(ratio) = self.stratified_ratio {
            // Group tasks by level
            let mut by_level: [Vec<usize>; 3] = [vec![], vec![], vec![]];
            for (i, task) in tasks.iter().enumerate() {
                let cat = task.metadata().category;
                if cat.contains("L1") || cat.contains("l1") {
                    by_level[0].push(i);
                } else if cat.contains("L2") || cat.contains("l2") {
                    by_level[1].push(i);
                } else {
                    by_level[2].push(i);
                }
            }

            let mut selected_indices: Vec<usize> = Vec::new();

            for (level_idx, level_tasks) in by_level.iter_mut().enumerate() {
                level_tasks.shuffle(&mut rng);
                let target = (self.count as f64 * ratio[level_idx]).round() as usize;
                let take = target.min(level_tasks.len());
                selected_indices.extend(&level_tasks[..take]);
            }

            // If we didn't get enough due to rounding or scarcity, fill from remaining
            if selected_indices.len() < self.count {
                let remaining: Vec<usize> = (0..total)
                    .filter(|i| !selected_indices.contains(i))
                    .collect();
                let need = self.count - selected_indices.len();
                let mut remaining_shuffled = remaining;
                remaining_shuffled.shuffle(&mut rng);
                selected_indices
                    .extend(&remaining_shuffled[..need.min(remaining_shuffled.len())]);
            }

            selected_indices.sort();
            selected_indices.truncate(self.count);

            // Convert indices to tasks
            let mut result: Vec<Box<dyn crate::task::EvalTask>> = Vec::new();
            let mut tasks_vec: Vec<Option<Box<dyn crate::task::EvalTask>>> =
                tasks.into_iter().map(Some).collect();
            for idx in selected_indices {
                if let Some(task) = tasks_vec[idx].take() {
                    result.push(task);
                }
            }
            result
        } else {
            // Simple random sampling
            let mut indices: Vec<usize> = (0..total).collect();
            indices.shuffle(&mut rng);
            indices.truncate(self.count);
            indices.sort();

            let mut result: Vec<Box<dyn crate::task::EvalTask>> = Vec::new();
            let mut tasks_vec: Vec<Option<Box<dyn crate::task::EvalTask>>> =
                tasks.into_iter().map(Some).collect();
            for idx in indices {
                if let Some(task) = tasks_vec[idx].take() {
                    result.push(task);
                }
            }
            result
        }
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
    fn test_sampling_preset_parsing() {
        assert_eq!(
            SamplingPreset::from_str("quick"),
            Some(SamplingPreset::Quick)
        );
        assert_eq!(
            SamplingPreset::from_str("Standard"),
            Some(SamplingPreset::Standard)
        );
        assert_eq!(
            SamplingPreset::from_str("FULL"),
            Some(SamplingPreset::Full)
        );
        assert_eq!(SamplingPreset::from_str("invalid"), None);
    }

    #[test]
    fn test_gaia_sampling_config() {
        let config = BenchmarkSamplingConfig::gaia(SamplingPreset::Quick);
        assert_eq!(config.count, 10);
        assert!(config.stratified_ratio.is_some());

        let config = BenchmarkSamplingConfig::gaia(SamplingPreset::Standard);
        assert_eq!(config.count, 30);

        let config = BenchmarkSamplingConfig::gaia(SamplingPreset::Full);
        assert_eq!(config.count, 165);
    }

    #[test]
    fn test_swe_bench_sampling_config() {
        let config = BenchmarkSamplingConfig::swe_bench(SamplingPreset::Quick);
        assert_eq!(config.count, 10);
        assert!(config.stratified_ratio.is_none());

        let config = BenchmarkSamplingConfig::swe_bench(SamplingPreset::Full);
        assert_eq!(config.count, 300);
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

    #[test]
    fn test_toml_agent_manifest_propagation() {
        let toml_str = r#"
[default]
agent_manifest = "config/agents/gaia_solver.yaml"

[[models]]
name = "TestModel"
provider = "openai"
model = "gpt-4o"
"#;
        let config: EvalTomlConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.default.agent_manifest,
            Some("config/agents/gaia_solver.yaml".into())
        );
        let entries = config.to_model_entries();
        assert_eq!(
            entries[0].engine.agent_manifest,
            Some("config/agents/gaia_solver.yaml".into())
        );
    }

    #[test]
    fn test_engine_config_default_no_manifest() {
        let config = EngineConfig::default();
        assert!(config.agent_manifest.is_none());
    }
}
