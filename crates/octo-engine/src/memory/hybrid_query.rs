use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::debug;

use super::vector_index::{VectorIndex, VectorSearchResult};

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
/// When a `VectorIndex` is available, natural language queries are routed to
/// vector similarity search. Structured queries (with `tag:`, `id:`, `key:`
/// prefixes) are routed to the existing SQLite-based MemoryStore.
pub struct HybridQueryEngine {
    vector_index: Option<Arc<VectorIndex>>,
}

impl HybridQueryEngine {
    /// Create a new hybrid query engine.
    ///
    /// Pass `Some(vector_index)` to enable semantic search, or `None` to
    /// fall back to structured-only search.
    pub fn new(vector_index: Option<Arc<VectorIndex>>) -> Self {
        debug!(
            has_vector_index = vector_index.is_some(),
            "Creating HybridQueryEngine"
        );
        Self { vector_index }
    }

    /// Classify a query into semantic vs structured.
    ///
    /// Heuristic:
    /// - If query contains `tag:`, `id:`, or `key:` prefixes -> Structured
    /// - If a vector index is available -> Semantic
    /// - Otherwise -> Structured (fallback)
    pub fn classify_query(&self, query: &str) -> QueryType {
        let trimmed = query.trim();

        // Check for structured query prefixes
        if trimmed.contains("tag:")
            || trimmed.contains("id:")
            || trimmed.contains("key:")
        {
            return QueryType::Structured;
        }

        // If we have a vector index, prefer semantic search
        if self.vector_index.is_some() {
            QueryType::Semantic
        } else {
            QueryType::Structured
        }
    }

    /// Search using the appropriate backend.
    ///
    /// For semantic queries, requires an embedding vector. For structured
    /// queries, returns an empty result set -- the caller should merge with
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
            QueryType::Semantic => {
                if let (Some(index), Some(emb)) = (&self.vector_index, embedding) {
                    let results = index.search(emb, limit, None).await;
                    Ok(vector_results_to_hybrid(results))
                } else {
                    debug!("Semantic query but no vector index or embedding provided");
                    Ok(vec![])
                }
            }
            QueryType::Structured | QueryType::Hybrid => {
                // Structured search is handled by existing MemoryStore.
                // Caller should merge with MemoryStore results.
                Ok(vec![])
            }
        }
    }

    /// Check whether a vector index is configured.
    pub fn has_vector_index(&self) -> bool {
        self.vector_index.is_some()
    }

    /// Get a reference to the underlying vector index, if available.
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

        assert_eq!(engine.classify_query("tag:important"), QueryType::Structured);
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
}
