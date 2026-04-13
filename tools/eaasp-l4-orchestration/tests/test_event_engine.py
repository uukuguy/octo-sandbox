"""Tests for EventEngine — ingest, pipeline execution, cluster writeback."""

from __future__ import annotations

import asyncio
import time

from eaasp_l4_orchestration.event_backend_sqlite import SqliteWalBackend
from eaasp_l4_orchestration.event_engine import EventEngine
from eaasp_l4_orchestration.event_models import Event, EventMetadata


async def test_ingest_persists_event(tmp_db_path: str, seed_session) -> None:
    sid = await seed_session("sess_eng_1")
    backend = SqliteWalBackend(tmp_db_path)
    engine = EventEngine(backend)
    await engine.start()
    try:
        event = Event(
            session_id=sid,
            event_type="STOP",
            payload={"reason": "done"},
            metadata=EventMetadata(source="test"),
        )
        seq, eid = await engine.ingest(event)
        assert seq >= 1
        assert len(eid) == 36
        events = await backend.list_events(sid)
        assert len(events) == 1
        assert events[0]["event_type"] == "STOP"
    finally:
        await engine.stop()


async def test_pipeline_assigns_cluster_id(
    tmp_db_path: str, seed_session
) -> None:
    sid = await seed_session("sess_eng_2")
    backend = SqliteWalBackend(tmp_db_path)
    engine = EventEngine(backend)
    await engine.start()
    try:
        now = int(time.time())
        e1 = Event(
            session_id=sid, event_type="A", payload={}, created_at=now,
            metadata=EventMetadata(source="test"),
        )
        e2 = Event(
            session_id=sid, event_type="B", payload={}, created_at=now + 1,
            metadata=EventMetadata(source="test"),
        )
        await engine.ingest(e1)
        await engine.ingest(e2)
        # Wait for pipeline worker to process
        await asyncio.sleep(1.5)
        events = await backend.list_events(sid)
        cluster_ids = [e.get("cluster_id") for e in events if e.get("cluster_id")]
        assert len(cluster_ids) >= 1
    finally:
        await engine.stop()


async def test_pipeline_deduplicates_preserves_both_in_backend(
    tmp_db_path: str, seed_session
) -> None:
    """Dedup happens in the pipeline but does NOT delete from backend.
    Both events are persisted; dedup only prevents cluster assignment for dup."""
    sid = await seed_session("sess_eng_3")
    backend = SqliteWalBackend(tmp_db_path)
    engine = EventEngine(backend)
    await engine.start()
    try:
        now = int(time.time())
        e1 = Event(
            session_id=sid, event_type="PRE_TOOL_USE",
            payload={"tool_name": "scada"}, created_at=now,
            metadata=EventMetadata(source="test"),
        )
        e2 = Event(
            session_id=sid, event_type="PRE_TOOL_USE",
            payload={"tool_name": "scada"}, created_at=now,
            metadata=EventMetadata(source="test"),
        )
        await engine.ingest(e1)
        await engine.ingest(e2)
        await asyncio.sleep(1.5)
        events = await backend.list_events(sid)
        assert len(events) == 2  # both persisted
        # Only first should have cluster_id (second was deduped)
        clusters = [e.get("cluster_id") for e in events if e.get("cluster_id")]
        assert len(clusters) == 1
    finally:
        await engine.stop()


async def test_engine_start_stop(tmp_db_path: str, seed_session) -> None:
    backend = SqliteWalBackend(tmp_db_path)
    engine = EventEngine(backend)
    await engine.start()
    assert engine._running
    await engine.stop()
    assert not engine._running


async def test_engine_double_start_is_safe(
    tmp_db_path: str, seed_session
) -> None:
    backend = SqliteWalBackend(tmp_db_path)
    engine = EventEngine(backend)
    await engine.start()
    await engine.start()  # should not create second worker
    assert engine._running
    await engine.stop()
