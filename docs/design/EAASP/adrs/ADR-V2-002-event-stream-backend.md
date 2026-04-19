---
id: ADR-V2-002
title: "Session Event Stream 后端选型：SQLite WAL + 接口抽象"
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
  - "tools/eaasp-l2-memory-engine/"
related: [ADR-V2-001, ADR-V2-003]
---

# ADR-V2-002 — Session Event Stream 后端选型：SQLite WAL + 接口抽象

**Status:** Accepted
**Date:** 2026-04-13
**Phase:** Phase 1 — Event-driven Foundation
**Related:** ADR-V2-001 (EmitEvent interface), ADR-V2-003 (Event clustering)
**Blocks:** L4 持久化平面

---

## 背景

Phase 0.5 实现了 `SessionEventStream`（`event_stream.py`），使用 SQLite `BEGIN IMMEDIATE` 串行写入 + append-only 表。Phase 1 需要决定是否替换为分布式消息系统，以支持 Event Engine 的 ingest→dedup→cluster→index 管线。

### 候选方案

| 方案 | 写延迟 | 外部依赖 | 多节点 | 吞吐 | 运维成本 |
|------|--------|---------|--------|------|---------|
| **A. SQLite WAL 增强** | <1ms | 零 | 单节点 | 中 | 零 |
| **B. NATS JetStream** | ~5ms | NATS server | ✅ | 高 | 中 |
| **C. Apache Kafka** | ~10ms | Kafka cluster + ZK/KRaft | ✅ | 极高 | 高 |
| **D. S3 append-only** | ~100ms | S3/MinIO | ✅ | 低 | 低 |

### 决策要素

1. **Phase 1 是单节点 dev 环境** — `dev-eaasp.sh` 已启动 8+ 进程，再加消息系统是负担
2. **Phase 6 才需要多租户/多节点** — 距 Phase 1 还有 5 个 Phase
3. **接口抽象成本低** — Python Protocol 定义 + 一个实现 = 可随时切换
4. **当前 SQLite 已工作** — Phase 0.5 的 47/47 L2 测试 + Phase 0.75 验收均通过

---

## 决策

**Phase 1 采用 SQLite WAL 增强（方案 A），通过 `EventStreamBackend` Protocol 抽象接口保留切换能力。**

### 接口定义

```python
from typing import Protocol, AsyncIterator, Any

class EventStreamBackend(Protocol):
    """可插拔的 Session Event Stream 后端接口。
    
    Phase 1: SqliteWalBackend
    Phase 6+: NatsJetstreamBackend / KafkaBackend
    """
    
    async def append(
        self,
        session_id: str,
        event_type: str,
        payload: dict[str, Any],
        metadata: dict[str, Any] | None = None,
    ) -> str:
        """追加事件，返回 event_id (UUID)。"""
        ...

    async def list_events(
        self,
        session_id: str,
        from_seq: int = 1,
        to_seq: int | None = None,
        limit: int = 500,
        event_types: list[str] | None = None,
    ) -> list[dict[str, Any]]:
        """查询事件列表（升序）。"""
        ...

    async def subscribe(
        self,
        session_id: str,
        from_seq: int = 0,
    ) -> AsyncIterator[dict[str, Any]]:
        """订阅事件流（用于 follow mode）。"""
        ...

    async def count(self, session_id: str) -> int:
        """返回 session 的事件总数。"""
        ...

    async def search(
        self,
        session_id: str,
        query: str,
        limit: int = 50,
    ) -> list[dict[str, Any]]:
        """全文搜索事件（FTS5）。"""
        ...
```

### SQLite WAL 增强实现

在 Phase 0.5 的 `SessionEventStream` 基础上增强：

| 增强项 | 说明 |
|--------|------|
| **WAL mode** | `PRAGMA journal_mode=WAL` — 并发读不阻塞写 |
| **FTS5 索引** | 对 `event_type + payload_json` 建立全文索引，支持 `search()` |
| **event_id 列** | 新增 UUID 列，替代纯 seq 自增（分布式友好） |
| **metadata_json 列** | 新增：trace_id, span_id, parent_event_id, source |
| **cluster_id 列** | 新增：Event Engine Clusterer 分配的聚类 ID |
| **subscribe() 实现** | 基于 polling + asyncio.Event 通知（0.5 秒间隔） |
| **event_types 过滤** | list_events 支持按 event_type 列表过滤 |

### DB Schema 变更

```sql
-- 扩展 session_events 表
ALTER TABLE session_events ADD COLUMN event_id TEXT;      -- UUID
ALTER TABLE session_events ADD COLUMN source TEXT;         -- "runtime:grid-runtime" | "interceptor" | "orchestrator"
ALTER TABLE session_events ADD COLUMN metadata_json TEXT;  -- {"trace_id":..., "parent_event_id":...}
ALTER TABLE session_events ADD COLUMN cluster_id TEXT;     -- Event Engine 分配

-- FTS5 虚拟表
CREATE VIRTUAL TABLE IF NOT EXISTS session_events_fts USING fts5(
    event_type,
    payload_json,
    content='session_events',
    content_rowid='seq'
);

-- 自动同步 trigger
CREATE TRIGGER session_events_ai AFTER INSERT ON session_events BEGIN
    INSERT INTO session_events_fts(rowid, event_type, payload_json)
    VALUES (new.seq, new.event_type, new.payload_json);
END;
```

### subscribe() 实现策略

```python
async def subscribe(self, session_id: str, from_seq: int = 0) -> AsyncIterator[dict]:
    """Long-poll based subscription.
    
    生产环境替换为 NATS JetStream push-based subscription 时，
    只需实现 NatsBackend.subscribe() 即可，调用方不变。
    """
    last_seq = from_seq
    while True:
        events = await self.list_events(session_id, from_seq=last_seq + 1, limit=100)
        for event in events:
            yield event
            last_seq = event["seq"]
        if not events:
            await asyncio.sleep(0.5)
```

---

## 后果

### 正面

1. **零外部依赖** — dev-eaasp.sh 不增加新进程
2. **渐进式改进** — Phase 0.5 → Phase 1 的 diff 最小
3. **接口抽象** — `EventStreamBackend` Protocol 允许后续切换到 NATS/Kafka
4. **FTS5** — 事件搜索能力（CLI `eaasp-cli session events --search "scada"` 可用）
5. **WAL** — 读写并发改善（follow mode + append 同时进行不阻塞）

### 负面

1. **单节点限制** — 多 L4 实例无法共享同一 SQLite 文件
2. **polling 延迟** — subscribe() 有 0.5 秒延迟（vs NATS push 的 ~5ms）
3. **无消息保证** — SQLite 没有 at-least-once 语义（进程 crash 可能丢 in-flight 事件）

### 新增 Deferred

| ID | 描述 | 目标 Phase |
|---|------|-----------|
| **D75** | EventStreamBackend 切换到 NATS JetStream（多节点支持） | Phase 6 |
| **D76** | subscribe() 从 polling 切换到 push-based（需要 NATS/WebSocket） | Phase 6 |

---

## 向后兼容

现有 `SessionEventStream.append()` 和 `list_events()` 签名保持不变。新 `SqliteWalBackend` 在内部包装旧实现，外部调用方（`session_orchestrator.py`）无需改动。新增的 `subscribe()` / `search()` / `count()` 是纯增量。
