//! Tests for SmartRouterProvider and QueryAnalyzer.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures_util::Stream;
use octo_engine::providers::{
    Provider, QueryAnalyzer, QueryComplexity, SmartRouterProvider, SmartRoutingConfig,
};
use octo_types::{
    ChatMessage, CompletionRequest, CompletionResponse, ContentBlock, StreamEvent, StopReason,
    TokenUsage, ToolSpec,
};

// ---------------------------------------------------------------------------
// Mock Provider — records the model name it receives
// ---------------------------------------------------------------------------

struct MockProvider {
    last_model: Arc<Mutex<String>>,
}

impl MockProvider {
    fn new() -> (Self, Arc<Mutex<String>>) {
        let model = Arc::new(Mutex::new(String::new()));
        (
            Self {
                last_model: Arc::clone(&model),
            },
            model,
        )
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        *self.last_model.lock().unwrap() = request.model.clone();
        Ok(CompletionResponse {
            id: "mock-id".to_string(),
            content: vec![ContentBlock::Text {
                text: "ok".to_string(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
            },
        })
    }

    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        Err(anyhow::anyhow!("stream not implemented for mock"))
    }
}

// ---------------------------------------------------------------------------
// Helper: build a default analyzer
// ---------------------------------------------------------------------------

fn default_analyzer() -> QueryAnalyzer {
    QueryAnalyzer::with_defaults()
}

fn make_request() -> CompletionRequest {
    CompletionRequest::default()
}

fn make_tier_models() -> HashMap<QueryComplexity, String> {
    let mut m = HashMap::new();
    m.insert(QueryComplexity::Simple, "claude-haiku".to_string());
    m.insert(QueryComplexity::Medium, "claude-sonnet".to_string());
    m.insert(QueryComplexity::Complex, "claude-opus".to_string());
    m
}

fn dummy_tool() -> ToolSpec {
    ToolSpec {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({}),
    }
}

// ---------------------------------------------------------------------------
// 1. Simple query classification
// ---------------------------------------------------------------------------

#[test]
fn test_simple_query_classification() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    req.messages.push(ChatMessage::user("What is 2+2?"));
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Simple);
}

// ---------------------------------------------------------------------------
// 2. Medium query classification
// ---------------------------------------------------------------------------

#[test]
fn test_medium_query_classification() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    // Moderate text (~600 chars -> score +1 for text)
    let text = "a".repeat(600);
    req.messages.push(ChatMessage::user(&text));
    // 2 tools -> score +1 for tools
    req.tools.push(dummy_tool());
    req.tools.push(dummy_tool());
    // Total: 1 + 1 = 2 -> Medium
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Medium);
}

// ---------------------------------------------------------------------------
// 3. Complex query classification
// ---------------------------------------------------------------------------

#[test]
fn test_complex_query_classification() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    // Long text -> +2
    let text = "a".repeat(4000);
    req.messages.push(ChatMessage::user(&text));
    // Many tools -> +2
    for _ in 0..6 {
        req.tools.push(dummy_tool());
    }
    // "architect" keyword -> +2
    req.messages
        .push(ChatMessage::user("Please architect a new system"));
    // Total: 2 + 2 + 2 = 6 (at least) -> Complex
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Complex);
}

// ---------------------------------------------------------------------------
// 4. Keyword boost — "refactor" bumps score
// ---------------------------------------------------------------------------

#[test]
fn test_keyword_boost_complex() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    // Moderate text -> +1
    let text = "a".repeat(600);
    req.messages.push(ChatMessage::user(&text));
    // 3 tools -> +1
    for _ in 0..3 {
        req.tools.push(dummy_tool());
    }
    // Many turns (>8) -> +2
    for i in 0..5 {
        req.messages
            .push(ChatMessage::assistant(&format!("Response {}", i)));
        req.messages
            .push(ChatMessage::user(&format!("Follow-up {}", i)));
    }
    // The last user message must contain the keyword for it to count.
    req.messages
        .push(ChatMessage::user("Please refactor this module"));
    // Score: text ~1 + tools 1 + turns 2 + keyword 2 = 6 -> Complex
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Complex);
}

// ---------------------------------------------------------------------------
// 5. Keyword reduce — "hello" stays Simple
// ---------------------------------------------------------------------------

#[test]
fn test_keyword_reduce_simple() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    req.messages.push(ChatMessage::user("hello"));
    // "hello" -> -1, short text -> 0, no tools -> 0
    // Total: -1 -> Simple (<=1)
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Simple);
}

// ---------------------------------------------------------------------------
// 6. High max_tokens boost
// ---------------------------------------------------------------------------

#[test]
fn test_high_max_tokens_boost() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    // Moderate text -> +1
    let text = "a".repeat(600);
    req.messages.push(ChatMessage::user(&text));
    // max_tokens > 8192 -> +1
    req.max_tokens = 16384;
    // Total: 1 + 1 = 2 -> Medium
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Medium);

    // Without the boost it would be Simple
    let mut req2 = make_request();
    req2.messages.push(ChatMessage::user(&"a".repeat(600)));
    req2.max_tokens = 4096;
    // Score: 1 (text only) -> Simple
    assert_eq!(analyzer.analyze(&req2), QueryComplexity::Simple);
}

// ---------------------------------------------------------------------------
// 7. Long system prompt boost
// ---------------------------------------------------------------------------

#[test]
fn test_long_system_prompt_boost() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    req.messages.push(ChatMessage::user("Do something"));
    // System prompt > 5000 -> +2
    req.system = Some("x".repeat(6000));
    let score = analyzer.score(&req);
    // text(0) + turns(0) + tools(0) + system(2) + keywords(0) + max_tokens(0) = 2
    assert_eq!(score, 2, "Expected score 2, got {}", score);
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Medium);

    // System prompt > 5000 + moderate text -> +2 + 1 = 3 -> Medium
    let mut req2 = make_request();
    req2.messages.push(ChatMessage::user(&"a".repeat(600)));
    req2.system = Some("x".repeat(6000));
    assert_eq!(analyzer.analyze(&req2), QueryComplexity::Medium);
}

// ---------------------------------------------------------------------------
// 8. Many turns boost
// ---------------------------------------------------------------------------

#[test]
fn test_many_turns_boost() {
    let analyzer = default_analyzer();
    let mut req = make_request();
    // 12 messages -> +2 for turns
    for i in 0..6 {
        req.messages.push(ChatMessage::user(&format!("Question {}", i)));
        req.messages
            .push(ChatMessage::assistant(&format!("Answer {}", i)));
    }
    // Total: 2 (turns) -> Medium
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Medium);
}

// ---------------------------------------------------------------------------
// 9. SmartRouterProvider changes request.model
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_model_override_applied() {
    let (mock, last_model) = MockProvider::new();
    let tier_models = make_tier_models();
    let router = SmartRouterProvider::new(
        Box::new(mock),
        default_analyzer(),
        tier_models,
        "claude-sonnet".to_string(),
    );

    // Simple request -> should route to haiku
    let mut req = make_request();
    req.messages.push(ChatMessage::user("hello"));
    let _ = router.complete(req).await.unwrap();
    assert_eq!(*last_model.lock().unwrap(), "claude-haiku");

    // Complex request -> should route to opus
    let mut req2 = make_request();
    req2.messages
        .push(ChatMessage::user(&"a".repeat(4000)));
    for _ in 0..6 {
        req2.tools.push(dummy_tool());
    }
    req2.messages
        .push(ChatMessage::user("Please architect the new system"));
    let _ = router.complete(req2).await.unwrap();
    assert_eq!(*last_model.lock().unwrap(), "claude-opus");
}

// ---------------------------------------------------------------------------
// 10. Fallback on missing tier
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fallback_on_missing_tier() {
    let (mock, last_model) = MockProvider::new();
    // Only configure Simple tier — Medium and Complex will use default
    let mut tier_models = HashMap::new();
    tier_models.insert(QueryComplexity::Simple, "claude-haiku".to_string());

    let router = SmartRouterProvider::new(
        Box::new(mock),
        default_analyzer(),
        tier_models,
        "fallback-model".to_string(),
    );

    // Medium request -> falls back to default
    let mut req = make_request();
    let text = "a".repeat(600);
    req.messages.push(ChatMessage::user(&text));
    req.tools.push(dummy_tool());
    // text 1 + tool 1 = 2 -> Medium
    let _ = router.complete(req).await.unwrap();
    assert_eq!(*last_model.lock().unwrap(), "fallback-model");
}

// ---------------------------------------------------------------------------
// 11. Analyzer default thresholds
// ---------------------------------------------------------------------------

#[test]
fn test_analyzer_default_thresholds() {
    let analyzer = default_analyzer();
    let t = analyzer.thresholds();
    assert_eq!(t.text_length_medium, 500);
    assert_eq!(t.text_length_complex, 3000);
    assert_eq!(t.tool_count_complex, 5);
    assert_eq!(t.system_length_medium, 2000);
    assert_eq!(t.system_length_complex, 5000);
    assert_eq!(t.max_tokens_boost, 8192);
}

// ---------------------------------------------------------------------------
// 12. Config deserialization
// ---------------------------------------------------------------------------

#[test]
fn test_config_deserialization() {
    let yaml = r#"
enabled: true
default_tier: medium
tiers:
  simple:
    model: claude-haiku
  medium:
    model: claude-sonnet
  complex:
    model: claude-opus
thresholds:
  text_length_medium: 300
  text_length_complex: 2000
  tool_count_complex: 3
"#;
    let config: SmartRoutingConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(config.enabled);
    assert_eq!(config.default_tier, "medium");
    assert_eq!(config.tiers.len(), 3);
    assert_eq!(config.tiers["simple"].model, "claude-haiku");
    assert_eq!(config.tiers["complex"].model, "claude-opus");
    let thresholds = config.thresholds.unwrap();
    assert_eq!(thresholds.text_length_medium, 300);
    assert_eq!(thresholds.text_length_complex, 2000);
    assert_eq!(thresholds.tool_count_complex, 3);
    // Defaults for unspecified fields
    assert_eq!(thresholds.system_length_medium, 2000);
    assert_eq!(thresholds.system_length_complex, 5000);
    assert_eq!(thresholds.max_tokens_boost, 8192);
}

// ---------------------------------------------------------------------------
// 13. Config build_provider disabled
// ---------------------------------------------------------------------------

#[test]
fn test_config_build_provider_disabled() {
    let config = SmartRoutingConfig::default();
    let (mock, _) = MockProvider::new();
    assert!(config.build_provider(Box::new(mock)).is_none());
}

// ---------------------------------------------------------------------------
// 14. Config build_provider enabled
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_config_build_provider_enabled() {
    let yaml = r#"
enabled: true
default_tier: medium
tiers:
  simple:
    model: claude-haiku
  medium:
    model: claude-sonnet
  complex:
    model: claude-opus
"#;
    let config: SmartRoutingConfig = serde_yaml::from_str(yaml).unwrap();
    let (mock, last_model) = MockProvider::new();
    let provider = config.build_provider(Box::new(mock)).unwrap();

    // Simple request -> haiku
    let mut req = make_request();
    req.messages.push(ChatMessage::user("hi"));
    let _ = provider.complete(req).await.unwrap();
    assert_eq!(*last_model.lock().unwrap(), "claude-haiku");
}

// ---------------------------------------------------------------------------
// 15. Score boundary check
// ---------------------------------------------------------------------------

#[test]
fn test_score_boundary() {
    let analyzer = default_analyzer();

    // Score exactly 1 -> Simple
    let mut req = make_request();
    req.messages.push(ChatMessage::user(&"a".repeat(600))); // +1
    assert_eq!(analyzer.score(&req), 1);
    assert_eq!(analyzer.analyze(&req), QueryComplexity::Simple);

    // Score exactly 5 -> Complex
    let mut req2 = make_request();
    req2.messages.push(ChatMessage::user(&"a".repeat(600))); // +1 text
    req2.tools.push(dummy_tool()); // +1 tools
    req2.system = Some("x".repeat(6000)); // +2 system
    req2.max_tokens = 16384; // +1 max_tokens
    // Total: 1 + 1 + 2 + 1 = 5 -> Complex
    assert_eq!(analyzer.score(&req2), 5);
    assert_eq!(analyzer.analyze(&req2), QueryComplexity::Complex);
}
