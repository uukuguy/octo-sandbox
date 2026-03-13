//! Model metadata for multi-model comparison evaluation.

use serde::{Deserialize, Serialize};

/// Model capability tier for cost/performance classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelTier {
    /// T0: Free / open-source models for CI regression
    Free,
    /// T1: Economy models for daily coding
    Economy,
    /// T2: Standard production models
    Standard,
    /// T3: High-performance models for complex reasoning
    HighPerformance,
    /// T4: Flagship models for architecture decisions
    Flagship,
    /// T5: Top-tier models for evaluation baseline
    TopTier,
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelTier::Free => write!(f, "T0-Free"),
            ModelTier::Economy => write!(f, "T1-Economy"),
            ModelTier::Standard => write!(f, "T2-Standard"),
            ModelTier::HighPerformance => write!(f, "T3-HighPerf"),
            ModelTier::Flagship => write!(f, "T4-Flagship"),
            ModelTier::TopTier => write!(f, "T5-TopTier"),
        }
    }
}

/// Model metadata attached to evaluation reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Display name (e.g. "DeepSeek V3.2", "Claude Sonnet 4.6")
    pub name: String,
    /// Model ID for the provider (e.g. "deepseek/deepseek-v3.2")
    pub model_id: String,
    /// Provider name (e.g. "openai", "anthropic", "openrouter")
    pub provider: String,
    /// Capability tier
    pub tier: ModelTier,
    /// Cost per 1M input tokens (USD)
    pub cost_per_1m_input: f64,
    /// Cost per 1M output tokens (USD)
    pub cost_per_1m_output: f64,
}

impl ModelInfo {
    /// Calculate cost in USD for given token counts.
    pub fn estimate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        (input_tokens as f64 / 1_000_000.0) * self.cost_per_1m_input
            + (output_tokens as f64 / 1_000_000.0) * self.cost_per_1m_output
    }
}

impl Default for ModelInfo {
    fn default() -> Self {
        Self {
            name: "mock".into(),
            model_id: "mock".into(),
            provider: "mock".into(),
            tier: ModelTier::Free,
            cost_per_1m_input: 0.0,
            cost_per_1m_output: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info_default() {
        let info = ModelInfo::default();
        assert_eq!(info.name, "mock");
        assert_eq!(info.tier, ModelTier::Free);
    }

    #[test]
    fn test_estimate_cost_zero_for_free() {
        let info = ModelInfo::default();
        assert!((info.estimate_cost(10000, 5000) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_estimate_cost_calculation() {
        let info = ModelInfo {
            name: "Claude Sonnet".into(),
            model_id: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Flagship,
            cost_per_1m_input: 3.0,
            cost_per_1m_output: 15.0,
        };
        // 1M input + 500K output = $3 + $7.5 = $10.5
        let cost = info.estimate_cost(1_000_000, 500_000);
        assert!((cost - 10.5).abs() < 0.01);
    }

    #[test]
    fn test_model_tier_display() {
        assert_eq!(ModelTier::Free.to_string(), "T0-Free");
        assert_eq!(ModelTier::Economy.to_string(), "T1-Economy");
        assert_eq!(ModelTier::Flagship.to_string(), "T4-Flagship");
    }

    #[test]
    fn test_model_info_serde_roundtrip() {
        let info = ModelInfo {
            name: "Test".into(),
            model_id: "test-model".into(),
            provider: "openrouter".into(),
            tier: ModelTier::Standard,
            cost_per_1m_input: 0.12,
            cost_per_1m_output: 0.18,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.tier, ModelTier::Standard);
    }
}
