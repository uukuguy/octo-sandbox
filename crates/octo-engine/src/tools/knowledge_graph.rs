//! Knowledge Graph tools — graph_query, graph_add, graph_relate
//!
//! Exposes the KnowledgeGraph as three LLM-callable tools registered
//! in the ToolRegistry.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use octo_types::{ToolContext, ToolOutput, ToolSource};

use crate::memory::graph::{Entity, KnowledgeGraph, Relation};

use super::traits::Tool;

// ---------------------------------------------------------------------------
// GraphQueryTool
// ---------------------------------------------------------------------------

/// Search the knowledge graph for entities by name/type pattern.
pub struct GraphQueryTool {
    kg: Arc<RwLock<KnowledgeGraph>>,
}

impl GraphQueryTool {
    pub fn new(kg: Arc<RwLock<KnowledgeGraph>>) -> Self {
        Self { kg }
    }
}

#[async_trait]
impl Tool for GraphQueryTool {
    fn name(&self) -> &str {
        "graph_query"
    }

    fn description(&self) -> &str {
        "Search the knowledge graph for entities matching a query string. \
         Optionally filter by entity type and limit results."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query (matched against entity name and type)"
                },
                "entity_type": {
                    "type": "string",
                    "description": "Optional entity type filter"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 20)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?;
        let entity_type = params["entity_type"].as_str();
        let limit = params["limit"].as_u64().unwrap_or(20) as usize;

        let guard = self.kg.read().await;

        let entities: Vec<&Entity> = if let Some(et) = entity_type {
            guard.get_entities_by_type(et)
        } else {
            guard.search(query)
        };

        let entities: Vec<&Entity> = entities.into_iter().take(limit).collect();

        if entities.is_empty() {
            return Ok(ToolOutput::success("No entities found.".to_string()));
        }

        let items: Vec<Value> = entities
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "name": e.name,
                    "entity_type": e.entity_type,
                    "properties": e.properties,
                })
            })
            .collect();

        let output = json!({
            "count": items.len(),
            "entities": items,
        });

        Ok(ToolOutput::success(
            serde_json::to_string_pretty(&output)?,
        ))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

// ---------------------------------------------------------------------------
// GraphAddTool
// ---------------------------------------------------------------------------

/// Add entities and relations to the knowledge graph.
pub struct GraphAddTool {
    kg: Arc<RwLock<KnowledgeGraph>>,
}

impl GraphAddTool {
    pub fn new(kg: Arc<RwLock<KnowledgeGraph>>) -> Self {
        Self { kg }
    }
}

#[async_trait]
impl Tool for GraphAddTool {
    fn name(&self) -> &str {
        "graph_add"
    }

    fn description(&self) -> &str {
        "Add entities and/or relations to the knowledge graph."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "entities": {
                    "type": "array",
                    "description": "Entities to add",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "entity_type": { "type": "string" },
                            "properties": {
                                "type": "object",
                                "description": "Arbitrary key-value properties"
                            }
                        },
                        "required": ["name", "entity_type"]
                    }
                },
                "relations": {
                    "type": "array",
                    "description": "Relations to add (both source and target entities must exist)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "from": {
                                "type": "string",
                                "description": "Source entity ID"
                            },
                            "to": {
                                "type": "string",
                                "description": "Target entity ID"
                            },
                            "relation_type": { "type": "string" }
                        },
                        "required": ["from", "to", "relation_type"]
                    }
                }
            }
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let now = chrono::Utc::now().timestamp();
        let mut entities_added: usize = 0;
        let mut relations_added: usize = 0;
        let mut relation_failures: usize = 0;

        let mut guard = self.kg.write().await;

        // --- entities ---
        if let Some(arr) = params["entities"].as_array() {
            for item in arr {
                let name = item["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let entity_type = item["entity_type"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let properties = item
                    .get("properties")
                    .cloned()
                    .unwrap_or(json!({}));

                let id = format!("{}:{}", entity_type, name);

                guard.add_entity(Entity {
                    id,
                    name,
                    entity_type,
                    properties,
                    created_at: now,
                    updated_at: now,
                });
                entities_added += 1;
            }
        }

        // --- relations ---
        if let Some(arr) = params["relations"].as_array() {
            for item in arr {
                let from = item["from"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let to = item["to"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let relation_type = item["relation_type"]
                    .as_str()
                    .unwrap_or("related_to")
                    .to_string();

                let id = format!("rel:{}->{}:{}", from, to, relation_type);

                let ok = guard.add_relation(Relation {
                    id,
                    source_id: from,
                    target_id: to,
                    relation_type,
                    properties: json!({}),
                    created_at: now,
                });
                if ok {
                    relations_added += 1;
                } else {
                    relation_failures += 1;
                }
            }
        }

        let mut msg = format!(
            "Added {} entities and {} relations.",
            entities_added, relations_added
        );
        if relation_failures > 0 {
            msg.push_str(&format!(
                " {} relations failed (source or target entity not found).",
                relation_failures
            ));
        }
        Ok(ToolOutput::success(msg))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

// ---------------------------------------------------------------------------
// GraphRelateTool
// ---------------------------------------------------------------------------

/// Query relation paths between two entities in the knowledge graph.
pub struct GraphRelateTool {
    kg: Arc<RwLock<KnowledgeGraph>>,
}

impl GraphRelateTool {
    pub fn new(kg: Arc<RwLock<KnowledgeGraph>>) -> Self {
        Self { kg }
    }
}

#[async_trait]
impl Tool for GraphRelateTool {
    fn name(&self) -> &str {
        "graph_relate"
    }

    fn description(&self) -> &str {
        "Find the shortest path between two entities in the knowledge graph, \
         or traverse from an entity up to a maximum number of hops."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "from_entity": {
                    "type": "string",
                    "description": "Source entity ID"
                },
                "to_entity": {
                    "type": "string",
                    "description": "Target entity ID (omit for BFS traversal from source)"
                },
                "max_hops": {
                    "type": "integer",
                    "description": "Maximum traversal depth (default: 3)"
                }
            },
            "required": ["from_entity"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let from = params["from_entity"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'from_entity' parameter"))?;
        let to = params["to_entity"].as_str();
        let max_hops = params["max_hops"].as_u64().unwrap_or(3) as usize;

        let guard = self.kg.read().await;

        if let Some(target) = to {
            // Shortest path mode
            match guard.find_path(from, target) {
                Some(path) => {
                    let output = json!({
                        "mode": "shortest_path",
                        "from": from,
                        "to": target,
                        "path": path,
                        "hops": path.len().saturating_sub(1),
                    });
                    Ok(ToolOutput::success(
                        serde_json::to_string_pretty(&output)?,
                    ))
                }
                None => Ok(ToolOutput::success(format!(
                    "No path found from '{}' to '{}'.",
                    from, target
                ))),
            }
        } else {
            // BFS traversal mode
            let results = guard.traverse_bfs(from, max_hops);
            if results.is_empty() {
                return Ok(ToolOutput::success(format!(
                    "Entity '{}' not found or has no connections.",
                    from
                )));
            }

            let items: Vec<Value> = results
                .iter()
                .map(|(id, entity, depth)| {
                    json!({
                        "id": id,
                        "name": entity.name,
                        "entity_type": entity.entity_type,
                        "depth": depth,
                    })
                })
                .collect();

            let output = json!({
                "mode": "bfs_traversal",
                "from": from,
                "max_hops": max_hops,
                "count": items.len(),
                "entities": items,
            });
            Ok(ToolOutput::success(
                serde_json::to_string_pretty(&output)?,
            ))
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }
}

// ---------------------------------------------------------------------------
// Registration helper
// ---------------------------------------------------------------------------

use super::ToolRegistry;

/// Register all knowledge graph tools into the given registry.
pub fn register_kg_tools(
    registry: &mut ToolRegistry,
    kg: Arc<RwLock<KnowledgeGraph>>,
) {
    registry.register(GraphQueryTool::new(kg.clone()));
    registry.register(GraphAddTool::new(kg.clone()));
    registry.register(GraphRelateTool::new(kg));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use octo_types::ToolContext;

    fn make_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: Default::default(),
            working_dir: "/tmp".into(),
            path_validator: None,
        }
    }

    fn make_kg() -> Arc<RwLock<KnowledgeGraph>> {
        Arc::new(RwLock::new(KnowledgeGraph::new()))
    }

    // --- spec tests ---

    #[test]
    fn test_graph_query_tool_spec() {
        let kg = make_kg();
        let tool = GraphQueryTool::new(kg);
        assert_eq!(tool.name(), "graph_query");
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn test_graph_add_tool_spec() {
        let kg = make_kg();
        let tool = GraphAddTool::new(kg);
        assert_eq!(tool.name(), "graph_add");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
    }

    #[test]
    fn test_graph_relate_tool_spec() {
        let kg = make_kg();
        let tool = GraphRelateTool::new(kg);
        assert_eq!(tool.name(), "graph_relate");
        let params = tool.parameters();
        let required = params["required"].as_array().unwrap();
        assert!(required.contains(&json!("from_entity")));
    }

    // --- execution tests ---

    #[tokio::test]
    async fn test_graph_add_tool_execution() {
        let kg = make_kg();
        let add_tool = GraphAddTool::new(kg.clone());
        let query_tool = GraphQueryTool::new(kg);
        let ctx = make_ctx();

        // Add an entity
        let result = add_tool
            .execute(
                json!({
                    "entities": [
                        { "name": "Alice", "entity_type": "person" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("Added 1 entities"));

        // Query it back
        let result = query_tool
            .execute(json!({ "query": "Alice" }), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("Alice"));
    }

    #[tokio::test]
    async fn test_graph_relate_tool_no_path() {
        let kg = make_kg();
        let add_tool = GraphAddTool::new(kg.clone());
        let relate_tool = GraphRelateTool::new(kg);
        let ctx = make_ctx();

        // Add two unconnected entities
        add_tool
            .execute(
                json!({
                    "entities": [
                        { "name": "X", "entity_type": "node" },
                        { "name": "Y", "entity_type": "node" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        let result = relate_tool
            .execute(
                json!({
                    "from_entity": "node:X",
                    "to_entity": "node:Y"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("No path found"));
    }

    #[tokio::test]
    async fn test_graph_query_by_type() {
        let kg = make_kg();
        let add_tool = GraphAddTool::new(kg.clone());
        let query_tool = GraphQueryTool::new(kg);
        let ctx = make_ctx();

        add_tool
            .execute(
                json!({
                    "entities": [
                        { "name": "Rust", "entity_type": "language" },
                        { "name": "Python", "entity_type": "language" },
                        { "name": "Alice", "entity_type": "person" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        // Query by type
        let result = query_tool
            .execute(
                json!({
                    "query": "",
                    "entity_type": "language"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.content.contains("Rust"));
        assert!(result.content.contains("Python"));
        assert!(!result.content.contains("Alice"));
    }

    #[tokio::test]
    async fn test_graph_add_with_relations() {
        let kg = make_kg();
        let add_tool = GraphAddTool::new(kg.clone());
        let relate_tool = GraphRelateTool::new(kg);
        let ctx = make_ctx();

        // Add entities first, then relations in a second call
        add_tool
            .execute(
                json!({
                    "entities": [
                        { "name": "A", "entity_type": "node" },
                        { "name": "B", "entity_type": "node" },
                        { "name": "C", "entity_type": "node" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        let result = add_tool
            .execute(
                json!({
                    "relations": [
                        { "from": "node:A", "to": "node:B", "relation_type": "connects" },
                        { "from": "node:B", "to": "node:C", "relation_type": "connects" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("2 relations"));

        // Verify path A -> B -> C
        let result = relate_tool
            .execute(
                json!({
                    "from_entity": "node:A",
                    "to_entity": "node:C"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("shortest_path"));
        assert!(result.content.contains("node:A"));
        assert!(result.content.contains("node:C"));
    }

    #[tokio::test]
    async fn test_graph_add_relation_failure() {
        let kg = make_kg();
        let add_tool = GraphAddTool::new(kg);
        let ctx = make_ctx();

        // Try to add a relation without entities
        let result = add_tool
            .execute(
                json!({
                    "relations": [
                        { "from": "missing:A", "to": "missing:B", "relation_type": "x" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("1 relations failed"));
    }

    #[tokio::test]
    async fn test_graph_query_empty() {
        let kg = make_kg();
        let tool = GraphQueryTool::new(kg);
        let ctx = make_ctx();

        let result = tool
            .execute(json!({ "query": "nonexistent" }), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("No entities found"));
    }

    #[tokio::test]
    async fn test_graph_relate_bfs_traversal() {
        let kg = make_kg();
        let add_tool = GraphAddTool::new(kg.clone());
        let relate_tool = GraphRelateTool::new(kg);
        let ctx = make_ctx();

        add_tool
            .execute(
                json!({
                    "entities": [
                        { "name": "Root", "entity_type": "node" },
                        { "name": "Child", "entity_type": "node" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        add_tool
            .execute(
                json!({
                    "relations": [
                        { "from": "node:Root", "to": "node:Child", "relation_type": "has" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        // Traverse without target (BFS mode)
        let result = relate_tool
            .execute(
                json!({
                    "from_entity": "node:Root",
                    "max_hops": 2
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("bfs_traversal"));
        assert!(result.content.contains("Root"));
        assert!(result.content.contains("Child"));
    }
}
