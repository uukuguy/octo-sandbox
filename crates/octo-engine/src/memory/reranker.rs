//! LLM-based reranking module for hybrid search results.
//!
//! Provides a `RerankStrategy` enum to optionally apply LLM-based
//! relevance scoring on top of initial retrieval (FTS + vector).

use std::sync::Arc;

use octo_types::memory::MemoryEntry;
use octo_types::provider::CompletionRequest;
use octo_types::message::{ChatMessage, ContentBlock};

use crate::providers::traits::Provider;

// ============================================================
// Strategy enum
// ============================================================

/// Reranking strategy applied after hybrid search retrieval.
#[derive(Clone, Default)]
pub enum RerankStrategy {
    /// No reranking — return results in original order (current behaviour).
    #[default]
    None,
    /// LLM-based reranking — score each candidate with a lightweight model.
    Llm(LlmRerankerConfig),
}

impl std::fmt::Debug for RerankStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "RerankStrategy::None"),
            Self::Llm(cfg) => f
                .debug_struct("RerankStrategy::Llm")
                .field("max_candidates", &cfg.max_candidates)
                .field("top_k", &cfg.top_k)
                .field("model", &cfg.model)
                .finish(),
        }
    }
}

// ============================================================
// Config
// ============================================================

/// Configuration for LLM-based reranking.
#[derive(Clone)]
pub struct LlmRerankerConfig {
    /// LLM provider used for scoring.
    pub provider: Arc<dyn Provider>,
    /// Maximum number of candidates to send to the LLM (default 20).
    pub max_candidates: usize,
    /// Number of results to keep after reranking (default 5).
    pub top_k: usize,
    /// Optional model override (uses provider default when `None`).
    pub model: Option<String>,
}

impl std::fmt::Debug for LlmRerankerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmRerankerConfig")
            .field("provider_id", &self.provider.id())
            .field("max_candidates", &self.max_candidates)
            .field("top_k", &self.top_k)
            .field("model", &self.model)
            .finish()
    }
}

// ============================================================
// LlmReranker
// ============================================================

/// Performs LLM-based relevance reranking on memory search candidates.
///
/// The reranker builds a scoring prompt, asks the LLM to assign 0-10
/// relevance scores, then re-orders and truncates the result set.
pub struct LlmReranker {
    config: LlmRerankerConfig,
}

/// Maximum content preview length per candidate in the rerank prompt.
const MAX_PREVIEW_LEN: usize = 200;

impl LlmReranker {
    pub fn new(config: LlmRerankerConfig) -> Self {
        Self { config }
    }

    /// Rerank `candidates` by query relevance using the configured LLM.
    ///
    /// On LLM failure the original order is preserved (graceful degradation).
    pub async fn rerank(
        &self,
        query: &str,
        mut candidates: Vec<MemoryEntry>,
    ) -> Vec<MemoryEntry> {
        candidates.truncate(self.config.max_candidates);

        if candidates.is_empty() {
            return candidates;
        }

        let prompt = self.build_rerank_prompt(query, &candidates);

        match self.call_llm_for_scores(&prompt).await {
            Ok(scores) if scores.len() == candidates.len() => {
                let mut scored: Vec<(MemoryEntry, f32)> = candidates
                    .into_iter()
                    .zip(scores)
                    .collect();
                scored.sort_by(|a, b| {
                    b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                });
                scored.truncate(self.config.top_k);
                scored.into_iter().map(|(entry, _)| entry).collect()
            }
            Ok(scores) => {
                tracing::warn!(
                    expected = candidates.len(),
                    got = scores.len(),
                    "LLM reranker returned wrong number of scores, falling back"
                );
                candidates.truncate(self.config.top_k);
                candidates
            }
            Err(e) => {
                tracing::warn!("LLM reranking failed, returning original order: {e}");
                candidates.truncate(self.config.top_k);
                candidates
            }
        }
    }

    /// Build the scoring prompt sent to the LLM.
    fn build_rerank_prompt(&self, query: &str, candidates: &[MemoryEntry]) -> String {
        let mut prompt = format!(
            "Rate the relevance of each document to the query on a scale of 0-10.\n\
             Query: {query}\n\n\
             Documents:\n"
        );
        for (i, entry) in candidates.iter().enumerate() {
            let preview = truncate_str(&entry.content, MAX_PREVIEW_LEN);
            prompt.push_str(&format!("[{i}] {preview}\n"));
        }
        prompt.push_str(
            "\nRespond with ONLY a JSON array of numeric scores, e.g. [8, 3, 7, ...]",
        );
        prompt
    }

    /// Call the LLM and parse the response into a score vector.
    async fn call_llm_for_scores(&self, prompt: &str) -> anyhow::Result<Vec<f32>> {
        let request = CompletionRequest {
            model: self
                .config
                .model
                .clone()
                .unwrap_or_default(),
            messages: vec![ChatMessage::user(prompt.to_string())],
            max_tokens: 256,
            temperature: Some(0.0),
            ..Default::default()
        };

        let response = self.config.provider.complete(request).await?;

        let text: String = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        self.parse_scores(&text)
    }

    /// Extract a JSON array of floats from (potentially noisy) LLM output.
    fn parse_scores(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let start = text
            .find('[')
            .ok_or_else(|| anyhow::anyhow!("No JSON array found in LLM response"))?;
        let end = text
            .rfind(']')
            .ok_or_else(|| anyhow::anyhow!("No closing bracket in LLM response"))?
            + 1;
        let json_str = &text[start..end];
        let scores: Vec<f32> = serde_json::from_str(json_str)?;
        Ok(scores)
    }
}

/// Truncate a string to at most `max_len` bytes on a char boundary.
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    // Find the largest char boundary <= max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use async_trait::async_trait;
    use octo_types::memory::{MemoryCategory, MemoryEntry};
    use octo_types::provider::{CompletionResponse, TokenUsage};

    use crate::providers::traits::CompletionStream;

    // -- Mock provider that returns configurable text ---------

    struct MockRerankerProvider {
        response_text: String,
    }

    impl MockRerankerProvider {
        fn with_text(text: impl Into<String>) -> Self {
            Self {
                response_text: text.into(),
            }
        }
    }

    #[async_trait]
    impl Provider for MockRerankerProvider {
        fn id(&self) -> &str {
            "mock-reranker"
        }

        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                id: "r-mock".into(),
                content: vec![ContentBlock::Text {
                    text: self.response_text.clone(),
                }],
                stop_reason: None,
                usage: TokenUsage {
                    input_tokens: 10,
                    output_tokens: 10,
                },
            })
        }

        async fn stream(&self, _req: CompletionRequest) -> Result<CompletionStream> {
            Err(anyhow::anyhow!("not implemented"))
        }
    }

    // -- Error provider for fallback tests --------------------

    struct ErrorProvider;

    #[async_trait]
    impl Provider for ErrorProvider {
        fn id(&self) -> &str {
            "error-provider"
        }

        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse> {
            Err(anyhow::anyhow!("provider unavailable"))
        }

        async fn stream(&self, _req: CompletionRequest) -> Result<CompletionStream> {
            Err(anyhow::anyhow!("not implemented"))
        }
    }

    // -- Helpers ----------------------------------------------

    fn make_entry(content: &str) -> MemoryEntry {
        MemoryEntry::new("test-user", MemoryCategory::Patterns, content)
    }

    fn make_config(provider: Arc<dyn Provider>) -> LlmRerankerConfig {
        LlmRerankerConfig {
            provider,
            max_candidates: 20,
            top_k: 3,
            model: None,
        }
    }

    // -- Unit tests -------------------------------------------

    #[test]
    fn test_parse_scores_valid() {
        let reranker = LlmReranker::new(make_config(Arc::new(ErrorProvider)));
        let scores = reranker.parse_scores("[8.5, 3.0, 7.2, 1.0]").unwrap();
        assert_eq!(scores, vec![8.5, 3.0, 7.2, 1.0]);
    }

    #[test]
    fn test_parse_scores_integers() {
        let reranker = LlmReranker::new(make_config(Arc::new(ErrorProvider)));
        let scores = reranker.parse_scores("[9, 2, 7]").unwrap();
        assert_eq!(scores, vec![9.0, 2.0, 7.0]);
    }

    #[test]
    fn test_parse_scores_with_surrounding_text() {
        let reranker = LlmReranker::new(make_config(Arc::new(ErrorProvider)));
        let scores = reranker
            .parse_scores("Here are the scores: [9, 2, 7]\nDone.")
            .unwrap();
        assert_eq!(scores, vec![9.0, 2.0, 7.0]);
    }

    #[test]
    fn test_parse_scores_no_array() {
        let reranker = LlmReranker::new(make_config(Arc::new(ErrorProvider)));
        assert!(reranker.parse_scores("no array here").is_err());
    }

    #[test]
    fn test_build_rerank_prompt_format() {
        let reranker = LlmReranker::new(make_config(Arc::new(ErrorProvider)));
        let entries = vec![
            make_entry("First document content"),
            make_entry("Second document content"),
        ];
        let prompt = reranker.build_rerank_prompt("test query", &entries);

        assert!(prompt.contains("Query: test query"));
        assert!(prompt.contains("[0] First document content"));
        assert!(prompt.contains("[1] Second document content"));
        assert!(prompt.contains("JSON array"));
    }

    #[test]
    fn test_build_rerank_prompt_truncates_long_content() {
        let reranker = LlmReranker::new(make_config(Arc::new(ErrorProvider)));
        let long_content = "x".repeat(500);
        let entries = vec![make_entry(&long_content)];
        let prompt = reranker.build_rerank_prompt("q", &entries);

        // The content in the prompt should be truncated to MAX_PREVIEW_LEN
        assert!(!prompt.contains(&long_content));
        assert!(prompt.contains(&"x".repeat(MAX_PREVIEW_LEN)));
    }

    #[tokio::test]
    async fn test_rerank_empty_candidates() {
        let provider = Arc::new(ErrorProvider);
        let reranker = LlmReranker::new(make_config(provider));
        let result = reranker.rerank("query", vec![]).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_rerank_sorts_by_score() {
        let provider = Arc::new(MockRerankerProvider::with_text("[2, 9, 5]"));
        let reranker = LlmReranker::new(make_config(provider));

        let candidates = vec![
            make_entry("low relevance"),
            make_entry("high relevance"),
            make_entry("medium relevance"),
        ];

        let result = reranker.rerank("test query", candidates).await;

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].content, "high relevance");
        assert_eq!(result[1].content, "medium relevance");
        assert_eq!(result[2].content, "low relevance");
    }

    #[tokio::test]
    async fn test_rerank_respects_top_k() {
        let provider = Arc::new(MockRerankerProvider::with_text("[1, 8, 3, 9, 5]"));
        let mut cfg = make_config(provider);
        cfg.top_k = 2;
        let reranker = LlmReranker::new(cfg);

        let candidates = vec![
            make_entry("a"),
            make_entry("b"),
            make_entry("c"),
            make_entry("d"),
            make_entry("e"),
        ];

        let result = reranker.rerank("q", candidates).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "d"); // score 9
        assert_eq!(result[1].content, "b"); // score 8
    }

    #[tokio::test]
    async fn test_rerank_fallback_on_provider_error() {
        let provider = Arc::new(ErrorProvider);
        let mut cfg = make_config(provider);
        cfg.top_k = 2;
        let reranker = LlmReranker::new(cfg);

        let candidates = vec![
            make_entry("first"),
            make_entry("second"),
            make_entry("third"),
        ];

        let result = reranker.rerank("q", candidates).await;
        // Falls back to original order, truncated to top_k
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "first");
        assert_eq!(result[1].content, "second");
    }

    #[tokio::test]
    async fn test_rerank_fallback_on_wrong_score_count() {
        // Return 2 scores for 3 candidates — should fallback
        let provider = Arc::new(MockRerankerProvider::with_text("[5, 3]"));
        let mut cfg = make_config(provider);
        cfg.top_k = 2;
        let reranker = LlmReranker::new(cfg);

        let candidates = vec![
            make_entry("a"),
            make_entry("b"),
            make_entry("c"),
        ];

        let result = reranker.rerank("q", candidates).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "a"); // original order preserved
    }

    #[tokio::test]
    async fn test_rerank_respects_max_candidates() {
        let provider = Arc::new(MockRerankerProvider::with_text("[5, 3]"));
        let mut cfg = make_config(provider);
        cfg.max_candidates = 2;
        cfg.top_k = 2;
        let reranker = LlmReranker::new(cfg);

        let candidates = vec![
            make_entry("a"),
            make_entry("b"),
            make_entry("c"), // should be truncated before rerank
        ];

        let result = reranker.rerank("q", candidates).await;
        // max_candidates=2, so only 2 sent; scores match => rerank works
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello world", 5), "hello");
        assert_eq!(truncate_str("hi", 10), "hi");
        assert_eq!(truncate_str("", 5), "");
    }

    #[test]
    fn test_truncate_str_multibyte() {
        // Chinese chars are 3 bytes each in UTF-8
        let s = "你好世界"; // 12 bytes
        let t = truncate_str(s, 7);
        // Can fit 2 full chars (6 bytes), byte 7 is mid-char
        assert_eq!(t, "你好");
    }

    #[test]
    fn test_rerank_strategy_debug() {
        let none = RerankStrategy::None;
        let dbg = format!("{none:?}");
        assert_eq!(dbg, "RerankStrategy::None");
    }
}
