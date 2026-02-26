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

        let defaults = vec![
            MemoryBlock::new(
                MemoryBlockKind::SandboxContext,
                "Sandbox Context",
                "Runtime: Native | Tools: bash, file_read",
            ),
            MemoryBlock::new(
                MemoryBlockKind::AgentPersona,
                "Agent Persona",
                "You are Octo, an AI coding assistant running inside a sandboxed environment. \
                 You can execute bash commands and read files to help users with their tasks. \
                 Be concise, accurate, and helpful.",
            ),
            MemoryBlock::new(
                MemoryBlockKind::UserProfile,
                "User Profile",
                "",
            ),
            MemoryBlock::new(
                MemoryBlockKind::TaskContext,
                "Task Context",
                "",
            ),
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

    async fn compile(
        &self,
        user_id: &UserId,
        sandbox_id: &SandboxId,
    ) -> Result<String> {
        let blocks = self.get_blocks(user_id, sandbox_id).await?;
        Ok(ContextInjector::compile(&blocks))
    }
}
