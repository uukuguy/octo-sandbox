"""EventStreamBackend Protocol — 可插拔的事件持久化接口。

Phase 1: SqliteWalBackend
Phase 6+: NatsJetstreamBackend / KafkaBackend

ADR-V2-002 defines this abstraction layer.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Any, Protocol


class EventStreamBackend(Protocol):
    """可插拔的 Session Event Stream 后端接口。"""

    async def append(
        self,
        session_id: str,
        event_type: str,
        payload: dict[str, Any],
        *,
        event_id: str | None = None,
        source: str = "",
        metadata: dict[str, Any] | None = None,
    ) -> tuple[int, str]:
        """追加事件。返回 (seq, event_id)。"""
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

    def subscribe(
        self,
        session_id: str,
        from_seq: int = 0,
    ) -> AsyncIterator[dict[str, Any]]:
        """订阅事件流（用于 follow mode）。返回 async generator。"""
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

    async def update_cluster(self, event_id: str, cluster_id: str) -> None:
        """回写 cluster_id（Event Engine Clusterer 调用）。"""
        ...
