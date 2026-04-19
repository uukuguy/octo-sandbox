---
id: ADR-V2-003
title: "Event Clustering 插件化接口：4 Handler Pipeline"
type: contract
status: Accepted
date: 2026-04-13
phase: "Phase 1 — Event-driven Foundation"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace: []
  review_checklist: null
affected_modules:
  - "tools/eaasp-l4-orchestration/"
related: [ADR-V2-001, ADR-V2-002]
---

# ADR-V2-003 — Event Clustering 插件化接口：4 Handler Pipeline

**Status:** Accepted
**Date:** 2026-04-13
**Phase:** Phase 1 — Event-driven Foundation
**Related:** ADR-V2-001 (EmitEvent interface), ADR-V2-002 (Event Stream backend)
**Blocks:** L4 Event Engine pipeline

---

## 背景

v2.0 设计规范要求 L4 Event Engine 具备"事件接收 → 去重 → 聚类 → 索引"的处理管线，支持 4 种 handler type 的插件化扩展。Phase 0 到 Phase 0.75 刻意跳过 Event Engine 实现，理由是：

1. Clusterer 需要电网 topology ontology 作为输入（"Transformer-001 和 Transformer-002 属于同一变电站"）
2. Event clustering strategy 尚未定义插件化接口
3. 避免在基础设施未就绪时过早设计

Phase 1 需要定义接口并提供默认实现，使 Event Engine 管线可运行，同时保留 Phase 5 引入 topology-aware 聚类的扩展能力。

---

## 决策

**定义 4 Handler Protocol 接口 + Phase 1 提供时间窗口默认实现。**

### Handler 接口定义

```python
from __future__ import annotations
from typing import Protocol, runtime_checkable

@runtime_checkable
class EventHandler(Protocol):
    """Event Engine pipeline 中的一个处理阶段。
    
    每个 handler 接收一个事件，返回处理后的事件或 None（丢弃）。
    handler 可以修改事件的字段（如添加 cluster_id），但不应改变 event_id。
    """
    
    @property
    def name(self) -> str:
        """Handler 名称，用于日志和指标。"""
        ...

    async def handle(self, event: Event) -> Event | None:
        """处理单个事件。返回 None 表示丢弃（如去重）。"""
        ...

    async def handle_batch(self, events: list[Event]) -> list[Event]:
        """批量处理事件（默认逐个调用 handle）。
        
        Clusterer 等需要批量上下文的 handler 应覆盖此方法。
        """
        results = []
        for event in events:
            result = await self.handle(event)
            if result is not None:
                results.append(result)
        return results
```

### 4 种 Handler 角色

| Handler | 职责 | 输入 | 输出 | Phase 1 实现 |
|---------|------|------|------|-------------|
| **Ingestor** | 标准化：分配 event_id、归一化 timestamp、标注 source | 原始事件（来自拦截器或 EmitEvent） | 标准化事件 | ✅ `DefaultIngestor` |
| **Deduplicator** | 去重：相同事件在时间窗口内合并 | 标准化事件 | 去重后事件（或 None） | ✅ `TimeWindowDeduplicator` |
| **Clusterer** | 聚类：按时间/因果/拓扑关系分组 | 去重后事件 | 带 cluster_id 的事件 | ⚠️ `TimeWindowClusterer`（简单） |
| **Indexer** | 索引：写入搜索索引供查询 | 聚类后事件 | 索引后事件（不改内容） | ✅ `FTS5Indexer` |

### Pipeline 执行模型

```
                    ┌─────────────────────────────────┐
                    │        Event Engine              │
                    │                                  │
  EmitEvent ──────→ │  [Queue] → Ingestor              │
  拦截器提取 ──────→ │            → Deduplicator         │
                    │            → Clusterer            │
                    │            → Indexer               │
                    │            → EventStreamBackend    │
                    └─────────────────────────────────┘
```

**关键设计决策**：

1. **Pipeline 异步执行** — 事件先写入 `EventStreamBackend`（保证持久化），再异步投递到 Pipeline 队列
2. **Pipeline 不阻塞写入** — `append()` 立即返回，handler 后台消费
3. **Pipeline 错误不丢事件** — handler 抛异常时事件保留在队列，可重试
4. **handler 可组合** — `EventEngine` 接受 `list[EventHandler]` 配置，顺序执行

### Phase 1 默认实现

#### DefaultIngestor

```python
class DefaultIngestor:
    """标准化原始事件。"""
    name = "default-ingestor"
    
    async def handle(self, event: Event) -> Event:
        # 分配 event_id（如果没有）
        if not event.event_id:
            event.event_id = str(uuid.uuid4())
        # 归一化 timestamp（确保 Unix epoch）
        if isinstance(event.created_at, str):
            event.created_at = int(parse_iso(event.created_at).timestamp())
        # 标注 source（如果没有）
        if not event.source:
            event.source = "unknown"
        return event
```

#### TimeWindowDeduplicator

```python
class TimeWindowDeduplicator:
    """时间窗口去重。
    
    相同 (session_id, event_type, tool_name) 在 window_seconds 内只保留第一条。
    """
    name = "time-window-deduplicator"
    
    def __init__(self, window_seconds: float = 2.0):
        self.window_seconds = window_seconds
        self._seen: dict[str, float] = {}  # dedup_key → last_seen_timestamp
    
    async def handle(self, event: Event) -> Event | None:
        key = f"{event.session_id}:{event.event_type}:{event.payload.get('tool_name', '')}"
        now = event.created_at
        last = self._seen.get(key)
        if last is not None and (now - last) < self.window_seconds:
            return None  # 重复，丢弃
        self._seen[key] = now
        return event
```

#### TimeWindowClusterer

```python
class TimeWindowClusterer:
    """时间窗口聚类（Phase 1 最简实现）。
    
    同一 session 内 window_seconds 秒内的连续事件归入同一 cluster。
    """
    name = "time-window-clusterer"
    
    def __init__(self, window_seconds: float = 30.0):
        self.window_seconds = window_seconds
        self._clusters: dict[str, tuple[str, float]] = {}  # session_id → (cluster_id, last_event_time)
    
    async def handle(self, event: Event) -> Event:
        session_id = event.session_id
        now = event.created_at
        
        current = self._clusters.get(session_id)
        if current is None or (now - current[1]) > self.window_seconds:
            # 新 cluster
            cluster_id = f"c-{uuid.uuid4().hex[:8]}"
            self._clusters[session_id] = (cluster_id, now)
        else:
            # 延续当前 cluster
            cluster_id = current[0]
            self._clusters[session_id] = (cluster_id, now)
        
        event.cluster_id = cluster_id
        return event
```

#### FTS5Indexer

```python
class FTS5Indexer:
    """FTS5 全文索引（委托给 SqliteWalBackend 的 trigger 机制）。
    
    Phase 1 实现：Indexer 本身不做额外工作，FTS5 同步由 SQLite trigger 自动完成。
    此 handler 主要用于未来扩展（如向量索引、因果图索引）。
    """
    name = "fts5-indexer"
    
    async def handle(self, event: Event) -> Event:
        # Phase 1: FTS5 indexing is handled by SQLite trigger (ADR-V2-002)
        # Future: add vector embedding, causal graph indexing here
        return event
```

### EventEngine 编排

```python
class EventEngine:
    """Event Engine — 管理 handler pipeline 的生命周期。"""
    
    def __init__(
        self,
        backend: EventStreamBackend,
        handlers: list[EventHandler] | None = None,
        queue_size: int = 1000,
    ):
        self.backend = backend
        self.handlers = handlers or [
            DefaultIngestor(),
            TimeWindowDeduplicator(),
            TimeWindowClusterer(),
            FTS5Indexer(),
        ]
        self._queue: asyncio.Queue[Event] = asyncio.Queue(maxsize=queue_size)
        self._running = False
        self._worker_task: asyncio.Task | None = None
    
    async def ingest(self, event: Event) -> str:
        """接收事件：先持久化到 backend，再投递到 pipeline 队列。"""
        event_id = await self.backend.append(
            session_id=event.session_id,
            event_type=event.event_type,
            payload=event.payload,
            metadata=event.metadata,
        )
        await self._queue.put(event)
        return event_id
    
    async def _worker(self):
        """后台 worker：从队列取事件，顺序执行 handler chain。"""
        while self._running:
            try:
                event = await asyncio.wait_for(self._queue.get(), timeout=1.0)
            except asyncio.TimeoutError:
                continue
            
            for handler in self.handlers:
                event = await handler.handle(event)
                if event is None:
                    break  # 被 deduplicator 丢弃
            
            if event is not None and event.cluster_id:
                # 回写 cluster_id 到 backend
                await self.backend.update_cluster(event.event_id, event.cluster_id)
    
    async def start(self):
        self._running = True
        self._worker_task = asyncio.create_task(self._worker())
    
    async def stop(self):
        self._running = False
        if self._worker_task:
            await self._worker_task
```

---

## 后果

### 正面

1. **接口完整** — 4 种 handler type 全部定义，符合 v2.0 规范要求
2. **实现最小** — Phase 1 只提供时间窗口默认实现，无 topology 依赖
3. **可扩展** — 替换 `TimeWindowClusterer` 为 `TopologyAwareClusterer` 只需一行配置
4. **异步不阻塞** — Pipeline 后台执行，不影响 EmitEvent / append 延迟

### 负面

1. **Clusterer 能力有限** — 时间窗口聚类不理解业务语义（Phase 5 补全）
2. **Deduplicator 内存状态** — `_seen` dict 随时间增长，需要定期清理（已通过 window 过期自动失效）
3. **单 worker** — Pipeline 单线程处理，高吞吐场景需要增加 worker 数（Phase 6）

### 新增 Deferred

| ID | 描述 | 目标 Phase |
|---|------|-----------|
| **D77** | TopologyAwareClusterer（需要 L2 Ontology Service 输入） | Phase 5 |
| **D78** | 向量索引 Indexer（event payload embedding） | Phase 2 |
| **D79** | Pipeline 多 worker 并行处理 | Phase 6 |
| **D80** | Clusterer 因果图聚类（parent_event_id → DAG） | Phase 4 |

---

## Phase 演进路径

```
Phase 1:  TimeWindowDedup + TimeWindowCluster + FTS5Index
                ↓
Phase 2:  + VectorIndexer (semantic search)
                ↓
Phase 4:  + CausalGraphClusterer (parent_event_id DAG)
                ↓
Phase 5:  + TopologyAwareClusterer (ontology-driven)
                ↓
Phase 6:  + Multi-worker pipeline + distributed backend
```
