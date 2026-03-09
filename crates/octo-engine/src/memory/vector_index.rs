use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// A vector entry in the index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub metadata: HashMap<String, String>,
}

/// Configuration for the vector index
#[derive(Debug, Clone)]
pub struct VectorIndexConfig {
    /// Embedding dimensions (e.g., 768, 1536)
    pub dimensions: usize,
    /// Similarity threshold for search results (0.0-1.0)
    pub default_threshold: f32,
    /// Maximum entries (for memory limits)
    pub max_entries: usize,
}

impl Default for VectorIndexConfig {
    fn default() -> Self {
        Self {
            dimensions: 1536, // OpenAI ada-002 / Anthropic default
            default_threshold: 0.7,
            max_entries: 100_000,
        }
    }
}

/// Result of a vector similarity search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub id: String,
    pub similarity: f32,
    pub metadata: HashMap<String, String>,
}

/// In-memory vector index with cosine similarity search.
///
/// Current implementation uses brute-force search for correctness.
/// The API is designed for future HNSW optimization (M=16, efConstruction=200)
/// which would provide O(log n) search instead of O(n).
pub struct VectorIndex {
    config: VectorIndexConfig,
    entries: RwLock<Vec<VectorEntry>>,
}

impl VectorIndex {
    /// Create a new vector index with the given configuration.
    pub fn new(config: VectorIndexConfig) -> Self {
        debug!(
            dimensions = config.dimensions,
            max_entries = config.max_entries,
            threshold = config.default_threshold,
            "Creating VectorIndex"
        );
        Self {
            config,
            entries: RwLock::new(Vec::new()),
        }
    }

    /// Insert a vector entry into the index.
    ///
    /// Returns an error if the embedding dimensions do not match the configured
    /// dimensions. If the index exceeds `max_entries`, the oldest entry is removed.
    pub async fn insert(&self, entry: VectorEntry) -> anyhow::Result<()> {
        if entry.embedding.len() != self.config.dimensions {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                entry.embedding.len()
            );
        }

        let mut entries = self.entries.write().await;

        // Remove existing entry with same ID (upsert behavior)
        entries.retain(|e| e.id != entry.id);

        // Evict oldest if at capacity
        if entries.len() >= self.config.max_entries {
            let to_remove = entries.len() - self.config.max_entries + 1;
            warn!(
                evicted = to_remove,
                max = self.config.max_entries,
                "VectorIndex at capacity, evicting oldest entries"
            );
            entries.drain(..to_remove);
        }

        debug!(id = %entry.id, "Inserted vector entry");
        entries.push(entry);
        Ok(())
    }

    /// Search for similar vectors by cosine similarity.
    ///
    /// Returns at most `limit` results above the given threshold (or the
    /// configured default threshold), sorted by similarity descending.
    pub async fn search(
        &self,
        query: &[f32],
        limit: usize,
        threshold: Option<f32>,
    ) -> Vec<VectorSearchResult> {
        if query.len() != self.config.dimensions {
            warn!(
                expected = self.config.dimensions,
                got = query.len(),
                "Query dimension mismatch, returning empty results"
            );
            return Vec::new();
        }

        let threshold = threshold.unwrap_or(self.config.default_threshold);
        let entries = self.entries.read().await;

        let mut results: Vec<VectorSearchResult> = entries
            .iter()
            .filter_map(|entry| {
                let sim = cosine_similarity(query, &entry.embedding);
                if sim >= threshold {
                    Some(VectorSearchResult {
                        id: entry.id.clone(),
                        similarity: sim,
                        metadata: entry.metadata.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);

        debug!(
            query_dim = query.len(),
            total = entries.len(),
            matched = results.len(),
            threshold = threshold,
            "Vector search complete"
        );

        results
    }

    /// Remove an entry by ID. Returns true if an entry was removed.
    pub async fn remove(&self, id: &str) -> bool {
        let mut entries = self.entries.write().await;
        let len_before = entries.len();
        entries.retain(|e| e.id != id);
        let removed = entries.len() < len_before;
        if removed {
            debug!(id = %id, "Removed vector entry");
        }
        removed
    }

    /// Get the number of entries in the index.
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Check if the index is empty.
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }

    /// Clear all entries from the index.
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
        debug!("Cleared all vector index entries");
    }

    /// Get a reference to the index configuration.
    pub fn config(&self) -> &VectorIndexConfig {
        &self.config
    }
}

/// Compute cosine similarity between two vectors.
///
/// Returns 0.0 for zero-length vectors, dimension mismatches, or zero-norm vectors.
/// The result is in the range [-1.0, 1.0] for normalized vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

// ── HnswIndex (feature = "hnsw") ─────────────────────────────────────────

/// Configuration for HNSW approximate nearest-neighbour index.
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// Number of bi-directional links per node (default 16).
    pub m: usize,
    /// Size of the dynamic candidate list during construction (default 200).
    pub ef_construction: usize,
    /// Embedding dimensionality (default 1536 for OpenAI, 1024 for Voyage).
    pub dimensions: usize,
    /// Expected maximum number of elements (default 100_000).
    pub max_elements: usize,
    /// Minimum cosine similarity to include in search results (default 0.7).
    pub default_threshold: f32,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            dimensions: 1536,
            max_elements: 100_000,
            default_threshold: 0.7,
        }
    }
}

#[cfg(feature = "hnsw")]
mod hnsw_impl {
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use hnsw_rs::prelude::*;
    use tokio::sync::RwLock;

    use super::{HnswConfig, VectorEntry, VectorSearchResult};

    /// HNSW-backed approximate nearest-neighbour index.
    ///
    /// All access to the inner `Hnsw` goes through `tokio::task::spawn_blocking`
    /// to keep CPU-bound graph traversal off the async executor.
    pub struct HnswIndex {
        config: HnswConfig,
        inner: Arc<Mutex<Hnsw<'static, f32, DistCosine>>>,
        /// usize internal ID → VectorEntry
        id_map: Arc<RwLock<HashMap<usize, VectorEntry>>>,
        /// entry.id string → usize internal ID
        rev_map: Arc<RwLock<HashMap<String, usize>>>,
        next_id: Arc<AtomicUsize>,
    }

    impl HnswIndex {
        pub fn new(config: HnswConfig) -> Self {
            let hnsw = Hnsw::new(
                config.m,
                config.max_elements,
                16,
                config.ef_construction,
                DistCosine,
            );
            Self {
                config,
                inner: Arc::new(Mutex::new(hnsw)),
                id_map: Arc::new(RwLock::new(HashMap::new())),
                rev_map: Arc::new(RwLock::new(HashMap::new())),
                next_id: Arc::new(AtomicUsize::new(0)),
            }
        }

        /// Insert or replace a vector entry.
        pub async fn upsert(&self, entry: VectorEntry) -> anyhow::Result<()> {
            // Assign (or reuse) an internal usize ID.
            let internal_id = {
                let rev = self.rev_map.read().await;
                if let Some(&id) = rev.get(&entry.id) {
                    id
                } else {
                    drop(rev);
                    self.next_id.fetch_add(1, Ordering::Relaxed)
                }
            };

            // Update maps.
            self.rev_map
                .write()
                .await
                .insert(entry.id.clone(), internal_id);
            self.id_map.write().await.insert(internal_id, entry.clone());

            // Insert into HNSW (spawn_blocking for CPU-bound graph construction).
            let inner = self.inner.clone();
            let vec = entry.embedding.clone();
            tokio::task::spawn_blocking(move || {
                let hnsw = inner.lock().unwrap();
                hnsw.insert((&vec, internal_id));
            })
            .await?;

            Ok(())
        }

        /// Search for the `limit` most similar vectors above `threshold`.
        pub async fn search(
            &self,
            query: &[f32],
            limit: usize,
            threshold: Option<f32>,
        ) -> Vec<VectorSearchResult> {
            let threshold = threshold.unwrap_or(self.config.default_threshold);
            let inner = self.inner.clone();
            let query_vec = query.to_vec();
            let ef_search = (limit * 4).max(50);

            let neighbours = tokio::task::spawn_blocking(move || {
                let hnsw = inner.lock().unwrap();
                hnsw.search(&query_vec, limit, ef_search)
            })
            .await
            .unwrap_or_default();

            let id_map = self.id_map.read().await;
            let mut results = Vec::new();
            for nb in neighbours {
                // DistCosine returns cosine distance; convert to similarity.
                let similarity = 1.0 - nb.distance;
                if similarity >= threshold {
                    if let Some(entry) = id_map.get(&nb.d_id) {
                        results.push(VectorSearchResult {
                            id: entry.id.clone(),
                            similarity,
                            metadata: entry.metadata.clone(),
                        });
                    }
                }
            }
            // Highest similarity first.
            results.sort_by(|a, b| {
                b.similarity
                    .partial_cmp(&a.similarity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results
        }

        /// Number of vectors currently indexed.
        pub async fn len(&self) -> usize {
            self.id_map.read().await.len()
        }

        /// `true` if no vectors have been indexed.
        pub async fn is_empty(&self) -> bool {
            self.len().await == 0
        }
    }
}

#[cfg(feature = "hnsw")]
pub use hnsw_impl::HnswIndex;

// ── VectorBackend ─────────────────────────────────────────────────────────

/// Unified interface over BruteForce and HNSW vector backends.
pub enum VectorBackend {
    BruteForce(VectorIndex),
    #[cfg(feature = "hnsw")]
    Hnsw(HnswIndex),
}

impl VectorBackend {
    /// Create a brute-force backend.
    pub fn brute_force(config: VectorIndexConfig) -> Self {
        Self::BruteForce(VectorIndex::new(config))
    }

    /// Create an HNSW backend (only available with `features = ["hnsw"]`).
    #[cfg(feature = "hnsw")]
    pub fn hnsw(config: HnswConfig) -> Self {
        Self::Hnsw(HnswIndex::new(config))
    }

    /// Insert or replace a vector entry.
    pub async fn insert(&self, entry: VectorEntry) -> anyhow::Result<()> {
        match self {
            Self::BruteForce(idx) => idx.insert(entry).await,
            #[cfg(feature = "hnsw")]
            Self::Hnsw(idx) => idx.upsert(entry).await,
        }
    }

    /// Search for the `limit` most similar vectors.
    pub async fn search(
        &self,
        query: &[f32],
        limit: usize,
        threshold: Option<f32>,
    ) -> Vec<VectorSearchResult> {
        match self {
            Self::BruteForce(idx) => idx.search(query, limit, threshold).await,
            #[cfg(feature = "hnsw")]
            Self::Hnsw(idx) => idx.search(query, limit, threshold).await,
        }
    }

    /// Returns true if empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Number of indexed vectors.
    pub async fn len(&self) -> usize {
        match self {
            Self::BruteForce(idx) => idx.len().await,
            #[cfg(feature = "hnsw")]
            Self::Hnsw(idx) => idx.len().await,
        }
    }

    /// Backend identifier for logging/metrics.
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::BruteForce(_) => "brute-force",
            #[cfg(feature = "hnsw")]
            Self::Hnsw(_) => "hnsw",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 2.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[tokio::test]
    async fn test_vector_index_insert_and_search() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = VectorIndex::new(config);

        let entry = VectorEntry {
            id: "e1".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: HashMap::from([("content".to_string(), "hello world".to_string())]),
        };
        index.insert(entry).await.unwrap();
        assert_eq!(index.len().await, 1);

        // Search with similar vector
        let results = index.search(&[0.9, 0.1, 0.0], 10, None).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "e1");
        assert!(results[0].similarity > 0.9);
    }

    #[tokio::test]
    async fn test_vector_index_dimension_mismatch_insert() {
        let config = VectorIndexConfig {
            dimensions: 3,
            default_threshold: 0.5,
            max_entries: 100,
        };
        let index = VectorIndex::new(config);

        let entry = VectorEntry {
            id: "bad".to_string(),
            embedding: vec![1.0, 2.0], // wrong dimensions
            metadata: HashMap::new(),
        };
        assert!(index.insert(entry).await.is_err());
    }

    #[tokio::test]
    async fn test_vector_index_remove() {
        let config = VectorIndexConfig {
            dimensions: 2,
            default_threshold: 0.0,
            max_entries: 100,
        };
        let index = VectorIndex::new(config);

        index
            .insert(VectorEntry {
                id: "a".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: HashMap::new(),
            })
            .await
            .unwrap();

        assert!(index.remove("a").await);
        assert!(!index.remove("a").await);
        assert!(index.is_empty().await);
    }

    #[tokio::test]
    async fn test_vector_index_eviction() {
        let config = VectorIndexConfig {
            dimensions: 2,
            default_threshold: 0.0,
            max_entries: 2,
        };
        let index = VectorIndex::new(config);

        for i in 0..3 {
            index
                .insert(VectorEntry {
                    id: format!("e{i}"),
                    embedding: vec![1.0, i as f32],
                    metadata: HashMap::new(),
                })
                .await
                .unwrap();
        }

        // Should have evicted oldest, keeping max 2
        assert_eq!(index.len().await, 2);
    }

    #[tokio::test]
    async fn test_vector_index_upsert() {
        let config = VectorIndexConfig {
            dimensions: 2,
            default_threshold: 0.0,
            max_entries: 100,
        };
        let index = VectorIndex::new(config);

        index
            .insert(VectorEntry {
                id: "x".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: HashMap::from([("v".to_string(), "1".to_string())]),
            })
            .await
            .unwrap();

        // Re-insert same ID with different data
        index
            .insert(VectorEntry {
                id: "x".to_string(),
                embedding: vec![0.0, 1.0],
                metadata: HashMap::from([("v".to_string(), "2".to_string())]),
            })
            .await
            .unwrap();

        assert_eq!(index.len().await, 1);

        let results = index.search(&[0.0, 1.0], 10, Some(0.9)).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].metadata.get("v").unwrap(), "2");
    }

    #[tokio::test]
    async fn test_vector_index_clear() {
        let config = VectorIndexConfig {
            dimensions: 2,
            default_threshold: 0.0,
            max_entries: 100,
        };
        let index = VectorIndex::new(config);

        index
            .insert(VectorEntry {
                id: "a".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: HashMap::new(),
            })
            .await
            .unwrap();

        index.clear().await;
        assert!(index.is_empty().await);
    }

    #[tokio::test]
    async fn test_vector_index_threshold_filtering() {
        let config = VectorIndexConfig {
            dimensions: 2,
            default_threshold: 0.99,
            max_entries: 100,
        };
        let index = VectorIndex::new(config);

        index
            .insert(VectorEntry {
                id: "a".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: HashMap::new(),
            })
            .await
            .unwrap();

        // With high threshold, a slightly different vector should not match
        let results = index.search(&[0.9, 0.4], 10, None).await;
        assert!(results.is_empty());

        // With low threshold, it should match
        let results = index.search(&[0.9, 0.4], 10, Some(0.5)).await;
        assert_eq!(results.len(), 1);
    }
}
