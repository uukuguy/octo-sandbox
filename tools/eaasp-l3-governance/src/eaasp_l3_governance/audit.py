"""Telemetry audit store — Contract 4 (async PostToolUse ingest).

Append-only table of PostToolUse events sent by L1 runtimes over HTTP. MVP
writes land in ``telemetry_events`` with a minimal metadata envelope; the
full payload is preserved verbatim as JSON so future phases can backfill
richer schemas without a migration.
"""

from __future__ import annotations

import json
import uuid
from typing import Any

from pydantic import BaseModel, Field

from .db import connect


class TelemetryEventIn(BaseModel):
    """Incoming event envelope.

    ``payload`` is free-form — the whole hook POST body is stashed here so
    evidence-chain consumers can reconstruct the original signal.
    """

    session_id: str = Field(..., min_length=1)
    agent_id: str | None = None
    hook_id: str | None = None
    phase: str | None = None  # "PreToolUse" | "PostToolUse" | "Stop" | ...
    payload: dict[str, Any] = Field(default_factory=dict)


class TelemetryEventOut(BaseModel):
    event_id: str
    session_id: str
    agent_id: str | None
    hook_id: str | None
    phase: str | None
    payload: dict[str, Any]
    received_at: str


class AuditStore:
    def __init__(self, db_path: str) -> None:
        self.db_path = db_path

    async def ingest(self, event: TelemetryEventIn) -> TelemetryEventOut:
        """Insert a telemetry event.

        Wrapped in ``BEGIN IMMEDIATE`` to serialize concurrent writers (C1),
        even though the primary key is client-unique — WAL + SQLite still
        benefits from explicit ordering on high-churn hosts.
        """
        event_id = f"tel_{uuid.uuid4().hex[:16]}"
        payload_json = json.dumps(event.payload)

        db = await connect(self.db_path)
        try:
            await db.execute("BEGIN IMMEDIATE")
            try:
                await db.execute(
                    """
                    INSERT INTO telemetry_events
                        (event_id, session_id, agent_id, hook_id, phase, payload_json)
                    VALUES (?, ?, ?, ?, ?, ?)
                    """,
                    (
                        event_id,
                        event.session_id,
                        event.agent_id,
                        event.hook_id,
                        event.phase,
                        payload_json,
                    ),
                )
                cur = await db.execute(
                    "SELECT received_at FROM telemetry_events WHERE event_id = ?",
                    (event_id,),
                )
                row = await cur.fetchone()
                await db.commit()
            except Exception:
                await db.rollback()
                raise
        finally:
            await db.close()

        assert row is not None
        return TelemetryEventOut(
            event_id=event_id,
            session_id=event.session_id,
            agent_id=event.agent_id,
            hook_id=event.hook_id,
            phase=event.phase,
            payload=event.payload,
            received_at=row["received_at"],
        )

    async def query(
        self,
        session_id: str | None = None,
        since: str | None = None,
        limit: int = 100,
    ) -> list[TelemetryEventOut]:
        """Return newest-first events matching filters. Limit is clamped (C3).

        ``since`` is an ISO-8601 timestamp (SQLite ``datetime('now')`` format
        compatible, e.g. ``2026-04-12 12:34:56``); events with
        ``received_at > since`` are returned.
        """
        safe_limit = _clamp_limit(limit, default=100, maximum=500)

        where: list[str] = []
        params: list[Any] = []
        if session_id is not None:
            where.append("session_id = ?")
            params.append(session_id)
        if since is not None:
            where.append("received_at > ?")
            params.append(since)
        where_clause = ("WHERE " + " AND ".join(where)) if where else ""

        sql = f"""
            SELECT event_id, session_id, agent_id, hook_id, phase,
                   payload_json, received_at
            FROM telemetry_events
            {where_clause}
            ORDER BY received_at DESC, event_id DESC
            LIMIT ?
        """
        params.append(safe_limit)

        db = await connect(self.db_path)
        try:
            cur = await db.execute(sql, params)
            rows = await cur.fetchall()
        finally:
            await db.close()

        return [_row_to_event(r) for r in rows]

    async def get(self, event_id: str) -> TelemetryEventOut | None:
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                """
                SELECT event_id, session_id, agent_id, hook_id, phase,
                       payload_json, received_at
                FROM telemetry_events WHERE event_id = ?
                """,
                (event_id,),
            )
            row = await cur.fetchone()
        finally:
            await db.close()
        return _row_to_event(row) if row else None


def _row_to_event(row: Any) -> TelemetryEventOut:
    payload_raw = row["payload_json"]
    payload = json.loads(payload_raw) if payload_raw else {}
    return TelemetryEventOut(
        event_id=row["event_id"],
        session_id=row["session_id"],
        agent_id=row["agent_id"],
        hook_id=row["hook_id"],
        phase=row["phase"],
        payload=payload,
        received_at=row["received_at"],
    )


def _clamp_limit(value: int | None, *, default: int, maximum: int) -> int:
    if value is None or value <= 0:
        return default
    return min(int(value), maximum)
