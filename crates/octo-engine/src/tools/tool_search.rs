//! ToolSearchTool — fuzzy search across registered tools.
//!
//! Provides an LLM-callable tool that searches the tool registry by name
//! and description, returning ranked results.

use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde::Serialize;
use serde_json::{json, Value};

use super::traits::Tool;
use super::ToolRegistry;

/// Result of a tool search query.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSearchResult {
    pub name: String,
    pub description: String,
    pub score: u32,
}

pub struct ToolSearchTool {
    registry: Arc<StdMutex<ToolRegistry>>,
}

impl ToolSearchTool {
    pub fn new(registry: Arc<StdMutex<ToolRegistry>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Search for available tools by name or description. Returns ranked results with relevance scores.\n\
         Use this when there are many tools available and you need to find the right one.\n\
         More efficient than listing all tools when the registry has >50 tools.\n\
         When NOT to use: when you already know the tool name."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (matched against tool names and descriptions)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 10)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let registry = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        let results = hybrid_search_tools(&registry, query, limit);

        if results.is_empty() {
            Ok(ToolOutput::success(format!(
                "No tools found matching '{query}'"
            )))
        } else {
            let output =
                serde_json::to_string_pretty(&results).unwrap_or_else(|_| format!("{results:?}"));
            Ok(ToolOutput::success(output))
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "discovery"
    }
}

/// Search tools in a registry by query string.
///
/// Scoring:
/// - Exact name match: 100
/// - Name contains query: 80
/// - Description contains query: 40
/// - No match: 0 (filtered out)
pub fn search_tools(registry: &ToolRegistry, query: &str, limit: usize) -> Vec<ToolSearchResult> {
    let query_lower = query.to_lowercase();
    let mut results: Vec<ToolSearchResult> = registry
        .iter()
        .filter_map(|(name, tool)| {
            let spec = tool.spec();
            let name_lower = name.to_lowercase();
            let desc_lower = spec.description.to_lowercase();

            let score = if name_lower == query_lower {
                100
            } else if name_lower.contains(&query_lower) {
                80
            } else if desc_lower.contains(&query_lower) {
                40
            } else {
                return None;
            };

            Some(ToolSearchResult {
                name: name.clone(),
                description: spec.description.chars().take(100).collect(),
                score,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.truncate(limit);
    results
}

// ---------------------------------------------------------------------------
// AR-T7: Hybrid search with token-overlap semantic fallback
// ---------------------------------------------------------------------------

/// Tokenize a string into lowercase word tokens (alphanumeric only).
fn tokenize(s: &str) -> std::collections::HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2)
        .map(String::from)
        .collect()
}

/// Compute query coverage: fraction of query tokens found in candidate (0.0 to 1.0).
///
/// Uses query coverage (intersection / query_size) instead of Jaccard
/// (intersection / union) because queries are typically short while tool
/// descriptions are long, making symmetric Jaccard unfairly penalize matches.
fn jaccard_similarity(
    query_tokens: &std::collections::HashSet<String>,
    candidate_tokens: &std::collections::HashSet<String>,
) -> f64 {
    if query_tokens.is_empty() || candidate_tokens.is_empty() {
        return 0.0;
    }
    let intersection = query_tokens.intersection(candidate_tokens).count() as f64;
    intersection / query_tokens.len() as f64
}

/// Hybrid search: substring match first, then token-overlap fallback
/// for remaining capacity.
///
/// This avoids requiring an external embedding provider while still
/// providing fuzzy matching beyond simple substring containment.
pub fn hybrid_search_tools(
    registry: &ToolRegistry,
    query: &str,
    limit: usize,
) -> Vec<ToolSearchResult> {
    // Phase 1: Exact / substring matches
    let mut results = search_tools(registry, query, limit);

    if results.len() >= limit {
        return results;
    }

    // Phase 2: Token-overlap semantic fallback for remaining slots
    let remaining = limit - results.len();
    let matched_names: std::collections::HashSet<&str> =
        results.iter().map(|r| r.name.as_str()).collect();
    let query_tokens = tokenize(query);

    if query_tokens.is_empty() {
        return results;
    }

    let mut semantic_hits: Vec<(String, String, f64)> = registry
        .iter()
        .filter(|(name, _)| !matched_names.contains(name.as_str()))
        .filter_map(|(name, tool)| {
            let spec = tool.spec();
            let combined = format!("{} {}", name, spec.description);
            let tool_tokens = tokenize(&combined);
            let sim = jaccard_similarity(&query_tokens, &tool_tokens);
            if sim > 0.05 {
                Some((
                    name.clone(),
                    spec.description.chars().take(100).collect(),
                    sim,
                ))
            } else {
                None
            }
        })
        .collect();

    semantic_hits.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    semantic_hits.truncate(remaining);

    for (name, description, sim) in semantic_hits {
        results.push(ToolSearchResult {
            name,
            description,
            score: (sim * 60.0) as u32, // Normalize to 0-60 range
        });
    }

    results
}

// ---------------------------------------------------------------------------
// AR-D4: Persistent tool search index
// ---------------------------------------------------------------------------

use serde::Deserialize;

/// Pre-computed token index for fast hybrid search (AR-D4).
///
/// Caches tokenized tool names and descriptions so that `hybrid_search_tools`
/// doesn't need to re-tokenize the entire registry on every query.
/// Can be serialized to JSON for cross-session persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchIndex {
    /// Indexed entries: (tool_name, description_preview, token_set)
    entries: Vec<ToolSearchIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolSearchIndexEntry {
    name: String,
    description: String,
    tokens: Vec<String>, // sorted for deterministic serialization
}

impl ToolSearchIndex {
    /// Build index from a ToolRegistry.
    pub fn build(registry: &ToolRegistry) -> Self {
        let entries = registry
            .iter()
            .map(|(name, tool)| {
                let spec = tool.spec();
                let combined = format!("{} {}", name, spec.description);
                let mut tokens: Vec<String> = tokenize(&combined).into_iter().collect();
                tokens.sort();
                ToolSearchIndexEntry {
                    name: name.clone(),
                    description: spec.description.chars().take(100).collect(),
                    tokens,
                }
            })
            .collect();
        Self { entries }
    }

    /// Search using the pre-computed index.
    pub fn search(&self, query: &str, limit: usize) -> Vec<ToolSearchResult> {
        let query_lower = query.to_lowercase();
        let query_tokens = tokenize(query);

        // Phase 1: Exact/substring matches
        let mut results: Vec<ToolSearchResult> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let name_lower = entry.name.to_lowercase();
                let desc_lower = entry.description.to_lowercase();
                let score = if name_lower == query_lower {
                    100
                } else if name_lower.contains(&query_lower) {
                    80
                } else if desc_lower.contains(&query_lower) {
                    40
                } else {
                    return None;
                };
                Some(ToolSearchResult {
                    name: entry.name.clone(),
                    description: entry.description.clone(),
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.cmp(&a.score));
        results.truncate(limit);

        if results.len() >= limit || query_tokens.is_empty() {
            return results;
        }

        // Phase 2: Token-overlap fallback
        let remaining = limit - results.len();
        let matched: std::collections::HashSet<&str> =
            results.iter().map(|r| r.name.as_str()).collect();

        let mut semantic_hits: Vec<(String, String, f64)> = self
            .entries
            .iter()
            .filter(|e| !matched.contains(e.name.as_str()))
            .filter_map(|e| {
                let entry_tokens: std::collections::HashSet<String> =
                    e.tokens.iter().cloned().collect();
                let sim = jaccard_similarity(&query_tokens, &entry_tokens);
                if sim > 0.05 {
                    Some((e.name.clone(), e.description.clone(), sim))
                } else {
                    None
                }
            })
            .collect();

        semantic_hits.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        semantic_hits.truncate(remaining);

        for (name, description, sim) in semantic_hits {
            results.push(ToolSearchResult {
                name,
                description,
                score: (sim * 60.0) as u32,
            });
        }

        results
    }

    /// Number of indexed tools.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize to JSON string for persistence.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolRegistry;

    use crate::tools::bash::BashTool;
    use crate::tools::file_read::FileReadTool;
    use crate::tools::file_write::FileWriteTool;
    use crate::tools::glob::GlobTool;
    use crate::tools::grep::GrepTool;

    fn test_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(BashTool::new());
        reg.register(FileReadTool::new());
        reg.register(FileWriteTool::new());
        reg.register(GrepTool::new());
        reg.register(GlobTool::new());
        reg
    }

    #[test]
    fn test_exact_name_match_score_100() {
        let reg = test_registry();
        let results = search_tools(&reg, "bash", 10);
        assert!(!results.is_empty());
        let bash = results.iter().find(|r| r.name == "bash").unwrap();
        assert_eq!(bash.score, 100);
    }

    #[test]
    fn test_name_contains_score_80() {
        let reg = test_registry();
        let results = search_tools(&reg, "file", 10);
        // file_read and file_write should match with score 80
        assert!(results.len() >= 2);
        for r in &results {
            if r.name.contains("file") {
                assert_eq!(r.score, 80);
            }
        }
    }

    #[test]
    fn test_description_contains_score_40() {
        let reg = test_registry();
        // "search" should match grep's description
        let results = search_tools(&reg, "search", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_no_match_returns_empty() {
        let reg = test_registry();
        let results = search_tools(&reg, "zzz_nonexistent_zzz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_limit_truncation() {
        let reg = test_registry();
        let results = search_tools(&reg, "file", 1);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_results_sorted_by_score_desc() {
        let reg = test_registry();
        let results = search_tools(&reg, "bash", 10);
        if results.len() > 1 {
            for i in 0..results.len() - 1 {
                assert!(results[i].score >= results[i + 1].score);
            }
        }
    }

    #[test]
    fn test_case_insensitive() {
        let reg = test_registry();
        let results_lower = search_tools(&reg, "bash", 10);
        let results_upper = search_tools(&reg, "BASH", 10);
        assert_eq!(results_lower.len(), results_upper.len());
    }

    // --- AR-T7: Hybrid search tests ---

    #[test]
    fn test_hybrid_returns_substring_first() {
        let reg = test_registry();
        let results = hybrid_search_tools(&reg, "bash", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "bash");
        assert_eq!(results[0].score, 100);
    }

    #[test]
    fn test_hybrid_fallback_to_semantic() {
        let reg = test_registry();
        // "execute command" should match bash via token overlap even though
        // "execute command" is not a substring of "bash"
        let results = hybrid_search_tools(&reg, "execute command", 10);
        // Should find at least bash (description contains "execute" and "command")
        assert!(!results.is_empty());
    }

    #[test]
    fn test_hybrid_dedup() {
        let reg = test_registry();
        // "file" matches file_read and file_write by substring
        let results = hybrid_search_tools(&reg, "file", 10);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        let unique: std::collections::HashSet<&str> = names.iter().cloned().collect();
        assert_eq!(names.len(), unique.len(), "No duplicates allowed");
    }

    #[test]
    fn test_tokenize_basic() {
        let tokens = super::tokenize("file read write");
        assert!(tokens.contains("file"));
        assert!(tokens.contains("read"));
        assert!(tokens.contains("write"));
    }

    #[test]
    fn test_jaccard_identical() {
        let a = super::tokenize("hello world");
        let sim = super::jaccard_similarity(&a, &a);
        assert!((sim - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let a = super::tokenize("hello world");
        let b = super::tokenize("foo bar");
        let sim = super::jaccard_similarity(&a, &b);
        assert!((sim - 0.0).abs() < f64::EPSILON);
    }

    // --- AR-D4: ToolSearchIndex tests ---

    #[test]
    fn test_search_index_build_and_search() {
        let reg = test_registry();
        let index = ToolSearchIndex::build(&reg);
        assert_eq!(index.len(), 5);

        let results = index.search("bash", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "bash");
        assert_eq!(results[0].score, 100);
    }

    #[test]
    fn test_search_index_semantic_fallback() {
        let reg = test_registry();
        let index = ToolSearchIndex::build(&reg);

        let results = index.search("execute command", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_index_json_roundtrip() {
        let reg = test_registry();
        let index = ToolSearchIndex::build(&reg);

        let json = index.to_json().unwrap();
        let restored = ToolSearchIndex::from_json(&json).unwrap();
        assert_eq!(restored.len(), index.len());

        // Search should produce same results
        let orig_results = index.search("file", 10);
        let restored_results = restored.search("file", 10);
        assert_eq!(orig_results.len(), restored_results.len());
        for (a, b) in orig_results.iter().zip(restored_results.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.score, b.score);
        }
    }
}
