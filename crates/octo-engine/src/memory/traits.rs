use anyhow::Result;
use async_trait::async_trait;
use octo_types::{MemoryBlock, SandboxId, UserId};

#[async_trait]
pub trait WorkingMemory: Send + Sync {
    async fn get_blocks(
        &self,
        user_id: &UserId,
        sandbox_id: &SandboxId,
    ) -> Result<Vec<MemoryBlock>>;

    async fn update_block(&self, block_id: &str, value: &str) -> Result<()>;

    async fn compile(
        &self,
        user_id: &UserId,
        sandbox_id: &SandboxId,
    ) -> Result<String>;
}
