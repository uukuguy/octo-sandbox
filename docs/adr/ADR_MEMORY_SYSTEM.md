# ADR：MEMORY SYSTEM 架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-07
**状态**：已完成

---

## 目录

- [ADR-019：四层记忆架构](#adr-019四层记忆架构)
- [ADR-020：HNSW 向量索引](#adr-020hnsw-向量索引)
- [ADR-021：混合查询引擎](#adr-021混合查询引擎)
- [ADR-022：ContextInjector Zone B 动态上下文](#adr-022contextinjector-zone-b-动态上下文)

---

## ADR-019：四层记忆架构

### 状态

**已完成** — 2026-03-07

### 上下文

系统需要支持多层记忆系统以满足不同场景：
- L0: 当前对话上下文（Working Memory）
- L1: 会话级记忆（Session Memory）
- L2: 长期记忆（Persistent Memory）
- L3: 知识图谱（Knowledge Graph）

### 决策

实现统一的 MemorySystem 包含四层：

```rust
pub struct MemorySystem {
    pub working: InMemoryWorkingMemory,      // L0
    pub session: SqliteSessionStore,        // L1
    pub persistent: SqliteMemoryStore,      // L2
    pub knowledge_graph: Arc<RwLock<KnowledgeGraph>>, // L3
}
```

**层级设计**：

| 层级 | 存储 | 生命周期 | 访问模式 |
|------|------|---------|---------|
| L0 Working | InMemory/HashMap | 当前对话 | 即时读写 |
| L1 Session | SQLite | 会话期间 | 持久化 |
| L2 Persistent | SQLite | 长期 | 搜索+检索 |
| L3 Knowledge | 内存+SQLite | 永久 | 图遍历 |

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/memory/mod.rs` | MemorySystem 入口 |
| `src/memory/working.rs` | InMemoryWorkingMemory |
| `src/memory/sqlite_working.rs` | SqliteWorkingMemory |
| `src/memory/sqlite_store.rs` | SqliteMemoryStore |
| `src/memory/traits.rs` | WorkingMemory trait |
| `src/memory/store_traits.rs` | MemoryStore trait |

---

## ADR-020：HNSW 向量索引

### 状态

**已完成** — 2026-03-07

### 上下文

语义搜索需要高效的大规模向量近似最近邻搜索能力。

### 决策

使用 hnsw_rs 实现 HNSW 索引，支持可选特性：

```rust
#[cfg(feature = "hnsw")]
pub struct HnswIndex {
    index: Arc<Mutex<Hnsw>>,
    config: HnswConfig,
}

pub struct HnswConfig {
    pub max_elements: usize,
    pub m: usize,
    pub ef_construction: usize,
    pub ef: usize,
}
```

通过 `VectorBackend` 抽象支持多种后端：

```rust
pub enum VectorBackend {
    #[cfg(feature = "hnsw")]
    Hnsw(HnswIndex),
    BruteForce(BruteForceIndex),
}
```

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/memory/vector_index.rs` | VectorIndex, HnswIndex |
| `src/memory/embedding.rs` | EmbeddingClient |

---

## ADR-021：混合查询引擎

### 状态

**已完成** — 2026-03-07

### 上下文

单一检索方式无法满足复杂查询需求，需要融合向量搜索和全文搜索。

### 决策

实现 `HybridQueryEngine` 融合多种检索：

```rust
pub struct HybridQueryEngine {
    vector_backend: Arc<VectorBackend>,
    fts_store: Arc<FtsStore>,
}

pub enum QueryType {
    Vector { query: String, top_k: usize },
    Fts { query: String, limit: usize },
    Hybrid { query: String, vector_weight: f32, fts_weight: f32 },
}
```

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/memory/hybrid_query.rs` | HybridQueryEngine |
| `src/memory/fts.rs` | FtsStore |

---

## ADR-022：ContextInjector Zone B 动态上下文

### 状态

**已完成** — 2026-03-07

### 上下文

LLM 消息构建需要动态注入上下文，包含当前时间和记忆块。

### 决策

实现 `ContextInjector` 编译 Zone B 动态上下文：

```rust
pub struct ContextInjector;

impl ContextInjector {
    pub fn compile(blocks: &[MemoryBlock]) -> String {
        // 输出 <context> XML 块
    }
}
```

输出格式：
```xml
<context>
<datetime>2026-03-07 14:30 CST</datetime>
<user_profile priority="128">User Profile 内容</user_profile>
<task_context priority="200">Task Context 内容</task_context>
</context>
```

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/memory/injector.rs` | ContextInjector |
