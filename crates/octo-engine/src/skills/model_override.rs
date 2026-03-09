use serde::{Deserialize, Serialize};

/// Model override specification for a skill.
/// When a skill has a model override, the agent loop should use
/// the specified provider/model instead of the default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillModelOverride {
    /// Provider name (e.g., "anthropic", "openai")
    pub provider: Option<String>,
    /// Model name (e.g., "claude-sonnet-4-5-20250514", "gpt-4o")
    pub model: String,
    /// Optional max tokens override
    pub max_tokens: Option<u32>,
    /// Optional temperature override
    pub temperature: Option<f64>,
}

impl SkillModelOverride {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            provider: None,
            model: model.into(),
            max_tokens: None,
            temperature: None,
        }
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// Resolve which model to use, given a skill's override and the default.
pub fn resolve_model(skill_override: Option<&SkillModelOverride>, default_model: &str) -> String {
    match skill_override {
        Some(ov) => ov.model.clone(),
        None => default_model.to_string(),
    }
}

/// Resolve which provider to use, given a skill's override and the default.
pub fn resolve_provider(
    skill_override: Option<&SkillModelOverride>,
    default_provider: &str,
) -> String {
    match skill_override {
        Some(ov) => ov
            .provider
            .as_deref()
            .unwrap_or(default_provider)
            .to_string(),
        None => default_provider.to_string(),
    }
}
