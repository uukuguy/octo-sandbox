//! Knowledge Graph - Entity-relation storage with graph queries

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Node in knowledge graph (entity)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub properties: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Edge in knowledge graph (relation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub properties: serde_json::Value,
    pub created_at: i64,
}

/// Knowledge graph with entity-relation storage
///
/// Note: This struct uses interior mutability with `RefCell` for thread-safe access
/// when integrated with the MemorySystem. Currently not thread-safe - must be
/// wrapped with `RwLock` or similar before sharing across threads.
pub struct KnowledgeGraph {
    entities: HashMap<String, Entity>,
    relations: HashMap<String, Relation>,
    // Index: entity_id -> relation_ids (outgoing)
    outgoing: HashMap<String, Vec<String>>,
    // Index: entity_id -> relation_ids (incoming)
    incoming: HashMap<String, Vec<String>>,
    // Index: type -> entity_ids
    by_type: HashMap<String, Vec<String>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            by_type: HashMap::new(),
        }
    }

    /// Add entity
    ///
    /// If an entity with the same ID already exists, it will be replaced.
    /// The type index will be updated to avoid duplicates.
    pub fn add_entity(&mut self, entity: Entity) {
        let entity_id = entity.id.clone();
        let entity_type = entity.entity_type.clone();

        // Check if entity already exists (update case)
        let is_update = self.entities.contains_key(&entity_id);

        self.entities.insert(entity_id.clone(), entity);

        // Only add to by_type index if this is a new entity (not an update)
        // For updates, the old ID already exists in the index
        if !is_update {
            self.by_type.entry(entity_type).or_default().push(entity_id);
        }
    }

    /// Add relation
    pub fn add_relation(&mut self, relation: Relation) -> bool {
        // Verify both entities exist
        if !self.entities.contains_key(&relation.source_id)
            || !self.entities.contains_key(&relation.target_id)
        {
            return false;
        }

        self.relations.insert(relation.id.clone(), relation.clone());
        self.outgoing
            .entry(relation.source_id.clone())
            .or_default()
            .push(relation.id.clone());
        self.incoming
            .entry(relation.target_id.clone())
            .or_default()
            .push(relation.id.clone());
        true
    }

    /// Get entity by ID
    pub fn get_entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Get entities by type
    pub fn get_entities_by_type(&self, entity_type: &str) -> Vec<&Entity> {
        self.by_type
            .get(entity_type)
            .map(|ids| ids.iter().filter_map(|id| self.entities.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get outgoing relations
    pub fn get_outgoing(&self, entity_id: &str) -> Vec<&Relation> {
        self.outgoing
            .get(entity_id)
            .map(|ids| ids.iter().filter_map(|id| self.relations.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get incoming relations
    pub fn get_incoming(&self, entity_id: &str) -> Vec<&Relation> {
        self.incoming
            .get(entity_id)
            .map(|ids| ids.iter().filter_map(|id| self.relations.get(id)).collect())
            .unwrap_or_default()
    }

    /// Breadth-first search traversal
    pub fn traverse_bfs(&self, start_id: &str, max_depth: usize) -> Vec<(String, Entity, usize)> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        queue.push_back((start_id.to_string(), 0));
        visited.insert(start_id.to_string());

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth > max_depth {
                continue;
            }

            if let Some(entity) = self.entities.get(&current_id) {
                results.push((current_id.clone(), entity.clone(), depth));
            }

            // Add neighbors to queue
            for relation in self.get_outgoing(&current_id) {
                if !visited.contains(&relation.target_id) {
                    visited.insert(relation.target_id.clone());
                    queue.push_back((relation.target_id.clone(), depth + 1));
                }
            }
        }

        results
    }

    /// Find shortest path between two entities (BFS)
    pub fn find_path(&self, start_id: &str, end_id: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(vec![start_id.to_string()]);
        visited.insert(start_id.to_string());

        while let Some(path) = queue.pop_front() {
            let current = path.last().unwrap();

            if current == end_id {
                return Some(path);
            }

            for relation in self.get_outgoing(current) {
                if !visited.contains(&relation.target_id) {
                    visited.insert(relation.target_id.clone());
                    let mut new_path = path.clone();
                    new_path.push(relation.target_id.clone());
                    queue.push_back(new_path);
                }
            }
        }

        None
    }

    /// Search entities by name pattern
    pub fn search(&self, query: &str) -> Vec<&Entity> {
        let query_lower = query.to_lowercase();
        self.entities
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&query_lower)
                    || e.entity_type.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Remove entity and its relations
    ///
    /// Removes the entity and cleans up:
    /// - All relations where this entity is source or target
    /// - Both outgoing and incoming indexes
    /// - The type index
    pub fn remove_entity(&mut self, id: &str) -> Option<Entity> {
        if let Some(entity) = self.entities.remove(id) {
            // Collect all relation IDs that reference this entity (as source OR target)
            let mut relation_ids_to_remove: Vec<String> = Vec::new();

            // Check all relations for references to this entity
            for (rel_id, relation) in &self.relations {
                if relation.source_id == id || relation.target_id == id {
                    relation_ids_to_remove.push(rel_id.clone());
                }
            }

            // Remove all collected relations from the relations HashMap
            for rel_id in &relation_ids_to_remove {
                self.relations.remove(rel_id);
            }

            // Clean up outgoing index (relations where this entity is source)
            self.outgoing.remove(id);

            // Clean up incoming index (relations where this entity is target)
            self.incoming.remove(id);

            // Also clean up any stale entries in other entities' indexes
            // (in case the relation cleanup above missed any edge cases)
            for rel_ids in self.outgoing.values_mut() {
                rel_ids.retain(|r| !relation_ids_to_remove.contains(r));
            }
            for rel_ids in self.incoming.values_mut() {
                rel_ids.retain(|r| !relation_ids_to_remove.contains(r));
            }

            // Remove from type index
            if let Some(mut ids) = self.by_type.remove(&entity.entity_type) {
                ids.retain(|i| i != id);
                if !ids.is_empty() {
                    self.by_type.insert(entity.entity_type.clone(), ids);
                }
            }

            Some(entity)
        } else {
            None
        }
    }

    /// Get stats
    pub fn stats(&self) -> GraphStats {
        GraphStats {
            entity_count: self.entities.len(),
            relation_count: self.relations.len(),
            type_count: self.by_type.len(),
        }
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub entity_count: usize,
    pub relation_count: usize,
    pub type_count: usize,
}
