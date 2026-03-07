use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{MemoryBlock, MemoryBlockKind, SandboxId, UserId};

use super::injector::ContextInjector;
use super::traits::WorkingMemory;

pub struct InMemoryWorkingMemory {
    blocks: RwLock<HashMap<String, MemoryBlock>>,
}

impl InMemoryWorkingMemory {
    pub fn new() -> Self {
        let mut blocks = HashMap::new();

        // Only UserProfile and TaskContext are default blocks.
        // SandboxContext and AgentPersona are deprecated: static agent identity
        // now lives in SystemPromptBuilder (Zone A), not in working memory.
        let defaults = vec![
            MemoryBlock::new(MemoryBlockKind::UserProfile, "User Profile", ""),
            MemoryBlock::new(MemoryBlockKind::TaskContext, "Task Context", ""),
        ];

        for block in defaults {
            blocks.insert(block.id.clone(), block);
        }

        Self {
            blocks: RwLock::new(blocks),
        }
    }
}

impl Default for InMemoryWorkingMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkingMemory for InMemoryWorkingMemory {
    async fn get_blocks(
        &self,
        _user_id: &UserId,
        _sandbox_id: &SandboxId,
    ) -> Result<Vec<MemoryBlock>> {
        let blocks = self.blocks.read().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(blocks.values().cloned().collect())
    }

    async fn update_block(&self, block_id: &str, value: &str) -> Result<()> {
        let mut blocks = self.blocks.write().map_err(|e| anyhow::anyhow!("{e}"))?;
        if let Some(block) = blocks.get_mut(block_id) {
            block.value = value.to_string();
        }
        Ok(())
    }

    async fn add_block(&self, block: MemoryBlock) -> Result<()> {
        let mut blocks = self.blocks.write().map_err(|e| anyhow::anyhow!("{e}"))?;
        blocks.insert(block.id.clone(), block);
        Ok(())
    }

    async fn remove_block(&self, block_id: &str) -> Result<bool> {
        let mut blocks = self.blocks.write().map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(blocks.remove(block_id).is_some())
    }

    async fn expire_blocks(&self, current_turn: u32) -> Result<usize> {
        let mut blocks = self.blocks.write().map_err(|e| anyhow::anyhow!("{e}"))?;
        let before = blocks.len();
        blocks.retain(|_, b| !b.is_expired(current_turn));
        Ok(before - blocks.len())
    }

    async fn compile(&self, user_id: &UserId, sandbox_id: &SandboxId) -> Result<String> {
        let blocks = self.get_blocks(user_id, sandbox_id).await?;
        Ok(ContextInjector::compile(&blocks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::{MemoryBlockKind, SandboxId, UserId};

    fn dummy_ids() -> (UserId, SandboxId) {
        (UserId::default(), SandboxId::default())
    }

    #[tokio::test]
    async fn test_get_default_blocks() {
        let wm = InMemoryWorkingMemory::new();
        let (user_id, sandbox_id) = dummy_ids();
        let blocks = wm.get_blocks(&user_id, &sandbox_id).await.unwrap();

        assert_eq!(blocks.len(), 2);
        let ids: Vec<&str> = blocks.iter().map(|b| b.id.as_str()).collect();
        assert!(ids.contains(&"user_profile"));
        assert!(ids.contains(&"task_context"));
    }

    #[tokio::test]
    async fn test_update_block() {
        let wm = InMemoryWorkingMemory::new();
        let (user_id, sandbox_id) = dummy_ids();

        wm.update_block("user_profile", "I am a Rust developer").await.unwrap();

        let blocks = wm.get_blocks(&user_id, &sandbox_id).await.unwrap();
        let profile = blocks.iter().find(|b| b.id == "user_profile").unwrap();
        assert_eq!(profile.value, "I am a Rust developer");
    }

    #[tokio::test]
    async fn test_add_block() {
        let wm = InMemoryWorkingMemory::new();
        let (user_id, sandbox_id) = dummy_ids();

        let custom_block = MemoryBlock::new(MemoryBlockKind::Custom, "Custom Block", "custom value");
        wm.add_block(custom_block).await.unwrap();

        let blocks = wm.get_blocks(&user_id, &sandbox_id).await.unwrap();
        assert_eq!(blocks.len(), 3);
    }

    #[tokio::test]
    async fn test_remove_block() {
        let wm = InMemoryWorkingMemory::new();
        let (user_id, sandbox_id) = dummy_ids();

        let removed = wm.remove_block("user_profile").await.unwrap();
        assert!(removed);

        let blocks = wm.get_blocks(&user_id, &sandbox_id).await.unwrap();
        assert_eq!(blocks.len(), 1);
    }

    #[tokio::test]
    async fn test_expire_blocks() {
        let wm = InMemoryWorkingMemory::new();
        let (user_id, sandbox_id) = dummy_ids();

        // Add a block with max_age_turns = 1
        let mut block = MemoryBlock::new(MemoryBlockKind::Custom, "Temp", "temp value");
        block.max_age_turns = Some(1);
        block.last_updated_turn = 0;
        wm.add_block(block).await.unwrap();

        // At turn 2, block should expire
        let expired = wm.expire_blocks(2).await.unwrap();
        assert_eq!(expired, 1);

        let blocks = wm.get_blocks(&user_id, &sandbox_id).await.unwrap();
        assert_eq!(blocks.len(), 2); // only default blocks remain
    }

    #[tokio::test]
    async fn test_compile_context() {
        let wm = InMemoryWorkingMemory::new();
        let (user_id, sandbox_id) = dummy_ids();

        wm.update_block("user_profile", "Rust developer").await.unwrap();

        let compiled = wm.compile(&user_id, &sandbox_id).await.unwrap();

        assert!(compiled.starts_with("<context>"));
        assert!(compiled.contains("Rust developer"));
        assert!(compiled.ends_with("</context>"));
    }
}
