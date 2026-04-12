"""Session orchestrator — Contract 2 (intent dispatch) + Contract 5 (handshake).

Top-level flow for ``create_session`` (the three-way handshake):

1. P3 — Query L2 for MemoryRefs (top_k hybrid search against intent_text).
2. P1 — Query L3 ``/v1/sessions/{id}/validate`` for PolicyContext.
3. Assemble SessionPayload via ``build_session_payload``.
4. Persist ``sessions`` row + append ``SESSION_CREATED`` event in one
   ``BEGIN IMMEDIATE`` transaction (C1).
5. Call L1 ``Initialize(SessionPayload)`` via gRPC → ``RUNTIME_INITIALIZED``
   or ``RUNTIME_INITIALIZE_FAILED`` event.

Notes:
- The initial ``SESSION_CREATED`` event is inserted **inside the same
  transaction** as the session row (using the same connection) so a crash
  between them cannot leave a session without its boot events.
- L1 Initialize is called **after** the transaction commits so that the
  session row is visible to other readers before the (potentially slow)
  gRPC call.
- Subsequent events (``USER_MESSAGE``, response chunks, …) use the
  standalone ``SessionEventStream.append`` path which also wraps ``BEGIN
  IMMEDIATE`` on its own connection.

Phase 0.5 S1.T2: Replaced RUNTIME_INITIALIZE_STUBBED / RUNTIME_SEND_STUBBED
with real gRPC calls via ``L1RuntimeClient``.
"""

from __future__ import annotations

import json
import time
import uuid
from collections.abc import AsyncIterator
from typing import Any

from .context_assembly import build_session_payload
from .db import connect
from .event_stream import SessionEventStream
from .handshake import L2Client, L3Client
from .l1_client import L1RuntimeClient, L1RuntimeError, create_l1_client


class SessionNotFound(Exception):
    """Raised when a session_id is not present in the ``sessions`` table."""

    def __init__(self, session_id: str) -> None:
        self.session_id = session_id
        super().__init__(f"session not found: {session_id}")


class InvalidStateTransition(Exception):
    """Raised when a session state transition is not allowed."""

    def __init__(self, session_id: str, current: str, target: str) -> None:
        self.session_id = session_id
        self.current = current
        self.target = target
        super().__init__(
            f"cannot transition session {session_id} from {current} to {target}"
        )


class SessionOrchestrator:
    def __init__(
        self,
        db_path: str,
        l2: L2Client,
        l3: L3Client,
        event_stream: SessionEventStream,
        *,
        l1_factory: Any | None = None,
    ) -> None:
        self.db_path = db_path
        self.l2 = l2
        self.l3 = l3
        self.event_stream = event_stream
        # l1_factory: callable(runtime_id) → L1RuntimeClient.
        # Default: create_l1_client. Tests can inject a mock factory.
        self._l1_factory = l1_factory or create_l1_client
        # Active L1 clients keyed by session_id.
        self._l1_clients: dict[str, L1RuntimeClient] = {}
        # L4 session_id → L1 session_id mapping (L1 may generate its own).
        self._l1_session_ids: dict[str, str] = {}

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
                "dependencies": [],
            },
            user_preferences={
                "user_id": user_id or "",
                "prefs": {},
                "language": "",
                "timezone": "",
            },
            created_at=created_at,
        )

        # Step 4 — persist session + SESSION_CREATED event in one txn (C1).
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
                await db.commit()
            except Exception:
                await db.rollback()
                raise
        finally:
            await db.close()

        # Step 5 — L1 Initialize via gRPC (Phase 0.5, closes D54/D27).
        l1 = self._l1_factory(runtime_pref)
        self._l1_clients[session_id] = l1
        try:
            handle = await l1.initialize(payload)
            l1_sid = handle.get("session_id", session_id)
            self._l1_session_ids[session_id] = l1_sid
            await self.event_stream.append(
                session_id,
                "RUNTIME_INITIALIZED",
                {
                    "runtime_id": handle.get("runtime_id", runtime_pref),
                    "l1_session_id": l1_sid,
                },
            )
            await self._update_status(session_id, "active")
        except L1RuntimeError as exc:
            await self.event_stream.append(
                session_id,
                "RUNTIME_INITIALIZE_FAILED",
                {"runtime_id": runtime_pref, "error": str(exc)},
            )
            await self._update_status(session_id, "failed")
            raise

        return {
            "session_id": session_id,
            "status": "active",
            "payload": payload,
        }

    # ─── Contract 5 (partial): send_message ──────────────────────────────────
    async def send_message(self, session_id: str, content: str) -> dict[str, Any]:
        """Send a user message to L1 via gRPC Send (server-streaming).

        Appends USER_MESSAGE event, then streams L1 response chunks and
        records each as a RESPONSE_CHUNK event. Returns the assembled
        response and event list.
        """
        await self._require_session(session_id)

        seq_user = await self.event_stream.append(
            session_id,
            "USER_MESSAGE",
            {"content": content},
        )

        l1 = self._l1_clients.get(session_id)
        if l1 is None:
            # Session was created but L1 client was lost (e.g. server restart).
            # Re-create client from session's runtime_id.
            session_info = await self.get_session(session_id)
            runtime_id = session_info.get("runtime_id", "grid-runtime")
            l1 = self._l1_factory(runtime_id)
            self._l1_clients[session_id] = l1

        # Use L1's own session_id (may differ from L4's).
        l1_sid = self._l1_session_ids.get(session_id, session_id)

        chunks: list[dict[str, Any]] = []
        full_text_parts: list[str] = []
        events: list[dict[str, Any]] = [
            {"seq": seq_user, "event_type": "USER_MESSAGE"},
        ]

        try:
            async for chunk in l1.send(l1_sid, content):
                chunks.append(chunk)
                if chunk.get("chunk_type") == "text_delta":
                    full_text_parts.append(chunk.get("content", ""))

                seq = await self.event_stream.append(
                    session_id,
                    "RESPONSE_CHUNK",
                    chunk,
                )
                events.append({"seq": seq, "event_type": "RESPONSE_CHUNK", **chunk})
        except L1RuntimeError as exc:
            seq_err = await self.event_stream.append(
                session_id,
                "RUNTIME_SEND_FAILED",
                {"error": str(exc)},
            )
            events.append({"seq": seq_err, "event_type": "RUNTIME_SEND_FAILED"})
            raise

        return {
            "session_id": session_id,
            "response_text": "".join(full_text_parts),
            "chunks": chunks,
            "events": events,
        }

    # ─── Contract 5 (partial): stream_message (SSE-friendly) ──────────────────
    async def stream_message(
        self, session_id: str, content: str
    ) -> AsyncIterator[dict[str, Any]]:
        """Send a user message and yield each response chunk as it arrives.

        Unlike ``send_message`` which buffers everything, this is an async
        generator that yields dicts suitable for SSE serialisation:

            {"event": "chunk", "data": {chunk_type, content, ...}}
            {"event": "done",  "data": {response_text, events}}
        """
        await self._require_session(session_id)

        seq_user = await self.event_stream.append(
            session_id,
            "USER_MESSAGE",
            {"content": content},
        )

        l1 = self._l1_clients.get(session_id)
        if l1 is None:
            session_info = await self.get_session(session_id)
            runtime_id = session_info.get("runtime_id", "grid-runtime")
            l1 = self._l1_factory(runtime_id)
            self._l1_clients[session_id] = l1

        l1_sid = self._l1_session_ids.get(session_id, session_id)

        full_text_parts: list[str] = []
        events: list[dict[str, Any]] = [
            {"seq": seq_user, "event_type": "USER_MESSAGE"},
        ]

        try:
            async for chunk in l1.send(l1_sid, content):
                if chunk.get("chunk_type") == "text_delta":
                    full_text_parts.append(chunk.get("content", ""))

                seq = await self.event_stream.append(
                    session_id,
                    "RESPONSE_CHUNK",
                    chunk,
                )
                events.append({"seq": seq, "event_type": "RESPONSE_CHUNK", **chunk})

                yield {"event": "chunk", "data": chunk}
        except L1RuntimeError as exc:
            seq_err = await self.event_stream.append(
                session_id,
                "RUNTIME_SEND_FAILED",
                {"error": str(exc)},
            )
            events.append({"seq": seq_err, "event_type": "RUNTIME_SEND_FAILED"})
            yield {
                "event": "error",
                "data": {"error": str(exc), "runtime_id": exc.runtime_id},
            }
            return

        yield {
            "event": "done",
            "data": {
                "session_id": session_id,
                "response_text": "".join(full_text_parts),
                "events": events,
            },
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

    # ─── Contract 5: close session ──────────────────────────────────────────
    async def close_session(self, session_id: str) -> dict[str, Any]:
        """Gracefully close a session: terminate L1 + update status."""
        session = await self.get_session(session_id)
        current = session["status"]
        if current not in ("created", "active"):
            raise InvalidStateTransition(session_id, current, "closed")

        # Terminate L1 runtime if active.
        l1 = self._l1_clients.pop(session_id, None)
        if l1 is not None:
            try:
                await l1.terminate()
            except L1RuntimeError:
                pass  # Best-effort terminate; session closes regardless.
            finally:
                await l1.close()

        await self._update_status(session_id, "closed")
        await self.event_stream.append(
            session_id,
            "SESSION_CLOSED",
            {"previous_status": current},
        )
        return {"session_id": session_id, "status": "closed"}

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

    async def _update_status(
        self, session_id: str, new_status: str
    ) -> None:
        """Update session status + closed_at if terminal."""
        db = await connect(self.db_path)
        try:
            closed_at = int(time.time()) if new_status in ("closed", "failed") else None
            await db.execute("BEGIN IMMEDIATE")
            await db.execute(
                """
                UPDATE sessions SET status = ?, closed_at = ?
                WHERE session_id = ?
                """,
                (new_status, closed_at, session_id),
            )
            await db.commit()
        finally:
            await db.close()
