"""Session orchestrator — Contract 2 (intent dispatch) + Contract 5 (handshake).

Top-level flow for ``create_session`` (the three-way handshake):

1. P3 — Query L2 for MemoryRefs (top_k hybrid search against intent_text).
2. P1 — Query L3 ``/v1/sessions/{id}/validate`` for PolicyContext.
3. Assemble SessionPayload via ``build_session_payload``.
4. Persist ``sessions`` row + append ``SESSION_CREATED`` event in one
   ``BEGIN IMMEDIATE`` transaction (C1).
5. Stub L1 ``Initialize`` as ``RUNTIME_INITIALIZE_STUBBED`` event (D27).

Notes:
- The initial ``SESSION_CREATED`` and ``RUNTIME_INITIALIZE_STUBBED`` events are
  inserted **inside the same transaction** as the session row (using the same
  connection) so a crash between them cannot leave a session without its
  boot events.
- Subsequent events (``USER_MESSAGE``, ``RUNTIME_SEND_STUBBED``, …) use the
  standalone ``SessionEventStream.append`` path which also wraps ``BEGIN
  IMMEDIATE`` on its own connection.
"""

from __future__ import annotations

import json
import time
import uuid
from typing import Any

from .context_assembly import build_session_payload
from .db import connect
from .event_stream import SessionEventStream
from .handshake import L2Client, L3Client


class SessionNotFound(Exception):
    """Raised when a session_id is not present in the ``sessions`` table."""

    def __init__(self, session_id: str) -> None:
        self.session_id = session_id
        super().__init__(f"session not found: {session_id}")


class SessionOrchestrator:
    def __init__(
        self,
        db_path: str,
        l2: L2Client,
        l3: L3Client,
        event_stream: SessionEventStream,
    ) -> None:
        self.db_path = db_path
        self.l2 = l2
        self.l3 = l3
        self.event_stream = event_stream

    # ─── Contract 2 / Contract 5: create ─────────────────────────────────────
    async def create_session(
        self,
        *,
        intent_text: str,
        skill_id: str,
        runtime_pref: str,
        user_id: str | None = None,
        intent_id: str | None = None,
    ) -> dict[str, Any]:
        """Execute the three-way handshake and persist a new session."""
        session_id = f"sess_{uuid.uuid4().hex[:12]}"
        created_at = int(time.time())

        # Step 1 — P3: MemoryRefs from L2. Failure here is fatal (no retry).
        memory_refs = await self.l2.search_memory(query=intent_text, top_k=10)

        # Step 2 — P1: PolicyContext from L3 validate.
        validate_resp = await self.l3.validate_session(
            session_id=session_id,
            skill_id=skill_id,
            runtime_tier=runtime_pref,
            agent_id=user_id,
        )
        policy_context = {
            "hooks": validate_resp.get("hooks_to_attach", []),
            "policy_version": str(validate_resp.get("managed_settings_version", "")),
            "deploy_timestamp": str(validate_resp.get("validated_at", "")),
            "org_unit": "",
            "quotas": {},
        }

        # Step 3 — assemble payload (P1..P5 + budget flags).
        payload = build_session_payload(
            session_id=session_id,
            user_id=user_id or "",
            runtime_id=runtime_pref,
            policy_context=policy_context,
            event_context=None,
            memory_refs=memory_refs,
            skill_instructions={
                "skill_id": skill_id,
                "name": "",
                "content": "",
                "frontmatter_hooks": [],
                "metadata": {},
            },
            user_preferences={
                "user_id": user_id or "",
                "prefs": {},
                "language": "",
                "timezone": "",
            },
            created_at=created_at,
        )

        # Step 4 — persist session + seed events in one transaction (C1).
        db = await connect(self.db_path)
        try:
            await db.execute("BEGIN IMMEDIATE")
            try:
                await db.execute(
                    """
                    INSERT INTO sessions
                        (session_id, intent_id, skill_id, runtime_id, user_id,
                         status, payload_json, created_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        session_id,
                        intent_id,
                        skill_id,
                        runtime_pref,
                        user_id,
                        "created",
                        json.dumps(payload, sort_keys=True),
                        created_at,
                    ),
                )
                await db.execute(
                    """
                    INSERT INTO session_events
                        (session_id, event_type, payload_json, created_at)
                    VALUES (?, ?, ?, ?)
                    """,
                    (
                        session_id,
                        "SESSION_CREATED",
                        json.dumps({"payload": payload}, sort_keys=True),
                        created_at,
                    ),
                )
                # Step 5 — L1 Initialize stubbed (D27).
                await db.execute(
                    """
                    INSERT INTO session_events
                        (session_id, event_type, payload_json, created_at)
                    VALUES (?, ?, ?, ?)
                    """,
                    (
                        session_id,
                        "RUNTIME_INITIALIZE_STUBBED",
                        json.dumps({"runtime_id": runtime_pref}, sort_keys=True),
                        int(time.time()),
                    ),
                )
                await db.commit()
            except Exception:
                await db.rollback()
                raise
        finally:
            await db.close()

        return {
            "session_id": session_id,
            "status": "created",
            "payload": payload,
        }

    # ─── Contract 5 (partial): send_message ──────────────────────────────────
    async def send_message(self, session_id: str, content: str) -> dict[str, Any]:
        """Append a USER_MESSAGE + RUNTIME_SEND_STUBBED event pair."""
        await self._require_session(session_id)

        seq_user = await self.event_stream.append(
            session_id,
            "USER_MESSAGE",
            {"content": content},
        )
        seq_runtime = await self.event_stream.append(
            session_id,
            "RUNTIME_SEND_STUBBED",
            {"runtime_hint": "stubbed", "content_bytes": len(content.encode("utf-8"))},
        )
        return {
            "session_id": session_id,
            "seq": seq_runtime,
            "events": [
                {"seq": seq_user, "event_type": "USER_MESSAGE"},
                {"seq": seq_runtime, "event_type": "RUNTIME_SEND_STUBBED"},
            ],
        }

    # ─── Contract 5 (partial): get_session ───────────────────────────────────
    async def get_session(self, session_id: str) -> dict[str, Any]:
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                """
                SELECT session_id, intent_id, skill_id, runtime_id, user_id,
                       status, payload_json, created_at, closed_at
                FROM sessions
                WHERE session_id = ?
                """,
                (session_id,),
            )
            row = await cur.fetchone()
        finally:
            await db.close()

        if row is None:
            raise SessionNotFound(session_id)

        return {
            "session_id": row["session_id"],
            "intent_id": row["intent_id"],
            "skill_id": row["skill_id"],
            "runtime_id": row["runtime_id"],
            "user_id": row["user_id"],
            "status": row["status"],
            "payload": json.loads(row["payload_json"]) if row["payload_json"] else {},
            "created_at": int(row["created_at"]),
            "closed_at": (
                int(row["closed_at"]) if row["closed_at"] is not None else None
            ),
        }

    # ─── Contract 5 (partial): list_events ───────────────────────────────────
    async def list_events(
        self,
        session_id: str,
        *,
        from_seq: int = 1,
        to_seq: int = 2**31 - 1,
        limit: int = 500,
    ) -> list[dict[str, Any]]:
        await self._require_session(session_id)
        return await self.event_stream.list_events(
            session_id, from_seq=from_seq, to_seq=to_seq, limit=limit
        )

    # ─── Internal helpers ────────────────────────────────────────────────────
    async def _require_session(self, session_id: str) -> None:
        """Raise ``SessionNotFound`` if the session row is missing."""
        db = await connect(self.db_path)
        try:
            cur = await db.execute(
                "SELECT 1 FROM sessions WHERE session_id = ?",
                (session_id,),
            )
            row = await cur.fetchone()
        finally:
            await db.close()
        if row is None:
            raise SessionNotFound(session_id)
