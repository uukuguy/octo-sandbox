//! Smart routing — query complexity classification + model routing.
//!
//! Automatically selects the optimal LLM model based on input complexity:
//! - Simple  -> lightweight model (e.g., Haiku)
//! - Medium  -> mid-tier model (e.g., Sonnet)
//! - Complex -> heavyweight model (e.g., Opus)
//!
//! The [`QueryAnalyzer`] is a pure CPU heuristic classifier (<1us).
//! [`SmartRouterProvider`] wraps an inner [`Provider`] and overrides
//! the request model based on the analyzed complexity tier.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use octo_types::{CompletionRequest, CompletionResponse, MessageRole};

use super::traits::{CompletionStream, Provider};

// ---------------------------------------------------------------------------
// Query Complexity
// ---------------------------------------------------------------------------

/// Complexity tier for a given query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryComplexity {
    /// Short text, no tools, simple greetings.
    Simple,
    /// Moderate text/tools, typical conversation.
    Medium,
    /// Long text, many tools, architect-level keywords.
    Complex,
}

impl std::fmt::Display for QueryComplexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple => write!(f, "simple"),
            Self::Medium => write!(f, "medium"),
            Self::Complex => write!(f, "complex"),
        }
    }
}

// ---------------------------------------------------------------------------
// Query Analyzer
// ---------------------------------------------------------------------------

/// Configurable thresholds for the complexity scoring system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerThresholds {
    /// Text length boundary for medium complexity (default 500).
    #[serde(default = "default_text_length_medium")]
    pub text_length_medium: usize,
    /// Text length boundary for complex complexity (default 3000).
    #[serde(default = "default_text_length_complex")]
    pub text_length_complex: usize,
    /// Tool count boundary for complex complexity (default 5).
    #[serde(default = "default_tool_count_complex")]
    pub tool_count_complex: usize,
    /// System prompt length for medium boost (default 2000).
    #[serde(default = "default_system_length_medium")]
    pub system_length_medium: usize,
    /// System prompt length for complex boost (default 5000).
    #[serde(default = "default_system_length_complex")]
    pub system_length_complex: usize,
    /// Max tokens threshold for score boost (default 8192).
    #[serde(default = "default_max_tokens_boost")]
    pub max_tokens_boost: u32,
}

fn default_text_length_medium() -> usize {
    500
}
fn default_text_length_complex() -> usize {
    3000
}
fn default_tool_count_complex() -> usize {
    5
}
fn default_system_length_medium() -> usize {
    2000
}
fn default_system_length_complex() -> usize {
    5000
}
fn default_max_tokens_boost() -> u32 {
    8192
}

impl Default for AnalyzerThresholds {
    fn default() -> Self {
        Self {
            text_length_medium: default_text_length_medium(),
            text_length_complex: default_text_length_complex(),
            tool_count_complex: default_tool_count_complex(),
            system_length_medium: default_system_length_medium(),
            system_length_complex: default_system_length_complex(),
            max_tokens_boost: default_max_tokens_boost(),
        }
    }
}

/// Keywords that signal complex reasoning tasks.
const COMPLEX_KEYWORDS: &[&str] = &[
    "architect",
    "architecture",
    "design",
    "refactor",
    "refactoring",
    "security",
    "audit",
    "optimize",
    "performance",
    "migration",
];

/// Keywords that signal trivial/simple tasks.
const SIMPLE_KEYWORDS: &[&str] = &[
    "hello",
    "hi",
    "thanks",
    "thank you",
    "ok",
    "bye",
    "yes",
    "no",
];

/// Check if `text` contains `word` as a whole word (bounded by non-alphanumeric chars or edges).
fn contains_word(text: &str, word: &str) -> bool {
    for (idx, _) in text.match_indices(word) {
        let before_ok = idx == 0 || !text.as_bytes()[idx - 1].is_ascii_alphanumeric();
        let after_idx = idx + word.len();
        let after_ok =
            after_idx >= text.len() || !text.as_bytes()[after_idx].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Pure CPU heuristic classifier for query complexity.
///
/// Scoring system (each dimension contributes 0-2 points):
/// - Input text length: <medium=0, medium..complex=1, >complex=2
/// - Conversation turns: 1-2=0, 3-8=1, >8=2
/// - Tool count: 0=0, 1..=complex=1, >complex=2
/// - System prompt length: <medium=0, medium..complex=1, >complex=2
/// - Keyword signals in last user message: complex keywords=+2, simple keywords=-1
/// - max_tokens: >threshold=+1
///
/// Total: <=1 -> Simple, 2-4 -> Medium, >=5 -> Complex
pub struct QueryAnalyzer {
    thresholds: AnalyzerThresholds,
}

impl QueryAnalyzer {
    /// Create an analyzer with the given thresholds.
    pub fn new(thresholds: AnalyzerThresholds) -> Self {
        Self { thresholds }
    }

    /// Create an analyzer with default thresholds.
    pub fn with_defaults() -> Self {
        Self::new(AnalyzerThresholds::default())
    }

    /// Return a reference to the configured thresholds.
    pub fn thresholds(&self) -> &AnalyzerThresholds {
        &self.thresholds
    }

    /// Analyze a [`CompletionRequest`] and return its complexity tier.
    pub fn analyze(&self, request: &CompletionRequest) -> QueryComplexity {
        let score = self.score(request);
        if score <= 1 {
            QueryComplexity::Simple
        } else if score <= 4 {
            QueryComplexity::Medium
        } else {
            QueryComplexity::Complex
        }
    }

    /// Compute the raw complexity score (exposed for testing).
    pub fn score(&self, request: &CompletionRequest) -> i32 {
        let t = &self.thresholds;
        let mut score: i32 = 0;

        // 1. Total user text length
        let total_text_len: usize = request
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.text_content().len())
            .sum();

        if total_text_len >= t.text_length_complex {
            score += 2;
        } else if total_text_len >= t.text_length_medium {
            score += 1;
        }

        // 2. Conversation turns (message count)
        let turn_count = request.messages.len();
        if turn_count > 8 {
            score += 2;
        } else if turn_count >= 3 {
            score += 1;
        }

        // 3. Tool count
        let tool_count = request.tools.len();
        if tool_count > t.tool_count_complex {
            score += 2;
        } else if tool_count >= 1 {
            score += 1;
        }

        // 4. System prompt length
        let system_len = request.system.as_ref().map(|s| s.len()).unwrap_or(0);
        if system_len > t.system_length_complex {
            score += 2;
        } else if system_len > t.system_length_medium {
            score += 1;
        }

        // 5. Keyword signals in the last user message
        if let Some(last_user) = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
        {
            let text = last_user.text_content().to_lowercase();
            let has_complex = COMPLEX_KEYWORDS.iter().any(|kw| contains_word(&text, kw));
            let has_simple = SIMPLE_KEYWORDS.iter().any(|kw| contains_word(&text, kw));

            if has_complex {
                score += 2;
            }
            if has_simple {
                score -= 1;
            }
        }

        // 6. max_tokens boost
        if request.max_tokens > t.max_tokens_boost {
            score += 1;
        }

        score
    }
}

// ---------------------------------------------------------------------------
// Smart Router Provider
// ---------------------------------------------------------------------------

/// Provider decorator that overrides the model based on query complexity.
///
/// Wraps an inner [`Provider`] and uses a [`QueryAnalyzer`] to determine
/// the complexity tier, then maps it to the corresponding model name.
pub struct SmartRouterProvider {
    inner: Box<dyn Provider>,
    analyzer: QueryAnalyzer,
    /// Maps complexity tier to model name.
    tier_models: HashMap<QueryComplexity, String>,
    /// Fallback model when the tier is not in the map.
    default_model: String,
}

impl SmartRouterProvider {
    /// Create a new smart router wrapping the given provider.
    pub fn new(
        inner: Box<dyn Provider>,
        analyzer: QueryAnalyzer,
        tier_models: HashMap<QueryComplexity, String>,
        default_model: String,
    ) -> Self {
        Self {
            inner,
            analyzer,
            tier_models,
            default_model,
        }
    }

    /// Determine the model to use based on request complexity.
    fn select_model(&self, request: &CompletionRequest) -> String {
        let complexity = self.analyzer.analyze(request);
        let model = self
            .tier_models
            .get(&complexity)
            .cloned()
            .unwrap_or_else(|| self.default_model.clone());
        debug!(
            %complexity,
            %model,
            "SmartRouter selected model"
        );
        model
    }
}

#[async_trait]
impl Provider for SmartRouterProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    async fn complete(&self, mut request: CompletionRequest) -> Result<CompletionResponse> {
        request.model = self.select_model(&request);
        self.inner.complete(request).await
    }

    async fn stream(&self, mut request: CompletionRequest) -> Result<CompletionStream> {
        request.model = self.select_model(&request);
        self.inner.stream(request).await
    }
}

// ---------------------------------------------------------------------------
// Configuration types (for server config integration)
// ---------------------------------------------------------------------------

/// Configuration for smart routing in config.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartRoutingConfig {
    /// Enable smart routing (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Default tier when no specific tier matches (default: "medium").
    #[serde(default = "default_tier")]
    pub default_tier: String,
    /// Per-tier model configuration.
    #[serde(default)]
    pub tiers: HashMap<String, TierConfig>,
    /// Optional threshold overrides.
    #[serde(default)]
    pub thresholds: Option<AnalyzerThresholds>,
}

fn default_tier() -> String {
    "medium".to_string()
}

impl Default for SmartRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_tier: default_tier(),
            tiers: HashMap::new(),
            thresholds: None,
        }
    }
}

/// Model configuration for a single tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Model name to use for this tier.
    pub model: String,
}

impl SmartRoutingConfig {
    /// Build a [`SmartRouterProvider`] wrapping the given inner provider.
    ///
    /// Returns `None` if smart routing is disabled.
    pub fn build_provider(&self, inner: Box<dyn Provider>) -> Option<Box<dyn Provider>> {
        if !self.enabled {
            return None;
        }

        let thresholds = self.thresholds.clone().unwrap_or_default();
        let analyzer = QueryAnalyzer::new(thresholds);

        let mut tier_models = HashMap::new();
        for (name, cfg) in &self.tiers {
            let complexity = match name.as_str() {
                "simple" => QueryComplexity::Simple,
                "medium" => QueryComplexity::Medium,
                "complex" => QueryComplexity::Complex,
                _ => continue,
            };
            tier_models.insert(complexity, cfg.model.clone());
        }

        // Resolve the default model from the default tier.
        let default_model = self
            .tiers
            .get(&self.default_tier)
            .map(|c| c.model.clone())
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

        Some(Box::new(SmartRouterProvider::new(
            inner,
            analyzer,
            tier_models,
            default_model,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::ChatMessage;

    fn make_request() -> CompletionRequest {
        CompletionRequest::default()
    }

    #[test]
    fn test_default_thresholds() {
        let t = AnalyzerThresholds::default();
        assert_eq!(t.text_length_medium, 500);
        assert_eq!(t.text_length_complex, 3000);
        assert_eq!(t.tool_count_complex, 5);
        assert_eq!(t.system_length_medium, 2000);
        assert_eq!(t.system_length_complex, 5000);
        assert_eq!(t.max_tokens_boost, 8192);
    }

    #[test]
    fn test_empty_request_is_simple() {
        let analyzer = QueryAnalyzer::with_defaults();
        let req = make_request();
        assert_eq!(analyzer.analyze(&req), QueryComplexity::Simple);
    }

    #[test]
    fn test_simple_hello() {
        let analyzer = QueryAnalyzer::with_defaults();
        let mut req = make_request();
        req.messages.push(ChatMessage::user("hello"));
        assert_eq!(analyzer.analyze(&req), QueryComplexity::Simple);
    }

    #[test]
    fn test_complexity_display() {
        assert_eq!(format!("{}", QueryComplexity::Simple), "simple");
        assert_eq!(format!("{}", QueryComplexity::Medium), "medium");
        assert_eq!(format!("{}", QueryComplexity::Complex), "complex");
    }
}
