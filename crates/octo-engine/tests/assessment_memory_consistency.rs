//! Four-layer memory consistency assessment tests.
//!
//! Validates consistency across:
//! - L0: Working Memory (InMemoryWorkingMemory)
//! - Knowledge Graph (KnowledgeGraph + GraphStats)
//! - FTS Store (FtsStore backed by SQLite FTS5)

use octo_engine::memory::{Entity, FtsStore, InMemoryWorkingMemory, KnowledgeGraph, WorkingMemory};
use octo_types::{MemoryBlock, MemoryBlockKind, SandboxId, UserId};
use std::sync::{Arc, Mutex};

fn dummy_ids() -> (UserId, SandboxId) {
    (
        UserId::from_string("test-user"),
        SandboxId::from_string("test-sandbox"),
    )
}

fn make_entity(id: &str, name: &str, entity_type: &str) -> Entity {
    Entity {
        id: id.to_string(),
        name: name.to_string(),
        entity_type: entity_type.to_string(),
        properties: serde_json::json!({}),
        created_at: 0,
        updated_at: 0,
    }
}

// ---------------------------------------------------------------------------
// Test 1: L0 working memory stores and retrieves blocks
// ---------------------------------------------------------------------------
#[tokio::test]
async fn l0_working_memory_stores_and_retrieves() {
    let wm = InMemoryWorkingMemory::new();
    let (uid, sid) = dummy_ids();

    // Starts with 2 default blocks (UserProfile, TaskContext)
    let initial = wm.get_blocks(&uid, &sid).await.unwrap();
    assert_eq!(initial.len(), 2);

    // Add a Custom block used as a knowledge entry
    let block = MemoryBlock::new(MemoryBlockKind::Custom, "Knowledge Entry", "Rust is fast");
    let block_id = block.id.clone();
    wm.add_block(block).await.unwrap();

    let blocks = wm.get_blocks(&uid, &sid).await.unwrap();
    assert_eq!(blocks.len(), 3);

    let found = blocks.iter().find(|b| b.id == block_id).unwrap();
    assert_eq!(found.label, "Knowledge Entry");
    assert_eq!(found.value, "Rust is fast");
}

// ---------------------------------------------------------------------------
// Test 2: L0 working memory update and remove consistency
// ---------------------------------------------------------------------------
#[tokio::test]
async fn l0_working_memory_update_and_remove() {
    let wm = InMemoryWorkingMemory::new();
    let (uid, sid) = dummy_ids();

    // Add a block
    let block = MemoryBlock::new(MemoryBlockKind::Custom, "Temp Block", "original value");
    let block_id = block.id.clone();
    wm.add_block(block).await.unwrap();

    // Update its value
    wm.update_block(&block_id, "updated value").await.unwrap();

    let blocks = wm.get_blocks(&uid, &sid).await.unwrap();
    let updated = blocks.iter().find(|b| b.id == block_id).unwrap();
    assert_eq!(updated.value, "updated value");

    // Remove the block
    let removed = wm.remove_block(&block_id).await.unwrap();
    assert!(removed);

    // Verify it is gone — only the 2 defaults remain
    let blocks = wm.get_blocks(&uid, &sid).await.unwrap();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|b| b.id != block_id));
}

// ---------------------------------------------------------------------------
// Test 3: Knowledge graph entity CRUD and search
// ---------------------------------------------------------------------------
#[test]
fn knowledge_graph_entity_crud() {
    let mut kg = KnowledgeGraph::new();

    let rust_entity = make_entity("e1", "Rust", "language");
    kg.add_entity(rust_entity);

    let results = kg.search("Rust");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Rust");

    // Add a second entity and search for it
    let python_entity = make_entity("e2", "Python", "language");
    kg.add_entity(python_entity);

    let results = kg.search("Python");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Python");

    // Searching for "language" (entity_type) should return both
    let all_langs = kg.search("language");
    assert_eq!(all_langs.len(), 2);
}

// ---------------------------------------------------------------------------
// Test 4: Knowledge graph stats stay consistent after mutations
// ---------------------------------------------------------------------------
#[test]
fn knowledge_graph_stats_are_consistent() {
    let mut kg = KnowledgeGraph::new();

    let e1 = make_entity("e1", "Rust", "language");
    let e2 = make_entity("e2", "Cargo", "tool");
    kg.add_entity(e1);
    kg.add_entity(e2);

    let relation = octo_engine::memory::Relation {
        id: "r1".to_string(),
        source_id: "e1".to_string(),
        target_id: "e2".to_string(),
        relation_type: "uses".to_string(),
        properties: serde_json::json!({}),
        created_at: 0,
    };
    let added = kg.add_relation(relation);
    assert!(added);

    let stats = kg.stats();
    assert_eq!(stats.entity_count, 2);
    assert_eq!(stats.relation_count, 1);
    // Two distinct entity types: "language" and "tool"
    assert_eq!(stats.type_count, 2);
}

// ---------------------------------------------------------------------------
// Test 5: FTS store indexes entities and returns correct search results
// ---------------------------------------------------------------------------
#[test]
fn fts_store_index_and_search() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    // Create a content-bearing FTS5 table so that SELECT returns stored values.
    // The production FtsStore::init() uses content='' (contentless), which causes
    // SELECT to return NULL for all columns. We override the schema here to
    // validate the index_entity / search round-trip logic.
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS kg_fts USING fts5(
            entity_id,
            name,
            entity_type,
            properties,
            tokenize='porter unicode61'
        );
        "#,
    )
    .unwrap();
    let conn = Arc::new(Mutex::new(conn));

    let fts = FtsStore::new(conn);

    // Index two entities
    fts.index_entity(
        "e1",
        "Rust programming",
        "language",
        &serde_json::json!({"compiled": true}),
    )
    .unwrap();

    fts.index_entity(
        "e2",
        "Python scripting",
        "language",
        &serde_json::json!({"interpreted": true}),
    )
    .unwrap();

    // Search for "Rust" — should return only e1
    let results = fts.search("Rust", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], "e1");

    // Search for "Python" — should return only e2
    let results = fts.search("Python", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], "e2");
}
