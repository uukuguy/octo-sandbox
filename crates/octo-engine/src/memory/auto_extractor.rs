//! Auto-memory extraction — automatically extracts key information from conversations
//!
//! This module provides a trait-based system for extracting memories from conversation
//! messages. The [`RuleBasedExtractor`] uses pattern matching for fast, zero-cost
//! extraction. For more nuanced extraction, see the existing [`FactExtractor`] which
//! uses LLM-based analysis.

use async_trait::async_trait;
use octo_types::ChatMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Category of automatically extracted memory.
///
/// These categories represent the types of knowledge that can be
/// automatically captured from conversations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoMemoryCategory {
    /// File paths, module relationships, directory structure
    ProjectStructure,
    /// Coding style, tool preferences, workflow patterns
    UserPreference,
    /// Frequently used command sequences
    CommandPattern,
    /// Technology choices, architecture decisions
    TechnicalDecision,
    /// Project-specific facts and context
    ContextualFact,
}

impl std::fmt::Display for AutoMemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectStructure => write!(f, "project_structure"),
            Self::UserPreference => write!(f, "user_preference"),
            Self::CommandPattern => write!(f, "command_pattern"),
            Self::TechnicalDecision => write!(f, "technical_decision"),
            Self::ContextualFact => write!(f, "contextual_fact"),
        }
    }
}

impl AutoMemoryCategory {
    /// Parse from string representation.
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "project_structure" => Some(Self::ProjectStructure),
            "user_preference" => Some(Self::UserPreference),
            "command_pattern" => Some(Self::CommandPattern),
            "technical_decision" => Some(Self::TechnicalDecision),
            "contextual_fact" => Some(Self::ContextualFact),
            _ => None,
        }
    }

    /// Return the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProjectStructure => "project_structure",
            Self::UserPreference => "user_preference",
            Self::CommandPattern => "command_pattern",
            Self::TechnicalDecision => "technical_decision",
            Self::ContextualFact => "contextual_fact",
        }
    }
}

/// An extracted memory entry before storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMemory {
    /// Unique key for deduplication
    pub key: String,
    /// Human-readable value/content
    pub value: String,
    /// Category of this memory
    pub category: AutoMemoryCategory,
    /// Tags for search and filtering
    pub tags: Vec<String>,
    /// Confidence score (0.0 - 1.0) -- how confident the extractor is
    pub confidence: f64,
}

/// Trait for extracting memories from conversation messages.
///
/// Implementations scan conversation history and produce [`ExtractedMemory`]
/// entries that can then be stored in persistent memory.
#[async_trait]
pub trait MemoryExtractor: Send + Sync {
    /// Extract memories from a sequence of conversation messages.
    async fn extract(&self, messages: &[ChatMessage]) -> Vec<ExtractedMemory>;

    /// Name of this extractor (for logging/debugging).
    fn name(&self) -> &str;
}

/// Rule-based memory extractor that uses pattern matching
/// to identify common patterns in conversations.
///
/// This is a fast, zero-cost extractor that catches obvious patterns.
/// For more nuanced extraction, use the LLM-based [`FactExtractor`](super::extractor::FactExtractor).
pub struct RuleBasedExtractor;

impl RuleBasedExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract file paths mentioned in messages.
    fn extract_file_paths(messages: &[ChatMessage]) -> Vec<ExtractedMemory> {
        let mut memories = Vec::new();
        let mut seen_paths = HashSet::new();

        for msg in messages {
            let text = msg.text_content();
            for word in text.split_whitespace() {
                let clean = word.trim_matches(|c: char| {
                    !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-'
                });
                if is_likely_file_path(clean) && seen_paths.insert(clean.to_string()) {
                    memories.push(ExtractedMemory {
                        key: format!("path:{}", clean),
                        value: clean.to_string(),
                        category: AutoMemoryCategory::ProjectStructure,
                        tags: vec!["auto".to_string(), "file_path".to_string()],
                        confidence: 0.7,
                    });
                }
            }
        }
        memories
    }

    /// Extract command patterns from messages.
    fn extract_commands(messages: &[ChatMessage]) -> Vec<ExtractedMemory> {
        let mut memories = Vec::new();
        let mut seen = HashSet::new();

        for msg in messages {
            let text = msg.text_content();
            for line in text.lines() {
                let trimmed = line.trim();
                // Match shell commands prefixed with "$ "
                if let Some(cmd) = trimmed.strip_prefix("$ ") {
                    if !cmd.is_empty() && seen.insert(cmd.to_string()) {
                        memories.push(ExtractedMemory {
                            key: format!("cmd:{}", &cmd[..cmd.len().min(50)]),
                            value: cmd.to_string(),
                            category: AutoMemoryCategory::CommandPattern,
                            tags: vec!["auto".to_string(), "command".to_string()],
                            confidence: 0.8,
                        });
                    }
                }
                // Match common build/test commands
                if (trimmed.starts_with("cargo ")
                    || trimmed.starts_with("npm ")
                    || trimmed.starts_with("make "))
                    && seen.insert(trimmed.to_string())
                {
                    memories.push(ExtractedMemory {
                        key: format!("cmd:{}", &trimmed[..trimmed.len().min(50)]),
                        value: trimmed.to_string(),
                        category: AutoMemoryCategory::CommandPattern,
                        tags: vec!["auto".to_string(), "command".to_string()],
                        confidence: 0.6,
                    });
                }
            }
        }
        memories
    }

    /// Extract technical decisions and preferences from messages.
    fn extract_preferences(messages: &[ChatMessage]) -> Vec<ExtractedMemory> {
        let mut memories = Vec::new();
        let mut seen_keys = HashSet::new();
        let patterns: &[(&str, AutoMemoryCategory, f64)] = &[
            ("always use", AutoMemoryCategory::UserPreference, 0.7),
            ("prefer ", AutoMemoryCategory::UserPreference, 0.6),
            ("never use", AutoMemoryCategory::UserPreference, 0.7),
            ("we decided", AutoMemoryCategory::TechnicalDecision, 0.8),
            (
                "the approach is",
                AutoMemoryCategory::TechnicalDecision,
                0.6,
            ),
            (
                "architecture:",
                AutoMemoryCategory::TechnicalDecision,
                0.7,
            ),
        ];

        for msg in messages {
            let text = msg.text_content().to_lowercase();
            for (pattern, category, confidence) in patterns {
                if text.contains(pattern) {
                    for sentence in text.split('.') {
                        if sentence.contains(pattern) {
                            let trimmed = sentence.trim();
                            if trimmed.len() > 10 {
                                let key =
                                    format!("pref:{}", &trimmed[..trimmed.len().min(40)]);
                                if seen_keys.insert(key.clone()) {
                                    memories.push(ExtractedMemory {
                                        key,
                                        value: trimmed.to_string(),
                                        category: category.clone(),
                                        tags: vec!["auto".to_string()],
                                        confidence: *confidence,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        memories
    }
}

impl Default for RuleBasedExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryExtractor for RuleBasedExtractor {
    async fn extract(&self, messages: &[ChatMessage]) -> Vec<ExtractedMemory> {
        let mut memories = Vec::new();
        memories.extend(Self::extract_file_paths(messages));
        memories.extend(Self::extract_commands(messages));
        memories.extend(Self::extract_preferences(messages));
        memories
    }

    fn name(&self) -> &str {
        "rule-based"
    }
}

/// Check if a string looks like a file path.
fn is_likely_file_path(s: &str) -> bool {
    if s.len() < 3 || s.len() > 200 {
        return false;
    }
    let has_separator = s.contains('/') || s.contains('.');
    if !has_separator {
        return false;
    }
    let has_extension = s.rsplit('.').next().is_some_and(|ext| {
        matches!(
            ext,
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "toml"
                | "yaml"
                | "yml"
                | "json"
                | "md"
                | "html"
                | "css"
                | "sql"
        )
    });
    let has_dirs = s.contains('/') && s.split('/').count() >= 2;
    has_extension || has_dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AutoMemoryCategory tests ──

    #[test]
    fn test_category_display_all_variants() {
        assert_eq!(AutoMemoryCategory::ProjectStructure.to_string(), "project_structure");
        assert_eq!(AutoMemoryCategory::UserPreference.to_string(), "user_preference");
        assert_eq!(AutoMemoryCategory::CommandPattern.to_string(), "command_pattern");
        assert_eq!(AutoMemoryCategory::TechnicalDecision.to_string(), "technical_decision");
        assert_eq!(AutoMemoryCategory::ContextualFact.to_string(), "contextual_fact");
    }

    #[test]
    fn test_category_from_str_all_variants() {
        assert_eq!(
            AutoMemoryCategory::from_str_name("project_structure"),
            Some(AutoMemoryCategory::ProjectStructure)
        );
        assert_eq!(
            AutoMemoryCategory::from_str_name("user_preference"),
            Some(AutoMemoryCategory::UserPreference)
        );
        assert_eq!(
            AutoMemoryCategory::from_str_name("command_pattern"),
            Some(AutoMemoryCategory::CommandPattern)
        );
        assert_eq!(
            AutoMemoryCategory::from_str_name("technical_decision"),
            Some(AutoMemoryCategory::TechnicalDecision)
        );
        assert_eq!(
            AutoMemoryCategory::from_str_name("contextual_fact"),
            Some(AutoMemoryCategory::ContextualFact)
        );
    }

    #[test]
    fn test_category_from_str_case_insensitive() {
        assert_eq!(
            AutoMemoryCategory::from_str_name("PROJECT_STRUCTURE"),
            Some(AutoMemoryCategory::ProjectStructure)
        );
        assert_eq!(
            AutoMemoryCategory::from_str_name("  User_Preference  "),
            Some(AutoMemoryCategory::UserPreference)
        );
    }

    #[test]
    fn test_category_from_str_invalid() {
        assert_eq!(AutoMemoryCategory::from_str_name("invalid"), None);
        assert_eq!(AutoMemoryCategory::from_str_name(""), None);
        assert_eq!(AutoMemoryCategory::from_str_name("project"), None);
    }

    #[test]
    fn test_category_as_str_roundtrip() {
        let categories = [
            AutoMemoryCategory::ProjectStructure,
            AutoMemoryCategory::UserPreference,
            AutoMemoryCategory::CommandPattern,
            AutoMemoryCategory::TechnicalDecision,
            AutoMemoryCategory::ContextualFact,
        ];
        for cat in &categories {
            let s = cat.as_str();
            let parsed = AutoMemoryCategory::from_str_name(s).unwrap();
            assert_eq!(&parsed, cat);
        }
    }

    // ── is_likely_file_path tests ──

    #[test]
    fn test_is_likely_file_path_valid() {
        assert!(is_likely_file_path("src/main.rs"));
        assert!(is_likely_file_path("crates/foo/bar.rs"));
        assert!(is_likely_file_path("Cargo.toml"));
        assert!(is_likely_file_path("package.json"));
        assert!(is_likely_file_path("web/src/App.tsx"));
        assert!(is_likely_file_path("docs/design/PLAN.md"));
        assert!(is_likely_file_path("config.yaml"));
        assert!(is_likely_file_path("schema.sql"));
        assert!(is_likely_file_path("index.html"));
        assert!(is_likely_file_path("styles.css"));
    }

    #[test]
    fn test_is_likely_file_path_directories() {
        assert!(is_likely_file_path("src/components/chat"));
        assert!(is_likely_file_path("/usr/local/bin"));
    }

    #[test]
    fn test_is_likely_file_path_invalid() {
        assert!(!is_likely_file_path("hello"));
        assert!(!is_likely_file_path("a"));
        assert!(!is_likely_file_path("ab"));
        assert!(!is_likely_file_path(""));
        assert!(!is_likely_file_path("just-a-word"));
        assert!(!is_likely_file_path("no_extension_no_slash"));
    }

    #[test]
    fn test_is_likely_file_path_edge_cases() {
        // Too long (> 200 chars)
        let long_path = "a/".repeat(101);
        assert!(!is_likely_file_path(&long_path));
        // Unknown extension without directories
        assert!(!is_likely_file_path("file.xyz"));
        // Has dots but no known extension and no slash
        assert!(!is_likely_file_path("foo.bar"));
    }

    // ── RuleBasedExtractor::extract_file_paths tests ──

    #[tokio::test]
    async fn test_extract_file_paths() {
        let messages = vec![
            ChatMessage::user("Please check src/main.rs and crates/octo-engine/Cargo.toml"),
            ChatMessage::assistant("I found the issue in web/src/App.tsx"),
        ];
        let results = RuleBasedExtractor::extract_file_paths(&messages);
        assert!(results.len() >= 3);
        let values: Vec<&str> = results.iter().map(|m| m.value.as_str()).collect();
        assert!(values.contains(&"src/main.rs"));
        assert!(values.contains(&"crates/octo-engine/Cargo.toml"));
        assert!(values.contains(&"web/src/App.tsx"));
        for mem in &results {
            assert_eq!(mem.category, AutoMemoryCategory::ProjectStructure);
            assert!(mem.tags.contains(&"file_path".to_string()));
            assert!(mem.tags.contains(&"auto".to_string()));
            assert!((mem.confidence - 0.7).abs() < f64::EPSILON);
        }
    }

    #[tokio::test]
    async fn test_extract_file_paths_deduplication() {
        let messages = vec![
            ChatMessage::user("Look at src/main.rs"),
            ChatMessage::user("Also check src/main.rs again"),
        ];
        let results = RuleBasedExtractor::extract_file_paths(&messages);
        let main_count = results.iter().filter(|m| m.value == "src/main.rs").count();
        assert_eq!(main_count, 1, "Duplicate paths should be deduplicated");
    }

    // ── RuleBasedExtractor::extract_commands tests ──

    #[tokio::test]
    async fn test_extract_commands_dollar_prefix() {
        let messages = vec![ChatMessage::user("Run this:\n$ cargo test --workspace\n$ npm install")];
        let results = RuleBasedExtractor::extract_commands(&messages);
        assert!(results.len() >= 2);
        let values: Vec<&str> = results.iter().map(|m| m.value.as_str()).collect();
        assert!(values.contains(&"cargo test --workspace"));
        assert!(values.contains(&"npm install"));
        for mem in &results {
            assert_eq!(mem.category, AutoMemoryCategory::CommandPattern);
            assert!(mem.tags.contains(&"command".to_string()));
        }
    }

    #[tokio::test]
    async fn test_extract_commands_bare_commands() {
        let messages =
            vec![ChatMessage::assistant("cargo build --release\nmake test\nnpm run lint")];
        let results = RuleBasedExtractor::extract_commands(&messages);
        let values: Vec<&str> = results.iter().map(|m| m.value.as_str()).collect();
        assert!(values.contains(&"cargo build --release"));
        assert!(values.contains(&"make test"));
        assert!(values.contains(&"npm run lint"));
    }

    #[tokio::test]
    async fn test_extract_commands_deduplication() {
        let messages = vec![
            ChatMessage::user("$ cargo test"),
            ChatMessage::user("cargo test"),
        ];
        let results = RuleBasedExtractor::extract_commands(&messages);
        let cargo_count = results
            .iter()
            .filter(|m| m.value == "cargo test")
            .count();
        assert_eq!(cargo_count, 1, "Duplicate commands should be deduplicated");
    }

    #[tokio::test]
    async fn test_extract_commands_empty_dollar() {
        let messages = vec![ChatMessage::user("$ ")];
        let results = RuleBasedExtractor::extract_commands(&messages);
        // "$ " with nothing after should be ignored
        assert!(
            results.is_empty(),
            "Empty dollar command should be ignored"
        );
    }

    // ── RuleBasedExtractor::extract_preferences tests ──

    #[tokio::test]
    async fn test_extract_preferences_always_use() {
        let messages =
            vec![ChatMessage::user("We should always use Rust for backend services.")];
        let results = RuleBasedExtractor::extract_preferences(&messages);
        assert!(!results.is_empty());
        assert_eq!(results[0].category, AutoMemoryCategory::UserPreference);
        assert!(results[0].value.contains("always use"));
    }

    #[tokio::test]
    async fn test_extract_preferences_technical_decision() {
        let messages = vec![ChatMessage::user(
            "We decided to use SQLite for local storage. The approach is event sourcing.",
        )];
        let results = RuleBasedExtractor::extract_preferences(&messages);
        let decisions: Vec<_> = results
            .iter()
            .filter(|m| m.category == AutoMemoryCategory::TechnicalDecision)
            .collect();
        assert!(
            !decisions.is_empty(),
            "Should extract technical decisions"
        );
    }

    #[tokio::test]
    async fn test_extract_preferences_short_sentence_ignored() {
        let messages = vec![ChatMessage::user("prefer x.")];
        let results = RuleBasedExtractor::extract_preferences(&messages);
        // "prefer x" is only 8 chars, below the 10-char minimum
        assert!(
            results.is_empty(),
            "Short sentences should be ignored"
        );
    }

    // ── Full extraction (MemoryExtractor trait) tests ──

    #[tokio::test]
    async fn test_full_extraction_mixed_messages() {
        let extractor = RuleBasedExtractor::new();
        let messages = vec![
            ChatMessage::user("Check src/lib.rs for the issue"),
            ChatMessage::assistant("I see the problem. Run:\n$ cargo test --workspace"),
            ChatMessage::user("We decided to use async-trait for all interfaces."),
        ];
        let results = extractor.extract(&messages).await;
        // Should have file path + command + preference
        let categories: HashSet<_> = results.iter().map(|m| m.category.clone()).collect();
        assert!(
            categories.contains(&AutoMemoryCategory::ProjectStructure),
            "Should extract file paths"
        );
        assert!(
            categories.contains(&AutoMemoryCategory::CommandPattern),
            "Should extract commands"
        );
        assert!(
            categories.contains(&AutoMemoryCategory::TechnicalDecision),
            "Should extract decisions"
        );
    }

    #[tokio::test]
    async fn test_full_extraction_empty_messages() {
        let extractor = RuleBasedExtractor::new();
        let results = extractor.extract(&[]).await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_extractor_name() {
        let extractor = RuleBasedExtractor::new();
        assert_eq!(extractor.name(), "rule-based");
    }

    #[test]
    fn test_default_impl() {
        let _extractor = RuleBasedExtractor::default();
    }

    // ── ExtractedMemory serde roundtrip ──

    #[test]
    fn test_extracted_memory_serde_roundtrip() {
        let memory = ExtractedMemory {
            key: "path:src/main.rs".to_string(),
            value: "src/main.rs".to_string(),
            category: AutoMemoryCategory::ProjectStructure,
            tags: vec!["auto".to_string(), "file_path".to_string()],
            confidence: 0.7,
        };
        let json = serde_json::to_string(&memory).unwrap();
        let deserialized: ExtractedMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.key, memory.key);
        assert_eq!(deserialized.value, memory.value);
        assert_eq!(deserialized.category, memory.category);
        assert_eq!(deserialized.tags, memory.tags);
        assert!((deserialized.confidence - memory.confidence).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extracted_memory_serde_all_categories() {
        let categories = [
            AutoMemoryCategory::ProjectStructure,
            AutoMemoryCategory::UserPreference,
            AutoMemoryCategory::CommandPattern,
            AutoMemoryCategory::TechnicalDecision,
            AutoMemoryCategory::ContextualFact,
        ];
        for cat in &categories {
            let memory = ExtractedMemory {
                key: "test".to_string(),
                value: "test".to_string(),
                category: cat.clone(),
                tags: vec![],
                confidence: 0.5,
            };
            let json = serde_json::to_string(&memory).unwrap();
            let deserialized: ExtractedMemory = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.category, *cat);
        }
    }
}
