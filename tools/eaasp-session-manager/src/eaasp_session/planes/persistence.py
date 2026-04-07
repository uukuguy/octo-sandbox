"""持久化平面 — SQLite session store (KD-BH5: schema 对标 PostgreSQL).

Manages sessions, execution_log, and telemetry_events tables.
"""

from __future__ import annotations

import json
import logging
import sqlite3
from datetime import datetime, timezone

logger = logging.getLogger(__name__)

_SCHEMA = """
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    org_unit TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    runtime_id TEXT,
    runtime_endpoint TEXT,
    status TEXT NOT NULL DEFAULT 'creating',
    managed_hooks_digest TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    terminated_at TEXT
);

CREATE TABLE IF NOT EXISTS execution_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    event_type TEXT NOT NULL,
    payload_json TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS telemetry_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    runtime_id TEXT,
    event_type TEXT NOT NULL,
    resource_usage_json TEXT,
    created_at TEXT NOT NULL
);
"""


class PersistencePlane:
    """SQLite-backed persistence for L4 sessions."""

    def __init__(self, db_path: str = ":memory:") -> None:
        self._conn = sqlite3.connect(db_path, check_same_thread=False)
        self._conn.row_factory = sqlite3.Row
        self._conn.executescript(_SCHEMA)

    def create_session(
        self,
        session_id: str,
        conversation_id: str,
        user_id: str,
        org_unit: str,
        skill_id: str,
        runtime_id: str = "",
        runtime_endpoint: str = "",
        managed_hooks_digest: str = "",
    ) -> None:
        now = datetime.now(timezone.utc).isoformat()
        self._conn.execute(
            """INSERT INTO sessions
               (id, conversation_id, user_id, org_unit, skill_id,
                runtime_id, runtime_endpoint, status, managed_hooks_digest,
                created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, ?)""",
            (session_id, conversation_id, user_id, org_unit, skill_id,
             runtime_id, runtime_endpoint, managed_hooks_digest, now, now),
        )
        self._conn.commit()

    def update_status(self, session_id: str, status: str) -> None:
        now = datetime.now(timezone.utc).isoformat()
        terminated_at = now if status == "terminated" else None
        self._conn.execute(
            """UPDATE sessions SET status=?, updated_at=?, terminated_at=?
               WHERE id=?""",
            (status, now, terminated_at, session_id),
        )
        self._conn.commit()

    def get_session(self, session_id: str) -> dict | None:
        row = self._conn.execute(
            "SELECT * FROM sessions WHERE id=?", (session_id,)
        ).fetchone()
        return dict(row) if row else None

    def list_sessions(self) -> list[dict]:
        rows = self._conn.execute("SELECT * FROM sessions ORDER BY created_at DESC").fetchall()
        return [dict(r) for r in rows]

    def log_event(self, session_id: str, event_type: str, payload: dict | None = None) -> None:
        now = datetime.now(timezone.utc).isoformat()
        self._conn.execute(
            """INSERT INTO execution_log (session_id, event_type, payload_json, created_at)
               VALUES (?, ?, ?, ?)""",
            (session_id, event_type, json.dumps(payload) if payload else None, now),
        )
        self._conn.commit()

    def log_telemetry(
        self, session_id: str, runtime_id: str, event_type: str,
        resource_usage: dict | None = None,
    ) -> None:
        now = datetime.now(timezone.utc).isoformat()
        self._conn.execute(
            """INSERT INTO telemetry_events
               (session_id, runtime_id, event_type, resource_usage_json, created_at)
               VALUES (?, ?, ?, ?, ?)""",
            (session_id, runtime_id, event_type,
             json.dumps(resource_usage) if resource_usage else None, now),
        )
        self._conn.commit()

    def get_telemetry(self, session_id: str) -> list[dict]:
        rows = self._conn.execute(
            "SELECT * FROM telemetry_events WHERE session_id=? ORDER BY created_at",
            (session_id,),
        ).fetchall()
        return [dict(r) for r in rows]

    def close(self) -> None:
        self._conn.close()
