use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEntity {
    pub id: String,
    pub name: String,
    pub entity_type: String,
    pub properties: serde_json::Value,
    pub relations: Vec<EntityRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRelation {
    pub target_id: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMemory {
    entities: HashMap<String, SemanticEntity>,
}

impl SemanticMemory {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, entity: SemanticEntity) {
        self.entities.insert(entity.id.clone(), entity);
    }

    pub fn get_entity(&self, id: &str) -> Option<&SemanticEntity> {
        self.entities.get(id)
    }

    pub fn remove_entity(&mut self, id: &str) -> Option<SemanticEntity> {
        self.entities.remove(id)
    }

    pub fn search(&self, query: &str) -> Vec<&SemanticEntity> {
        let query_lower = query.to_lowercase();
        self.entities
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&query_lower)
                    || e.entity_type.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    pub fn get_all_entities(&self) -> Vec<&SemanticEntity> {
        self.entities.values().collect()
    }

    pub fn get_entities_by_type(&self, entity_type: &str) -> Vec<&SemanticEntity> {
        let type_lower = entity_type.to_lowercase();
        self.entities
            .values()
            .filter(|e| e.entity_type.to_lowercase() == type_lower)
            .collect()
    }

    pub fn add_relation(&mut self, source_id: &str, relation: EntityRelation) -> bool {
        if let Some(entity) = self.entities.get_mut(source_id) {
            entity.relations.push(relation);
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.entities.clear();
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

impl Default for SemanticMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_memory_add_and_get() {
        let mut memory = SemanticMemory::new();

        let entity = SemanticEntity {
            id: "1".to_string(),
            name: "test_entity".to_string(),
            entity_type: "test".to_string(),
            properties: serde_json::json!({"key": "value"}),
            relations: vec![],
        };

        memory.add_entity(entity);

        assert_eq!(memory.len(), 1);
        assert_eq!(memory.get_entity("1").unwrap().name, "test_entity");
    }

    #[test]
    fn test_semantic_memory_search() {
        let mut memory = SemanticMemory::new();

        memory.add_entity(SemanticEntity {
            id: "1".to_string(),
            name: "User Alice".to_string(),
            entity_type: "person".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        memory.add_entity(SemanticEntity {
            id: "2".to_string(),
            name: "User Bob".to_string(),
            entity_type: "person".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        let results = memory.search("Alice");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");

        let results = memory.search("user");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_semantic_memory_get_by_type() {
        let mut memory = SemanticMemory::new();

        memory.add_entity(SemanticEntity {
            id: "1".to_string(),
            name: "entity1".to_string(),
            entity_type: "person".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        memory.add_entity(SemanticEntity {
            id: "2".to_string(),
            name: "entity2".to_string(),
            entity_type: "location".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        let persons = memory.get_entities_by_type("person");
        assert_eq!(persons.len(), 1);
    }

    #[test]
    fn test_semantic_memory_relations() {
        let mut memory = SemanticMemory::new();

        memory.add_entity(SemanticEntity {
            id: "1".to_string(),
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        memory.add_entity(SemanticEntity {
            id: "2".to_string(),
            name: "Bob".to_string(),
            entity_type: "person".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        let result = memory.add_relation(
            "1",
            EntityRelation {
                target_id: "2".to_string(),
                relation_type: "friend".to_string(),
            },
        );

        assert!(result);
        assert_eq!(
            memory.get_entity("1").unwrap().relations.len(),
            1
        );
    }

    #[test]
    fn test_semantic_memory_remove() {
        let mut memory = SemanticMemory::new();

        memory.add_entity(SemanticEntity {
            id: "1".to_string(),
            name: "test".to_string(),
            entity_type: "test".to_string(),
            properties: serde_json::json!({}),
            relations: vec![],
        });

        assert_eq!(memory.len(), 1);

        let removed = memory.remove_entity("1");
        assert!(removed.is_some());
        assert!(memory.is_empty());
    }
}
