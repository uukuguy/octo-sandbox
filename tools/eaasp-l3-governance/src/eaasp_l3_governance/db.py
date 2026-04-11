"""Shared SQLite schema and connection helpers for L3 governance.

Mirrors the conventions locked in S3.T2 (L2 memory engine):
- WAL journal mode for concurrent readers.
- ``foreign_keys`` pragma on every open (defense-in-depth).
- Row factory set so callers can use ``row["col"]`` access.
- Writes must be wrapped in ``BEGIN IMMEDIATE`` (enforced at call sites — C1).
"""

from __future__ import annotations

import aiosqlite

SCHEMA = """
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- Contract 1 — versioned managed-settings.json snapshots.
-- Append-only: a policy "deploy" is always a new row.
CREATE TABLE IF NOT EXISTS managed_settings_versions (
    version       INTEGER PRIMARY KEY AUTOINCREMENT,
    payload_json  TEXT NOT NULL,
    hook_count    INTEGER NOT NULL,
    mode_summary  TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_managed_settings_created_at
    ON managed_settings_versions(created_at DESC);

-- Contract 1 — per-hook mode overrides that float above the version rows.
-- One row per hook_id; updated in place (kept separate so mode flips do not
-- bump the policy version number — aligns with §3.3 "thin L3" semantics).
CREATE TABLE IF NOT EXISTS managed_hooks_mode_overrides (
    hook_id    TEXT PRIMARY KEY,
    mode       TEXT NOT NULL CHECK(mode IN ('enforce','shadow')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Contract 4 — async PostToolUse telemetry ingest.
CREATE TABLE IF NOT EXISTS telemetry_events (
    event_id     TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    agent_id     TEXT,
    hook_id      TEXT,
    phase        TEXT,
    payload_json TEXT NOT NULL,
    received_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_telemetry_session
    ON telemetry_events(session_id, received_at DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_received_at
    ON telemetry_events(received_at DESC);
"""


async def init_db(path: str) -> None:
    """Create schema if absent."""
    async with aiosqlite.connect(path) as db:
        await db.executescript(SCHEMA)
        await db.commit()


async def connect(path: str) -> aiosqlite.Connection:
    """Open a connection with row factory set and pragmas applied."""
    db = await aiosqlite.connect(path)
    db.row_factory = aiosqlite.Row
    # Defense-in-depth: reapply pragmas on every connection. WAL is persistent
    # (set on the file) but foreign_keys is a per-connection flag.
    await db.execute("PRAGMA foreign_keys=ON")
    return db
