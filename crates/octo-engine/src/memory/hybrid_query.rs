use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::embedding::EmbeddingClient;
use super::vector_index::{VectorBackend, VectorIndex, VectorSearchResult};

/// Query type classification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryType {
    /// Semantic / natural language query -> use vector search
    Semantic,
    /// Structured query (exact match, tag filter) -> use SQLite
    Structured,
    /// Hybrid -- try both and merge
    Hybrid,
}

/// A unified search result from either backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub id: String,
    pub content: String,
    pub score: f32,
    /// Source backend: "vector", "structured", or "fts"
    pub source: String,
    pub metadata: HashMap<String, String>,
}

/// Routes queries to the appropriate search backend.
///
/// Three construction modes are available:
/// - `new_structured()` — keyword/structured search only, no vectors
/// - `with_vector_backend(backend)` — vector search with caller-provided embeddings
/// - `with_semantic_search(backend, client)` — full auto-embedding on Semantic queries
///
/// The legacy `new(vector_index)` constructor is retained for backward compatibility.
pub struct HybridQueryEngine {
    /// Legacy brute-force index (kept for backward compatibility with `new()`).
    vector_index: Option<Arc<VectorIndex>>,
    /// Pluggable backend (brute-force or HNSW).
    vector_backend: Option<Arc<VectorBackend>>,
    /// Optional embedding client for auto-embedding on Semantic queries.
    embedding_client: Option<Arc<EmbeddingClient>>,
}

impl HybridQueryEngine {
    // ── Constructors ──────────────────────────────────────────────────────

    /// Create a hybrid query engine with no vector search (structured only).
    pub fn new_structured() -> Self {
        debug!("Creating HybridQueryEngine (structured-only)");
        Self {
            vector_index: None,
            vector_backend: None,
            embedding_client: None,
        }
    }

    /// Create a hybrid query engine with a pluggable vector backend.
    ///
    /// The caller is responsible for computing embeddings and passing them
    /// as the `embedding` parameter in `search()`.
    pub fn with_vector_backend(backend: Arc<VectorBackend>) -> Self {
        debug!(
            backend = backend.backend_name(),
            "Creating HybridQueryEngine with vector backend"
        );
        Self {
            vector_index: None,
            vector_backend: Some(backend),
            embedding_client: None,
        }
    }

    /// Create a hybrid query engine with full semantic search support.
    ///
    /// On Semantic queries the engine automatically calls `client.embed(query)`
    /// to produce a vector and then delegates to `backend.search()`.
    pub fn with_semantic_search(backend: Arc<VectorBackend>, client: Arc<EmbeddingClient>) -> Self {
        debug!(
            backend = backend.backend_name(),
            "Creating HybridQueryEngine with semantic search"
        );
        Self {
            vector_index: None,
            vector_backend: Some(backend),
            embedding_client: Some(client),
        }
    }

    /// Create a new hybrid query engine (legacy constructor).
    ///
    /// Pass `Some(vector_index)` to enable semantic search, or `None` to
    /// fall back to structured-only search.
    pub fn new(vector_index: Option<Arc<VectorIndex>>) -> Self {
        debug!(
            has_vector_index = vector_index.is_some(),
            "Creating HybridQueryEngine"
        );
        Self {
            vector_index,
            vector_backend: None,
            embedding_client: None,
        }
    }

    // ── Query routing ─────────────────────────────────────────────────────

    /// Classify a query into semantic vs structured.
    ///
    /// Heuristic:
    /// - If query contains `tag:`, `id:`, or `key:` prefixes -> Structured
    /// - If a vector backend or legacy index is available -> Semantic
    /// - Otherwise -> Structured (fallback)
    pub fn classify_query(&self, query: &str) -> QueryType {
        let trimmed = query.trim();

        // Check for structured query prefixes
        if trimmed.contains("tag:") || trimmed.contains("id:") || trimmed.contains("key:") {
            return QueryType::Structured;
        }

        // If we have any vector capability, prefer semantic search
        if self.vector_backend.is_some() || self.vector_index.is_some() {
            QueryType::Semantic
        } else {
            QueryType::Structured
        }
    }

    /// Search using the appropriate backend.
    ///
    /// For semantic queries:
    /// - If `embedding_client` is configured, it auto-generates the embedding from `query`.
    /// - Otherwise the caller must supply a pre-computed `embedding`.
    /// - If neither is available, returns an empty result set.
    ///
    /// For structured queries, returns an empty result set — the caller should merge with
    /// results from the existing `MemoryStore`.
    pub async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        limit: usize,
    ) -> anyhow::Result<Vec<HybridSearchResult>> {
        let query_type = self.classify_query(query);
        debug!(?query_type, query = %query, "HybridQueryEngine routing query");

        match query_type {
            QueryType::Semantic => self.semantic_search(query, embedding, limit).await,
            QueryType::Structured | QueryType::Hybrid => {
                // Structured search is handled by existing MemoryStore.
                // Caller should merge with MemoryStore results.
                Ok(vec![])
            }
        }
    }

    async fn semantic_search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        limit: usize,
    ) -> anyhow::Result<Vec<HybridSearchResult>> {
        // Prefer the new VectorBackend path.
        if let Some(backend) = &self.vector_backend {
            let emb: Vec<f32> = if let Some(e) = embedding {
                e.to_vec()
            } else if let Some(client) = &self.embedding_client {
                client.embed(query).await?
            } else {
                debug!("Semantic query but no embedding or embedding client provided");
                return Ok(vec![]);
            };
            let results = backend.search(&emb, limit, None).await;
            return Ok(vector_results_to_hybrid(results));
        }

        // Legacy VectorIndex path (backward compat).
        if let (Some(index), Some(emb)) = (&self.vector_index, embedding) {
            let results = index.search(emb, limit, None).await;
            return Ok(vector_results_to_hybrid(results));
        }

        debug!("Semantic query but no vector index or embedding provided");
        Ok(vec![])
    }

    // ── Observability ─────────────────────────────────────────────────────

    /// Check whether a vector index is configured (legacy or new backend).
    pub fn has_vector_index(&self) -> bool {
        self.vector_index.is_some() || self.vector_backend.is_some()
    }

    /// Check whether an embedding client is configured.
    pub fn has_embedding_client(&self) -> bool {
        self.embedding_client.is_some()
    }

    /// Backend identifier string for logging/metrics.
    pub fn backend_name(&self) -> &'static str {
        if let Some(backend) = &self.vector_backend {
            return backend.backend_name();
        }
        if self.vector_index.is_some() {
            return "brute-force";
        }
        "none"
    }

    /// Get a reference to the underlying legacy vector index, if available.
    pub fn vector_index(&self) -> Option<&Arc<VectorIndex>> {
        self.vector_index.as_ref()
    }
}

/// Convert vector search results into hybrid search results.
fn vector_results_to_hybrid(results: Vec<VectorSearchResult>) -> Vec<HybridSearchResult> {
    results
        .into_iter()
        .map(|r| HybridSearchResult {
            id: r.id,
            content: r.metadata.get("content").cloned().unwrap_or_default(),
            score: r.similarity,
            source: "vector".to_string(),
            metadata: r.metadata,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::vector_index::{VectorEntry, VectorIndexConfig};

    #[test]
    fn test_classify_structured_queries() {
        let engine = HybridQueryEngine::new(None);

        assert_eq!(
            engine.classify_query("tag:important"),
            QueryType::Structured
        );
        assert_eq!(engine.classify_query("id:abc-123"), QueryType::Structured);
        assert_eq!(
            engine.classify_query("key:my_setting"),
            QueryType::Structured
        );
    }

    #[test]
    fn test_classify_semantic_without_index() {
        let engine = HybridQueryEngine::new(None);

        // Without a vector index, natural language falls back to structured
        assert_eq!(
            engine.classify_query("how do I authenticate users?"),
            QueryType::Structured
        );
    }

    #[test]
    fn test_classify_semantic_with_index() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = Arc::new(VectorIndex::new(config));
        let engine = HybridQueryEngine::new(Some(index));

        assert_eq!(
            engine.classify_query("how do I authenticate users?"),
            QueryType::Semantic
        );
    }

    #[test]
    fn test_classify_structured_prefix_with_index() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = Arc::new(VectorIndex::new(config));
        let engine = HybridQueryEngine::new(Some(index));

        // Even with vector index, structured prefixes win
        assert_eq!(engine.classify_query("tag:auth"), QueryType::Structured);
    }

    #[tokio::test]
    async fn test_search_semantic() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = Arc::new(VectorIndex::new(config));

        index
            .insert(VectorEntry {
                id: "mem1".to_string(),
                embedding: vec![1.0, 0.0, 0.0],
                metadata: HashMap::from([("content".to_string(), "auth pattern".to_string())]),
            })
            .await
            .unwrap();

        let engine = HybridQueryEngine::new(Some(index));
        let query_embedding = vec![0.9, 0.1, 0.0];
        let results = engine
            .search("authentication", Some(&query_embedding), 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "mem1");
        assert_eq!(results[0].source, "vector");
        assert_eq!(results[0].content, "auth pattern");
    }

    #[tokio::test]
    async fn test_search_structured_returns_empty() {
        let engine = HybridQueryEngine::new(None);
        let results = engine.search("tag:important", None, 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_semantic_no_embedding() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = Arc::new(VectorIndex::new(config));
        let engine = HybridQueryEngine::new(Some(index));

        // Semantic query but no embedding provided
        let results = engine.search("hello world", None, 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_has_vector_index() {
        let engine_without = HybridQueryEngine::new(None);
        assert!(!engine_without.has_vector_index());

        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = Arc::new(VectorIndex::new(config));
        let engine_with = HybridQueryEngine::new(Some(index));
        assert!(engine_with.has_vector_index());
    }

    #[test]
    fn test_new_structured_no_vector() {
        let engine = HybridQueryEngine::new_structured();
        assert!(!engine.has_vector_index());
        assert!(!engine.has_embedding_client());
        assert_eq!(engine.backend_name(), "none");
        assert_eq!(
            engine.classify_query("natural language query"),
            QueryType::Structured
        );
    }

    #[test]
    fn test_with_vector_backend_brute_force() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let backend = Arc::new(VectorBackend::brute_force(config));
        let engine = HybridQueryEngine::with_vector_backend(backend);
        assert!(engine.has_vector_index());
        assert!(!engine.has_embedding_client());
        assert_eq!(engine.backend_name(), "brute-force");
        assert_eq!(
            engine.classify_query("natural language query"),
            QueryType::Semantic
        );
    }

    #[tokio::test]
    async fn test_search_via_vector_backend() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let backend = Arc::new(VectorBackend::brute_force(config));
        backend
            .insert(VectorEntry {
                id: "b1".to_string(),
                embedding: vec![1.0, 0.0, 0.0],
                metadata: HashMap::from([("content".to_string(), "backend entry".to_string())]),
            })
            .await
            .unwrap();

        let engine = HybridQueryEngine::with_vector_backend(backend);
        let results = engine
            .search("query", Some(&[0.9_f32, 0.1, 0.0]), 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "b1");
        assert_eq!(results[0].content, "backend entry");
    }
}
