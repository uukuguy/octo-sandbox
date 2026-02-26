use anyhow::Result;
use octo_types::{ChatMessage, MemoryBlock, MemoryBlockKind, MemoryEntry, MemorySource};
use tracing::{debug, warn};

use crate::memory::extractor::FactExtractor;
use crate::memory::store_traits::MemoryStore;
use crate::memory::traits::WorkingMemory;
use crate::providers::Provider;

pub struct MemoryFlusher;

impl MemoryFlusher {
    /// Flush important facts from messages before they are pruned.
    ///
    /// Returns the number of facts extracted and saved.
    pub async fn flush(
        messages: &[ChatMessage],
        compaction_boundary: usize,
        provider: &dyn Provider,
        memory: &dyn WorkingMemory,
        memory_store: Option<&dyn MemoryStore>,
        model: &str,
        user_id: &str,
    ) -> Result<usize> {
        if compaction_boundary == 0 || messages.is_empty() {
            return Ok(0);
        }

        let to_flush = &messages[..compaction_boundary.min(messages.len())];
        if to_flush.is_empty() {
            return Ok(0);
        }

        // Extract facts using LLM
        let facts = match FactExtractor::extract(provider, to_flush, model).await {
            Ok(f) => f,
            Err(e) => {
                warn!("FactExtractor failed: {e}");
                return Ok(0);
            }
        };

        if facts.is_empty() {
            debug!("No facts extracted from compaction boundary");
            return Ok(0);
        }

        let count = facts.len();

        // Write to WorkingMemory as AutoExtracted blocks
        for (i, fact) in facts.iter().enumerate() {
            let priority = (fact.importance * 200.0).min(255.0) as u8;
            let block = MemoryBlock {
                id: format!("auto_extracted_{}", i),
                kind: MemoryBlockKind::AutoExtracted,
                label: format!("Extracted: {}", fact.category.as_str()),
                value: fact.fact.clone(),
                priority,
                max_age_turns: Some(10),
                last_updated_turn: 0,
                char_limit: 2000,
                is_readonly: false,
            };
            let _ = memory.add_block(block).await;
        }

        // Optionally persist to MemoryStore (best-effort)
        if let Some(store) = memory_store {
            for fact in &facts {
                let entry = MemoryEntry::new(user_id, fact.category.clone(), &fact.fact);
                let mut entry = entry;
                entry.importance = fact.importance;
                entry.source_type = MemorySource::Extracted;
                if let Err(e) = store.store(entry).await {
                    warn!("Failed to persist extracted fact: {e}");
                }
            }
        }

        debug!(count, "Memory flush complete");
        Ok(count)
    }
}
