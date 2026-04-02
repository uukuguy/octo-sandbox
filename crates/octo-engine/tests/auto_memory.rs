//! Integration tests for the auto-memory system (D4-1 through D4-5).
//!
//! Covers: RuleBasedExtractor pipeline, file/command/preference extraction,
//! SessionEndMemoryHook, MemoryInjector formatting, category roundtrip, serde.

use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use octo_engine::memory::{
    AutoMemoryCategory, ExtractedMemory, MemoryExtractor, MemoryInjectionConfig, MemoryInjector,
    MemoryStore, RuleBasedExtractor, SessionEndMemoryHook, SessionMemoryConfig,
};
use octo_types::{
    ChatMessage, MemoryCategory, MemoryEntry, MemoryFilter, MemoryId, MemoryResult, MemorySource,
    SearchOptions,
};

// ── Mock MemoryStore ──

struct MockMemoryStore {
    stored: Mutex<Vec<MemoryEntry>>,
    search_results: Vec<MemoryResult>,
}

impl MockMemoryStore {
    fn new() -> Self {
        Self {
            stored: Mutex::new(Vec::new()),
            search_results: vec![],
        }
    }

    fn with_search_results(results: Vec<MemoryResult>) -> Self {
        Self {
            stored: Mutex::new(Vec::new()),
            search_results: results,
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
        Ok(self.search_results.clone())
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

fn make_result(category: MemoryCategory, content: &str, score: f32) -> MemoryResult {
    MemoryResult {
        entry: MemoryEntry::new("test-user", category, content),
        score,
        match_source: "fts".to_string(),
    }
}

// ── Test 1: Full extraction pipeline ──

#[tokio::test]
async fn test_extraction_pipeline() {
    let extractor = RuleBasedExtractor::new();
    let messages = vec![
        ChatMessage::user("I modified src/main.rs and crates/octo-engine/src/agent/dual.rs"),
        ChatMessage::assistant("I'll update those files. Run:\n$ cargo test --workspace"),
        ChatMessage::user("We decided to use async-trait for all interfaces."),
    ];

    let memories = extractor.extract(&messages).await;
    assert!(!memories.is_empty(), "Should extract at least one memory");

    // Check that multiple categories are present
    let has_structure = memories
        .iter()
        .any(|m| m.category == AutoMemoryCategory::ProjectStructure);
    let has_command = memories
        .iter()
        .any(|m| m.category == AutoMemoryCategory::CommandPattern);
    let has_decision = memories
        .iter()
        .any(|m| m.category == AutoMemoryCategory::TechnicalDecision);

    assert!(has_structure, "Should extract ProjectStructure from file paths");
    assert!(has_command, "Should extract CommandPattern from $ command");
    assert!(has_decision, "Should extract TechnicalDecision from 'we decided'");
}

// ── Test 2: File path extraction accuracy ──

#[tokio::test]
async fn test_file_path_extraction() {
    let extractor = RuleBasedExtractor::new();
    let messages = vec![
        ChatMessage::user("I modified src/main.rs and crates/octo-engine/src/agent/dual.rs"),
        ChatMessage::assistant("I'll update those files."),
    ];

    let memories = extractor.extract(&messages).await;
    let file_memories: Vec<_> = memories
        .iter()
        .filter(|m| m.category == AutoMemoryCategory::ProjectStructure)
        .collect();

    assert!(
        file_memories.len() >= 2,
        "Should extract at least 2 file paths, got {}",
        file_memories.len()
    );

    let values: Vec<&str> = file_memories.iter().map(|m| m.value.as_str()).collect();
    assert!(
        values.contains(&"src/main.rs"),
        "Should extract src/main.rs, got: {:?}",
        values
    );
    assert!(
        values.contains(&"crates/octo-engine/src/agent/dual.rs"),
        "Should extract dual.rs path, got: {:?}",
        values
    );

    // All file path memories should have correct tags and confidence
    for mem in &file_memories {
        assert!(mem.tags.contains(&"file_path".to_string()));
        assert!((mem.confidence - 0.7).abs() < f64::EPSILON);
    }
}

// ── Test 3: Command extraction accuracy ──

#[tokio::test]
async fn test_command_extraction() {
    let extractor = RuleBasedExtractor::new();
    // The extractor matches lines starting with "$ " or bare "cargo "/"npm "/"make "
    let messages = vec![
        ChatMessage::user("Run this:\n$ cargo test --workspace"),
        ChatMessage::assistant("cargo build --release"),
    ];

    let memories = extractor.extract(&messages).await;
    let cmd_memories: Vec<_> = memories
        .iter()
        .filter(|m| m.category == AutoMemoryCategory::CommandPattern)
        .collect();

    assert!(
        !cmd_memories.is_empty(),
        "Should extract at least one command"
    );

    let values: Vec<&str> = cmd_memories.iter().map(|m| m.value.as_str()).collect();
    assert!(
        values.contains(&"cargo test --workspace"),
        "Should extract '$ cargo test --workspace', got: {:?}",
        values
    );

    // Bare "cargo build --release" from assistant message should also be captured
    assert!(
        values.iter().any(|v| v.contains("cargo build")),
        "Should extract bare 'cargo build' command, got: {:?}",
        values
    );
}

// ── Test 4: Preference extraction ──

#[tokio::test]
async fn test_preference_extraction() {
    let extractor = RuleBasedExtractor::new();
    let messages = vec![ChatMessage::user(
        "We always use black formatter for Python code. Never use print for debugging.",
    )];

    let memories = extractor.extract(&messages).await;
    let pref_memories: Vec<_> = memories
        .iter()
        .filter(|m| m.category == AutoMemoryCategory::UserPreference)
        .collect();

    assert!(
        !pref_memories.is_empty(),
        "Should extract user preferences from 'always use' / 'never use' patterns"
    );

    // Verify at least one contains "always use" or "never use"
    let any_match = pref_memories
        .iter()
        .any(|m| m.value.contains("always use") || m.value.contains("never use"));
    assert!(any_match, "Extracted preferences should contain keyword patterns");
}

// ── Test 5: SessionEndHook end-to-end ──

#[tokio::test]
async fn test_session_end_hook_stores_memories() {
    let hook = SessionEndMemoryHook::with_defaults();
    let store = MockMemoryStore::new();

    let messages = vec![
        ChatMessage::user("Please look at src/main.rs and crates/octo-engine/Cargo.toml"),
        ChatMessage::assistant("Found it. Run:\n$ cargo test --workspace"),
        ChatMessage::user("We decided to use async-trait for all interfaces."),
    ];

    let count = hook.on_session_end(&messages, &store, "test-user").await;
    assert!(count > 0, "Should store at least one memory, got {count}");

    let entries = store.stored_entries();
    assert!(!entries.is_empty());

    // All entries should have correct user_id and source_type
    for entry in &entries {
        assert_eq!(entry.user_id, "test-user");
        assert_eq!(entry.source_type, MemorySource::Extracted);
    }

    // Check category mapping: ProjectStructure -> Profile
    let profile_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.category == MemoryCategory::Profile)
        .collect();
    assert!(
        !profile_entries.is_empty(),
        "File paths should map to Profile category"
    );
}

// ── Test 6: MemoryInjector output format ──

#[tokio::test]
async fn test_memory_injector_output_format() {
    let store = MockMemoryStore::with_search_results(vec![
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

    assert!(
        result.contains("<cross-session-memory>"),
        "Output should contain header"
    );
    assert!(
        result.contains("[profile] User prefers dark mode"),
        "Should format entries as [category] content"
    );
    assert!(
        result.contains("[preferences] Use TypeScript for new projects"),
        "Should include second entry"
    );
    assert!(
        result.contains("Do NOT repeat or output these"),
        "Should contain instruction text"
    );
}

// ── Test 7: Category mapping roundtrip ──

#[test]
fn test_auto_memory_category_display_parse_roundtrip() {
    let categories = [
        AutoMemoryCategory::ProjectStructure,
        AutoMemoryCategory::UserPreference,
        AutoMemoryCategory::CommandPattern,
        AutoMemoryCategory::TechnicalDecision,
        AutoMemoryCategory::ContextualFact,
    ];

    for cat in &categories {
        let display_str = cat.to_string();
        let as_str = cat.as_str();

        // Display and as_str should produce the same string
        assert_eq!(
            display_str, as_str,
            "Display and as_str mismatch for {:?}",
            cat
        );

        // Parsing back should yield the same variant
        let parsed = AutoMemoryCategory::from_str_name(&display_str);
        assert_eq!(
            parsed.as_ref(),
            Some(cat),
            "Roundtrip failed for {:?} -> '{}' -> {:?}",
            cat,
            display_str,
            parsed
        );
    }
}

// ── Test 8: Empty conversation yields no memories ──

#[tokio::test]
async fn test_empty_conversation_no_memories() {
    let extractor = RuleBasedExtractor::new();
    let memories = extractor.extract(&[]).await;
    assert!(memories.is_empty(), "Empty messages should produce no memories");
}

// ── Test 9: Confidence filtering in hook ──

#[tokio::test]
async fn test_confidence_filtering_in_hook() {
    let hook = SessionEndMemoryHook::new(
        Box::new(RuleBasedExtractor::new()),
        SessionMemoryConfig {
            min_confidence: 0.95, // Higher than any rule-based confidence
            max_extractions: 20,
            enabled: true,
        },
    );
    let store = MockMemoryStore::new();

    let messages = vec![
        ChatMessage::user("Check src/main.rs"),           // confidence 0.7
        ChatMessage::assistant("Run:\n$ cargo test"),      // confidence 0.8
        ChatMessage::user("We always use Rust for APIs."), // confidence 0.7
    ];

    let count = hook.on_session_end(&messages, &store, "user1").await;
    assert_eq!(
        count, 0,
        "No memories should pass 0.95 confidence filter, got {count}"
    );
    assert_eq!(store.stored_count(), 0);
}

// ── Test 10: ExtractedMemory serde roundtrip ──

#[test]
fn test_extracted_memory_serde_roundtrip() {
    let mem = ExtractedMemory {
        key: "path:src/main.rs".to_string(),
        value: "src/main.rs".to_string(),
        category: AutoMemoryCategory::ProjectStructure,
        tags: vec!["auto".to_string(), "file_path".to_string()],
        confidence: 0.7,
    };

    let json = serde_json::to_string(&mem).unwrap();
    let decoded: ExtractedMemory = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.key, mem.key);
    assert_eq!(decoded.value, mem.value);
    assert_eq!(decoded.category, mem.category);
    assert_eq!(decoded.tags, mem.tags);
    assert!((decoded.confidence - mem.confidence).abs() < f64::EPSILON);
}

// ── Test 11: MemoryInjector disabled returns empty ──

#[tokio::test]
async fn test_memory_injector_disabled_returns_empty() {
    let store = MockMemoryStore::with_search_results(vec![make_result(
        MemoryCategory::Profile,
        "some memory",
        0.9,
    )]);

    let injector = MemoryInjector::new(MemoryInjectionConfig {
        enabled: false,
        ..Default::default()
    });

    let result = injector
        .build_memory_context(&store, "user1", "query")
        .await;
    assert!(
        result.is_empty(),
        "Disabled injector should return empty string"
    );
}

// ── Test 12: SessionEndHook with empty messages returns zero ──

#[tokio::test]
async fn test_session_end_hook_empty_messages() {
    let hook = SessionEndMemoryHook::with_defaults();
    let store = MockMemoryStore::new();

    let count = hook.on_session_end(&[], &store, "user1").await;
    assert_eq!(count, 0, "Empty messages should store nothing");
    assert_eq!(store.stored_count(), 0);
}
