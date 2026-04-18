"""Layer 1 — Evidence Anchor Store (append-only immutable)."""

from __future__ import annotations

import json
import time
import uuid
from typing import Any

from pydantic import BaseModel, Field

from .db import get_shared_connection


class AnchorIn(BaseModel):
    event_id: str
    session_id: str
    type: str
    data_ref: str | None = None
    snapshot_hash: str | None = None
    source_system: str | None = None
    tool_version: str | None = None
    model_version: str | None = None
    rule_version: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)


class AnchorOut(BaseModel):
    anchor_id: str
    event_id: str
    session_id: str
    type: str
    data_ref: str | None
    snapshot_hash: str | None
    source_system: str | None
    tool_version: str | None
    model_version: str | None
    rule_version: str | None
    created_at: int
    metadata: dict[str, Any]


class AnchorStore:
    def __init__(self, db_path: str) -> None:
        self.db_path = db_path

    async def write(self, anchor: AnchorIn) -> AnchorOut:
        anchor_id = f"anc_{uuid.uuid4().hex[:16]}"
        created_at = int(time.time() * 1000)
        db = await get_shared_connection(self.db_path)
        await db.execute(
            """
            INSERT INTO anchors (
                anchor_id, event_id, session_id, type, data_ref, snapshot_hash,
                source_system, tool_version, model_version, rule_version,
                created_at, metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                anchor_id,
                anchor.event_id,
                anchor.session_id,
                anchor.type,
                anchor.data_ref,
                anchor.snapshot_hash,
                anchor.source_system,
                anchor.tool_version,
                anchor.model_version,
                anchor.rule_version,
                created_at,
                json.dumps(anchor.metadata),
            ),
        )
        await db.commit()
        return AnchorOut(
            anchor_id=anchor_id,
            event_id=anchor.event_id,
            session_id=anchor.session_id,
            type=anchor.type,
            data_ref=anchor.data_ref,
            snapshot_hash=anchor.snapshot_hash,
            source_system=anchor.source_system,
            tool_version=anchor.tool_version,
            model_version=anchor.model_version,
            rule_version=anchor.rule_version,
            created_at=created_at,
            metadata=anchor.metadata,
        )

    async def get(self, anchor_id: str) -> AnchorOut | None:
        db = await get_shared_connection(self.db_path)
        cur = await db.execute(
            "SELECT * FROM anchors WHERE anchor_id = ?", (anchor_id,)
        )
        row = await cur.fetchone()
        return _row_to_anchor(row) if row else None

    async def list_by_event(self, event_id: str) -> list[AnchorOut]:
        db = await get_shared_connection(self.db_path)
        cur = await db.execute(
            "SELECT * FROM anchors WHERE event_id = ? ORDER BY created_at ASC",
            (event_id,),
        )
        rows = await cur.fetchall()
        return [_row_to_anchor(r) for r in rows]

    async def list_by_session(self, session_id: str) -> list[AnchorOut]:
        db = await get_shared_connection(self.db_path)
        cur = await db.execute(
            "SELECT * FROM anchors WHERE session_id = ? ORDER BY created_at ASC",
            (session_id,),
        )
        rows = await cur.fetchall()
        return [_row_to_anchor(r) for r in rows]


def _row_to_anchor(row: Any) -> AnchorOut:
    metadata_raw = row["metadata"]
    metadata = json.loads(metadata_raw) if metadata_raw else {}
    return AnchorOut(
        anchor_id=row["anchor_id"],
        event_id=row["event_id"],
        session_id=row["session_id"],
        type=row["type"],
        data_ref=row["data_ref"],
        snapshot_hash=row["snapshot_hash"],
        source_system=row["source_system"],
        tool_version=row["tool_version"],
        model_version=row["model_version"],
        rule_version=row["rule_version"],
        created_at=row["created_at"],
        metadata=metadata,
    )
