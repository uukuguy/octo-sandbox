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
import logging
import time
import uuid
from collections.abc import AsyncIterator
from typing import Any

logger = logging.getLogger(__name__)

from .context_assembly import build_session_payload
from .db import connect
from .event_engine import EventEngine
from .event_interceptor import EventInterceptor
from .event_stream import SessionEventStream
from .handshake import L2Client, L3Client, SkillRegistryClient
from .l1_client import L1RuntimeClient, L1RuntimeError, create_l1_client
from .mcp_resolver import McpResolver, McpResolveError


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
        skill_registry: SkillRegistryClient | None = None,
        l1_factory: Any | None = None,
        mcp_resolver: McpResolver | None = None,
        event_engine: EventEngine | None = None,
        event_interceptor: EventInterceptor | None = None,
    ) -> None:
        self.db_path = db_path
        self.l2 = l2
        self.l3 = l3
        self.skill_registry = skill_registry
        self.event_stream = event_stream
        # l1_factory: callable(runtime_id) → L1RuntimeClient.
        # Default: create_l1_client. Tests can inject a mock factory.
        self._l1_factory = l1_factory or create_l1_client
        # McpResolver for wiring MCP servers after L1 Initialize.
        self.mcp_resolver = mcp_resolver
        # Phase 1 Event Engine — optional, None degrades to Phase 0.5 behavior.
        self.event_engine = event_engine
        self.event_interceptor = event_interceptor or EventInterceptor()
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

        # Step 2b — P4: Fetch skill content from Skill Registry.
        skill_instructions: dict[str, Any] = {
            "skill_id": skill_id,
            "name": "",
            "content": "",
            "frontmatter_hooks": [],
            "metadata": {},
            "dependencies": [],
        }
        import logging as _log
        _logger = _log.getLogger(__name__)
        _logger.info("skill_registry=%s, skill_id=%s", self.skill_registry, skill_id)
        if self.skill_registry is not None:
            try:
                skill_data = await self.skill_registry.read_skill(skill_id)
                _logger.info("skill_data keys: %s, prose_len=%d", list(skill_data.keys()), len(skill_data.get("prose", "")))
                # skill_data shape: {meta, frontmatter_yaml, prose, parsed_v2?}
                parsed_v2 = skill_data.get("parsed_v2") or {}
                scoped_hooks = parsed_v2.get("scoped_hooks") or {}
                # Flatten scoped hooks into the list format context_assembly expects.
                # Resolve ${SKILL_DIR} using skill_dir from L2 registry.
                skill_dir = skill_data.get("skill_dir") or ""
                frontmatter_hooks: list[dict[str, Any]] = []
                for scope in ("PreToolUse", "PostToolUse", "Stop"):
                    for hook in scoped_hooks.get(scope) or scoped_hooks.get(scope.lower()) or []:
                        resolved_hook = dict(hook)
                        # Substitute ${SKILL_DIR} in command hooks.
                        if "command" in resolved_hook and skill_dir:
                            resolved_hook["command"] = resolved_hook["command"].replace(
                                "${SKILL_DIR}", skill_dir
                            )
                        if "prompt" in resolved_hook and skill_dir:
                            resolved_hook["prompt"] = resolved_hook["prompt"].replace(
                                "${SKILL_DIR}", skill_dir
                            )
                        frontmatter_hooks.append({**resolved_hook, "scope": scope})

                skill_instructions = {
                    "skill_id": skill_id,
                    "name": skill_data.get("meta", {}).get("name", skill_id),
                    "content": skill_data.get("prose", ""),
                    "frontmatter_hooks": frontmatter_hooks,
                    "metadata": {},
                    "dependencies": parsed_v2.get("dependencies") or [],
                }
            except Exception as exc:
                # Skill fetch failure is non-fatal for MVP — log and continue
                # with empty instructions. Agent will run without skill context.
                import logging, traceback
                logging.getLogger(__name__).error(
                    "Failed to fetch skill '%s' from registry: %s\n%s",
                    skill_id, exc, traceback.format_exc(),
                )

        # Step 3 — assemble payload (P1..P5 + budget flags).
        payload = build_session_payload(
            session_id=session_id,
            user_id=user_id or "",
            runtime_id=runtime_pref,
            policy_context=policy_context,
            event_context=None,
            memory_refs=memory_refs,
            skill_instructions=skill_instructions,
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

            # Phase 1: Interceptor emits SESSION_START event via Event Engine.
            if self.event_engine is not None:
                start_event = self.event_interceptor.create_session_start(
                    session_id, runtime_pref
                )
                await self.event_engine.ingest(start_event)
        except L1RuntimeError as exc:
            await self.event_stream.append(
                session_id,
                "RUNTIME_INITIALIZE_FAILED",
                {"runtime_id": runtime_pref, "error": str(exc)},
            )
            await self._update_status(session_id, "failed")
            raise

        # Step 6 — ConnectMCP: wire MCP servers from skill dependencies.
        # skill_instructions["dependencies"] was populated in Step 2b from
        # the skill registry (or left as [] when registry is absent).
        skill_deps = skill_instructions.get("dependencies") or []
        if self.mcp_resolver and skill_deps:
            mcp_deps = [d for d in skill_deps if d.startswith("mcp:")]
            if mcp_deps:
                try:
                    servers = await self.mcp_resolver.resolve(
                        mcp_deps, runtime_id=runtime_pref,
                    )
                    if servers:
                        l1_sid = self._l1_session_ids[session_id]
                        mcp_result = await l1.connect_mcp(l1_sid, servers)
                        await self.event_stream.append(
                            session_id,
                            "SESSION_MCP_CONNECTED",
                            {
                                "connected": mcp_result.get("connected", []),
                                "failed": mcp_result.get("failed", []),
                            },
                        )
                        logger.info(
                            "ConnectMCP: session=%s connected=%s failed=%s",
                            session_id,
                            mcp_result.get("connected"),
                            mcp_result.get("failed"),
                        )
                except (McpResolveError, Exception) as exc:
                    # MCP connection failure is non-fatal — session remains active.
                    logger.warning(
                        "ConnectMCP failed for session %s: %s (non-fatal)",
                        session_id, exc,
                    )
                    await self.event_stream.append(
                        session_id,
                        "SESSION_MCP_CONNECT_FAILED",
                        {"error": str(exc)},
                    )

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

        # Resolve runtime_id for interceptor source tagging (Phase 1).
        _runtime_id = ""
        if self.event_engine is not None:
            try:
                _sess_info = await self.get_session(session_id)
                _runtime_id = _sess_info.get("runtime_id", "")
            except Exception:
                pass

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

                # Phase 1: Interceptor extracts lifecycle events from chunks.
                # Must also fire in send_message (not just stream_message) —
                # both endpoints call L1.send().
                if self.event_engine is not None:
                    extracted = self.event_interceptor.extract_from_chunk(
                        session_id, chunk, runtime_id=_runtime_id
                    )
                    if extracted:
                        await self.event_engine.ingest(extracted)
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

        # Resolve runtime_id for interceptor source tagging.
        _runtime_id = ""
        try:
            _sess_info = await self.get_session(session_id)
            _runtime_id = _sess_info.get("runtime_id", "")
        except Exception:
            pass

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

                # Phase 1: Interceptor extracts lifecycle events from chunks.
                if self.event_engine is not None:
                    extracted = self.event_interceptor.extract_from_chunk(
                        session_id, chunk, runtime_id=_runtime_id
                    )
                    if extracted:
                        await self.event_engine.ingest(extracted)

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

        # Phase 1: Interceptor emits POST_SESSION_END event via Event Engine.
        if self.event_engine is not None:
            end_event = self.event_interceptor.create_session_end(session_id)
            await self.event_engine.ingest(end_event)

        return {"session_id": session_id, "status": "closed"}

    # ─── List sessions (D41) ──────────────────────────────────────────────
    async def list_sessions(
        self,
        *,
        limit: int = 50,
        status: str | None = None,
    ) -> list[dict[str, Any]]:
        """Return sessions ordered by created_at DESC, optionally filtered."""
        db = await connect(self.db_path)
        try:
            if status is not None:
                cur = await db.execute(
                    """
                    SELECT session_id, status, runtime_id, skill_id,
                           created_at, closed_at
                    FROM sessions
                    WHERE status = ?
                    ORDER BY created_at DESC
                    LIMIT ?
                    """,
                    (status, limit),
                )
            else:
                cur = await db.execute(
                    """
                    SELECT session_id, status, runtime_id, skill_id,
                           created_at, closed_at
                    FROM sessions
                    ORDER BY created_at DESC
                    LIMIT ?
                    """,
                    (limit,),
                )
            rows = await cur.fetchall()
        finally:
            await db.close()

        return [
            {
                "session_id": row["session_id"],
                "status": row["status"],
                "runtime_id": row["runtime_id"],
                "skill_id": row["skill_id"],
                "created_at": int(row["created_at"]),
                "closed_at": (
                    int(row["closed_at"]) if row["closed_at"] is not None else None
                ),
            }
            for row in rows
        ]

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
