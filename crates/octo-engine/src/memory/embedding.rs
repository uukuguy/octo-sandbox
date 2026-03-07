//! EmbeddingClient — converts text to embedding vectors via external APIs.
//!
//! Supports OpenAI (`text-embedding-3-small`) and Anthropic Voyage
//! (`voyage-3-lite`). Results are cached in-memory with FIFO eviction
//! (first inserted entry evicted when full, max 1000 entries).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

/// Which embedding API to call.
#[derive(Debug, Clone)]
pub enum EmbeddingProvider {
    OpenAI,
    /// Anthropic's Voyage embedding API
    Anthropic,
}

/// Configuration for EmbeddingClient.
#[derive(Clone)]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProvider,
    pub api_key: String,
    /// Model name: "text-embedding-3-small" (OpenAI) or "voyage-3-lite" (Voyage).
    pub model: String,
    /// Output dimension: 1536 (OpenAI) or 1024 (Voyage).
    pub dimensions: usize,
    /// Max texts per API call.
    pub batch_size: usize,
}

impl std::fmt::Debug for EmbeddingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key_preview = if self.api_key.len() > 4 {
            format!("{}***", &self.api_key[..4])
        } else {
            "***".to_string()
        };
        f.debug_struct("EmbeddingConfig")
            .field("provider", &self.provider)
            .field("api_key", &key_preview)
            .field("model", &self.model)
            .finish()
    }
}

impl EmbeddingConfig {
    /// Default OpenAI config (text-embedding-3-small, 1536 dims).
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            provider: EmbeddingProvider::OpenAI,
            api_key: api_key.into(),
            model: "text-embedding-3-small".to_string(),
            dimensions: 1536,
            batch_size: 100,
        }
    }

    /// Default Anthropic Voyage config (voyage-3-lite, 1024 dims).
    pub fn anthropic(api_key: impl Into<String>) -> Self {
        Self {
            provider: EmbeddingProvider::Anthropic,
            api_key: api_key.into(),
            model: "voyage-3-lite".to_string(),
            dimensions: 1024,
            batch_size: 8,
        }
    }
}

// ── API request/response types ─────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    input: &'a [&'a str],
    model: &'a str,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    data: Vec<OpenAiEmbedding>,
}

#[derive(Deserialize)]
struct OpenAiEmbedding {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct VoyageRequest<'a> {
    input: &'a [&'a str],
    model: &'a str,
}

#[derive(Deserialize)]
struct VoyageResponse {
    data: Vec<VoyageEmbedding>,
}

#[derive(Deserialize)]
struct VoyageEmbedding {
    embedding: Vec<f32>,
}

// ── EmbeddingCache ─────────────────────────────────────────────────────────

/// In-memory FIFO eviction cache for embedding vectors.
///
/// Entries are evicted in insertion order (oldest first) once `capacity` is
/// reached. The `VecDeque` tracks insertion order so eviction is always
/// deterministic — unlike a plain `HashMap` whose iteration order is
/// unspecified.
struct EmbeddingCache {
    map: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
    capacity: usize,
}

impl EmbeddingCache {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn get(&self, key: &str) -> Option<&Vec<f32>> {
        self.map.get(key)
    }

    /// Insert a new entry.  If `key` already exists the call is a no-op (the
    /// existing value is kept and the insertion order is preserved).  When the
    /// cache is full the first-inserted entry is evicted before the new one is
    /// added.
    fn insert(&mut self, key: String, value: Vec<f32>) {
        if self.map.contains_key(&key) {
            return;
        }
        if self.map.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }
}

// ── EmbeddingClient ────────────────────────────────────────────────────────

/// HTTP client for embedding APIs with in-memory FIFO eviction caching.
pub struct EmbeddingClient {
    config: EmbeddingConfig,
    http: reqwest::Client,
    /// text → embedding cache (FIFO eviction, max 1000 entries)
    cache: Arc<RwLock<EmbeddingCache>>,
}

impl EmbeddingClient {
    pub fn new(config: EmbeddingConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            http,
            cache: Arc::new(RwLock::new(EmbeddingCache::new(1_000))),
        }
    }

    /// Embed a single text, using cache if available.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Cache hit
        if let Some(v) = self.cache.read().await.get(text) {
            debug!("embedding cache hit");
            return Ok(v.clone());
        }

        let result = self.call_api(&[text]).await?;
        let vec = result.into_iter().next().context("empty embedding response")?;

        // Cache store (evict oldest entry if at capacity — deterministic FIFO)
        self.cache.write().await.insert(text.to_string(), vec.clone());

        Ok(vec)
    }

    /// Embed multiple texts, batching API calls as needed.
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(self.config.batch_size) {
            let mut batch_results = self.call_api(chunk).await?;
            results.append(&mut batch_results);
        }
        Ok(results)
    }

    async fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        match self.config.provider {
            EmbeddingProvider::OpenAI => self.call_openai(texts).await,
            EmbeddingProvider::Anthropic => self.call_voyage(texts).await,
        }
    }

    async fn call_openai(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let resp = self
            .http
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.config.api_key)
            .json(&OpenAiRequest {
                input: texts,
                model: &self.config.model,
            })
            .send()
            .await
            .context("OpenAI embedding request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::debug!("OpenAI embedding raw error body: {}", body);
            anyhow::bail!("OpenAI embedding request failed (status {})", status);
        }

        let parsed: OpenAiResponse = resp
            .json()
            .await
            .context("failed to parse OpenAI embedding response")?;
        Ok(parsed.data.into_iter().map(|e| e.embedding).collect())
    }

    async fn call_voyage(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let resp = self
            .http
            .post("https://api.voyageai.com/v1/embeddings")
            .bearer_auth(&self.config.api_key)
            .json(&VoyageRequest {
                input: texts,
                model: &self.config.model,
            })
            .send()
            .await
            .context("Voyage embedding request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::debug!("Voyage embedding raw error body: {}", body);
            anyhow::bail!("Voyage embedding request failed (status {})", status);
        }

        let parsed: VoyageResponse = resp
            .json()
            .await
            .context("failed to parse Voyage embedding response")?;
        Ok(parsed.data.into_iter().map(|e| e.embedding).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_config_defaults() {
        let cfg = EmbeddingConfig::openai("key");
        assert_eq!(cfg.dimensions, 1536);
        assert_eq!(cfg.batch_size, 100);
        assert_eq!(cfg.model, "text-embedding-3-small");
    }

    #[test]
    fn test_anthropic_config_defaults() {
        let cfg = EmbeddingConfig::anthropic("key");
        assert_eq!(cfg.dimensions, 1024);
        assert_eq!(cfg.batch_size, 8);
        assert_eq!(cfg.model, "voyage-3-lite");
    }

    #[tokio::test]
    async fn test_cache_hit_does_not_panic() {
        let client = EmbeddingClient::new(EmbeddingConfig::openai("fake"));
        // Manually seed cache via EmbeddingCache::insert
        client
            .cache
            .write()
            .await
            .insert("hello".to_string(), vec![0.1, 0.2, 0.3]);
        let result = client.embed("hello").await.unwrap();
        assert_eq!(result, vec![0.1f32, 0.2, 0.3]);
    }

    #[tokio::test]
    async fn test_cache_fifo_eviction() {
        let client = EmbeddingClient::new(EmbeddingConfig::openai("fake"));
        // Shrink capacity to 2 for testing
        {
            let mut c = client.cache.write().await;
            *c = EmbeddingCache::new(2);
            c.insert("a".to_string(), vec![1.0]);
            c.insert("b".to_string(), vec![2.0]);
            // At capacity; inserting "c" should evict "a" (first inserted)
            c.insert("c".to_string(), vec![3.0]);
            assert_eq!(c.len(), 2, "cache should hold at most 2 entries");
            assert!(c.get("a").is_none(), "oldest entry 'a' should be evicted");
            assert!(c.get("b").is_some());
            assert!(c.get("c").is_some());
        }
    }
}
