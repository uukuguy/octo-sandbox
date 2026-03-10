//! Session end hook — triggers auto-memory extraction when a session ends.
//!
//! When a session concludes, [`SessionEndMemoryHook`] extracts key information
//! (file paths, commands, preferences, decisions) from the conversation and
//! persists them as [`MemoryEntry`] records in the L2 persistent store.

use octo_types::{ChatMessage, MemoryCategory, MemoryEntry, MemorySource};

use super::auto_extractor::{AutoMemoryCategory, ExtractedMemory, MemoryExtractor, RuleBasedExtractor};
use super::store_traits::MemoryStore;

/// Configuration for the session end memory extraction.
#[derive(Debug, Clone)]
pub struct SessionMemoryConfig {
    /// Minimum confidence to store an extracted memory.
    pub min_confidence: f64,
    /// Maximum number of memories to extract per session.
    pub max_extractions: usize,
    /// Whether auto-extraction is enabled.
    pub enabled: bool,
}

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            max_extractions: 20,
            enabled: true,
        }
    }
}

/// Hook that runs at session end to extract and store memories.
///
/// Uses a [`MemoryExtractor`] to scan conversation messages and persists
/// the results via a [`MemoryStore`].
pub struct SessionEndMemoryHook {
    extractor: Box<dyn MemoryExtractor>,
    config: SessionMemoryConfig,
}

impl SessionEndMemoryHook {
    /// Create with the default rule-based extractor.
    pub fn with_defaults() -> Self {
        Self {
            extractor: Box::new(RuleBasedExtractor::new()),
            config: SessionMemoryConfig::default(),
        }
    }

    /// Create with a custom extractor and config.
    pub fn new(extractor: Box<dyn MemoryExtractor>, config: SessionMemoryConfig) -> Self {
        Self { extractor, config }
    }

    /// Run extraction on the session's messages and store results.
    ///
    /// Returns the number of memories successfully stored.
    pub async fn on_session_end(
        &self,
        messages: &[ChatMessage],
        store: &dyn MemoryStore,
        user_id: &str,
    ) -> usize {
        if !self.config.enabled || messages.is_empty() {
            return 0;
        }

        let extracted = self.extractor.extract(messages).await;

        let filtered: Vec<&ExtractedMemory> = extracted
            .iter()
            .filter(|m| m.confidence >= self.config.min_confidence)
            .take(self.config.max_extractions)
            .collect();

        let mut stored_count = 0;

        for memory in &filtered {
            let entry = create_memory_entry(memory, user_id);
            if store.store(entry).await.is_ok() {
                stored_count += 1;
            }
        }

        stored_count
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SessionMemoryConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: SessionMemoryConfig) {
        self.config = config;
    }
}

/// Convert an [`ExtractedMemory`] into a [`MemoryEntry`] for persistent storage.
fn create_memory_entry(extracted: &ExtractedMemory, user_id: &str) -> MemoryEntry {
    let category = match extracted.category {
        AutoMemoryCategory::UserPreference => MemoryCategory::Preferences,
        AutoMemoryCategory::TechnicalDecision => MemoryCategory::Patterns,
        AutoMemoryCategory::CommandPattern => MemoryCategory::Tools,
        AutoMemoryCategory::ProjectStructure => MemoryCategory::Profile,
        AutoMemoryCategory::ContextualFact => MemoryCategory::Profile,
    };

    let mut entry = MemoryEntry::new(user_id, category, &extracted.value);
    entry.source_type = MemorySource::Extracted;
    entry.source_ref = extracted.key.clone();
    entry.importance = extracted.confidence as f32;
    entry.metadata = serde_json::json!({
        "auto_category": extracted.category.as_str(),
        "tags": extracted.tags,
    });
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use async_trait::async_trait;
    use octo_types::{MemoryFilter, MemoryId, MemoryResult, SearchOptions};
    use std::sync::Mutex;

    // ── Mock MemoryStore ──

    struct MockMemoryStore {
        stored: Mutex<Vec<MemoryEntry>>,
    }

    impl MockMemoryStore {
        fn new() -> Self {
            Self {
                stored: Mutex::new(Vec::new()),
            }
        }

        fn stored_count(&self) -> usize {
            self.stored.lock().unwrap().len()
        }

        fn stored_entries(&self) -> Vec<MemoryEntry> {
            self.stored.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl MemoryStore for MockMemoryStore {
        async fn store(&self, entry: MemoryEntry) -> Result<MemoryId> {
            let id = entry.id.clone();
            self.stored.lock().unwrap().push(entry);
            Ok(id)
        }

        async fn search(&self, _query: &str, _opts: SearchOptions) -> Result<Vec<MemoryResult>> {
            Ok(vec![])
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

        async fn batch_store(&self, entries: Vec<MemoryEntry>) -> Result<Vec<MemoryId>> {
            let mut guard = self.stored.lock().unwrap();
            let ids: Vec<MemoryId> = entries.iter().map(|e| e.id.clone()).collect();
            guard.extend(entries);
            Ok(ids)
        }
    }

    // ── Tests ──

    #[test]
    fn test_config_default() {
        let config = SessionMemoryConfig::default();
        assert!((config.min_confidence - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.max_extractions, 20);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_disabled_returns_zero() {
        let mut hook = SessionEndMemoryHook::with_defaults();
        hook.set_config(SessionMemoryConfig {
            enabled: false,
            ..Default::default()
        });

        let messages = vec![ChatMessage::user("Check src/main.rs")];
        let store = MockMemoryStore::new();
        let count = hook.on_session_end(&messages, &store, "user1").await;
        assert_eq!(count, 0);
        assert_eq!(store.stored_count(), 0);
    }

    #[tokio::test]
    async fn test_empty_messages_returns_zero() {
        let hook = SessionEndMemoryHook::with_defaults();
        let store = MockMemoryStore::new();
        let count = hook.on_session_end(&[], &store, "user1").await;
        assert_eq!(count, 0);
        assert_eq!(store.stored_count(), 0);
    }

    #[tokio::test]
    async fn test_extraction_and_storage() {
        let hook = SessionEndMemoryHook::with_defaults();
        let store = MockMemoryStore::new();

        let messages = vec![
            ChatMessage::user("Please look at src/main.rs and crates/octo-engine/Cargo.toml"),
            ChatMessage::assistant("Found it. Run:\n$ cargo test --workspace"),
            ChatMessage::user("We decided to use async-trait for all interfaces."),
        ];

        let count = hook.on_session_end(&messages, &store, "test-user").await;
        assert!(count > 0, "Should store at least one memory");

        let entries = store.stored_entries();
        assert!(!entries.is_empty());

        // Verify all entries have correct user_id and source_type
        for entry in &entries {
            assert_eq!(entry.user_id, "test-user");
            assert_eq!(entry.source_type, MemorySource::Extracted);
        }
    }

    #[tokio::test]
    async fn test_confidence_filter() {
        // With very high min_confidence, fewer memories should pass
        let hook = SessionEndMemoryHook::new(
            Box::new(RuleBasedExtractor::new()),
            SessionMemoryConfig {
                min_confidence: 0.95,
                max_extractions: 20,
                enabled: true,
            },
        );
        let store = MockMemoryStore::new();

        let messages = vec![
            ChatMessage::user("Check src/main.rs"),
            ChatMessage::assistant("Run:\n$ cargo test"),
        ];

        let count = hook.on_session_end(&messages, &store, "user1").await;
        // File paths have confidence 0.7, bare commands 0.6, $ commands 0.8
        // None reach 0.95, so count should be 0
        assert_eq!(count, 0, "No memories should pass 0.95 confidence filter");
    }

    #[tokio::test]
    async fn test_max_extractions_limit() {
        let hook = SessionEndMemoryHook::new(
            Box::new(RuleBasedExtractor::new()),
            SessionMemoryConfig {
                min_confidence: 0.0,
                max_extractions: 2,
                enabled: true,
            },
        );
        let store = MockMemoryStore::new();

        // Generate many extractable patterns
        let messages = vec![
            ChatMessage::user(
                "Files: src/a.rs src/b.rs src/c.rs src/d.rs src/e.rs src/f.rs",
            ),
            ChatMessage::assistant("$ cargo test\n$ cargo build\n$ cargo clippy"),
        ];

        let count = hook.on_session_end(&messages, &store, "user1").await;
        assert!(count <= 2, "Should not exceed max_extractions of 2, got {count}");
    }

    #[test]
    fn test_create_memory_entry_mapping() {
        let extracted = ExtractedMemory {
            key: "path:src/main.rs".to_string(),
            value: "src/main.rs".to_string(),
            category: AutoMemoryCategory::ProjectStructure,
            tags: vec!["auto".to_string(), "file_path".to_string()],
            confidence: 0.7,
        };

        let entry = create_memory_entry(&extracted, "user1");
        assert_eq!(entry.user_id, "user1");
        assert_eq!(entry.category, MemoryCategory::Profile);
        assert_eq!(entry.content, "src/main.rs");
        assert_eq!(entry.source_type, MemorySource::Extracted);
        assert_eq!(entry.source_ref, "path:src/main.rs");
        assert!((entry.importance - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_create_memory_entry_category_mapping() {
        let cases = vec![
            (AutoMemoryCategory::UserPreference, MemoryCategory::Preferences),
            (AutoMemoryCategory::TechnicalDecision, MemoryCategory::Patterns),
            (AutoMemoryCategory::CommandPattern, MemoryCategory::Tools),
            (AutoMemoryCategory::ProjectStructure, MemoryCategory::Profile),
            (AutoMemoryCategory::ContextualFact, MemoryCategory::Profile),
        ];

        for (auto_cat, expected_cat) in cases {
            let extracted = ExtractedMemory {
                key: "test".to_string(),
                value: "test".to_string(),
                category: auto_cat,
                tags: vec![],
                confidence: 0.5,
            };
            let entry = create_memory_entry(&extracted, "u");
            assert_eq!(entry.category, expected_cat);
        }
    }

    #[test]
    fn test_config_accessors() {
        let mut hook = SessionEndMemoryHook::with_defaults();
        assert!(hook.config().enabled);
        assert_eq!(hook.config().max_extractions, 20);

        hook.set_config(SessionMemoryConfig {
            min_confidence: 0.9,
            max_extractions: 5,
            enabled: false,
        });
        assert!(!hook.config().enabled);
        assert_eq!(hook.config().max_extractions, 5);
        assert!((hook.config().min_confidence - 0.9).abs() < f64::EPSILON);
    }
}
