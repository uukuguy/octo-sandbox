pub mod budget;
pub mod extractor;
pub mod fts;
pub mod graph;
pub mod graph_store;
pub mod hybrid_query;
pub mod injector;
pub mod semantic;
pub mod sqlite_store;
pub mod sqlite_working;
pub mod store_traits;
pub mod traits;
pub mod vector_index;
pub mod working;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use rusqlite::Connection;
use tokio::sync::RwLock;
use tokio::task;

use super::session::SqliteSessionStore;

pub use budget::TokenBudgetManager;
pub use fts::FtsStore;
pub use graph::{Entity, GraphStats, KnowledgeGraph, Relation};
pub use graph_store::GraphStore;
pub use semantic::{EntityRelation, SemanticEntity, SemanticMemory};
pub use sqlite_store::SqliteMemoryStore;
pub use sqlite_working::SqliteWorkingMemory;
pub use store_traits::MemoryStore;
pub use traits::WorkingMemory;
pub use vector_index::{VectorEntry, VectorIndex, VectorIndexConfig, VectorSearchResult};
pub use hybrid_query::{HybridQueryEngine, HybridSearchResult, QueryType};
pub use working::InMemoryWorkingMemory;

/// Unified memory system including working, session, persistent, and knowledge graph
pub struct MemorySystem {
    /// Working memory (current conversation context)
    pub working: InMemoryWorkingMemory,
    /// Session store (per-session data)
    pub session: SqliteSessionStore,
    /// Persistent memory store (long-term storage)
    pub persistent: SqliteMemoryStore,
    /// Knowledge graph (entities and relations)
    pub knowledge_graph: Arc<RwLock<KnowledgeGraph>>,
    /// Graph storage (persistence layer for knowledge graph)
    pub graph_store: GraphStore,
}

impl MemorySystem {
    /// Create a new MemorySystem with the given database path
    /// Opens separate connections for async (tokio_rusqlite) and sync (rusqlite) stores
    pub async fn new(db_path: &Path) -> Result<Self> {
        // Open async connection for SqliteMemoryStore and SqliteSessionStore
        let async_conn = tokio_rusqlite::Connection::open(db_path).await?;

        // Open sync connection for GraphStore
        let sync_conn = Connection::open(db_path)?;

        let store = SqliteMemoryStore::new(async_conn.clone());
        let session = SqliteSessionStore::new(async_conn).await?;
        let graph_store = GraphStore::new(sync_conn);
        graph_store.init()?;

        Ok(Self {
            working: InMemoryWorkingMemory::new(),
            session,
            persistent: store,
            knowledge_graph: Arc::new(RwLock::new(KnowledgeGraph::new())),
            graph_store,
        })
    }

    /// Load knowledge graph from storage
    pub async fn load_knowledge_graph(&self) -> Result<()> {
        // Use spawn_blocking to avoid blocking the async runtime
        let graph_store = self.graph_store.clone();
        let graph = task::spawn_blocking(move || graph_store.load_graph()).await??;
        let mut guard = self.knowledge_graph.write().await;
        *guard = graph;
        Ok(())
    }

    /// Add entity to knowledge graph
    pub async fn add_entity(&self, entity: Entity) -> Result<()> {
        // Use spawn_blocking to avoid blocking the async runtime
        let graph_store = self.graph_store.clone();
        let entity_clone = entity.clone();
        task::spawn_blocking(move || graph_store.save_entity(&entity_clone)).await??;
        let mut guard = self.knowledge_graph.write().await;
        guard.add_entity(entity);
        Ok(())
    }

    /// Add relation to knowledge graph
    pub async fn add_relation(&self, relation: Relation) -> Result<bool> {
        // Persist to DB first (without holding lock) to avoid state inconsistency
        // Use spawn_blocking to avoid blocking the async runtime
        let graph_store = self.graph_store.clone();
        let relation_clone = relation.clone();
        task::spawn_blocking(move || graph_store.save_relation(&relation_clone)).await??;

        // Now update in-memory graph after DB persisted successfully
        let mut guard = self.knowledge_graph.write().await;
        let result = guard.add_relation(relation);
        Ok(result)
    }

    /// Search knowledge graph by query string
    pub async fn search_knowledge(&self, query: &str) -> Vec<Entity> {
        let guard = self.knowledge_graph.read().await;
        guard.search(query).into_iter().cloned().collect()
    }

    /// Traverse knowledge graph from a starting entity
    pub async fn traverse_knowledge(
        &self,
        start_id: &str,
        max_depth: usize,
    ) -> Vec<(String, Entity, usize)> {
        let guard = self.knowledge_graph.read().await;
        guard.traverse_bfs(start_id, max_depth)
    }

    /// FTS search in knowledge graph
    pub async fn search_knowledge_fts(&self, query: &str, limit: usize) -> Result<Vec<Entity>> {
        // Use spawn_blocking to avoid blocking the async runtime
        let graph_store = self.graph_store.clone();
        let query = query.to_string();
        let ids = task::spawn_blocking(move || graph_store.fts_search(&query, limit)).await??;
        let guard = self.knowledge_graph.read().await;
        Ok(ids
            .iter()
            .filter_map(|id| guard.get_entity(id).cloned())
            .collect())
    }
}
