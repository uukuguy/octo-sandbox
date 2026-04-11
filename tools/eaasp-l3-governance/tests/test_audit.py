"""Contract 4 — Telemetry ingest store tests."""

from __future__ import annotations

import asyncio

import pytest

from eaasp_l3_governance.audit import AuditStore, TelemetryEventIn


pytestmark = pytest.mark.asyncio


async def test_ingest_and_get(audit_store: AuditStore) -> None:
    out = await audit_store.ingest(
        TelemetryEventIn(
            session_id="sess_1",
            agent_id="agent_threshold",
            hook_id="h_audit_post",
            phase="PostToolUse",
            payload={"tool": "bash", "exit_code": 0},
        )
    )
    assert out.event_id.startswith("tel_")
    assert out.payload == {"tool": "bash", "exit_code": 0}

    fetched = await audit_store.get(out.event_id)
    assert fetched is not None
    assert fetched.session_id == "sess_1"
    assert fetched.phase == "PostToolUse"


async def test_query_filters_by_session(audit_store: AuditStore) -> None:
    for i in range(3):
        await audit_store.ingest(
            TelemetryEventIn(
                session_id="sess_target",
                hook_id=f"h_{i}",
                payload={"i": i},
            )
        )
    await audit_store.ingest(
        TelemetryEventIn(session_id="sess_other", payload={"i": 99})
    )

    rows = await audit_store.query(session_id="sess_target")
    assert len(rows) == 3
    assert all(r.session_id == "sess_target" for r in rows)


async def test_query_limit_clamped(audit_store: AuditStore) -> None:
    for i in range(10):
        await audit_store.ingest(
            TelemetryEventIn(session_id="sess_bulk", payload={"i": i})
        )

    # Oversized limit is clamped down to the maximum (500).
    rows = await audit_store.query(session_id="sess_bulk", limit=99999)
    assert len(rows) == 10

    # Negative / zero → default (100).
    rows_default = await audit_store.query(session_id="sess_bulk", limit=0)
    assert len(rows_default) == 10

    # Explicit small cap is respected.
    rows_cap = await audit_store.query(session_id="sess_bulk", limit=3)
    assert len(rows_cap) == 3


async def test_query_newest_first(audit_store: AuditStore) -> None:
    first = await audit_store.ingest(
        TelemetryEventIn(session_id="sess_ord", payload={"which": "first"})
    )
    # Sleep briefly so SQLite datetime('now') ticks a second forward.
    await asyncio.sleep(1.1)
    second = await audit_store.ingest(
        TelemetryEventIn(session_id="sess_ord", payload={"which": "second"})
    )
    rows = await audit_store.query(session_id="sess_ord")
    assert [r.event_id for r in rows] == [second.event_id, first.event_id]


async def test_query_since_filter(audit_store: AuditStore) -> None:
    a = await audit_store.ingest(TelemetryEventIn(session_id="sx", payload={}))
    await asyncio.sleep(1.1)
    b = await audit_store.ingest(TelemetryEventIn(session_id="sx", payload={}))

    rows_since_a = await audit_store.query(session_id="sx", since=a.received_at)
    # `a` itself has received_at == cursor, so it must NOT appear (strict >).
    event_ids = {r.event_id for r in rows_since_a}
    assert b.event_id in event_ids
    assert a.event_id not in event_ids


async def test_get_missing_returns_none(audit_store: AuditStore) -> None:
    assert await audit_store.get("tel_missing") is None
