# ADR：EVENT SOURCING 架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-07
**状态**：已完成

---

## 目录

- [ADR-026：EventBus 事件总线](#adr-026eventbus-事件总线)
- [ADR-027：EventStore 事件持久化](#adr-027eventstore-事件持久化)
- [ADR-028：ProjectionEngine 投影引擎](#adr-028projectionengine-投影引擎)
- [ADR-029：StateReconstructor 状态重放](#adr-029statereconstructor-状态重放)

---

## ADR-026：EventBus 事件总线

### 状态

**已完成** — 2026-03-07

### 上下文

系统组件间需要松耦合的通信机制。

### 决策

实现基于 broadcast channel 的 EventBus：

```rust
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

pub struct Event {
    pub topic: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub source: String,
}
```

支持：
- 多订阅者
- 事件过滤
- 死信处理

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/event/bus.rs` | EventBus 实现 |

---

## ADR-027：EventStore 事件持久化

### 状态

**已完成** — 2026-03-07

### 上下文

事件需要持久化以支持审计和重放。

### 决策

实现 SQLite EventStore：

```rust
pub struct EventStore {
    conn: SqliteConnection,
}

impl EventStore {
    pub async fn append(&self, event: &Event) -> Result<EventId>;
    pub async fn get_events(&self, aggregate_id: &str) -> Result<Vec<Event>>;
}
```

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/event/store.rs` | EventStore |

---

## ADR-028：ProjectionEngine 投影引擎

### 状态

**已完成** — 2026-03-07

### 上下文

从事件流构建读模型，支持不同视图。

### 决策

实现 ProjectionEngine：

```rust
pub struct ProjectionEngine {
    projections: HashMap<String, Box<dyn Projection>>,
    checkpoint: Checkpoint,
}
```

**Checkpoint 线程安全**：
- 使用 Arc<RwLock<Checkpoint>>
- 定期持久化到 SQLite

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/event/projection.rs` | ProjectionEngine |

---

## ADR-029：StateReconstructor 状态重放

### 状态

**已完成** — 2026-03-07

### 上下文

需要从事件历史重建状态。

### 决策

实现 StateReconstructor：

```rust
pub struct StateReconstructor {
    event_store: EventStore,
    aggregates: HashMap<String, Aggregate>,
}
```

**事件限制**：
- 默认限制 1000 条事件
- 可配置 `max_events` 参数

### 涉及文件

| 文件 | 职责 |
|------|------|
| `src/event/reconstructor.rs` | StateReconstructor |
