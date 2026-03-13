//! Memory consistency evaluation suite.
//!
//! Tests the 4-layer memory system for store/retrieve consistency.
//! Does NOT use the agent loop — tests memory APIs directly.
//! Returns `EvalReport` for integration with the eval reporting pipeline.

use std::time::Instant;

use anyhow::Result;

use crate::runner::{EvalReport, TaskResult};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::AgentOutput;

/// Memory consistency evaluation suite.
///
/// Exercises the 4 memory layers:
/// - L0: WorkingMemory (in-conversation blocks)
/// - L1: SessionStore (cross-turn message persistence)
/// - L2: MemoryStore (long-term persistent entries via SQLite)
/// - KG: KnowledgeGraph (entity-relation graph)
pub struct MemorySuite;

impl MemorySuite {
    /// Run all memory consistency tests and return an aggregated report.
    pub async fn run() -> Result<EvalReport> {
        let mut results = Vec::new();

        // L0: WorkingMemory tests
        results.push(Self::test_working_memory_store_retrieve().await);
        results.push(Self::test_working_memory_overwrite().await);
        results.push(Self::test_working_memory_clear().await);

        // L1: SessionStore tests
        results.push(Self::test_session_create_and_retrieve().await);
        results.push(Self::test_session_message_persistence().await);
        results.push(Self::test_session_multi_sessions().await);

        // L2: MemoryStore tests (SQLite-backed)
        results.push(Self::test_memory_store_save_load().await);
        results.push(Self::test_memory_store_update().await);
        results.push(Self::test_memory_store_list_filter().await);

        // KG: KnowledgeGraph tests
        results.push(Self::test_kg_add_entity().await);
        results.push(Self::test_kg_add_relation().await);
        results.push(Self::test_kg_search_and_traverse().await);

        Ok(EvalReport::from_results(results))
    }

    // ── L0: WorkingMemory ────────────────────────────────────────

    /// Store a custom block and retrieve it back.
    async fn test_working_memory_store_retrieve() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L0-01";

        let result = async {
            use octo_engine::memory::InMemoryWorkingMemory;
            use octo_engine::memory::traits::WorkingMemory;
            use octo_types::{MemoryBlock, MemoryBlockKind, SandboxId, UserId};

            let wm = InMemoryWorkingMemory::new();
            let uid = UserId::default();
            let sid = SandboxId::default();

            // Add a custom block
            let block = MemoryBlock::new(MemoryBlockKind::Custom, "Test Block", "hello world")
                .with_id("test-block-1");
            wm.add_block(block).await?;

            // Retrieve all blocks and find ours
            let blocks = wm.get_blocks(&uid, &sid).await?;
            let found = blocks.iter().find(|b| b.id == "test-block-1");

            match found {
                Some(b) if b.value == "hello world" => Ok(true),
                Some(b) => Ok({
                    eprintln!(
                        "  [{}] value mismatch: expected 'hello world', got '{}'",
                        task_id, b.value
                    );
                    false
                }),
                None => Ok({
                    eprintln!("  [{}] block 'test-block-1' not found", task_id);
                    false
                }),
            }
        }
        .await;

        make_result(task_id, start, result, "hello world")
    }

    /// Overwrite an existing block's value and confirm the update.
    async fn test_working_memory_overwrite() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L0-02";

        let result = async {
            use octo_engine::memory::InMemoryWorkingMemory;
            use octo_engine::memory::traits::WorkingMemory;
            use octo_types::{SandboxId, UserId};

            let wm = InMemoryWorkingMemory::new();
            let uid = UserId::default();
            let sid = SandboxId::default();

            // Update the default "user_profile" block
            wm.update_block("user_profile", "original").await?;

            // Overwrite
            wm.update_block("user_profile", "updated").await?;

            let blocks = wm.get_blocks(&uid, &sid).await?;
            let profile = blocks.iter().find(|b| b.id == "user_profile");

            match profile {
                Some(b) if b.value == "updated" => Ok(true),
                _ => Ok(false),
            }
        }
        .await;

        make_result(task_id, start, result, "updated")
    }

    /// Remove a block and confirm it is gone.
    async fn test_working_memory_clear() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L0-03";

        let result = async {
            use octo_engine::memory::InMemoryWorkingMemory;
            use octo_engine::memory::traits::WorkingMemory;
            use octo_types::{SandboxId, UserId};

            let wm = InMemoryWorkingMemory::new();
            let uid = UserId::default();
            let sid = SandboxId::default();

            // Default has 2 blocks (user_profile, task_context)
            let before = wm.get_blocks(&uid, &sid).await?.len();
            assert_eq!(before, 2);

            // Remove one
            let removed = wm.remove_block("user_profile").await?;
            assert!(removed);

            let after = wm.get_blocks(&uid, &sid).await?.len();
            Ok(after == 1)
        }
        .await;

        make_result(task_id, start, result, "1 block remaining")
    }

    // ── L1: SessionStore ─────────────────────────────────────────

    /// Create a session and retrieve it by ID.
    async fn test_session_create_and_retrieve() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L1-01";

        let result = async {
            use octo_engine::session::{InMemorySessionStore, SessionStore};

            let store = InMemorySessionStore::new();
            let session = store.create_session().await;
            let retrieved = store.get_session(&session.session_id).await;

            match retrieved {
                Some(s) => Ok(s.session_id.as_str() == session.session_id.as_str()),
                None => Ok(false),
            }
        }
        .await;

        make_result(task_id, start, result, "session retrieved by ID")
    }

    /// Push messages to a session and retrieve them.
    async fn test_session_message_persistence() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L1-02";

        let result = async {
            use octo_engine::session::{InMemorySessionStore, SessionStore};
            use octo_types::ChatMessage;

            let store = InMemorySessionStore::new();
            let session = store.create_session().await;
            let sid = &session.session_id;

            store
                .push_message(sid, ChatMessage::user("Hello"))
                .await;
            store
                .push_message(sid, ChatMessage::assistant("Hi there"))
                .await;

            let messages = store.get_messages(sid).await;
            match messages {
                Some(msgs) if msgs.len() == 2 => Ok(true),
                Some(msgs) => {
                    eprintln!(
                        "  [{}] expected 2 messages, got {}",
                        task_id,
                        msgs.len()
                    );
                    Ok(false)
                }
                None => {
                    eprintln!("  [{}] no messages found", task_id);
                    Ok(false)
                }
            }
        }
        .await;

        make_result(task_id, start, result, "2 messages persisted")
    }

    /// Create multiple sessions and verify list returns them all.
    async fn test_session_multi_sessions() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L1-03";

        let result = async {
            use octo_engine::session::{InMemorySessionStore, SessionStore};

            let store = InMemorySessionStore::new();
            store.create_session().await;
            store.create_session().await;
            store.create_session().await;

            let sessions = store.list_sessions(10, 0).await;
            Ok(sessions.len() == 3)
        }
        .await;

        make_result(task_id, start, result, "3 sessions listed")
    }

    // ── L2: MemoryStore (SQLite) ─────────────────────────────────

    /// Store a MemoryEntry and retrieve it by ID.
    async fn test_memory_store_save_load() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L2-01";

        let result = async {
            use octo_engine::db::Database;
            use octo_engine::memory::SqliteMemoryStore;
            use octo_engine::memory::store_traits::MemoryStore;
            use octo_types::{MemoryCategory, MemoryEntry};

            let db = Database::open_in_memory().await?;
            let store = SqliteMemoryStore::new(db.conn().clone());

            let entry = MemoryEntry::new("test-user", MemoryCategory::Profile, "I prefer Rust");
            let id = entry.id.clone();
            store.store(entry).await?;

            let retrieved = store.get(&id).await?;
            match retrieved {
                Some(e) if e.content == "I prefer Rust" => Ok(true),
                Some(e) => {
                    eprintln!(
                        "  [{}] content mismatch: expected 'I prefer Rust', got '{}'",
                        task_id, e.content
                    );
                    Ok(false)
                }
                None => {
                    eprintln!("  [{}] entry not found by ID", task_id);
                    Ok(false)
                }
            }
        }
        .await;

        make_result(task_id, start, result, "entry stored and retrieved")
    }

    /// Store an entry, update its content, verify the update.
    async fn test_memory_store_update() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L2-02";

        let result = async {
            use octo_engine::db::Database;
            use octo_engine::memory::SqliteMemoryStore;
            use octo_engine::memory::store_traits::MemoryStore;
            use octo_types::{MemoryCategory, MemoryEntry};

            let db = Database::open_in_memory().await?;
            let store = SqliteMemoryStore::new(db.conn().clone());

            let entry =
                MemoryEntry::new("test-user", MemoryCategory::Preferences, "dark theme");
            let id = entry.id.clone();
            store.store(entry).await?;

            // Update
            store.update(&id, "light theme").await?;

            let retrieved = store.get(&id).await?;
            match retrieved {
                Some(e) if e.content == "light theme" => Ok(true),
                _ => Ok(false),
            }
        }
        .await;

        make_result(task_id, start, result, "content updated to 'light theme'")
    }

    /// Store multiple entries, list with category filter.
    async fn test_memory_store_list_filter() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-L2-03";

        let result = async {
            use octo_engine::db::Database;
            use octo_engine::memory::SqliteMemoryStore;
            use octo_engine::memory::store_traits::MemoryStore;
            use octo_types::{MemoryCategory, MemoryEntry, MemoryFilter};

            let db = Database::open_in_memory().await?;
            let store = SqliteMemoryStore::new(db.conn().clone());

            // Store 3 entries: 2 Profile, 1 Preferences
            let e1 = MemoryEntry::new("user-a", MemoryCategory::Profile, "profile info 1");
            let e2 = MemoryEntry::new("user-a", MemoryCategory::Profile, "profile info 2");
            let e3 =
                MemoryEntry::new("user-a", MemoryCategory::Preferences, "pref info");
            store.store(e1).await?;
            store.store(e2).await?;
            store.store(e3).await?;

            // Filter by Profile category only
            let filter = MemoryFilter {
                user_id: "user-a".to_string(),
                categories: Some(vec![MemoryCategory::Profile]),
                ..Default::default()
            };
            let results = store.list(filter).await?;

            Ok(results.len() == 2)
        }
        .await;

        make_result(task_id, start, result, "2 Profile entries listed")
    }

    // ── KG: KnowledgeGraph ───────────────────────────────────────

    /// Add an entity and retrieve it by ID.
    async fn test_kg_add_entity() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-KG-01";

        let result = async {
            use octo_engine::memory::graph::{Entity, KnowledgeGraph};

            let mut kg = KnowledgeGraph::new();
            let now = chrono::Utc::now().timestamp();

            let entity = Entity {
                id: "rust-lang".to_string(),
                name: "Rust".to_string(),
                entity_type: "language".to_string(),
                properties: serde_json::json!({"paradigm": "systems"}),
                created_at: now,
                updated_at: now,
            };
            kg.add_entity(entity);

            match kg.get_entity("rust-lang") {
                Some(e) if e.name == "Rust" => Ok(true),
                _ => Ok(false),
            }
        }
        .await;

        make_result(task_id, start, result, "entity 'Rust' added and retrieved")
    }

    /// Add two entities and a relation; verify the relation links them.
    async fn test_kg_add_relation() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-KG-02";

        let result = async {
            use octo_engine::memory::graph::{Entity, KnowledgeGraph, Relation};

            let mut kg = KnowledgeGraph::new();
            let now = chrono::Utc::now().timestamp();

            let e1 = Entity {
                id: "alice".to_string(),
                name: "Alice".to_string(),
                entity_type: "person".to_string(),
                properties: serde_json::json!({}),
                created_at: now,
                updated_at: now,
            };
            let e2 = Entity {
                id: "bob".to_string(),
                name: "Bob".to_string(),
                entity_type: "person".to_string(),
                properties: serde_json::json!({}),
                created_at: now,
                updated_at: now,
            };
            kg.add_entity(e1);
            kg.add_entity(e2);

            let rel = Relation {
                id: "rel-1".to_string(),
                source_id: "alice".to_string(),
                target_id: "bob".to_string(),
                relation_type: "knows".to_string(),
                properties: serde_json::json!({}),
                created_at: now,
            };
            let added = kg.add_relation(rel);
            assert!(added);

            let outgoing = kg.get_outgoing("alice");
            Ok(outgoing.len() == 1 && outgoing[0].target_id == "bob")
        }
        .await;

        make_result(task_id, start, result, "relation alice->bob created")
    }

    /// Search entities by name pattern and traverse BFS.
    async fn test_kg_search_and_traverse() -> TaskResult {
        let start = Instant::now();
        let task_id = "mem-KG-03";

        let result = async {
            use octo_engine::memory::graph::{Entity, KnowledgeGraph, Relation};

            let mut kg = KnowledgeGraph::new();
            let now = chrono::Utc::now().timestamp();

            // Build a small graph: A -> B -> C
            for (id, name) in [("a", "Alpha"), ("b", "Beta"), ("c", "Charlie")] {
                kg.add_entity(Entity {
                    id: id.to_string(),
                    name: name.to_string(),
                    entity_type: "node".to_string(),
                    properties: serde_json::json!({}),
                    created_at: now,
                    updated_at: now,
                });
            }
            kg.add_relation(Relation {
                id: "r1".to_string(),
                source_id: "a".to_string(),
                target_id: "b".to_string(),
                relation_type: "links_to".to_string(),
                properties: serde_json::json!({}),
                created_at: now,
            });
            kg.add_relation(Relation {
                id: "r2".to_string(),
                source_id: "b".to_string(),
                target_id: "c".to_string(),
                relation_type: "links_to".to_string(),
                properties: serde_json::json!({}),
                created_at: now,
            });

            // Search by name pattern
            let search_results = kg.search("beta");
            let search_ok = search_results.len() == 1 && search_results[0].id == "b";

            // BFS from "a" with depth 2 should reach all 3 nodes
            let traversal = kg.traverse_bfs("a", 2);
            let traverse_ok = traversal.len() == 3;

            // Find path from a to c
            let path = kg.find_path("a", "c");
            let path_ok = path.as_ref().map(|p| p.len()) == Some(3); // [a, b, c]

            Ok(search_ok && traverse_ok && path_ok)
        }
        .await;

        make_result(task_id, start, result, "search + BFS + path all correct")
    }
}

/// Helper to build a `TaskResult` from a test outcome.
fn make_result(
    task_id: &str,
    start: Instant,
    result: Result<bool>,
    expected_desc: &str,
) -> TaskResult {
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(true) => TaskResult {
            task_id: task_id.to_string(),
            output: AgentOutput::default(),
            score: EvalScore::pass(
                1.0,
                ScoreDetails::Custom {
                    message: format!("PASS: {}", expected_desc),
                },
            ),
            duration_ms,
        },
        Ok(false) => TaskResult {
            task_id: task_id.to_string(),
            output: AgentOutput::default(),
            score: EvalScore::fail(
                0.0,
                ScoreDetails::Custom {
                    message: format!("FAIL: expected {}", expected_desc),
                },
            ),
            duration_ms,
        },
        Err(e) => TaskResult {
            task_id: task_id.to_string(),
            output: AgentOutput::default(),
            score: EvalScore::fail(
                0.0,
                ScoreDetails::Custom {
                    message: format!("ERROR: {}", e),
                },
            ),
            duration_ms,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_suite_runs() {
        let report = MemorySuite::run().await.unwrap();
        assert_eq!(report.total, 12, "Expected 12 memory tests");
        // All tests should pass — they use in-memory backends
        assert_eq!(
            report.passed, report.total,
            "Expected all {} tests to pass, but only {} passed. Failed: {:?}",
            report.total,
            report.passed,
            report
                .results
                .iter()
                .filter(|r| !r.score.passed)
                .map(|r| (&r.task_id, &r.score.details))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_l0_working_memory() {
        let r = MemorySuite::test_working_memory_store_retrieve().await;
        assert!(r.score.passed, "L0 store/retrieve failed: {:?}", r.score.details);
    }

    #[tokio::test]
    async fn test_l1_session_messages() {
        let r = MemorySuite::test_session_message_persistence().await;
        assert!(r.score.passed, "L1 message persistence failed: {:?}", r.score.details);
    }

    #[tokio::test]
    async fn test_l2_memory_store() {
        let r = MemorySuite::test_memory_store_save_load().await;
        assert!(r.score.passed, "L2 save/load failed: {:?}", r.score.details);
    }

    #[tokio::test]
    async fn test_kg_operations() {
        let r = MemorySuite::test_kg_search_and_traverse().await;
        assert!(r.score.passed, "KG search/traverse failed: {:?}", r.score.details);
    }
}
