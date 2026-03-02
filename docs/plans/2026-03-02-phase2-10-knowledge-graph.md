# Memory 知识图谱增强实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**：扩展现有 SemanticMemory 为完整知识图谱系统，支持持久化存储、图查询、语义搜索，为 Agent 提供长期记忆能力。

**架构**：三层存储（Working/Session/Persistent）+ 知识图谱（实体/关系/查询），支持 FTS5 全文搜索。

**技术栈**：Rust async/tokio、SQLite (rusqlite)、FTS5

---

## 背景：当前代码状态

- `crates/octo-engine/src/memory/semantic.rs` — 基础 SemanticMemory（内存版）
- `crates/octo-engine/src/memory/sqlite_store.rs` — SQLite 持久化
- `crates/octo-engine/src/memory/working.rs` — Working Memory

**当前局限**：
- SemanticMemory 仅内存存储，无持久化
- 无图查询能力（关系遍历）
- 无 FTS5 全文搜索

---

## 文件索引

### 新增文件
| 文件 | 任务 | 说明 |
|------|------|------|
| `crates/octo-engine/src/memory/graph.rs` | Task 1 | KnowledgeGraph 核心实现 |
| `crates/octo-engine/src/memory/graph_store.rs` | Task 2 | 知识图谱 SQLite 持久化 |
| `crates/octo-engine/src/memory/fts.rs` | Task 3 | FTS5 全文搜索集成 |

### 修改文件
| 文件 | 任务 | 说明 |
|------|------|------|
| `crates/octo-engine/src/memory/mod.rs` | Task 1 | 导出新模块 |
| `crates/octo-engine/src/memory/semantic.rs` | Task 4 | 集成持久化 |
| `crates/octo-engine/src/db/mod.rs` | Task 2 | 添加图谱表迁移 |

---

## Task 1：创建 KnowledgeGraph 核心结构

**目标**：在现有 SemanticMemory 基础上，扩展为支持图查询的知识图谱。

**文件**：
- 新增：`crates/octo-engine/src/memory/graph.rs`

### Step 1: 创建 graph.rs

```rust
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
    pub fn add_entity(&mut self, entity: Entity) {
        self.entities.insert(entity.id.clone(), entity.clone());
        self.by_type
            .entry(entity.entity_type.clone())
            .or_default()
            .push(entity.id.clone());
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
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.entities.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get outgoing relations
    pub fn get_outgoing(&self, entity_id: &str) -> Vec<&Relation> {
        self.outgoing
            .get(entity_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.relations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get incoming relations
    pub fn get_incoming(&self, entity_id: &str) -> Vec<&Relation> {
        self.incoming
            .get(entity_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.relations.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Breadth-first search traversal
    pub fn traverse_bfs(
        &self,
        start_id: &str,
        max_depth: usize,
    ) -> Vec<(String, Entity, usize)> {
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
    pub fn remove_entity(&mut self, id: &str) -> Option<Entity> {
        if let Some(entity) = self.entities.remove(id) {
            // Remove outgoing relations
            if let Some(rel_ids) = self.outgoing.remove(id) {
                for rel_id in rel_ids {
                    self.relations.remove(&rel_id);
                }
            }

            // Remove incoming relations
            if let Some(rel_ids) = self.incoming.remove(id) {
                for rel_id in rel_ids {
                    self.relations.remove(&rel_id);
                }
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
```

### Step 2: 更新 memory/mod.rs

```rust
pub mod graph;
pub use graph::{Entity, KnowledgeGraph, Relation, GraphStats};
```

### Step 3: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/memory/graph.rs
git commit -m "feat(memory): add KnowledgeGraph with entity-relation storage and BFS traversal"
```

---

## Task 2：知识图谱持久化存储

**目标**：将 KnowledgeGraph 持久化到 SQLite，支持迁移和加载。

**文件**：
- 新增：`crates/octo-engine/src/memory/graph_store.rs`
- 修改：`crates/octo-engine/src/db/mod.rs`

### Step 1: 创建 graph_store.rs

```rust
//! Knowledge Graph SQLite storage

use super::graph::{Entity, GraphStats, KnowledgeGraph, Relation};
use anyhow::Result;
use rusqlite::{params, Connection};

pub struct GraphStore {
    conn: Connection,
}

impl GraphStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Initialize tables
    pub fn init(&self) -> Result<()> {
        self.conn.execute_batch(
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
        Ok(())
    }

    /// Save entity
    pub fn save_entity(&self, entity: &Entity) -> Result<()> {
        self.conn.execute(
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
        Ok(())
    }

    /// Save relation
    pub fn save_relation(&self, relation: &Relation) -> Result<()> {
        self.conn.execute(
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
        let mut stmt = self.conn.prepare(
            "SELECT id, name, entity_type, properties, created_at, updated_at FROM kg_entities"
        )?;

        let entities = stmt
            .query_map([], |row| {
                Ok(Entity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    properties: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entities)
    }

    /// Load all relations
    pub fn load_relations(&self) -> Result<Vec<Relation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, target_id, relation_type, properties, created_at FROM kg_relations"
        )?;

        let relations = stmt
            .query_map([], |row| {
                Ok(Relation {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    relation_type: row.get(3)?,
                    properties: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(relations)
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
        self.conn.execute("DELETE FROM kg_relations WHERE source_id = ?1 OR target_id = ?1", params![id])?;
        self.conn.execute("DELETE FROM kg_entities WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Get stats
    pub fn stats(&self) -> Result<GraphStats> {
        let entity_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM kg_entities",
            [],
            |row| row.get(0),
        )?;
        let relation_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM kg_relations",
            [],
            |row| row.get(0),
        )?;
        let type_count: i64 = self.conn.query_row(
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
}
```

### Step 2: 更新 memory/mod.rs

```rust
pub mod graph_store;
pub use graph_store::GraphStore;
```

### Step 3: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/memory/graph_store.rs
git commit -m "feat(memory): add GraphStore for SQLite persistence"
```

---

## Task 3：FTS5 全文搜索集成

**目标**：为知识图谱添加 FTS5 全文搜索支持。

**文件**：
- 新增：`crates/octo-engine/src/memory/fts.rs`
- 修改：`crates/octo-engine/src/memory/graph_store.rs`

### Step 1: 创建 fts.rs

```rust
//! FTS5 Full-text search for knowledge graph

use anyhow::Result;
use rusqlite::{params, Connection};

pub struct FtsStore {
    conn: Connection,
}

impl FtsStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Initialize FTS5 virtual table
    pub fn init(&self) -> Result<()> {
        self.conn.execute_batch(
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
    pub fn index_entity(&self, entity_id: &str, name: &str, entity_type: &str, properties: &serde_json::Value) -> Result<()> {
        self.conn.execute(
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
        self.conn.execute(
            "DELETE FROM kg_fts WHERE entity_id = ?1",
            params![entity_id],
        )?;
        Ok(())
    }

    /// Search entities
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT entity_id FROM kg_fts WHERE kg_fts MATCH ?1 LIMIT ?2"
        )?;

        let ids = stmt
            .query_map(params![query, limit as i64], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(ids)
    }

    /// Rebuild index from entities
    pub fn rebuild(&self, entities: &[(String, String, String, serde_json::Value)]) -> Result<()> {
        self.conn.execute("DELETE FROM kg_fts", [])?;

        for (id, name, etype, props) in entities {
            self.index_entity(id, name, etype, &props)?;
        }

        Ok(())
    }
}
```

### Step 2: 更新 graph_store.rs 集成 FTS

在 GraphStore 中添加：

```rust
use super::fts::FtsStore;

pub struct GraphStore {
    conn: Connection,
    fts: FtsStore,  // Add this
}

impl GraphStore {
    pub fn new(conn: Connection) -> Self {
        let fts = FtsStore::new(conn.clone());
        Self { conn, fts }
    }

    pub fn init(&self) -> Result<()> {
        self.conn.execute_batch(/* ... */)?;
        self.fts.init()?;  // Add FTS init
        Ok(())
    }

    pub fn save_entity(&self, entity: &Entity) -> Result<()> {
        // ... existing code
        self.fts.index_entity(
            &entity.id,
            &entity.name,
            &entity.entity_type,
            &entity.properties,
        )?;  // Add FTS indexing
        Ok(())
    }

    pub fn delete_entity(&self, id: &str) -> Result<()> {
        self.fts.remove_entity(id)?;  // Remove from FTS
        // ... existing code
    }

    /// FTS search
    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        self.fts.search(query, limit)
    }
}
```

### Step 3: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 4: Commit

```bash
git add crates/octo-engine/src/memory/fts.rs crates/octo-engine/src/memory/graph_store.rs
git commit -m "feat(memory): add FTS5 full-text search for knowledge graph"
```

---

## Task 4：集成到现有 Memory 系统

**目标**：将 KnowledgeGraph 集成到现有 memory 模块，支持 Agent 使用。

**文件**：
- 修改：`crates/octo-engine/src/memory/mod.rs`
- 修改：`crates/octo-engine/src/memory/semantic.rs`

### Step 1: 更新 mod.rs 导出

```rust
pub use graph::KnowledgeGraph;
pub use graph_store::GraphStore;
pub use fts::FtsStore;
```

### Step 2: 创建 MemorySystem 统一入口

在 memory/mod.rs 添加：

```rust
/// Unified memory system including working, session, persistent, and knowledge graph
pub struct MemorySystem {
    pub working: WorkingMemory,
    pub session: SqliteSessionStore,
    pub persistent: SqliteMemoryStore,
    pub knowledge_graph: Arc<RwLock<KnowledgeGraph>>,
    pub graph_store: GraphStore,
}

impl MemorySystem {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let store = SqliteMemoryStore::new(conn.clone());
        let session = SqliteSessionStore::new(conn.clone());
        let graph_store = GraphStore::new(conn.clone());
        graph_store.init()?;

        Ok(Self {
            working: WorkingMemory::new(),
            session,
            persistent: store,
            knowledge_graph: Arc::new(RwLock::new(KnowledgeGraph::new())),
            graph_store,
        })
    }

    /// Load knowledge graph from storage
    pub async fn load_knowledge_graph(&self) -> Result<()> {
        let graph = self.graph_store.load_graph()?;
        let mut guard = self.knowledge_graph.write().await;
        *guard = graph;
        Ok(())
    }

    /// Add entity to knowledge graph
    pub async fn add_entity(&self, entity: Entity) -> Result<()> {
        self.graph_store.save_entity(&entity)?;
        let mut guard = self.knowledge_graph.write().await;
        guard.add_entity(entity);
        Ok(())
    }

    /// Add relation to knowledge graph
    pub async fn add_relation(&self, relation: Relation) -> Result<bool> {
        let mut guard = self.knowledge_graph.write().await;
        let result = guard.add_relation(relation.clone());
        if result {
            self.graph_store.save_relation(&relation)?;
        }
        Ok(result)
    }

    /// Search knowledge graph
    pub async fn search_knowledge(&self, query: &str) -> Vec<Entity> {
        let guard = self.knowledge_graph.read().await;
        guard.search(query).into_iter().cloned().collect()
    }

    /// Traverse knowledge graph
    pub async fn traverse_knowledge(&self, start_id: &str, max_depth: usize) -> Vec<(String, Entity, usize)> {
        let guard = self.knowledge_graph.read().await;
        guard.traverse_bfs(start_id, max_depth)
    }
}
```

### Step 3: 更新 lib.rs

确认 memory 模块导出：

```rust
pub mod memory;
pub use memory::{MemorySystem, Entity, Relation, KnowledgeGraph};
```

### Step 4: 验证编译

```bash
cargo check -p octo-engine 2>&1 | grep "^error" | head -20
```

### Step 5: Commit

```bash
git add crates/octo-engine/src/memory/
git commit -m "feat(memory): integrate KnowledgeGraph into unified MemorySystem"
```

---

## Task 5：构建验证

### Step 1: 完整编译检查

```bash
cargo check --workspace 2>&1 | tail -5
```

### Step 2: 运行测试

```bash
cargo test -p octo-engine memory 2>&1 | tail -20
```

### Step 3: TypeScript 检查

```bash
cd web && npx tsc --noEmit 2>&1 | tail -10 && cd ..
```

### Step 4: 更新文档

在 `docs/dev/NEXT_SESSION_GUIDE.md` 添加：

```
| Knowledge Graph | P2 | ✅ 已实施 |
```

在 `docs/dev/MEMORY_INDEX.md` 追加：

```
- {时间} | Memory 知识图谱完成: Entity/Relation + Graph + FTS5 + 持久化
```

### Step 5: Commit

```bash
git add docs/dev/
git commit -m "docs: Memory Knowledge Graph complete - entity-relation storage with FTS5"
```

---

## 完成标准

| 检查项 | 验收标准 |
|--------|---------|
| 编译 | `cargo check --workspace` 0 errors |
| KnowledgeGraph | Entity/Relation 存储 + BFS 遍历 + 路径查询 |
| GraphStore | SQLite 持久化 + 迁移 |
| FTS5 | 全文搜索 + 索引 |
| MemorySystem | 统一入口 + async 接口 |
| 测试 | `cargo test` 相关测试通过 |

---

## 提交历史预期

```
feat(memory): add KnowledgeGraph with entity-relation storage and BFS traversal
feat(memory): add GraphStore for SQLite persistence
feat(memory): add FTS5 full-text search for knowledge graph
feat(memory): integrate KnowledgeGraph into unified MemorySystem
docs: Memory Knowledge Graph complete - entity-relation storage with FTS5
```
