"""Layer 2 — File-based Memory (versioned, with status state machine)."""

from __future__ import annotations

import json
import logging
import os
import time
import uuid
from typing import Any, Literal

from pydantic import BaseModel, Field

from .db import get_shared_connection, get_write_lock, pack_embedding

logger = logging.getLogger(__name__)

MemoryStatus = Literal["agent_suggested", "confirmed", "archived"]

_ALLOWED_TRANSITIONS: dict[str, set[str]] = {
    "agent_suggested": {"confirmed", "archived"},
    "confirmed": {"archived"},
    "archived": set(),
}


class MemoryFileIn(BaseModel):
    memory_id: str | None = None
    scope: str
    category: str
    content: str
    evidence_refs: list[str] = Field(default_factory=list)
    status: MemoryStatus = "agent_suggested"


class MemoryFileOut(BaseModel):
    memory_id: str
    version: int
    scope: str
    category: str
    content: str
    evidence_refs: list[str]
    status: MemoryStatus
    created_at: int
    updated_at: int
    # S2.T3: surface embedding metadata for observability. NOTE: embedding_vec
    # (raw f32 blob) is NOT surfaced — it is an internal HNSW detail and
    # surfacing it would waste bandwidth and leak format choices.
    embedding_model_id: str | None = None
    embedding_dim: int | None = None


class InvalidStatusTransition(ValueError):
    pass


class MemoryFileStore:
    def __init__(self, db_path: str, octo_root: str | None = None) -> None:
        """Construct a memory file store.

        Args:
            db_path: Path to the SQLite database.
            octo_root: Directory under which HNSW indices are persisted
                (``{octo_root}/l2-memory/hnsw-{model_id_safe}/``). Defaults
                to the directory containing ``db_path`` so tests that use
                ``tmp_path`` get isolated indices automatically.
        """
        self.db_path = db_path
        self.octo_root = octo_root or os.path.dirname(os.path.abspath(db_path))

    async def write(self, memory: MemoryFileIn) -> MemoryFileOut:
        """Insert new memory or bump version of an existing memory_id.

        Wrapped in BEGIN IMMEDIATE to avoid racy (SELECT MAX + INSERT) version
        collisions (C1). When memory_id is provided, status transition is
        validated against the latest version (M4).

        Phase 2 S2.T1 — dual-write embedding:
            1. Compute embedding OUTSIDE the BEGIN IMMEDIATE block so that
               a slow/failing provider never holds a write lock. Failure here
               is non-fatal: the base row is still inserted, just with NULL
               embedding columns.
            2. INSERT writes embedding_model_id / embedding_dim /
               embedding_vec atomically with the base row (same transaction).
            3. AFTER commit, HNSW ``add()`` + ``save()`` run best-effort.
               Any failure is logged but never propagates — the DB row is
               authoritative and can be re-indexed later.
        """
        now = int(time.time() * 1000)
        memory_id = memory.memory_id or f"mem_{uuid.uuid4().hex[:16]}"

        # Step 1 — compute embedding pre-txn. Keep all import + compute
        # inside a single try so any failure path (import error, provider
        # crash, dimension surprise) falls through to NULL embedding columns
        # rather than aborting the write.
        embedding_model_id: str | None = None
        embedding_dim: int | None = None
        embedding_blob: bytes | None = None
        embedding_vec: list[float] | None = None
        try:
            from .embedding import get_embedding_provider

            embedder = get_embedding_provider()
            _vec = await embedder.embed(memory.content)
            embedding_vec = _vec
            embedding_model_id = embedder.model_id
            embedding_dim = embedder.dimension
            embedding_blob = pack_embedding(_vec)
        except Exception as e:  # noqa: BLE001 — embedding must never block writes
            logger.warning("memory_write embedding skipped: %s", e)

        db = await get_shared_connection(self.db_path)
        async with get_write_lock(self.db_path):
            await db.execute("BEGIN IMMEDIATE")
            try:
                cur = await db.execute(
                    """
                    SELECT MAX(version) AS v, MIN(created_at) AS c,
                           (SELECT status FROM memory_files
                              WHERE memory_id = ?
                              ORDER BY version DESC LIMIT 1) AS latest_status
                      FROM memory_files WHERE memory_id = ?
                    """,
                    (memory_id, memory_id),
                )
                row = await cur.fetchone()
                latest_version = row["v"] if row and row["v"] is not None else 0
                created_at = row["c"] if row and row["c"] is not None else now
                latest_status = row["latest_status"] if row else None
                new_version = latest_version + 1

                # M4: enforce status transitions when bumping an existing memory_id.
                if latest_status is not None and memory.status != latest_status:
                    if memory.status not in _ALLOWED_TRANSITIONS[latest_status]:
                        raise InvalidStatusTransition(
                            f"Cannot transition {latest_status} → {memory.status} "
                            f"for {memory_id}"
                        )

                await db.execute(
                    """
                    INSERT INTO memory_files (
                        memory_id, version, scope, category, content, evidence_refs,
                        status, created_at, updated_at,
                        embedding_model_id, embedding_dim, embedding_vec
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        memory_id,
                        new_version,
                        memory.scope,
                        memory.category,
                        memory.content,
                        json.dumps(memory.evidence_refs),
                        memory.status,
                        created_at,
                        now,
                        embedding_model_id,
                        embedding_dim,
                        embedding_blob,
                    ),
                )
                await db.execute(
                    """
                    INSERT INTO memory_fts (memory_id, version, content_text, category, scope)
                    VALUES (?, ?, ?, ?, ?)
                    """,
                    (
                        memory_id,
                        new_version,
                        memory.content,
                        memory.category,
                        memory.scope,
                    ),
                )
                await db.commit()
            except Exception:
                await db.rollback()
                raise

        # Step 3 — HNSW add/save is post-commit and fully non-fatal. The DB
        # row is authoritative; HNSW is a rebuild-able index. Import is
        # inside the try so missing hnswlib / vector_index stays soft.
        if (
            embedding_vec is not None
            and embedding_model_id is not None
            and embedding_dim is not None
        ):
            try:
                from .vector_index import HNSWVectorIndex

                idx = HNSWVectorIndex(
                    model_id=embedding_model_id,
                    octo_root=self.octo_root,
                    dim=embedding_dim,
                )
                await idx.add(f"{memory_id}:v{new_version}", embedding_vec)
                await idx.save()
            except Exception as e:  # noqa: BLE001 — HNSW must never block writes
                logger.warning("memory_write HNSW add skipped: %s", e)

        return MemoryFileOut(
            memory_id=memory_id,
            version=new_version,
            scope=memory.scope,
            category=memory.category,
            content=memory.content,
            evidence_refs=memory.evidence_refs,
            status=memory.status,
            created_at=created_at,
            updated_at=now,
        )

    async def read_latest(self, memory_id: str) -> MemoryFileOut | None:
        db = await get_shared_connection(self.db_path)
        cur = await db.execute(
            """
            SELECT * FROM memory_files
            WHERE memory_id = ?
            ORDER BY version DESC
            LIMIT 1
            """,
            (memory_id,),
        )
        row = await cur.fetchone()
        return _row_to_memory(row) if row else None

    async def list(
        self,
        scope: str | None = None,
        category: str | None = None,
        status: MemoryStatus | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> list[MemoryFileOut]:
        """Latest version of each memory_id matching filters.

        Args:
            scope: optional exact-match filter on ``scope``.
            category: optional exact-match filter on ``category``.
            status: optional exact-match filter on ``status``.
            limit: max rows returned (default 50).
            offset: skip N rows before returning (default 0, S2.T3).

        Ordered by ``updated_at DESC``. ``offset`` is clamped to ``>= 0`` by
        callers (see :func:`mcp_tools._memory_list`); this method itself
        passes the value through unchanged so the SQL-level default keeps
        working when called programmatically.
        """
        where: list[str] = []
        params: list[Any] = []
        if scope is not None:
            where.append("scope = ?")
            params.append(scope)
        if category is not None:
            where.append("category = ?")
            params.append(category)
        if status is not None:
            where.append("status = ?")
            params.append(status)

        where_clause = ("WHERE " + " AND ".join(where)) if where else ""
        sql = f"""
            SELECT mf.*
            FROM memory_files mf
            INNER JOIN (
                SELECT memory_id, MAX(version) AS max_v
                FROM memory_files
                GROUP BY memory_id
            ) latest
              ON mf.memory_id = latest.memory_id AND mf.version = latest.max_v
            {where_clause}
            ORDER BY mf.updated_at DESC
            LIMIT ? OFFSET ?
        """
        params.append(limit)
        params.append(offset)

        db = await get_shared_connection(self.db_path)
        cur = await db.execute(sql, params)
        rows = await cur.fetchall()
        return [_row_to_memory(r) for r in rows]

    async def archive(self, memory_id: str) -> MemoryFileOut:
        """Transition status → archived (creates new version)."""
        latest = await self.read_latest(memory_id)
        if latest is None:
            raise KeyError(
                f"memory_id '{memory_id}' not found. memory_id must come from "
                f"memory_search.hits[*].memory_id or memory_write_file return. "
                f"Do not invent IDs."
            )
        if "archived" not in _ALLOWED_TRANSITIONS[latest.status]:
            raise InvalidStatusTransition(
                f"Cannot transition {latest.status} → archived for {memory_id}"
            )
        return await self.write(
            MemoryFileIn(
                memory_id=memory_id,
                scope=latest.scope,
                category=latest.category,
                content=latest.content,
                evidence_refs=latest.evidence_refs,
                status="archived",
            )
        )

    async def confirm(self, memory_id: str) -> MemoryFileOut:
        latest = await self.read_latest(memory_id)
        if latest is None:
            raise KeyError(
                f"memory_id '{memory_id}' not found. memory_id must come from "
                f"memory_search.hits[*].memory_id or memory_write_file return. "
                f"Do not invent IDs."
            )
        if "confirmed" not in _ALLOWED_TRANSITIONS[latest.status]:
            raise InvalidStatusTransition(
                f"Cannot transition {latest.status} → confirmed for {memory_id}"
            )
        return await self.write(
            MemoryFileIn(
                memory_id=memory_id,
                scope=latest.scope,
                category=latest.category,
                content=latest.content,
                evidence_refs=latest.evidence_refs,
                status="confirmed",
            )
        )


def _row_to_memory(row: Any) -> MemoryFileOut:
    refs_raw = row["evidence_refs"]
    refs = json.loads(refs_raw) if refs_raw else []
    # S2.T3: defensive read of embedding columns. The migration in
    # apply_embedding_migration() is idempotent so these should always be
    # present on post-S2.T1 DBs, but legacy rows (or projections that do
    # not SELECT them) may lack the columns entirely. ``aiosqlite.Row`` is
    # backed by sqlite3.Row whose .keys() returns a list, so membership
    # testing works; missing keys otherwise raise IndexError (not KeyError).
    keys = row.keys()
    embedding_model_id = row["embedding_model_id"] if "embedding_model_id" in keys else None
    embedding_dim = row["embedding_dim"] if "embedding_dim" in keys else None
    return MemoryFileOut(
        memory_id=row["memory_id"],
        version=row["version"],
        scope=row["scope"],
        category=row["category"],
        content=row["content"],
        evidence_refs=refs,
        status=row["status"],
        created_at=row["created_at"],
        updated_at=row["updated_at"],
        embedding_model_id=embedding_model_id,
        embedding_dim=embedding_dim,
    )
