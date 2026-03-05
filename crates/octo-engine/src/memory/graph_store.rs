//! Knowledge Graph SQLite storage

use super::fts::FtsStore;
use super::graph::{Entity, GraphStats, KnowledgeGraph, Relation};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct GraphStore {
    conn: Arc<Mutex<Connection>>,
    fts: FtsStore,
}

impl GraphStore {
    pub fn new(conn: Connection) -> Self {
        let conn = Arc::new(Mutex::new(conn));
        let fts = FtsStore::new(conn.clone());
        Self { conn, fts }
    }

    /// Initialize tables
    pub fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS kg_entities (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                properties TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_kg_entities_type
                ON kg_entities(entity_type);
            CREATE INDEX IF NOT EXISTS idx_kg_entities_name
                ON kg_entities(name);

            CREATE TABLE IF NOT EXISTS kg_relations (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                properties TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL,
                FOREIGN KEY (source_id) REFERENCES kg_entities(id),
                FOREIGN KEY (target_id) REFERENCES kg_entities(id)
            );

            CREATE INDEX IF NOT EXISTS idx_kg_relations_source
                ON kg_relations(source_id);
            CREATE INDEX IF NOT EXISTS idx_kg_relations_target
                ON kg_relations(target_id);
            CREATE INDEX IF NOT EXISTS idx_kg_relations_type
                ON kg_relations(relation_type);
            "#,
        )?;
        // Mutex guard automatically drops here when conn goes out of scope
        self.fts.init()?;
        Ok(())
    }

    /// Save entity
    pub fn save_entity(&self, entity: &Entity) -> Result<()> {
        // Save entity to main database
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                r#"
                INSERT OR REPLACE INTO kg_entities
                    (id, name, entity_type, properties, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    entity.id,
                    entity.name,
                    entity.entity_type,
                    serde_json::to_string(&entity.properties)?,
                    entity.created_at,
                    entity.updated_at,
                ],
            )?;
            // Mutex guard auto-drops when conn goes out of scope here
        }

        // Index in FTS (FtsStore has its own mutex for thread safety)
        self.fts.index_entity(
            &entity.id,
            &entity.name,
            &entity.entity_type,
            &entity.properties,
        )?;
        Ok(())
    }

    /// Save relation
    pub fn save_relation(&self, relation: &Relation) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO kg_relations
                (id, source_id, target_id, relation_type, properties, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                relation.id,
                relation.source_id,
                relation.target_id,
                relation.relation_type,
                serde_json::to_string(&relation.properties)?,
                relation.created_at,
            ],
        )?;
        Ok(())
    }

    /// Load all entities
    pub fn load_entities(&self) -> Result<Vec<Entity>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, entity_type, properties, created_at, updated_at FROM kg_entities",
        )?;

        let entities: Vec<(String, String, String, String, i64, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        entities
            .into_iter()
            .map(
                |(id, name, entity_type, properties_json, created_at, updated_at)| {
                    Ok(Entity {
                        id,
                        name,
                        entity_type,
                        properties: serde_json::from_str(&properties_json)?,
                        created_at,
                        updated_at,
                    })
                },
            )
            .collect()
    }

    /// Load all relations
    pub fn load_relations(&self) -> Result<Vec<Relation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, target_id, relation_type, properties, created_at FROM kg_relations"
        )?;

        let relations: Vec<(String, String, String, String, String, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        relations
            .into_iter()
            .map(
                |(id, source_id, target_id, relation_type, properties_json, created_at)| {
                    Ok(Relation {
                        id,
                        source_id,
                        target_id,
                        relation_type,
                        properties: serde_json::from_str(&properties_json)?,
                        created_at,
                    })
                },
            )
            .collect()
    }

    /// Load full graph
    pub fn load_graph(&self) -> Result<KnowledgeGraph> {
        let mut graph = KnowledgeGraph::new();

        for entity in self.load_entities()? {
            graph.add_entity(entity);
        }

        for relation in self.load_relations()? {
            graph.add_relation(relation);
        }

        Ok(graph)
    }

    /// Delete entity (cascades relations)
    pub fn delete_entity(&self, id: &str) -> Result<()> {
        self.fts.remove_entity(id)?;
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM kg_relations WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM kg_entities WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    /// Get stats
    pub fn stats(&self) -> Result<GraphStats> {
        let conn = self.conn.lock().unwrap();
        let entity_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM kg_entities", [], |row| row.get(0))?;
        let relation_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM kg_relations", [], |row| row.get(0))?;
        let type_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT entity_type) FROM kg_entities",
            [],
            |row| row.get(0),
        )?;

        Ok(GraphStats {
            entity_count: entity_count as usize,
            relation_count: relation_count as usize,
            type_count: type_count as usize,
        })
    }

    /// FTS search
    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        self.fts.search(query, limit)
    }
}
