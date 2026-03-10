//! Memory commands implementation — search/list/add/graph via MemoryStore + KnowledgeGraph

use crate::commands::{AppState, MemoryCommands};
use crate::output::{self, TextOutput};
use crate::ui::table::Table;
use anyhow::Result;
use octo_types::{MemoryCategory, MemoryEntry, MemoryFilter, SearchOptions};
use serde::Serialize;

/// Handle memory commands
pub async fn handle_memory(action: MemoryCommands, state: &AppState) -> Result<()> {
    match action {
        MemoryCommands::Search { query, limit } => search_memory(query, limit, state).await?,
        MemoryCommands::List { limit } => list_memories(limit, state).await?,
        MemoryCommands::Add { content, tags } => add_memory(content, tags, state).await?,
        MemoryCommands::Graph { query, limit } => show_graph(query, limit, state).await?,
    }
    Ok(())
}

// ── Output types ──────────────────────────────────────────────

#[derive(Serialize)]
struct MemorySearchOutput {
    query: String,
    results: Vec<MemoryResultRow>,
}

#[derive(Serialize)]
struct MemoryResultRow {
    id: String,
    category: String,
    score: f32,
    content: String,
}

impl TextOutput for MemorySearchOutput {
    fn to_text(&self) -> String {
        if self.results.is_empty() {
            return format!("No results for \"{}\"", self.query);
        }
        let mut t = Table::new(vec!["ID", "Category", "Score", "Content"]);
        for r in &self.results {
            t.add_row(vec![
                r.id.clone(),
                r.category.clone(),
                format!("{:.2}", r.score),
                truncate(&r.content, 60),
            ]);
        }
        format!("Search results for \"{}\":\n\n{}", self.query, t.render())
    }
}

#[derive(Serialize)]
struct MemoryListOutput {
    entries: Vec<MemoryEntryRow>,
}

#[derive(Serialize)]
struct MemoryEntryRow {
    id: String,
    category: String,
    content: String,
    importance: f32,
    created_at: String,
}

impl TextOutput for MemoryListOutput {
    fn to_text(&self) -> String {
        if self.entries.is_empty() {
            return "No memory entries found.".to_string();
        }
        let mut t = Table::new(vec!["ID", "Category", "Importance", "Created", "Content"]);
        for e in &self.entries {
            t.add_row(vec![
                e.id.clone(),
                e.category.clone(),
                format!("{:.1}", e.importance),
                e.created_at.clone(),
                truncate(&e.content, 50),
            ]);
        }
        t.render()
    }
}

#[derive(Serialize)]
struct MemoryAddOutput {
    id: String,
    content: String,
}

impl TextOutput for MemoryAddOutput {
    fn to_text(&self) -> String {
        format!("Memory added (id: {}): {}", self.id, truncate(&self.content, 80))
    }
}

#[derive(Serialize)]
struct GraphOutput {
    entities: Vec<GraphEntityRow>,
}

#[derive(Serialize)]
struct GraphEntityRow {
    id: String,
    name: String,
    entity_type: String,
}

impl TextOutput for GraphOutput {
    fn to_text(&self) -> String {
        if self.entities.is_empty() {
            return "No knowledge graph entities found.".to_string();
        }
        let mut t = Table::new(vec!["ID", "Name", "Type"]);
        for e in &self.entities {
            t.add_row(vec![e.id.clone(), e.name.clone(), e.entity_type.clone()]);
        }
        t.render()
    }
}

// ── Handlers ──────────────────────────────────────────────────

async fn search_memory(query: String, limit: usize, state: &AppState) -> Result<()> {
    let memory_store = state.agent_runtime.memory_store();
    let opts = SearchOptions {
        user_id: "cli-user".to_string(),
        limit,
        ..Default::default()
    };

    let results = memory_store.search(&query, opts).await?;
    let out = MemorySearchOutput {
        query,
        results: results
            .into_iter()
            .map(|r| MemoryResultRow {
                id: r.entry.id.to_string(),
                category: r.entry.category.as_str().to_string(),
                score: r.score,
                content: r.entry.content,
            })
            .collect(),
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn list_memories(limit: usize, state: &AppState) -> Result<()> {
    let memory_store = state.agent_runtime.memory_store();
    let filter = MemoryFilter {
        user_id: "cli-user".to_string(),
        limit,
        ..Default::default()
    };

    let entries = memory_store.list(filter).await?;
    let out = MemoryListOutput {
        entries: entries
            .into_iter()
            .map(|e| MemoryEntryRow {
                id: e.id.to_string(),
                category: e.category.as_str().to_string(),
                content: e.content,
                importance: e.importance,
                created_at: format_timestamp(e.timestamps.created_at),
            })
            .collect(),
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn add_memory(content: String, tags: Option<String>, state: &AppState) -> Result<()> {
    let memory_store = state.agent_runtime.memory_store();

    // Parse tags into category (use first tag or default to Patterns)
    let category = tags
        .as_deref()
        .and_then(|t| t.split(',').next())
        .and_then(|t| match t.trim().to_lowercase().as_str() {
            "profile" => Some(MemoryCategory::Profile),
            "preferences" => Some(MemoryCategory::Preferences),
            "tools" => Some(MemoryCategory::Tools),
            "debug" => Some(MemoryCategory::Debug),
            "patterns" => Some(MemoryCategory::Patterns),
            _ => None,
        })
        .unwrap_or(MemoryCategory::Patterns);

    let entry = MemoryEntry::new("cli-user", category, &content);
    let id = memory_store.store(entry).await?;

    let out = MemoryAddOutput {
        id: id.to_string(),
        content,
    };
    output::print_output(&out, &state.output_config);
    Ok(())
}

async fn show_graph(query: Option<String>, limit: usize, state: &AppState) -> Result<()> {
    // search_knowledge is on MemorySystem, not WorkingMemory trait.
    // Fall back to memory_store search for graph-like queries.
    let entities = match &query {
        Some(q) => {
            let memory_store = state.agent_runtime.memory_store();
            let opts = SearchOptions {
                user_id: "cli-user".to_string(),
                limit,
                ..Default::default()
            };
            let results = memory_store.search(q, opts).await.unwrap_or_default();
            results
                .into_iter()
                .map(|r| GraphEntityRow {
                    id: r.entry.id.to_string(),
                    name: truncate(&r.entry.content, 40),
                    entity_type: r.entry.category.as_str().to_string(),
                })
                .collect()
        }
        None => {
            let filter = MemoryFilter {
                user_id: "cli-user".to_string(),
                limit,
                ..Default::default()
            };
            let entries = state
                .agent_runtime
                .memory_store()
                .list(filter)
                .await
                .unwrap_or_default();
            entries
                .into_iter()
                .map(|e| GraphEntityRow {
                    id: e.id.to_string(),
                    name: truncate(&e.content, 40),
                    entity_type: e.category.as_str().to_string(),
                })
                .collect()
        }
    };

    let out = GraphOutput { entities };
    output::print_output(&out, &state.output_config);
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max {
        s
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let long = "a".repeat(100);
        let result = truncate(&long, 20);
        assert!(result.len() <= 20);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_newlines() {
        assert_eq!(truncate("hello\nworld", 20), "hello world");
    }

    #[test]
    fn test_format_timestamp_valid() {
        let ts = 1710000000i64; // 2024-03-09
        let result = format_timestamp(ts);
        assert!(result.contains("2024"));
    }

    #[test]
    fn test_memory_search_output_empty() {
        let out = MemorySearchOutput {
            query: "test".to_string(),
            results: vec![],
        };
        assert!(out.to_text().contains("No results"));
    }

    #[test]
    fn test_memory_list_output_empty() {
        let out = MemoryListOutput { entries: vec![] };
        assert!(out.to_text().contains("No memory"));
    }

    #[test]
    fn test_graph_output_empty() {
        let out = GraphOutput { entities: vec![] };
        assert!(out.to_text().contains("No knowledge"));
    }

    #[test]
    fn test_memory_add_output() {
        let out = MemoryAddOutput {
            id: "abc123".to_string(),
            content: "test content".to_string(),
        };
        let text = out.to_text();
        assert!(text.contains("abc123"));
        assert!(text.contains("test content"));
    }

    #[test]
    fn test_memory_search_output_with_results() {
        let out = MemorySearchOutput {
            query: "test".to_string(),
            results: vec![MemoryResultRow {
                id: "id1".to_string(),
                category: "patterns".to_string(),
                score: 0.95,
                content: "found pattern".to_string(),
            }],
        };
        let text = out.to_text();
        assert!(text.contains("id1"));
        assert!(text.contains("0.95"));
    }

    #[test]
    fn test_memory_list_output_with_entries() {
        let out = MemoryListOutput {
            entries: vec![MemoryEntryRow {
                id: "id1".to_string(),
                category: "debug".to_string(),
                content: "debug info".to_string(),
                importance: 0.7,
                created_at: "2024-01-01 00:00".to_string(),
            }],
        };
        let text = out.to_text();
        assert!(text.contains("id1"));
        assert!(text.contains("debug"));
    }
}
