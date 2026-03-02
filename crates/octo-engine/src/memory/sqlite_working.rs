use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{MemoryBlock, MemoryBlockKind, SandboxId, UserId};
use tracing::debug;

use super::injector::ContextInjector;
use super::traits::WorkingMemory;

/// SQLite-backed WorkingMemory with write-through cache.
pub struct SqliteWorkingMemory {
    conn: tokio_rusqlite::Connection,
    cache: RwLock<HashMap<String, MemoryBlock>>,
    loaded: RwLock<bool>,
}

impl SqliteWorkingMemory {
    pub async fn new(conn: tokio_rusqlite::Connection) -> Result<Self> {
        let wm = Self {
            conn,
            cache: RwLock::new(HashMap::new()),
            loaded: RwLock::new(false),
        };
        Ok(wm)
    }

    /// Ensure cache is loaded from DB. If DB is empty, insert default blocks.
    async fn ensure_loaded(&self, user_id: &str, sandbox_id: &str) -> Result<()> {
        {
            let loaded = self.loaded.read().map_err(|e| anyhow::anyhow!("{e}"))?;
            if *loaded {
                return Ok(());
            }
        }

        let uid = user_id.to_string();
        let sid = sandbox_id.to_string();

        let blocks: Vec<MemoryBlock> = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, label, value, priority, max_age_turns, last_updated_turn, char_limit, is_readonly
                     FROM memory_blocks WHERE user_id = ?1 AND sandbox_id = ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![uid, sid], |row| {
                    let id: String = row.get(0)?;
                    let label: String = row.get(1)?;
                    let value: String = row.get(2)?;
                    let priority: u8 = row.get(3)?;
                    let max_age_turns: Option<u32> = row.get(4)?;
                    let last_updated_turn: u32 = row.get(5)?;
                    let char_limit: usize = row.get::<_, i64>(6)? as usize;
                    let is_readonly: bool = row.get::<_, i64>(7)? != 0;

                    // Allow deprecated variants here: DB may contain legacy rows from
                    // previous versions.  ContextInjector::compile() will skip them.
                    #[allow(deprecated)]
                    let kind = match id.as_str() {
                        "sandbox_context" => MemoryBlockKind::SandboxContext,
                        "agent_persona" => MemoryBlockKind::AgentPersona,
                        "user_profile" => MemoryBlockKind::UserProfile,
                        "task_context" => MemoryBlockKind::TaskContext,
                        "auto_extracted" => MemoryBlockKind::AutoExtracted,
                        _ => MemoryBlockKind::Custom,
                    };

                    Ok(MemoryBlock {
                        id,
                        kind,
                        label,
                        value,
                        priority,
                        max_age_turns,
                        last_updated_turn,
                        char_limit,
                        is_readonly,
                    })
                })?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row?);
                }
                Ok(result)
            })
            .await?;

        if blocks.is_empty() {
            // Insert defaults
            let defaults = default_blocks();
            {
                let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{e}"))?;
                for block in &defaults {
                    cache.insert(block.id.clone(), block.clone());
                }
            }
            // Persist defaults (cache lock released before await)
            self.persist_defaults(user_id, sandbox_id, &defaults)
                .await?;
        } else {
            let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{e}"))?;
            for block in blocks {
                cache.insert(block.id.clone(), block);
            }
        }

        let mut loaded = self.loaded.write().map_err(|e| anyhow::anyhow!("{e}"))?;
        *loaded = true;
        Ok(())
    }

    async fn persist_defaults(
        &self,
        user_id: &str,
        sandbox_id: &str,
        blocks: &[MemoryBlock],
    ) -> Result<()> {
        let uid = user_id.to_string();
        let sid = sandbox_id.to_string();
        let blocks = blocks.to_vec();

        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                for block in &blocks {
                    tx.execute(
                        "INSERT OR IGNORE INTO memory_blocks (id, user_id, sandbox_id, label, value, priority, max_age_turns, last_updated_turn, char_limit, is_readonly)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        rusqlite::params![
                            block.id,
                            uid,
                            sid,
                            block.label,
                            block.value,
                            block.priority,
                            block.max_age_turns,
                            block.last_updated_turn,
                            block.char_limit as i64,
                            block.is_readonly as i64,
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(())
    }
}

#[async_trait]
impl WorkingMemory for SqliteWorkingMemory {
    async fn get_blocks(
        &self,
        user_id: &UserId,
        sandbox_id: &SandboxId,
    ) -> Result<Vec<MemoryBlock>> {
        self.ensure_loaded(user_id.as_str(), sandbox_id.as_str())
            .await?;
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(cache.values().cloned().collect())
    }

    async fn update_block(&self, block_id: &str, value: &str) -> Result<()> {
        // Update cache
        {
            let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{e}"))?;
            if let Some(block) = cache.get_mut(block_id) {
                block.value = value.to_string();
            } else {
                return Ok(());
            }
        }

        // Write through to DB
        let bid = block_id.to_string();
        let val = value.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE memory_blocks SET value = ?1, updated_at = strftime('%s','now') WHERE id = ?2",
                    rusqlite::params![val, bid],
                )?;
                Ok(())
            })
            .await?;

        debug!(block_id, "Updated working memory block");
        Ok(())
    }

    async fn add_block(&self, block: MemoryBlock) -> Result<()> {
        let block_id = block.id.clone();

        // Update cache
        {
            let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{e}"))?;
            cache.insert(block.id.clone(), block.clone());
        }

        // Write through to DB (use a default user/sandbox for blocks added without context)
        let b = block;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO memory_blocks (id, user_id, sandbox_id, label, value, priority, max_age_turns, last_updated_turn, char_limit, is_readonly)
                     VALUES (?1, 'default', '', ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        b.id,
                        b.label,
                        b.value,
                        b.priority,
                        b.max_age_turns,
                        b.last_updated_turn,
                        b.char_limit as i64,
                        b.is_readonly as i64,
                    ],
                )?;
                Ok(())
            })
            .await?;

        debug!(block_id, "Added working memory block");
        Ok(())
    }

    async fn remove_block(&self, block_id: &str) -> Result<bool> {
        let removed = {
            let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{e}"))?;
            cache.remove(block_id).is_some()
        };

        if removed {
            let bid = block_id.to_string();
            self.conn
                .call(move |conn| {
                    conn.execute(
                        "DELETE FROM memory_blocks WHERE id = ?1",
                        rusqlite::params![bid],
                    )?;
                    Ok(())
                })
                .await?;
            debug!(block_id, "Removed working memory block");
        }

        Ok(removed)
    }

    async fn expire_blocks(&self, current_turn: u32) -> Result<usize> {
        let expired_ids: Vec<String> = {
            let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{e}"))?;
            cache
                .iter()
                .filter(|(_, b)| b.is_expired(current_turn))
                .map(|(id, _)| id.clone())
                .collect()
        };

        let count = expired_ids.len();
        if count > 0 {
            {
                let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{e}"))?;
                for id in &expired_ids {
                    cache.remove(id);
                }
            }

            let ids = expired_ids;
            self.conn
                .call(move |conn| {
                    let tx = conn.transaction()?;
                    for id in &ids {
                        tx.execute(
                            "DELETE FROM memory_blocks WHERE id = ?1",
                            rusqlite::params![id],
                        )?;
                    }
                    tx.commit()?;
                    Ok(())
                })
                .await?;
            debug!(count, "Expired working memory blocks");
        }

        Ok(count)
    }

    async fn compile(&self, user_id: &UserId, sandbox_id: &SandboxId) -> Result<String> {
        let blocks = self.get_blocks(user_id, sandbox_id).await?;
        Ok(ContextInjector::compile(&blocks))
    }
}

fn default_blocks() -> Vec<MemoryBlock> {
    // SandboxContext and AgentPersona are deprecated.
    // Static agent identity now lives in SystemPromptBuilder (Zone A).
    // Only dynamic, user-facing blocks are initialised here.
    vec![
        MemoryBlock::new(MemoryBlockKind::UserProfile, "User Profile", ""),
        MemoryBlock::new(MemoryBlockKind::TaskContext, "Task Context", ""),
    ]
}
