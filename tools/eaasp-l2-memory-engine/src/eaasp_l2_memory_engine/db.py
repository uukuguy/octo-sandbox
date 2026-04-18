"""Shared SQLite schema and connection helpers."""

from __future__ import annotations

import asyncio
import struct

import aiosqlite


# D94 — process-level singleton connections, one per db_path.
# Avoids opening a new aiosqlite.Connection on every store method call.
# Guarded by a per-path asyncio.Lock so concurrent coroutines don't race
# during first-time initialisation.
_shared_connections: dict[str, aiosqlite.Connection] = {}
_init_locks: dict[str, asyncio.Lock] = {}
# Per-path write serialization locks. aiosqlite's underlying sqlite3 connection
# can only hold one transaction at a time — concurrent BEGIN IMMEDIATE calls
# raise OperationalError. This lock ensures write paths are mutually exclusive.
_write_locks: dict[str, asyncio.Lock] = {}


async def get_shared_connection(path: str) -> aiosqlite.Connection:
    """Return the cached aiosqlite.Connection for *path*, creating it on first call.

    The returned connection is long-lived (process lifetime). Callers MUST NOT
    call ``db.close()`` on it — it is shared. For write paths that issue
    BEGIN IMMEDIATE, acquire ``get_write_lock(path)`` first to prevent
    concurrent transaction conflicts.
    """
    if path not in _shared_connections:
        # One lock per path to avoid contention across unrelated databases.
        if path not in _init_locks:
            _init_locks[path] = asyncio.Lock()
        async with _init_locks[path]:
            # Double-checked locking after acquiring per-path lock.
            if path not in _shared_connections:
                db = await aiosqlite.connect(path)
                db.row_factory = aiosqlite.Row
                _shared_connections[path] = db
    return _shared_connections[path]


def get_write_lock(path: str) -> asyncio.Lock:
    """Return the per-path write serialization lock.

    Callers that issue ``BEGIN IMMEDIATE`` on the shared connection MUST hold
    this lock for the duration of the transaction to prevent concurrent
    ``OperationalError: cannot start a transaction within a transaction``.
    """
    if path not in _write_locks:
        _write_locks[path] = asyncio.Lock()
    return _write_locks[path]

SCHEMA = """
PRAGMA journal_mode=WAL;

CREATE TABLE IF NOT EXISTS anchors (
    anchor_id      TEXT PRIMARY KEY,
    event_id       TEXT NOT NULL,
    session_id     TEXT NOT NULL,
    type           TEXT NOT NULL,
    data_ref       TEXT,
    snapshot_hash  TEXT,
    source_system  TEXT,
    tool_version   TEXT,
    model_version  TEXT,
    rule_version   TEXT,
    created_at     INTEGER NOT NULL,
    metadata       TEXT
);

CREATE INDEX IF NOT EXISTS idx_anchors_event_id   ON anchors(event_id);
CREATE INDEX IF NOT EXISTS idx_anchors_session_id ON anchors(session_id);

-- M3: DB-enforced append-only invariant on anchors.
CREATE TRIGGER IF NOT EXISTS anchors_no_update
BEFORE UPDATE ON anchors
BEGIN SELECT RAISE(ABORT, 'anchors are append-only'); END;

CREATE TRIGGER IF NOT EXISTS anchors_no_delete
BEFORE DELETE ON anchors
BEGIN SELECT RAISE(ABORT, 'anchors are append-only'); END;

CREATE TABLE IF NOT EXISTS memory_files (
    memory_id      TEXT NOT NULL,
    version        INTEGER NOT NULL,
    scope          TEXT NOT NULL,
    category       TEXT NOT NULL,
    content        TEXT NOT NULL,
    evidence_refs  TEXT,
    status         TEXT NOT NULL CHECK(status IN ('agent_suggested','confirmed','archived')),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (memory_id, version)
);

CREATE INDEX IF NOT EXISTS idx_memory_files_scope    ON memory_files(scope);
CREATE INDEX IF NOT EXISTS idx_memory_files_category ON memory_files(category);
CREATE INDEX IF NOT EXISTS idx_memory_files_status   ON memory_files(status);

CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    memory_id UNINDEXED,
    version   UNINDEXED,
    content_text,
    category,
    scope,
    tokenize = 'unicode61'
);
"""


# Phase 2 S2.T1 — embedding columns on memory_files.
# Stored as f32 BLOB (packed via struct). Migration is idempotent via
# PRAGMA table_info check so apply_embedding_migration() is safe to call
# multiple times.
#
# D12 status: *partially addressed* — migration is idempotent, but the full
# MemoryStore singleton refactor (shared aiosqlite.Connection + global
# write_lock) is intentionally deferred to Phase 2.5. All callers continue
# to use per-call connect() for now. Do NOT open a long-lived shared
# connection here; existing AnchorStore / MemoryFileStore / HybridIndex
# paths all assume per-call connections.


async def apply_embedding_migration(db: aiosqlite.Connection) -> None:
    """Idempotent embedding-columns migration on memory_files.

    Adds three nullable columns + one index:
        - embedding_model_id TEXT    (e.g. 'bge-m3:fp16@ollama')
        - embedding_dim     INTEGER
        - embedding_vec     BLOB     (packed f32 array, len = dim * 4 bytes)
        - idx_memory_files_embedding_model ON memory_files(embedding_model_id)

    Safe to call multiple times. Uses PRAGMA table_info before ALTER so
    existing columns are not re-added. Caller must pass a connection with
    ``row_factory = aiosqlite.Row`` so that ``row["name"]`` works.
    """
    cur = await db.execute("PRAGMA table_info(memory_files)")
    cols = await cur.fetchall()
    col_names = {row["name"] for row in cols}

    if "embedding_model_id" not in col_names:
        await db.execute("ALTER TABLE memory_files ADD COLUMN embedding_model_id TEXT")
    if "embedding_dim" not in col_names:
        await db.execute("ALTER TABLE memory_files ADD COLUMN embedding_dim INTEGER")
    if "embedding_vec" not in col_names:
        await db.execute("ALTER TABLE memory_files ADD COLUMN embedding_vec BLOB")

    await db.execute(
        """CREATE INDEX IF NOT EXISTS idx_memory_files_embedding_model
              ON memory_files(embedding_model_id)"""
    )
    await db.commit()


async def init_db(path: str) -> None:
    """Create schema if absent and apply idempotent Phase 2 migrations."""
    async with aiosqlite.connect(path) as db:
        await db.executescript(SCHEMA)
        await db.commit()
        # Phase 2 S2.T1 — embedding columns (idempotent).
        # Row factory needed so apply_embedding_migration can access
        # row["name"] on PRAGMA table_info output.
        db.row_factory = aiosqlite.Row
        await apply_embedding_migration(db)


async def connect(path: str) -> aiosqlite.Connection:
    """Open a connection with row factory set."""
    db = await aiosqlite.connect(path)
    db.row_factory = aiosqlite.Row
    return db


def pack_embedding(vec: list[float]) -> bytes:
    """Pack list[float] → f32 BLOB for SQLite storage.

    Length invariant: ``len(pack_embedding(vec)) == len(vec) * 4``.
    """
    return struct.pack(f"{len(vec)}f", *vec)


def unpack_embedding(blob: bytes, dim: int) -> list[float]:
    """Unpack f32 BLOB → list[float] of length ``dim``.

    Callers that already know ``dim`` via ``embedding_dim`` column should
    pass it explicitly; blob length must equal ``dim * 4``.
    """
    return list(struct.unpack(f"{dim}f", blob))
