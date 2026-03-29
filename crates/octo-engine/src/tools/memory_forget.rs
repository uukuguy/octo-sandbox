use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{MemoryCategory, MemoryFilter, MemoryId, ToolContext, ToolOutput, ToolSource};

use crate::memory::store_traits::MemoryStore;

use super::traits::Tool;

pub struct MemoryForgetTool {
    store: Arc<dyn MemoryStore>,
}

impl MemoryForgetTool {
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for MemoryForgetTool {
    fn name(&self) -> &str {
        "memory_forget"
    }

    fn description(&self) -> &str {
        "Delete memories by ID, category, or smart forgetting criteria (low importance, old age, low access count). Supports dry_run to preview before deleting."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Delete a single memory by ID"
                },
                "category": {
                    "type": "string",
                    "enum": ["profile", "preferences", "tools", "debug", "patterns"],
                    "description": "Delete all memories in this category"
                },
                "max_importance": {
                    "type": "number",
                    "description": "Delete memories with importance <= this value (0.0-1.0)"
                },
                "older_than_days": {
                    "type": "integer",
                    "description": "Delete memories older than this many days"
                },
                "max_access_count": {
                    "type": "integer",
                    "description": "Delete memories accessed <= this many times"
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Preview matching memories without deleting (default: false)"
                }
            }
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let id = params["id"].as_str();
        let category = params["category"].as_str();
        let max_importance = params["max_importance"].as_f64().map(|f| f as f32);
        let older_than_days = params["older_than_days"].as_u64();
        let max_access_count = params["max_access_count"].as_u64().map(|v| v as u32);
        let dry_run = params["dry_run"].as_bool().unwrap_or(false);

        let has_smart_criteria =
            max_importance.is_some() || older_than_days.is_some() || max_access_count.is_some();

        if id.is_none() && category.is_none() && !has_smart_criteria {
            return Ok(ToolOutput::error(
                "At least one criterion must be provided: 'id', 'category', 'max_importance', 'older_than_days', or 'max_access_count'".to_string(),
            ));
        }

        // Single delete by ID (dry_run not applicable)
        if let Some(id_str) = id {
            if dry_run {
                let mem_id = MemoryId::from_string(id_str);
                let existing = self.store.get(&mem_id).await?;
                return match existing {
                    Some(entry) => Ok(ToolOutput::success(format!(
                        "[dry_run] Would delete memory {id_str}: [{}] {}",
                        entry.category.as_str(),
                        truncate_content(&entry.content, 100),
                    ))),
                    None => Ok(ToolOutput::error(format!(
                        "Memory with id '{id_str}' not found"
                    ))),
                };
            }

            let mem_id = MemoryId::from_string(id_str);
            let existing = self.store.get(&mem_id).await?;
            if existing.is_none() {
                return Ok(ToolOutput::error(format!(
                    "Memory with id '{id_str}' not found"
                )));
            }
            self.store.delete(&mem_id).await?;
            debug!(id = id_str, "Forgot memory");
            return Ok(ToolOutput::success(format!("Deleted memory {id_str}")));
        }

        // Build filter from criteria
        let mut filter = MemoryFilter {
            user_id: "default".to_string(),
            ..Default::default()
        };

        if let Some(cat_str) = category {
            let cat = MemoryCategory::parse(cat_str)
                .ok_or_else(|| anyhow::anyhow!("invalid category: {cat_str}"))?;
            filter.categories = Some(vec![cat]);
        }

        if let Some(imp) = max_importance {
            filter.max_importance = Some(imp.clamp(0.0, 1.0));
        }

        if let Some(days) = older_than_days {
            filter.older_than_secs = Some(days as i64 * 86400);
        }

        if let Some(ac) = max_access_count {
            filter.max_access_count = Some(ac);
        }

        if dry_run {
            let matches = self.store.list(filter).await?;
            if matches.is_empty() {
                return Ok(ToolOutput::success(
                    "[dry_run] No memories match the criteria.".to_string(),
                ));
            }
            let mut output = format!("[dry_run] Would delete {} memories:\n\n", matches.len());
            for (i, entry) in matches.iter().enumerate().take(20) {
                output.push_str(&format!(
                    "{}. [{}] (importance: {:.2}, access: {}, category: {})\n   {}\n\n",
                    i + 1,
                    entry.id,
                    entry.importance,
                    entry.access_count,
                    entry.category.as_str(),
                    truncate_content(&entry.content, 80),
                ));
            }
            if matches.len() > 20 {
                output.push_str(&format!("... and {} more\n", matches.len() - 20));
            }
            return Ok(ToolOutput::success(output));
        }

        let count = self.store.delete_by_filter(filter).await?;
        let mut criteria_desc = Vec::new();
        if let Some(cat_str) = category {
            criteria_desc.push(format!("category={cat_str}"));
        }
        if let Some(imp) = max_importance {
            criteria_desc.push(format!("importance<={imp:.2}"));
        }
        if let Some(days) = older_than_days {
            criteria_desc.push(format!("older_than={days}d"));
        }
        if let Some(ac) = max_access_count {
            criteria_desc.push(format!("access_count<={ac}"));
        }

        debug!(count, criteria = %criteria_desc.join(", "), "Smart forget completed");
        Ok(ToolOutput::success(format!(
            "Deleted {count} memories matching criteria: {}",
            criteria_desc.join(", ")
        )))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len])
    }
}
