"""EventEngine — 管理 handler pipeline 的生命周期。

事件先持久化到 EventStreamBackend，然后异步投递到 pipeline 队列，
由后台 worker 顺序执行 handler chain（Ingest → Dedup → Cluster → Index）。

ADR-V2-003 定义了 pipeline 架构。D74 决策：Pipeline 异步不阻塞写入。
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any

from .event_backend import EventStreamBackend
from .event_handlers import (
    DefaultIngestor,
    EventHandler,
    FTS5Indexer,
    TimeWindowClusterer,
    TimeWindowDeduplicator,
)
from .event_models import Event

logger = logging.getLogger(__name__)


def _default_handlers() -> list[EventHandler]:
    return [
        DefaultIngestor(),
        TimeWindowDeduplicator(window_seconds=2.0),
        TimeWindowClusterer(window_seconds=30.0),
        FTS5Indexer(),
    ]


class EventEngine:
    """Event Engine — async pipeline for event processing."""

    def __init__(
        self,
        backend: EventStreamBackend,
        handlers: list[EventHandler] | None = None,
        queue_size: int = 1000,
    ) -> None:
        self.backend = backend
        self.handlers: list[Any] = handlers if handlers is not None else _default_handlers()
        self._queue: asyncio.Queue[Event] = asyncio.Queue(maxsize=queue_size)
        self._running = False
        self._worker_task: asyncio.Task[None] | None = None

    async def ingest(self, event: Event) -> tuple[int, str]:
        """接收事件：先持久化到 backend，再异步投递到 pipeline 队列。

        Returns (seq, event_id). Pipeline 处理在后台异步完成。
        """
        seq, eid = await self.backend.append(
            session_id=event.session_id,
            event_type=event.event_type,
            payload=event.payload,
            event_id=event.event_id,
            source=event.metadata.source,
            metadata=event.metadata.to_dict(),
        )
        event.seq = seq
        event.event_id = eid

        # Fire-and-forget: 队列满时丢弃
        try:
            self._queue.put_nowait(event)
        except asyncio.QueueFull:
            logger.warning(
                "Event pipeline queue full, dropping event %s", eid
            )

        return seq, eid

    async def start(self) -> None:
        """启动后台 worker。"""
        if self._running:
            return
        self._running = True
        self._worker_task = asyncio.create_task(self._worker())

    async def stop(self) -> None:
        """停止后台 worker，等待队列清空。"""
        self._running = False
        if self._worker_task is not None:
            # Give worker a chance to drain remaining items
            try:
                await asyncio.wait_for(self._worker_task, timeout=5.0)
            except asyncio.TimeoutError:
                self._worker_task.cancel()
                try:
                    await self._worker_task
                except asyncio.CancelledError:
                    pass
            self._worker_task = None

    async def _worker(self) -> None:
        """后台 worker：取事件 → 执行 handler chain → 回写 cluster_id。"""
        while self._running or not self._queue.empty():
            try:
                event: Event = await asyncio.wait_for(
                    self._queue.get(), timeout=1.0
                )
            except asyncio.TimeoutError:
                continue

            try:
                current: Event | None = event
                for handler in self.handlers:
                    if current is None:
                        break
                    current = await handler.handle(current)

                if current is not None and current.cluster_id:
                    try:
                        await self.backend.update_cluster(
                            current.event_id, current.cluster_id
                        )
                    except Exception as exc:
                        logger.warning(
                            "Failed to update cluster_id for event %s: %s",
                            current.event_id, exc,
                        )
            except Exception as exc:
                logger.error(
                    "Event pipeline handler error for event %s: %s",
                    event.event_id, exc, exc_info=True,
                )
