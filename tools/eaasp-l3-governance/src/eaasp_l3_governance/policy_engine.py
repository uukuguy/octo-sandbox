"""Policy engine — persistence for managed-settings versions + hook modes.

Contract 1 (Policy Deployment) surface:

- ``deploy()``  — accept a pre-compiled ``ManagedSettings`` and insert a new
  row in ``managed_settings_versions``.
- ``switch_mode()`` — flip an individual hook between ``enforce`` / ``shadow``
  by upserting ``managed_hooks_mode_overrides``. Does **not** bump the
  version number (overrides float above versions — see design note in db.py).
- ``list_versions()`` — newest-first metadata for the UI / CLI ``policy
  versions`` command.
- ``latest_version()`` — most recent version row, used by session validate.
- ``get_mode_override()`` — look up a single hook's override (None if unset).

All write operations are wrapped in ``BEGIN IMMEDIATE`` transactions per
reviewer note C1 (L2 S3.T2 lesson).
"""

from __future__ import annotations

import json
from typing import Any

from pydantic import BaseModel

from .db import connect
from .managed_settings import ManagedSettings, ensure_mode


class DeployResult(BaseModel):
    version: int
    created_at: str
    hook_count: int
    mode_summary: dict[str, int]


class VersionSummary(BaseModel):
    version: int
    created_at: str
    hook_count: int
    mode_summary: dict[str, int]


class VersionDetail(BaseModel):
    version: int
    created_at: str
    hook_count: int
    mode_summary: dict[str, int]
    payload: dict[str, Any]


class ModeOverride(BaseModel):
    hook_id: str
    mode: str
    updated_at: str


class PolicyEngine:
    def __init__(self, db_path: str) -> None:
        self.db_path = db_path

    # ─── Contract 1: PUT /v1/policies/managed-hooks ───────────────────────
    async def deploy(self, settings: ManagedSettings) -> DeployResult:
        """Persist a new managed-settings version.

        The payload is serialized with ``model_dump(mode='json')`` so the
        extras (``ConfigDict(extra="allow")``) round-trip cleanly.
        """
        payload_json = json.dumps(settings.model_dump(mode="json"), sort_keys=True)
        hook_count = len(settings.hooks)
        mode_summary = settings.mode_summary()
        mode_summary_json = json.dumps(mode_summary, sort_keys=True)

        db = await connect(self.db_path)
        try:
            await db.execute("BEGIN IMMEDIATE")
            try:
                cur = await db.execute(
                    """
                    INSERT INTO managed_settings_versions
                        (payload_json, hook_count, mode_summary)
                    VALUES (?, ?, ?)
                    """,
                    (payload_json, hook_count, mode_summary_json),
                )
                version = cur.lastrowid
                # Read back created_at so the response is DB-authoritative.
                cur2 = await db.execute(
                    "SELECT created_at FROM managed_settings_versions WHERE version = ?",
                    (version,),
                )
                row = await cur2.fetchone()
                await db.commit()
            except Exception:
                await db.rollback()
                raise
        finally:
            await db.close()

        assert row is not None and version is not None  # sanity for mypy
        return DeployResult(
            version=int(version),
            created_at=row["created_at"],
            hook_count=hook_count,
            mode_summary=mode_summary,
        )

    # ─── Contract 1: PUT /v1/policies/{hook_id}/mode ──────────────────────
    async def switch_mode(self, hook_id: str, mode: str) -> ModeOverride:
        """Upsert a mode override. Rejects unknown modes (M4)."""
        validated = ensure_mode(mode)
        if not hook_id:
            raise ValueError("hook_id must be a non-empty string")

        db = await connect(self.db_path)
        try:
            await db.execute("BEGIN IMMEDIATE")
            try:
                await db.execute(
                    """
                    INSERT INTO managed_hooks_mode_overrides (hook_id, mode)
                    VALUES (?, ?)
                    ON CONFLICT(hook_id) DO UPDATE SET
                        mode = excluded.mode,
                        updated_at = datetime('now')
                    """,
                    (hook_id, validated),
                )
                cur = await db.execute(
                    "SELECT hook_id, mode, updated_at "
                    "FROM managed_hooks_mode_overrides WHERE hook_id = ?",
                    (hook_id,),
                )
                row = await cur.fetchone()
                await db.commit()
            except Exception:
                await db.rollback()
                raise
        finally:
            await db.close()

        assert row is not None
        return ModeOverride(
            hook_id=row["hook_id"],
            mode=row["mode"],
            updated_at=row["updated_at"],
        )

    async def get_mode_override(self, hook_id: str) -> ModeOverride | None:
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                "SELECT hook_id, mode, updated_at "
                "FROM managed_hooks_mode_overrides WHERE hook_id = ?",
                (hook_id,),
            )
            row = await cur.fetchone()
        finally:
            await db.close()
        if row is None:
            return None
        return ModeOverride(
            hook_id=row["hook_id"],
            mode=row["mode"],
            updated_at=row["updated_at"],
        )

    # ─── Contract 1: GET /v1/policies/versions ────────────────────────────
    async def list_versions(self, limit: int = 100) -> list[VersionSummary]:
        """Newest-first list of deployed policy versions. Limit clamped (C3)."""
        safe_limit = _clamp_limit(limit, default=100, maximum=500)
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                """
                SELECT version, created_at, hook_count, mode_summary
                FROM managed_settings_versions
                ORDER BY version DESC
                LIMIT ?
                """,
                (safe_limit,),
            )
            rows = await cur.fetchall()
        finally:
            await db.close()

        return [
            VersionSummary(
                version=r["version"],
                created_at=r["created_at"],
                hook_count=r["hook_count"],
                mode_summary=_load_mode_summary(r["mode_summary"]),
            )
            for r in rows
        ]

    async def latest_version(self) -> VersionDetail | None:
        """Most-recent version row with full payload (used by validate)."""
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                """
                SELECT version, created_at, hook_count, mode_summary, payload_json
                FROM managed_settings_versions
                ORDER BY version DESC
                LIMIT 1
                """,
            )
            row = await cur.fetchone()
        finally:
            await db.close()
        if row is None:
            return None
        return VersionDetail(
            version=row["version"],
            created_at=row["created_at"],
            hook_count=row["hook_count"],
            mode_summary=_load_mode_summary(row["mode_summary"]),
            payload=json.loads(row["payload_json"]),
        )

    async def get_version(self, version: int) -> VersionDetail | None:
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                """
                SELECT version, created_at, hook_count, mode_summary, payload_json
                FROM managed_settings_versions
                WHERE version = ?
                """,
                (version,),
            )
            row = await cur.fetchone()
        finally:
            await db.close()
        if row is None:
            return None
        return VersionDetail(
            version=row["version"],
            created_at=row["created_at"],
            hook_count=row["hook_count"],
            mode_summary=_load_mode_summary(row["mode_summary"]),
            payload=json.loads(row["payload_json"]),
        )


def _load_mode_summary(raw: str | None) -> dict[str, int]:
    if not raw:
        return {"enforce": 0, "shadow": 0}
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return {"enforce": 0, "shadow": 0}
    return {k: int(v) for k, v in data.items()}


def _clamp_limit(value: int | None, *, default: int, maximum: int) -> int:
    """Clamp a query limit to a safe range. Reviewer note C3 (S3.T2)."""
    if value is None or value <= 0:
        return default
    return min(int(value), maximum)
