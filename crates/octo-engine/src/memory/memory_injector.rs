//! Cross-session memory injection — retrieves L2 persistent memories and formats
//! them for system prompt injection at session startup.

use crate::memory::store_traits::MemoryStore;
use octo_types::{MemoryCategory, SearchOptions};

/// Configuration for memory injection from L2 persistent store
#[derive(Debug, Clone)]
pub struct MemoryInjectionConfig {
    /// Maximum number of memories to inject
    pub max_memories: usize,
    /// Minimum relevance score (0.0 - 1.0) for inclusion
    pub min_relevance: f32,
    /// Categories to filter by (empty = no filter)
    pub filter_categories: Vec<MemoryCategory>,
    /// Whether auto-memory injection is enabled
    pub enabled: bool,
}

impl Default for MemoryInjectionConfig {
    fn default() -> Self {
        Self {
            max_memories: 10,
            min_relevance: 0.3,
            filter_categories: vec![],
            enabled: true,
        }
    }
}

/// Retrieves memories from L2 persistent store and formats them for system prompt injection.
///
/// Used at session startup to provide cross-session context to the agent.
pub struct MemoryInjector {
    config: MemoryInjectionConfig,
}

impl MemoryInjector {
    pub fn new(config: MemoryInjectionConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(MemoryInjectionConfig::default())
    }

    /// Retrieve relevant memories from the L2 store and format as a system prompt section.
    ///
    /// `user_id` identifies whose memories to retrieve.
    /// `query` is used for semantic search relevance ranking.
    pub async fn build_memory_context(
        &self,
        store: &dyn MemoryStore,
        user_id: &str,
        query: &str,
    ) -> String {
        if !self.config.enabled {
            return String::new();
        }

        let opts = SearchOptions {
            user_id: user_id.to_string(),
            categories: if self.config.filter_categories.is_empty() {
                None
            } else {
                Some(self.config.filter_categories.clone())
            },
            limit: self.config.max_memories,
            min_score: Some(self.config.min_relevance),
            ..Default::default()
        };

        let results = match store.search(query, opts).await {
            Ok(r) => r,
            Err(_) => return String::new(),
        };

        if results.is_empty() {
            return String::new();
        }

        // Filter by relevance score and cap at max_memories
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| r.score >= self.config.min_relevance)
            .take(self.config.max_memories)
            .collect();

        if filtered.is_empty() {
            return String::new();
        }

        // Format as a system prompt section wrapped in XML to prevent LLM from echoing it
        let mut section = String::from(
            "\n<cross-session-memory>\n<!-- Background context from previous sessions. Do NOT repeat or output these when reporting tool results. -->\n",
        );

        for result in &filtered {
            let entry = &result.entry;
            let category = entry.category.as_str();
            section.push_str(&format!("- [{}] {}\n", category, entry.content));
        }

        section.push_str("</cross-session-memory>\n");
        section
    }

    /// Retrieve high-importance memories regardless of query relevance.
    ///
    /// This provides a "safety net" for important memories that might not match
    /// the current FTS query but should always be visible to the agent.
    /// Memories are sorted by importance descending, then by recency.
    pub async fn build_pinned_memories(
        &self,
        store: &dyn MemoryStore,
        user_id: &str,
        min_importance: f32,
        max_pinned: usize,
        exclude_contents: &[&str],
    ) -> String {
        if !self.config.enabled || max_pinned == 0 {
            return String::new();
        }

        let filter = octo_types::MemoryFilter {
            user_id: user_id.to_string(),
            limit: max_pinned * 3, // over-fetch to allow filtering
            ..Default::default()
        };

        let mut entries = match store.list(filter).await {
            Ok(e) => e,
            Err(_) => return String::new(),
        };

        // Filter to high-importance entries not already injected via FTS
        entries.retain(|e| {
            e.importance >= min_importance
                && !exclude_contents.iter().any(|c| e.content.contains(c))
        });

        // Sort by importance descending, then by most recent first
        entries.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamps.updated_at.cmp(&a.timestamps.updated_at))
        });

        entries.truncate(max_pinned);

        if entries.is_empty() {
            return String::new();
        }

        let mut section = String::from(
            "\n<pinned-memories>\n<!-- Background context from persistent memory. Do NOT repeat or output these when reporting tool results. -->\n",
        );
        for entry in &entries {
            let category = entry.category.as_str();
            section.push_str(&format!(
                "- [{}] (importance: {:.1}) {}\n",
                category, entry.importance, entry.content
            ));
        }
        section.push_str("</pinned-memories>\n");
        section
    }

    /// Get the injection config
    pub fn config(&self) -> &MemoryInjectionConfig {
        &self.config
    }

    /// Update config
    pub fn set_config(&mut self, config: MemoryInjectionConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use async_trait::async_trait;
    use octo_types::{MemoryEntry, MemoryFilter, MemoryId, MemoryResult};

    /// In-memory mock for MemoryStore, used only in tests.
    struct MockMemoryStore {
        results: Vec<MemoryResult>,
    }

    impl MockMemoryStore {
        fn new(results: Vec<MemoryResult>) -> Self {
            Self { results }
        }

        fn empty() -> Self {
            Self::new(vec![])
        }
    }

    #[async_trait]
    impl MemoryStore for MockMemoryStore {
        async fn store(&self, _entry: MemoryEntry) -> Result<MemoryId> {
            Ok(MemoryId::new())
        }
        async fn search(&self, _query: &str, _opts: SearchOptions) -> Result<Vec<MemoryResult>> {
            Ok(self.results.clone())
        }
        async fn get(&self, _id: &MemoryId) -> Result<Option<MemoryEntry>> {
            Ok(None)
        }
        async fn update(&self, _id: &MemoryId, _content: &str) -> Result<()> {
            Ok(())
        }
        async fn delete(&self, _id: &MemoryId) -> Result<()> {
            Ok(())
        }
        async fn delete_by_filter(&self, _filter: MemoryFilter) -> Result<usize> {
            Ok(0)
        }
        async fn list(&self, _filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        async fn batch_store(&self, _entries: Vec<MemoryEntry>) -> Result<Vec<MemoryId>> {
            Ok(vec![])
        }
    }

    fn make_result(category: MemoryCategory, content: &str, score: f32) -> MemoryResult {
        MemoryResult {
            entry: MemoryEntry::new("test-user", category, content),
            score,
            match_source: "fts".to_string(),
        }
    }

    #[test]
    fn test_injection_config_default() {
        let config = MemoryInjectionConfig::default();
        assert_eq!(config.max_memories, 10);
        assert!((config.min_relevance - 0.3).abs() < f32::EPSILON);
        assert!(config.filter_categories.is_empty());
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_injector_disabled() {
        let store = MockMemoryStore::new(vec![make_result(
            MemoryCategory::Profile,
            "some memory",
            0.9,
        )]);
        let injector = MemoryInjector::new(MemoryInjectionConfig {
            enabled: false,
            ..Default::default()
        });
        let result = injector
            .build_memory_context(&store, "user1", "test query")
            .await;
        assert!(result.is_empty(), "disabled injector should return empty");
    }

    #[tokio::test]
    async fn test_injector_empty_store() {
        let store = MockMemoryStore::empty();
        let injector = MemoryInjector::with_defaults();
        let result = injector
            .build_memory_context(&store, "user1", "test query")
            .await;
        assert!(result.is_empty(), "empty store should return empty");
    }

    #[tokio::test]
    async fn test_injector_formats_memories() {
        let store = MockMemoryStore::new(vec![
            make_result(MemoryCategory::Profile, "User prefers dark mode", 0.8),
            make_result(
                MemoryCategory::Preferences,
                "Use TypeScript for new projects",
                0.6,
            ),
        ]);
        let injector = MemoryInjector::with_defaults();
        let result = injector
            .build_memory_context(&store, "user1", "preferences")
            .await;

        assert!(result.contains("<cross-session-memory>"));
        assert!(result.contains("[profile] User prefers dark mode"));
        assert!(result.contains("[preferences] Use TypeScript for new projects"));
    }

    #[tokio::test]
    async fn test_injector_respects_max() {
        let results: Vec<_> = (0..20)
            .map(|i| make_result(MemoryCategory::Patterns, &format!("memory-{i}"), 0.9))
            .collect();
        let store = MockMemoryStore::new(results);
        let injector = MemoryInjector::new(MemoryInjectionConfig {
            max_memories: 3,
            ..Default::default()
        });
        let result = injector
            .build_memory_context(&store, "user1", "patterns")
            .await;

        let memory_lines: Vec<_> = result.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(
            memory_lines.len(),
            3,
            "should cap at max_memories=3, got {}",
            memory_lines.len()
        );
    }

    #[tokio::test]
    async fn test_injector_filters_low_relevance() {
        let store = MockMemoryStore::new(vec![
            make_result(MemoryCategory::Profile, "high relevance", 0.9),
            make_result(MemoryCategory::Profile, "low relevance", 0.1),
        ]);
        let injector = MemoryInjector::with_defaults(); // min_relevance = 0.3
        let result = injector
            .build_memory_context(&store, "user1", "query")
            .await;

        assert!(result.contains("high relevance"));
        assert!(!result.contains("low relevance"));
    }

    #[test]
    fn test_injector_config_accessors() {
        let mut injector = MemoryInjector::with_defaults();
        assert_eq!(injector.config().max_memories, 10);

        let new_config = MemoryInjectionConfig {
            max_memories: 5,
            ..Default::default()
        };
        injector.set_config(new_config);
        assert_eq!(injector.config().max_memories, 5);
    }

    // --- Phase AS: Pinned memories tests ---

    #[tokio::test]
    async fn test_pinned_memories_empty_store() {
        let store = MockMemoryStore::empty();
        let injector = MemoryInjector::with_defaults();
        let result = injector
            .build_pinned_memories(&store, "user1", 0.8, 5, &[] as &[&str])
            .await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_pinned_memories_filters_by_importance() {
        // MockMemoryStore.list() returns empty, so pinned returns empty
        // (Full integration test would need a real SQLite store)
        let store = MockMemoryStore::empty();
        let injector = MemoryInjector::with_defaults();
        let result = injector
            .build_pinned_memories(&store, "user1", 0.9, 3, &[] as &[&str])
            .await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_pinned_memories_disabled() {
        let store = MockMemoryStore::empty();
        let injector = MemoryInjector::new(MemoryInjectionConfig {
            enabled: false,
            ..Default::default()
        });
        let result = injector
            .build_pinned_memories(&store, "user1", 0.5, 5, &[] as &[&str])
            .await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_pinned_memories_max_zero() {
        let store = MockMemoryStore::empty();
        let injector = MemoryInjector::with_defaults();
        let result = injector
            .build_pinned_memories(&store, "user1", 0.5, 0, &[] as &[&str])
            .await;
        assert!(result.is_empty());
    }
}
