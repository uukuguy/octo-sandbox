"""Session event stream — append-only per-session ordered log.

Used by ``SessionOrchestrator`` for standalone ``append`` calls (outside the
create_session transaction) and by the API layer for ``list_events`` queries.

Writes use ``BEGIN IMMEDIATE`` per reviewer C1 to serialize concurrent writers.
``list_events`` clamps ``limit`` to ``[1..500]`` per C3.
"""

from __future__ import annotations

import json
import time
from typing import Any

from .db import connect


class SessionEventStream:
    def __init__(self, db_path: str) -> None:
        self.db_path = db_path

    async def append(
        self,
        session_id: str,
        event_type: str,
        payload: dict[str, Any],
        created_at: int | None = None,
    ) -> int:
        """Append a single event; returns the new ``seq``.

        Raises ``aiosqlite.IntegrityError`` when ``session_id`` does not exist
        in the ``sessions`` table (FK violation). Callers are expected to
        surface that as a 404 at the HTTP layer.
        """
        if not session_id:
            raise ValueError("session_id must be a non-empty string")
        if not event_type:
            raise ValueError("event_type must be a non-empty string")

        ts = int(created_at if created_at is not None else time.time())
        payload_json = json.dumps(payload, sort_keys=True)

        db = await connect(self.db_path)
        try:
            await db.execute("BEGIN IMMEDIATE")
            try:
                cur = await db.execute(
                    """
                    INSERT INTO session_events
                        (session_id, event_type, payload_json, created_at)
                    VALUES (?, ?, ?, ?)
                    """,
                    (session_id, event_type, payload_json, ts),
                )
                seq = cur.lastrowid
                await db.commit()
            except Exception:
                await db.rollback()
                raise
        finally:
            await db.close()

        assert seq is not None
        return int(seq)

    async def list_events(
        self,
        session_id: str,
        from_seq: int = 1,
        to_seq: int = 2**31 - 1,
        limit: int = 500,
    ) -> list[dict[str, Any]]:
        """Return events in ascending seq order inside ``[from_seq, to_seq]``.

        ``limit`` is clamped to ``[1..500]`` (C3).
        """
        safe_limit = _clamp_limit(limit, default=500, maximum=500)
        if from_seq is None or from_seq < 1:
            from_seq = 1
        if to_seq is None:
            to_seq = 2**31 - 1
        # N1 (reviewer): reject nonsensical ranges instead of silently
        # rewriting them — callers that truly want "everything from X" should
        # omit ``to_seq`` rather than passing ``to_seq < from_seq``.
        if to_seq < from_seq:
            raise ValueError(
                f"to_seq ({to_seq}) must be >= from_seq ({from_seq})"
            )

        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                """
                SELECT seq, session_id, event_type, payload_json, created_at
                FROM session_events
                WHERE session_id = ?
                  AND seq BETWEEN ? AND ?
                ORDER BY seq ASC
                LIMIT ?
                """,
                (session_id, from_seq, to_seq, safe_limit),
            )
            rows = await cur.fetchall()
        finally:
            await db.close()

        return [
            {
                "seq": int(r["seq"]),
                "session_id": r["session_id"],
                "event_type": r["event_type"],
                "payload": json.loads(r["payload_json"]) if r["payload_json"] else {},
                "created_at": int(r["created_at"]),
            }
            for r in rows
        ]


def _clamp_limit(value: int | None, *, default: int, maximum: int) -> int:
    """Clamp a query limit to a safe range. Reviewer note C3 (S3.T2)."""
    if value is None or value <= 0:
        return default
    return min(int(value), maximum)
