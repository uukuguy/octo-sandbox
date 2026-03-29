//! memory_timeline tool — query memories by date, time range, session, or type.
//!
//! Provides agents with the ability to answer "what did I do yesterday?" or
//! "show events from session X" by translating temporal queries into
//! [`SearchOptions`] filters against the L2 memory store.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use serde_json::{json, Value};

use octo_types::{MemoryFilter, MemoryType, SortField, ToolContext, ToolOutput, ToolSource};

use crate::memory::store_traits::MemoryStore;

use super::traits::Tool;

pub struct MemoryTimelineTool {
    store: Arc<dyn MemoryStore>,
}

impl MemoryTimelineTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryTimelineTool {
    fn name(&self) -> &str {
        "memory_timeline"
    }

    fn description(&self) -> &str {
        "Query memories by date, time range, session, or type. Use to answer questions about past events, history, and what was done on specific dates."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "date": {
                    "type": "string",
                    "description": "Query a specific date (format: YYYY-MM-DD)"
                },
                "range": {
                    "type": "string",
                    "description": "Query a range: today, yesterday, last_week, last_month, or YYYY-MM-DD..YYYY-MM-DD"
                },
                "query": {
                    "type": "string",
                    "description": "Semantic search query, results sorted by time"
                },
                "session_id": {
                    "type": "string",
                    "description": "Query all memories from a specific session"
                },
                "type": {
                    "type": "string",
                    "enum": ["semantic", "episodic", "procedural"],
                    "description": "Filter by memory type"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results (default: 20)"
                }
            }
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let limit = params["limit"].as_u64().unwrap_or(20) as usize;

        // Parse time range from date or range parameter
        let time_range = if let Some(date_str) = params["date"].as_str() {
            Some(parse_date_range(date_str)?)
        } else if let Some(range_str) = params["range"].as_str() {
            Some(parse_range(range_str)?)
        } else {
            None
        };

        let session_id = params["session_id"].as_str().map(|s| s.to_string());

        let memory_types = params["type"]
            .as_str()
            .and_then(MemoryType::parse)
            .map(|t| vec![t]);

        // If we have a query, use search with time filtering
        if let Some(query) = params["query"].as_str() {
            let opts = octo_types::SearchOptions {
                user_id: "default".to_string(),
                limit,
                time_range,
                session_id,
                memory_types,
                sort_by: SortField::CreatedAt,
                ..Default::default()
            };
            let results = self.store.search(query, opts).await?;

            if results.is_empty() {
                return Ok(ToolOutput::success("No memories found for this query.".to_string()));
            }

            let output = format_search_results(
                &results
                    .iter()
                    .map(|r| &r.entry)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            return Ok(ToolOutput::success(output));
        }

        // Otherwise use list with filters
        let filter = MemoryFilter {
            user_id: "default".to_string(),
            limit,
            time_range,
            session_id,
            memory_types,
            ..Default::default()
        };
        let entries = self.store.list(filter).await?;

        if entries.is_empty() {
            return Ok(ToolOutput::success("No memories found for this time range.".to_string()));
        }

        let output = format_search_results(&entries);
        Ok(ToolOutput::success(output))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

/// Parse a YYYY-MM-DD date string into a (start, end) timestamp range (full day).
fn parse_date_range(date_str: &str) -> Result<(i64, i64)> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| anyhow::anyhow!("Invalid date format '{}': {}. Use YYYY-MM-DD.", date_str, e))?;
    let start = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?
        .and_utc()
        .timestamp();
    let end = date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?
        .and_utc()
        .timestamp();
    Ok((start, end))
}

/// Parse a range string into a (start, end) timestamp range.
///
/// Supported formats:
///   - "today", "yesterday", "last_week", "last_month"
///   - "YYYY-MM-DD..YYYY-MM-DD"
fn parse_range(range_str: &str) -> Result<(i64, i64)> {
    let now = Utc::now();
    let today = now.date_naive();

    match range_str {
        "today" => parse_date_range(&today.format("%Y-%m-%d").to_string()),
        "yesterday" => {
            let yesterday = today - chrono::Duration::days(1);
            parse_date_range(&yesterday.format("%Y-%m-%d").to_string())
        }
        "last_week" => {
            let week_ago = today - chrono::Duration::days(7);
            let start = week_ago
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("Invalid date"))?
                .and_utc()
                .timestamp();
            let end = now.timestamp();
            Ok((start, end))
        }
        "last_month" => {
            let month_ago = today - chrono::Duration::days(30);
            let start = month_ago
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| anyhow::anyhow!("Invalid date"))?
                .and_utc()
                .timestamp();
            let end = now.timestamp();
            Ok((start, end))
        }
        other if other.contains("..") => {
            let parts: Vec<&str> = other.split("..").collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!(
                    "Invalid range format '{}'. Use YYYY-MM-DD..YYYY-MM-DD",
                    other
                ));
            }
            let (start, _) = parse_date_range(parts[0])?;
            let (_, end) = parse_date_range(parts[1])?;
            Ok((start, end))
        }
        _ => Err(anyhow::anyhow!(
            "Unknown range '{}'. Use: today, yesterday, last_week, last_month, or YYYY-MM-DD..YYYY-MM-DD",
            range_str
        )),
    }
}

/// Format memory entries into a human-readable timeline.
fn format_search_results(entries: &[octo_types::MemoryEntry]) -> String {
    let mut output = format!("Found {} memories:\n\n", entries.len());
    for (i, entry) in entries.iter().enumerate() {
        let timestamp = chrono::DateTime::from_timestamp(entry.timestamps.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let type_badge = match entry.memory_type {
            MemoryType::Semantic => "[semantic]",
            MemoryType::Episodic => "[episodic]",
            MemoryType::Procedural => "[procedural]",
        };

        let session_info = entry
            .session_id
            .as_deref()
            .map(|s| format!(" (session: {})", truncate(s, 12)))
            .unwrap_or_default();

        output.push_str(&format!(
            "{}. {} {} {}{}\n   {}\n\n",
            i + 1,
            timestamp,
            type_badge,
            entry.category.as_str(),
            session_info,
            entry.content,
        ));
    }
    output
}

/// Truncate a string to max_len, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date_range_valid() {
        let (start, end) = parse_date_range("2026-03-29").unwrap();
        assert!(start < end);
        // Start should be midnight, end should be 23:59:59
        assert_eq!(end - start, 86399); // 24h - 1s
    }

    #[test]
    fn test_parse_date_range_invalid() {
        assert!(parse_date_range("not-a-date").is_err());
        assert!(parse_date_range("2026/03/29").is_err());
    }

    #[test]
    fn test_parse_range_today() {
        let result = parse_range("today");
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(start < end);
    }

    #[test]
    fn test_parse_range_yesterday() {
        let result = parse_range("yesterday");
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(start < end);
    }

    #[test]
    fn test_parse_range_last_week() {
        let result = parse_range("last_week");
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(end - start >= 7 * 86400 - 86400); // ~7 days
    }

    #[test]
    fn test_parse_range_last_month() {
        let result = parse_range("last_month");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_range_date_range() {
        let result = parse_range("2026-03-01..2026-03-29");
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(start < end);
    }

    #[test]
    fn test_parse_range_invalid() {
        assert!(parse_range("invalid").is_err());
        assert!(parse_range("2026-03-01..").is_err());
    }

    #[test]
    fn test_format_search_results_empty() {
        let output = format_search_results(&[]);
        assert!(output.contains("Found 0 memories"));
    }

    #[test]
    fn test_format_search_results_with_entries() {
        let entry = octo_types::MemoryEntry::new("user1", octo_types::MemoryCategory::Profile, "Test memory");
        let output = format_search_results(&[entry]);
        assert!(output.contains("Test memory"));
        assert!(output.contains("[semantic]"));
    }

    #[test]
    fn test_format_episodic_entry() {
        let event = octo_types::EventData::new("create", "auth module", "success");
        let entry = octo_types::MemoryEntry::new_episodic("user1", &event, "session-123");
        let output = format_search_results(&[entry]);
        assert!(output.contains("[episodic]"));
        assert!(output.contains("session: session-123"));
    }
}
