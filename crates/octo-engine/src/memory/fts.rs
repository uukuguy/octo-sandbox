//! FTS5 Full-text search for knowledge graph

use anyhow::Result;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct FtsStore {
    conn: Arc<Mutex<Connection>>,
}

impl FtsStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize FTS5 virtual table
    pub fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS kg_fts USING fts5(
                entity_id,
                name,
                entity_type,
                properties,
                content='',
                tokenize='porter unicode61'
            );
            "#,
        )?;
        Ok(())
    }

    /// Index entity
    pub fn index_entity(
        &self,
        entity_id: &str,
        name: &str,
        entity_type: &str,
        properties: &serde_json::Value,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO kg_fts (entity_id, name, entity_type, properties) VALUES (?1, ?2, ?3, ?4)",
            params![
                entity_id,
                name,
                entity_type,
                serde_json::to_string(properties)?
            ],
        )?;
        Ok(())
    }

    /// Remove from index
    pub fn remove_entity(&self, entity_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM kg_fts WHERE entity_id = ?1",
            params![entity_id],
        )?;
        Ok(())
    }

    /// Search entities
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt =
            conn.prepare("SELECT entity_id FROM kg_fts WHERE kg_fts MATCH ?1 LIMIT ?2")?;

        let ids = stmt
            .query_map(params![query, limit as i64], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(ids)
    }

    /// Rebuild index from entities
    pub fn rebuild(&self, entities: &[(String, String, String, serde_json::Value)]) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM kg_fts", [])?;

        for (id, name, etype, props) in entities {
            conn.execute(
                "INSERT OR REPLACE INTO kg_fts (entity_id, name, entity_type, properties) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    name,
                    etype,
                    serde_json::to_string(props)?
                ],
            )?;
        }

        Ok(())
    }
}
