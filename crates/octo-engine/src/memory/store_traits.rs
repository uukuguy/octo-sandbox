use anyhow::Result;
use async_trait::async_trait;
use octo_types::{MemoryEntry, MemoryFilter, MemoryId, MemoryResult, SearchOptions};

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn store(&self, entry: MemoryEntry) -> Result<MemoryId>;
    async fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<MemoryResult>>;
    async fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;
    async fn update(&self, id: &MemoryId, content: &str) -> Result<()>;
    async fn delete(&self, id: &MemoryId) -> Result<()>;
    async fn delete_by_filter(&self, filter: MemoryFilter) -> Result<usize>;
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>>;
    async fn batch_store(&self, entries: Vec<MemoryEntry>) -> Result<Vec<MemoryId>>;
}
