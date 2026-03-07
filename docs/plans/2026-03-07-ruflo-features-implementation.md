# RuFlo 功能引入实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**目标**: 将 RuFlo 设计分析中识别的 P0/P1 级功能引入 octo-engine

**架构**: 基于现有 octo-engine 模块扩展，零或极少新依赖，全量向后兼容

**技术栈**: Rust async/tokio, SQLite, hnsw_rs (可选), reqwest, regex, serde_yaml

**参考文档**: `docs/design/RUFLO_DESIGN_ADOPTION_ANALYSIS.md`

---

## 依赖变更汇总（先读再做）

| 功能 | 需新增 crate | 说明 |
|------|-------------|------|
| P0-1 Event Sourcing | 无 | serde_json/rusqlite 已有 |
| P0-2 HNSW 向量索引 | `hnsw_rs = "0.3"` (optional feature) | 设为 `features = ["hnsw"]` |
| P0-3 Hook 扩展 | 无 | async-trait/tokio 已有 |
| P0-4 Agent 路由 | 无 | 已有 capability.rs/router.rs |
| P1-1 声明式 Agent | 无 | serde_yaml 已在 workspace |
| P1-3 AIDefence | 无 | regex 已在 octo-engine |

**Cargo.toml 唯一变更（仅 P0-2 阶段需要）**:

workspace `Cargo.toml`:
```toml
hnsw_rs = "0.3"
```

`crates/octo-engine/Cargo.toml`:
```toml
hnsw_rs = { workspace = true, optional = true }

[features]
hnsw = ["dep:hnsw_rs"]
```

---

## P0-1: Event Sourcing（事件溯源）

### 现状

- `event/store.rs` — EventStore 已有，缺 `aggregate_id` 字段
- `event/projection.rs` — Projection trait + EventCountProjection 已有，缺 ProjectionEngine
- `event/reconstructor.rs` — 不存在，需新建

---

### Task 1: 为 StoredEvent 添加 aggregate_id 并迁移 Schema

**Files:**
- Modify: `crates/octo-engine/src/event/store.rs`
- Modify: `crates/octo-engine/src/event/bus.rs`

**Step 1: 更新 StoredEvent 结构体**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: i64,
    pub aggregate_id: Option<String>,   // 新增
    pub event_type: String,
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp: i64,
    pub sequence: i64,
}
```

**Step 2: 更新 EventStore::new 的 DDL**

```rust
c.execute_batch(
    "CREATE TABLE IF NOT EXISTS events (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        aggregate_id TEXT,
        event_type  TEXT NOT NULL,
        payload     TEXT NOT NULL,
        session_id  TEXT,
        agent_id    TEXT,
        timestamp   INTEGER NOT NULL,
        sequence    INTEGER NOT NULL UNIQUE
    );
    CREATE INDEX IF NOT EXISTS idx_events_aggregate ON events(aggregate_id);
    CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
    CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
    CREATE INDEX IF NOT EXISTS idx_events_sequence ON events(sequence);",
)?;
```

**Step 3: 更新 append 方法签名（aggregate_id 参数位于 session_id 之前）**

```rust
pub async fn append(
    &self,
    event_type: &str,
    payload: serde_json::Value,
    aggregate_id: Option<&str>,   // 新增
    session_id: Option<&str>,
    agent_id: Option<&str>,
) -> anyhow::Result<i64>
```

**Step 4: 添加 read_by_aggregate 方法**

```rust
pub async fn read_by_aggregate(
    &self,
    aggregate_id: &str,
    after_sequence: i64,
    limit: usize,
) -> anyhow::Result<Vec<StoredEvent>>
```

SQL:
```sql
SELECT id, aggregate_id, event_type, payload, session_id, agent_id, timestamp, sequence
FROM events
WHERE aggregate_id = ?1 AND sequence > ?2
ORDER BY sequence ASC
LIMIT ?3
```

**Step 5: 更新 map_row 读取新列顺序**

列顺序：0=id, 1=aggregate_id, 2=event_type, 3=payload, 4=session_id, 5=agent_id, 6=timestamp, 7=sequence

**Step 6: 更新 bus.rs 的 append 调用**

```rust
// session_id 兼作 aggregate_id（会话即聚合根边界）
store.append(&event_type, payload, session_id.as_deref(), session_id.as_deref(), None).await
```

**验证:**
```bash
cargo check -p octo-engine
cargo test -p octo-engine -- event::store --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(event): add aggregate_id to StoredEvent schema and EventStore API"
```

---

### Task 2: 实现 ProjectionEngine（投影注册表 + checkpoint 回放）

**Files:**
- Modify: `crates/octo-engine/src/event/projection.rs`
- Modify: `crates/octo-engine/src/event/mod.rs`

**Step 1: 在 projection.rs 末尾追加 ProjectionCheckpoint 和 ProjectionEngine**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::event::store::EventStore;

#[derive(Debug, Clone, Default)]
struct ProjectionCheckpoint {
    last_sequence: i64,
}

pub struct ProjectionEngine {
    store: Arc<EventStore>,
    projections: RwLock<Vec<Arc<dyn Projection>>>,
    checkpoints: RwLock<HashMap<String, ProjectionCheckpoint>>,
    replay_batch: usize,
}

impl ProjectionEngine {
    pub fn new(store: Arc<EventStore>) -> Self {
        Self {
            store,
            projections: RwLock::new(Vec::new()),
            checkpoints: RwLock::new(HashMap::new()),
            replay_batch: 500,
        }
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.replay_batch = size;
        self
    }

    pub async fn register(&self, projection: Arc<dyn Projection>) {
        let name = projection.name().to_string();
        self.projections.write().await.push(projection);
        self.checkpoints.write().await.entry(name).or_default();
    }

    /// Advance all projections from their last checkpoint to the end of the stream.
    pub async fn catch_up(&self) -> anyhow::Result<()> { /* ... */ }

    /// Reset all checkpoints and replay the full stream.
    pub async fn rebuild_all(&self) -> anyhow::Result<()> { /* ... */ }

    /// Query the last processed sequence for a named projection.
    pub async fn checkpoint(&self, name: &str) -> i64 { /* ... */ }
}
```

**Step 2: 更新 event/mod.rs 导出**

```rust
pub use projection::{EventCountProjection, Projection, ProjectionEngine};
```

**验证:**
```bash
cargo check -p octo-engine
cargo test -p octo-engine -- event::projection --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(event): implement ProjectionEngine with per-projection checkpoints"
```

---

### Task 3: 实现 StateReconstructor（按时间点重建聚合状态）

**Files:**
- Create: `crates/octo-engine/src/event/reconstructor.rs`
- Modify: `crates/octo-engine/src/event/mod.rs`

**Step 1: 定义 AggregateState trait**

```rust
/// Domain aggregate state derived by replaying ordered events.
pub trait AggregateState: Send + Sync + Default {
    fn apply_event(&mut self, event: &StoredEvent);
}
```

**Step 2: 定义 ReconstructionPoint**

```rust
pub enum ReconstructionPoint {
    Current,
    AtSequence(i64),
    AtTimestamp(i64),  // Unix ms
}
```

**Step 3: 实现 StateReconstructor**

```rust
pub struct StateReconstructor {
    store: Arc<EventStore>,
    max_events: usize,
}

impl StateReconstructor {
    pub fn new(store: Arc<EventStore>) -> Self { /* ... */ }

    pub async fn reconstruct<S: AggregateState>(
        &self,
        aggregate_id: &str,
        point: ReconstructionPoint,
    ) -> anyhow::Result<S> {
        let all_events = self.store.read_by_aggregate(aggregate_id, 0, self.max_events).await?;
        let events = apply_point_filter(all_events, &point);
        let mut state = S::default();
        for event in &events {
            state.apply_event(event);
        }
        Ok(state)
    }

    pub async fn at_sequence<S: AggregateState>(&self, aggregate_id: &str, sequence: i64) -> anyhow::Result<S>;
    pub async fn at_timestamp<S: AggregateState>(&self, aggregate_id: &str, timestamp_ms: i64) -> anyhow::Result<S>;
}
```

**Step 4: 内联测试（CallCounter toy aggregate）**

- `test_reconstruct_current`: 验证当前状态，跨 aggregate 不泄漏
- `test_reconstruct_at_sequence`: stop before seq=3，验证增量重建

**Step 5: 更新 event/mod.rs 导出**

```rust
pub mod reconstructor;
pub use reconstructor::{AggregateState, ReconstructionPoint, StateReconstructor};
```

**验证:**
```bash
cargo check -p octo-engine
cargo test -p octo-engine -- event::reconstructor --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(event): add StateReconstructor for point-in-time aggregate replay"
```

---

## P0-2: HNSW 向量索引 + 语义记忆搜索

### 现状

- `memory/vector_index.rs` — VectorIndex（暴力 O(n)）已有，缺 HnswIndex
- `memory/hybrid_query.rs` — HybridQueryEngine 已有，缺 EmbeddingClient 集成
- `memory/embedding.rs` — 不存在，需新建

---

### Task 4: 添加 hnsw_rs 依赖并实现 HnswIndex

**Files:**
- Modify: `crates/octo-engine/Cargo.toml`
- Modify: `crates/octo-engine/src/memory/vector_index.rs`

**Step 1: 在 Cargo.toml 中添加 hnsw_rs（feature-gated）**

```toml
[dependencies]
hnsw_rs = { workspace = true, optional = true }

[features]
hnsw = ["dep:hnsw_rs"]
```

同步在 workspace `Cargo.toml` 添加：
```toml
hnsw_rs = "0.3"
```

**Step 2: 添加 HnswConfig**

```rust
#[derive(Debug, Clone)]
pub struct HnswConfig {
    pub m: usize,               // 默认 16
    pub ef_construction: usize, // 默认 200
    pub dimensions: usize,      // 默认 1536 (OpenAI) 或 1024 (Voyage)
    pub max_elements: usize,    // 默认 100_000
    pub default_threshold: f32, // 默认 0.7
}
```

**Step 3: 实现 HnswIndex（仅在 feature = "hnsw" 时编译）**

```rust
#[cfg(feature = "hnsw")]
pub struct HnswIndex {
    config: HnswConfig,
    // Hnsw 是 !Send，必须通过 std::sync::Mutex + spawn_blocking 访问
    inner: StdArc<Mutex<Hnsw<f32, DistCosine>>>,
    id_map: Arc<RwLock<HashMap<usize, VectorEntry>>>,
    rev_map: Arc<RwLock<HashMap<String, usize>>>,
    next_id: Arc<AtomicUsize>,
}
```

关键注意事项：
- `upsert()` 和 `search()` 均通过 `tokio::task::spawn_blocking` 调用 HNSW
- similarity = 1.0 - DistCosine distance（cosine 距离转相似度）
- search 返回结果按 similarity 降序排列

**Step 4: 在 memory/mod.rs 条件导出**

```rust
#[cfg(feature = "hnsw")]
pub use vector_index::{HnswConfig, HnswIndex};
```

**验证:**
```bash
cargo check -p octo-engine
cargo check -p octo-engine --features hnsw
```

**Commit:**
```bash
git commit -m "feat(memory): add HnswIndex with hnsw_rs O(log n) vector search"
```

---

### Task 5: 实现 EmbeddingClient（调用 LLM Embedding API）

**Files:**
- Create: `crates/octo-engine/src/memory/embedding.rs`
- Modify: `crates/octo-engine/src/memory/mod.rs`

**Step 1: 定义 EmbeddingProvider 和 EmbeddingConfig**

```rust
pub enum EmbeddingProvider { Anthropic, OpenAI }

pub struct EmbeddingConfig {
    pub provider: EmbeddingProvider,
    pub api_key: String,
    pub model: String,      // "text-embedding-3-small" 或 "voyage-3-lite"
    pub dimensions: usize,  // 1536 (OpenAI) 或 1024 (Voyage)
    pub batch_size: usize,  // OpenAI=100, Anthropic=8
}
```

工厂方法：`EmbeddingConfig::openai(api_key)` / `EmbeddingConfig::anthropic(api_key)`

**Step 2: 实现 EmbeddingClient**

```rust
pub struct EmbeddingClient {
    config: EmbeddingConfig,
    http: reqwest::Client,
    // 简单 in-memory 缓存，最多 1000 条
    cache: Arc<RwLock<HashMap<String, Vec<f32>>>>,
    cache_max: usize,
}

impl EmbeddingClient {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
```

- OpenAI: `POST https://api.openai.com/v1/embeddings`
- Voyage: `POST https://api.voyageai.com/v1/embeddings`
- HTTP timeout: 30s，使用现有 `reqwest` crate（无需新增）

**Step 3: 更新 memory/mod.rs 导出**

```rust
pub mod embedding;
pub use embedding::{EmbeddingClient, EmbeddingConfig, EmbeddingProvider};
```

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(memory): implement EmbeddingClient for OpenAI and Anthropic Voyage APIs"
```

---

### Task 6: VectorBackend 枚举（统一暴力搜索和 HNSW）

**Files:**
- Modify: `crates/octo-engine/src/memory/vector_index.rs`
- Modify: `crates/octo-engine/src/memory/mod.rs`

**Step 1: 定义 VectorBackend**

```rust
pub enum VectorBackend {
    BruteForce(VectorIndex),
    #[cfg(feature = "hnsw")]
    Hnsw(HnswIndex),
}

impl VectorBackend {
    pub fn brute_force(config: VectorIndexConfig) -> Self;
    #[cfg(feature = "hnsw")]
    pub fn hnsw(config: HnswConfig) -> Self;

    pub async fn insert(&self, entry: VectorEntry) -> anyhow::Result<()>;
    pub async fn search(&self, query: &[f32], limit: usize, threshold: Option<f32>) -> Vec<VectorSearchResult>;
    pub async fn len(&self) -> usize;
    pub fn backend_name(&self) -> &'static str; // "brute-force" 或 "hnsw"
}
```

**Step 2: 更新 memory/mod.rs 导出**

```rust
pub use vector_index::{VectorBackend, VectorEntry, VectorIndex, VectorIndexConfig, VectorSearchResult};
```

**验证:**
```bash
cargo check -p octo-engine
cargo check -p octo-engine --features hnsw
cargo test -p octo-engine -- memory::vector_index --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(memory): add VectorBackend enum for brute-force/HNSW backend switching"
```

---

### Task 7: 将 EmbeddingClient + VectorBackend 接入 HybridQueryEngine

**Files:**
- Modify: `crates/octo-engine/src/memory/hybrid_query.rs`

**Step 1: 替换字段**

```rust
pub struct HybridQueryEngine {
    vector_backend: Option<Arc<VectorBackend>>,
    embedding_client: Option<Arc<EmbeddingClient>>,
}
```

**Step 2: 新增构造方法**

```rust
pub fn new_structured() -> Self;                                           // 无向量
pub fn with_vector_backend(backend: Arc<VectorBackend>) -> Self;           // 有向量，无 embedding
pub fn with_semantic_search(backend: Arc<VectorBackend>, client: Arc<EmbeddingClient>) -> Self; // 全功能
```

**Step 3: 更新 search() 自动生成 embedding**

```rust
// 在 QueryType::Semantic 路径：
// 1. 使用调用者提供的 embedding（如有）
// 2. 否则调用 embedding_client.embed(query) 自动生成
// 3. 否则返回空结果
```

**Step 4: 添加可观测性方法**

```rust
pub fn has_embedding_client(&self) -> bool;
pub fn backend_name(&self) -> &'static str;
```

**验证:**
```bash
cargo check -p octo-engine
cargo check -p octo-engine --features hnsw
cargo test -p octo-engine -- memory::hybrid_query --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(memory): wire EmbeddingClient and VectorBackend into HybridQueryEngine"
```

---

## P0-3: 扩展 Hook 体系至 10+ Hook 点

### 现状

- `hooks/mod.rs` — HookPoint（10 个，缺 ContextDegraded），HookAction（缺 Block/Redirect）
- `hooks/registry.rs` — execute() 缺 Block/Redirect 处理
- `agent/loop_.rs` — PreToolUse/PostToolUse 已接入，SessionStart/End 等 5 个未接入

---

### Task 1: 扩展 HookAction 和 HookContext

**Files:**
- Modify: `crates/octo-engine/src/hooks/handler.rs`
- Modify: `crates/octo-engine/src/hooks/context.rs`

**Step 1: 在 handler.rs 中增加 Block 和 Redirect**

```rust
pub enum HookAction {
    Continue,
    Modify(HookContext),
    Block(String),      // 软拒绝，已记录，调用方决定
    Abort(String),      // 硬拒绝，立即终止
    Redirect(String),   // 重定向到其他 agent 或 tool
}
```

**Step 2: 在 context.rs 中增加字段**

```rust
pub struct HookContext {
    // ... 现有字段 ...
    pub degradation_level: Option<String>,  // 新增：上下文降级级别
    pub redirect_target: Option<String>,    // 新增：重定向目标
}
```

添加构建器方法：
```rust
pub fn with_degradation(mut self, level: impl Into<String>) -> Self;
```

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(hooks): add Block/Redirect to HookAction, add degradation_level to HookContext"
```

---

### Task 2: 更新 HookPoint 枚举并修复 HookRegistry

**Files:**
- Modify: `crates/octo-engine/src/hooks/mod.rs`
- Modify: `crates/octo-engine/src/hooks/registry.rs`

**Step 1: 将 PreCompact 重命名为 ContextDegraded，保留向后兼容别名**

```rust
pub enum HookPoint {
    PreToolUse, PostToolUse,
    PreTask, PostTask,
    SessionStart, SessionEnd,
    ContextDegraded,   // 原 PreCompact
    LoopTurnStart, LoopTurnEnd,
    AgentRoute,
}

// 向后兼容别名
pub use HookPoint::ContextDegraded as PreCompact;
```

**Step 2: 在 registry.rs execute() 中处理 Block 和 Redirect**

```rust
Ok(HookAction::Block(reason)) => {
    warn!(hook_point = ?point, handler = handler.name(), reason = %reason, "Hook blocked (soft-deny)");
    return HookAction::Block(reason);
}
Ok(HookAction::Redirect(target)) => {
    debug!(hook_point = ?point, target = %target, "Hook redirected");
    return HookAction::Redirect(target);
}
```

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(hooks): rename PreCompact to ContextDegraded, handle Block/Redirect in registry"
```

---

### Task 3: 在 AgentLoop 接入 SessionStart/SessionEnd hook

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: 在 run() 方法中**

- for 循环之前：触发 `HookPoint::SessionStart`
- 每个 `return Ok(())` 路径前：触发 `HookPoint::SessionEnd`

使用显式调用模式（非 Drop，因为 Rust 不支持 async Drop）：

```rust
// Run 开始时
if let Some(ref hooks) = self.hook_registry {
    let ctx = HookContext::new().with_session(session_id.as_str()).with_task(task_text);
    hooks.execute(HookPoint::SessionStart, &ctx).await;
}

// 每个 return 前
if let Some(ref hooks) = self.hook_registry {
    let ctx = HookContext::new().with_session(session_id.as_str());
    hooks.execute(HookPoint::SessionEnd, &ctx).await;
}
```

注意：loop_.rs 中有 4 个 `return Ok(())` 和 2 个 `return Err(e)` 需要覆盖

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(hooks): integrate SessionStart/SessionEnd hooks into AgentLoop"
```

---

### Task 4: 在 AgentLoop 接入 PreTask/PostTask hook

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: PreTask 在 round == 0 时触发，可 Abort**

```rust
if round == 0 {
    if let Some(ref hooks) = self.hook_registry {
        let ctx = HookContext::new().with_session(...).with_task(task_text).with_turn(round);
        if let HookAction::Abort(reason) = hooks.execute(HookPoint::PreTask, &ctx).await {
            let _ = tx.send(AgentEvent::Error { message: reason.clone() });
            let _ = tx.send(AgentEvent::Done);
            return Err(anyhow::anyhow!("PreTask hook aborted: {}", reason));
        }
    }
}
```

**Step 2: PostTask 在发送 Done 前触发（stop_reason != ToolUse 路径）**

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(hooks): integrate PreTask/PostTask hooks into AgentLoop"
```

---

### Task 5: 在 AgentLoop 接入 LoopTurnStart/LoopTurnEnd hook

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: LoopTurnStart 在已有 EventBus LoopTurnStarted 事件后插入**

```rust
if let Some(ref hooks) = self.hook_registry {
    let ctx = HookContext::new().with_session(session_id.as_str()).with_turn(round);
    if let HookAction::Abort(reason) = hooks.execute(HookPoint::LoopTurnStart, &ctx).await {
        // ... 发送 Error + Done，return Err
    }
}
let turn_start = std::time::Instant::now();
```

**Step 2: LoopTurnEnd 在工具结果 push 完成后、Reset 之前**

```rust
if let Some(ref hooks) = self.hook_registry {
    let ctx = HookContext::new().with_session(...).with_turn(round)
        .with_result(true, turn_start.elapsed().as_millis() as u64);
    hooks.execute(HookPoint::LoopTurnEnd, &ctx).await;
}
```

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(hooks): integrate LoopTurnStart/LoopTurnEnd hooks into AgentLoop"
```

---

### Task 6: 在 AgentLoop 接入 ContextDegraded hook

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**Step 1: 在降级级别 != None 时触发**

找到 `if level != DegradationLevel::None {` 代码块，在 debug 日志之后插入：

```rust
if let Some(ref hooks) = self.hook_registry {
    let ctx = HookContext::new()
        .with_session(session_id.as_str())
        .with_turn(round)
        .with_degradation(format!("{:?}", level));
    if let HookAction::Abort(reason) = hooks.execute(HookPoint::ContextDegraded, &ctx).await {
        // ... 发送 Error + Done，return Err
    }
}
```

**验证:**
```bash
cargo check -p octo-engine
cargo test -p octo-engine -- --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(hooks): integrate ContextDegraded hook into AgentLoop context pruning path"
```

---

### Task 7: Hook 体系单元测试

**Files:**
- Modify: `crates/octo-engine/src/hooks/registry.rs`（追加 tests 模块）

测试覆盖：
- `test_block_action_stops_chain`
- `test_redirect_action`
- `test_modify_accumulates`
- `test_no_handlers_returns_continue`
- `test_context_degraded_hook_point_registered`

**验证:**
```bash
cargo test -p octo-engine hooks -- --test-threads=1
```

**Commit:**
```bash
git commit -m "test(hooks): add unit tests for Block/Redirect/ContextDegraded hook paths"
```

---

## P0-4: Agent 路由器（能力匹配）

### 现状

`capability.rs` 和 `router.rs` 已完整实现。需要的是**集成层**。

---

### Task 1: AgentCapability 补充 General 和 DataAnalysis 变体

**Files:**
- Modify: `crates/octo-engine/src/agent/capability.rs`

**Step 1: 在枚举中追加**

```rust
DataAnalysis,  // "data", "analysis", "analytics", "metrics", "statistics"
General,       // "help", "assist", "general", "anything"
Custom(String),
```

在 `keywords()` 和 `from_str_loose()` 中对应追加。

**验证:**
```bash
cargo test -p octo-engine capability -- --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(agent/capability): add General and DataAnalysis variants to AgentCapability"
```

---

### Task 2: AgentManifest::to_agent_profile() 便捷方法

**Files:**
- Modify: `crates/octo-engine/src/agent/entry.rs`

**Step 1: 在 AgentManifest impl 块中添加**

```rust
pub fn to_agent_profile(&self, agent_id: impl Into<String>) -> crate::agent::router::AgentProfile {
    use crate::agent::capability::AgentCapability;
    use crate::agent::router::AgentProfile;

    let capabilities: Vec<AgentCapability> = if self.tags.is_empty() {
        vec![AgentCapability::General]
    } else {
        // tags 中 "cap:xxx" 前缀 → AgentCapability::from_str_loose(xxx)
        // 若全部都是 Custom → 追加 General 作为后备
        todo!()
    };

    AgentProfile { agent_id: agent_id.into(), capabilities, priority: 100 }
}
```

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(agent/entry): add AgentManifest::to_agent_profile() convenience method"
```

---

### Task 3: 在 AgentRuntime 中持有 AgentRouter 并暴露路由 API

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: 在 AgentRuntime struct 中追加**

```rust
router: tokio::sync::RwLock<crate::agent::router::AgentRouter>,
```

在 `new()` 中初始化：

```rust
router: tokio::sync::RwLock::new(crate::agent::router::AgentRouter::new()),
```

**Step 2: 追加路由 API 方法**

```rust
pub async fn router_register(&self, profile: crate::agent::AgentProfile);
pub async fn router_unregister(&self, agent_id: &str);
pub async fn route_task(&self, task: &str) -> Option<crate::agent::router::RouteResult>;
pub async fn router_register_manifest(&self, agent_id: impl Into<String>, manifest: &crate::agent::AgentManifest);
```

**验证:**
```bash
cargo check -p octo-engine
```

**Commit:**
```bash
git commit -m "feat(agent/runtime): integrate AgentRouter into AgentRuntime with route_task() API"
```

---

### Task 4: 路由器集成测试

**Files:**
- Modify: `crates/octo-engine/src/agent/router.rs`（追加 tests）

新增测试：
- `test_general_capability_fallback`
- `test_data_analysis_routing`
- `test_priority_tiebreak`

**验证:**
```bash
cargo test -p octo-engine router -- --test-threads=1
```

**Commit:**
```bash
git commit -m "test(agent/router): add General, DataAnalysis, and priority tiebreak routing tests"
```

---

## P1-1: 声明式 Agent 定义（YAML）

### 现状

- `skills/mod.rs` — SkillLoader（YAML 加载模式可参考）
- `agent/entry.rs` — AgentManifest 已有，需扩展字段
- `serde_yaml = "0.9"` 已在 workspace

---

### Task 1: 扩展 AgentManifest 添加 YAML 字段

**Files:**
- Modify: `crates/octo-engine/src/agent/entry.rs`

**Step 1: 在 AgentManifest 末尾追加字段（向后兼容，serde default）**

```rust
#[serde(default)]
pub max_concurrent_tasks: u32,   // 0 = unlimited
#[serde(default)]
pub priority: Option<String>,    // "high" | "medium" | "low"
```

**验证:** `cargo check -p octo-engine`

**Commit:**
```bash
git commit -m "feat(agent): add max_concurrent_tasks and priority fields to AgentManifest"
```

---

### Task 2: 创建 AgentYamlDef（YAML 文件格式定义）

**Files:**
- Create: `crates/octo-engine/src/agent/yaml_def.rs`

YAML 格式：
```yaml
name: code-reviewer
type: reviewer
capabilities: [code_review, security_audit]
system_prompt_template: "prompts/code-reviewer.md"
max_concurrent_tasks: 3
priority: high
description: "专业代码审查智能体"
```

**关键转换逻辑（into_manifest）：**
- `type` → tag `"type:{t}"`
- 每个 capability → tag `"cap:{cap}"`
- `system_prompt_template` 相对于 YAML 文件目录解析
- `description` → `AgentManifest.backstory`

**内联测试：**
- `test_parse_minimal_yaml`
- `test_parse_full_yaml`
- `test_into_manifest_tags_composition`
- `test_missing_name_returns_error`
- `test_system_prompt_template_resolved`

**验证:** `cargo check -p octo-engine`

**Commit:**
```bash
git commit -m "feat(agent): add AgentYamlDef for declarative YAML agent definitions"
```

---

### Task 3: 创建 AgentManifestLoader（目录扫描 + 热加载）

**Files:**
- Create: `crates/octo-engine/src/agent/manifest_loader.rs`

```rust
pub struct AgentManifestLoader {
    agents_dir: PathBuf,
}

impl AgentManifestLoader {
    pub fn new(agents_dir: impl Into<PathBuf>) -> Self;
    pub fn load_all(&self, catalog: &AgentCatalog) -> Result<usize>;
    pub fn watch(&self, catalog: Arc<AgentCatalog>) -> Result<impl Drop>;
}
```

- `load_all()`: 扫描 `*.yaml` / `*.yml`，去重（相同 name 跳过），跳过无效文件并 warn
- `watch()`: 使用 `notify-debouncer-mini`（已在 workspace），文件变更时热重载

**内联测试（6 个）：**
- `test_load_all_empty_dir`、`test_load_all_missing_dir`
- `test_load_all_registers_agents`、`test_load_all_skips_invalid_yaml`
- `test_load_all_deduplicates_names`、`test_capability_tags_indexed`

**验证:**
```bash
cargo test -p octo-engine agent::manifest_loader -- --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(agent): add AgentManifestLoader with directory scan and hot-reload"
```

---

### Task 4: 注册新模块到 agent/mod.rs

**Files:**
- Modify: `crates/octo-engine/src/agent/mod.rs`

```rust
pub mod yaml_def;
pub mod manifest_loader;

pub use manifest_loader::AgentManifestLoader;
pub use yaml_def::AgentYamlDef;
```

**验证:** `cargo check -p octo-engine`

**Commit:**
```bash
git commit -m "feat(agent): export AgentManifestLoader and AgentYamlDef from agent module"
```

---

### Task 5: 在 AgentRuntime 启动时加载 YAML 定义

**Files:**
- Modify: `crates/octo-engine/src/agent/runtime.rs`

**Step 1: 在 AgentRuntimeConfig 中添加字段**

```rust
pub agents_dir: Option<PathBuf>,
```

**Step 2: 在 AgentRuntime::new() 后调用 loader**

```rust
if let Some(ref dir) = config.agents_dir {
    let loader = crate::agent::AgentManifestLoader::new(dir);
    match loader.load_all(&self.catalog) {
        Ok(n) => info!(count = n, "Loaded YAML agent manifests"),
        Err(e) => warn!(error = %e, "Failed to load agent YAML manifests"),
    }
}
```

**验证:**
```bash
cargo check -p octo-engine && cargo check -p octo-server
```

**Commit:**
```bash
git commit -m "feat(agent/runtime): wire AgentManifestLoader into AgentRuntime startup"
```

---

### Task 6: 创建示例 Agent YAML 文件

**Files:**
- Create: `config/agents/code-reviewer.yaml`
- Create: `config/agents/coder.yaml`

`config/agents/code-reviewer.yaml`:
```yaml
name: code-reviewer
type: reviewer
capabilities: [code_review, security_audit]
max_concurrent_tasks: 3
priority: high
description: "专业代码审查智能体，检查代码质量、安全性和规范合规性"
model: claude-3-5-sonnet-20241022
```

`config/agents/coder.yaml`:
```yaml
name: coder
type: coder
capabilities: [code_generation, refactoring, bug_fix]
max_concurrent_tasks: 2
priority: medium
description: "通用编码智能体，负责代码生成、重构和 Bug 修复"
tool_filter: [bash, file_read, file_write]
```

**Commit:**
```bash
git commit -m "feat(config): add example agent YAML definitions in config/agents/"
```

---

## P1-3: AIDefence 安全防护层

### 现状

- `security/mod.rs` — SecurityPolicy + ActionTracker 已有
- `regex = "1"` 已在 octo-engine 直接依赖中

---

### Task 1: 实现 InjectionDetector

**Files:**
- Create: `crates/octo-engine/src/security/ai_defence.rs`（首次创建）

关键设计：
- 24 个关键字（lowercase 匹配）：`"ignore previous instructions"`, `"jailbreak"`, `"dan mode"` 等
- 6 个正则模式：system role marker, assistant role marker, instruction block（`[INST]`）, 中文角色切换

```rust
pub struct InjectionDetector { keywords: Vec<String>, patterns: Vec<(String, Regex)> }

impl InjectionDetector {
    pub fn new() -> Self;
    pub fn check(&self, text: &str) -> Result<(), DefenceViolation>;
    pub fn has_injection(&self, text: &str) -> bool;
}
```

**Commit:**
```bash
git commit -m "feat(security): implement InjectionDetector with keyword + regex patterns"
```

---

### Task 2: 实现 PiiScanner

**Files:**
- Modify: `crates/octo-engine/src/security/ai_defence.rs`（追加）

6 个 PII 类别：`email`, `phone_cn`, `phone_us`, `ssn_us`, `credit_card`, `china_id`

```rust
pub struct PiiScanner { rules: Vec<(String, Regex)> }

impl PiiScanner {
    pub fn new() -> Self;
    pub fn scan(&self, text: &str) -> Option<PiiMatch>;
    pub fn has_pii(&self, text: &str) -> bool;
    pub fn redact(&self, text: &str) -> String;  // 替换为 [REDACTED]
}
```

**Commit:**
```bash
git commit -m "feat(security): implement PiiScanner with 6 PII categories and redact support"
```

---

### Task 3: 实现 OutputValidator 和 AiDefence 统一入口

**Files:**
- Modify: `crates/octo-engine/src/security/ai_defence.rs`（追加）

```rust
pub struct OutputValidator {
    max_length: usize,   // 默认 100_000 (100KB)
    pii: PiiScanner,
    bypass_indicators: Vec<String>,
}

pub struct AiDefence {
    injection: InjectionDetector,
    pii: PiiScanner,
    output: OutputValidator,
    pub injection_enabled: bool,
    pub pii_enabled: bool,
    pub output_validation_enabled: bool,
}

impl AiDefence {
    pub fn new() -> Self;
    pub fn disabled() -> Self;  // 全部检查关闭（用于测试）
    pub fn check_input(&self, text: &str) -> Result<(), DefenceViolation>;
    pub fn check_output(&self, text: &str) -> Result<(), DefenceViolation>;
    pub fn has_pii(&self, text: &str) -> bool;
    pub fn redact_pii(&self, text: &str) -> String;
    pub fn has_injection(&self, text: &str) -> bool;
}
```

**Commit:**
```bash
git commit -m "feat(security): implement AiDefence unified entry with InjectionDetector, PiiScanner, OutputValidator"
```

---

### Task 4: 添加完整单元测试

**Files:**
- Modify: `crates/octo-engine/src/security/ai_defence.rs`（追加 tests 模块）

28 个测试覆盖：
- InjectionDetector: 干净文本 / 关键字 / jailbreak / system role / instruction block / 中文切换
- PiiScanner: 干净文本 / email / 中国手机号 / SSN / 信用卡 / 身份证 / redact
- OutputValidator: 有效输出 / 超长 / bypass indicator / PII 输出
- AiDefence 集成: 干净 / 注入 / PII / disabled 模式 / display

**验证:**
```bash
cargo test -p octo-engine security::ai_defence -- --test-threads=1
```

**Commit:**
```bash
git commit -m "test(security): add 28 unit tests for AiDefence components"
```

---

### Task 5: 注册 ai_defence 模块到 security/mod.rs

**Files:**
- Modify: `crates/octo-engine/src/security/mod.rs`

```rust
pub mod ai_defence;

pub use ai_defence::{AiDefence, DefenceViolation, InjectionDetector, OutputValidator, PiiScanner};
```

**验证:** `cargo check -p octo-engine`

**Commit:**
```bash
git commit -m "feat(security): export AiDefence types from security module"
```

---

### Task 6: 集成 AiDefence 到 AgentLoop

**Files:**
- Modify: `crates/octo-engine/src/agent/loop_.rs`

**先读取 loop_.rs 确认 provider.complete() 调用位置，然后：**

1. 在 AgentLoop struct 中添加：
   ```rust
   defence: Arc<crate::security::AiDefence>,
   ```
   默认值：`AiDefence::default()`

2. 在 LLM 调用前（构建 request 后）插入输入检查：
   ```rust
   if let Err(v) = self.defence.check_input(&last_user_message) {
       warn!(violation = %v, "AIDefence blocked input");
       let _ = tx.send(AgentEvent::Error { message: format!("Security: {v}") });
       return Ok(());
   }
   ```

3. 在收到 TextComplete 后插入输出检查（warn-only，不阻断）：
   ```rust
   if let Err(v) = self.defence.check_output(text) {
       warn!(violation = %v, "AIDefence flagged output");
   }
   ```

**验证:**
```bash
cargo check -p octo-engine && cargo check -p octo-server
cargo test --workspace -- --test-threads=1
```

**Commit:**
```bash
git commit -m "feat(agent/loop): integrate AiDefence checks into AgentLoop hot path"
```

---

## 全量验证（所有功能完成后）

```bash
# 无可选 feature
cargo check --workspace

# 带 HNSW feature
cargo check --workspace --features octo-engine/hnsw

# 全量测试
cargo test --workspace -- --test-threads=1

# 针对新模块快速回归
cargo test -p octo-engine -- \
  event::store \
  event::projection \
  event::reconstructor \
  memory::vector_index \
  memory::hybrid_query \
  hooks \
  agent::router \
  agent::manifest_loader \
  security::ai_defence \
  --test-threads=1

# Clippy 检查
cargo clippy --workspace -- -D warnings
```

---

## 实施优先级与依赖关系

```
Phase 1（零依赖变更，并行可做）:
  P0-1 Event Sourcing     → Tasks 1-3
  P0-3 Hook 体系扩展      → Tasks 1-7
  P0-4 Agent 路由         → Tasks 1-4
  P1-1 声明式 Agent       → Tasks 1-6
  P1-3 AIDefence          → Tasks 1-6

Phase 2（单一新依赖 hnsw_rs）:
  P0-2 HNSW 向量索引      → Tasks 4-7
```

**关键路径（P0-2）：**
```
Task 4 (HnswIndex) → Task 6 (VectorBackend) → Task 7 (HybridQueryEngine wiring)
Task 5 (EmbeddingClient) ↗
```

---

## 完成检查单

- [ ] `cargo check --workspace` 通过（无 errors）
- [ ] `cargo check --workspace --features octo-engine/hnsw` 通过
- [ ] `cargo clippy --workspace -- -D warnings` 无新增 warning
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过（339+ 原有 + 新增测试）
- [ ] P0-1: StoredEvent 含 aggregate_id，ProjectionEngine 可注册和 catch_up，StateReconstructor 可按序列重建
- [ ] P0-2: HnswIndex（feature-gated），EmbeddingClient，VectorBackend 枚举，HybridQueryEngine 自动 embed
- [ ] P0-3: 10 个 HookPoint，5 个 HookAction，AgentLoop 7 个调用点接入
- [ ] P0-4: General/DataAnalysis capability，AgentRuntime.route_task() 可用
- [ ] P1-1: AgentYamlDef + AgentManifestLoader，config/agents/ 目录示例，AgentRuntime 热加载
- [ ] P1-3: 28 个 AIDefence 测试通过，AiDefence 集成到 AgentLoop
