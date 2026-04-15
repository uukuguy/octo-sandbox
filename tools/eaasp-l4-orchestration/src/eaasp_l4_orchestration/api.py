"""FastAPI app exposing L4 orchestration REST surface.

Endpoints (MVP + Phase 1 Event Engine):

- ``GET  /health``                                    — liveness probe
- ``POST /v1/intents/dispatch``                       — Contract 2 intent dispatch
- ``POST /v1/sessions/create``                        — Contract 5 handshake (alias)
- ``POST /v1/sessions/{session_id}/message``          — append user message
- ``POST /v1/sessions/{session_id}/message/stream``   — SSE streaming message
- ``GET  /v1/sessions/{session_id}/events``           — list events (+ follow SSE)
- ``GET  /v1/sessions/{session_id}``                  — fetch session + payload
- ``GET  /v1/sessions``                               — list all sessions
- ``POST /v1/events/ingest``                          — Phase 1: L1 EmitEvent REST fallback
"""

from __future__ import annotations

import asyncio
import json
import logging
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

import httpx
from fastapi import Depends, FastAPI, HTTPException, Query
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, Field, ValidationError

from .db import init_db
from .event_backend_sqlite import SqliteWalBackend
from .event_engine import EventEngine
from .event_models import Event, EventMetadata
from .event_stream import SessionEventStream
from .handshake import (
    L2_URL_DEFAULT,
    L3_URL_DEFAULT,
    SKILL_REGISTRY_URL_DEFAULT,
    L2Client,
    L3Client,
    SkillRegistryClient,
    UpstreamError,
)
from .l1_client import L1RuntimeError
from .mcp_resolver import McpResolver
from .session_orchestrator import (
    InvalidStateTransition,
    SessionNotFound,
    SessionOrchestrator,
)

logger = logging.getLogger(__name__)

# ─── Request models ─────────────────────────────────────────────────────────


class IntentDispatchRequest(BaseModel):
    intent_text: str = Field(..., min_length=1)
    skill_id: str = Field(..., min_length=1)
    runtime_pref: str = Field(..., min_length=1)
    user_id: str | None = None
    intent_id: str | None = None


class SendMessageRequest(BaseModel):
    content: str = Field(..., min_length=0)


class EventIngestRequest(BaseModel):
    """Phase 1: L1 EmitEvent REST fallback (ADR-V2-001)."""
    session_id: str = Field(..., min_length=1)
    event_type: str = Field(..., min_length=1)
    payload: dict[str, Any] = Field(default_factory=dict)
    source: str = Field(default="")
    event_id: str | None = None


# ─── App factory ────────────────────────────────────────────────────────────


def create_app(
    db_path: str,
    *,
    l2_base_url: str | None = None,
    l3_base_url: str | None = None,
    skill_registry_base_url: str | None = None,
    http_client: httpx.AsyncClient | None = None,
    l1_factory: Any | None = None,
) -> FastAPI:
    """Build the FastAPI app.

    ``http_client`` is injectable for tests — when None the lifespan builds
    its own ``httpx.AsyncClient`` with a 5s timeout. Tests override this via
    the ``l4_http_client`` fixture so respx can intercept requests.
    """

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        await init_db(db_path)
        owned_client = False
        if http_client is None:
            # trust_env=False prevents L4 from picking up macOS system proxies
            # (Clash etc.) when calling L2/L3 over 127.0.0.1. Without this guard
            # the proxy turns localhost calls into 502 upstream_error reports —
            # see MEMORY.md "Ollama 本地模型已知问题" for the reqwest precedent.
            client = httpx.AsyncClient(timeout=5.0, trust_env=False)
            owned_client = True
        else:
            client = http_client
        app.state.http_client = client
        app.state.l2 = L2Client(client, base_url=l2_base_url or L2_URL_DEFAULT)
        app.state.l3 = L3Client(client, base_url=l3_base_url or L3_URL_DEFAULT)
        app.state.skill_registry = SkillRegistryClient(
            client, base_url=skill_registry_base_url or SKILL_REGISTRY_URL_DEFAULT
        )
        app.state.event_stream = SessionEventStream(db_path)
        app.state.mcp_resolver = McpResolver(client)
        # Phase 1: Event Engine with SqliteWalBackend.
        event_backend = SqliteWalBackend(db_path)
        event_engine = EventEngine(event_backend)
        await event_engine.start()
        app.state.event_engine = event_engine
        app.state.event_backend = event_backend
        app.state.orchestrator = SessionOrchestrator(
            db_path,
            l2=app.state.l2,
            l3=app.state.l3,
            skill_registry=app.state.skill_registry,
            event_stream=app.state.event_stream,
            l1_factory=l1_factory,
            mcp_resolver=app.state.mcp_resolver,
            event_engine=event_engine,
        )
        try:
            yield
        finally:
            await event_engine.stop()
            if owned_client:
                await client.aclose()

    app = FastAPI(
        title="EAASP L4 Orchestration",
        version="0.1.0",
        description=(
            "Thin L4 orchestration plane — Intent dispatch + Session handshake "
            "+ Event stream (MVP)"
        ),
        lifespan=lifespan,
    )

    def get_orchestrator() -> SessionOrchestrator:
        return app.state.orchestrator  # type: ignore[no-any-return]

    # ─── Health ──────────────────────────────────────────────────────────
    @app.get("/health")
    async def health() -> dict[str, str]:
        return {"status": "ok"}

    # ─── Contract 2: Intent dispatch ─────────────────────────────────────
    @app.post("/v1/intents/dispatch")
    async def dispatch_intent(
        body: IntentDispatchRequest,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        return await _run_create_session(orchestrator, body)

    # ─── Contract 5: Session create (alias — same body shape) ────────────
    @app.post("/v1/sessions/create")
    async def create_session(
        body: IntentDispatchRequest,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        return await _run_create_session(orchestrator, body)

    # ─── Contract 5: send message ────────────────────────────────────────
    @app.post("/v1/sessions/{session_id}/message")
    async def send_message(
        session_id: str,
        body: SendMessageRequest,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        try:
            return await orchestrator.send_message(session_id, body.content)
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={"code": "session_not_found", "session_id": exc.session_id},
            ) from exc
        except L1RuntimeError as exc:
            raise HTTPException(
                status_code=502,
                detail={
                    "code": "l1_runtime_error",
                    "runtime_id": exc.runtime_id,
                    "method": exc.method,
                    "detail": exc.detail,
                },
            ) from exc

    # ─── Contract 5: send message (SSE streaming) ─────────────────────────
    @app.post("/v1/sessions/{session_id}/message/stream")
    async def send_message_stream(
        session_id: str,
        body: SendMessageRequest,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> StreamingResponse:
        """SSE endpoint — streams response chunks as ``text/event-stream``."""
        # Validate session existence up front (fail fast with 404).
        try:
            await orchestrator._require_session(session_id)
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={"code": "session_not_found", "session_id": exc.session_id},
            ) from exc

        async def _sse_generator() -> AsyncIterator[str]:
            async for msg in orchestrator.stream_message(session_id, body.content):
                event = msg.get("event", "chunk")
                data = json.dumps(msg.get("data", {}), ensure_ascii=False, default=str)
                yield f"event: {event}\ndata: {data}\n\n"

        return StreamingResponse(
            _sse_generator(),
            media_type="text/event-stream",
            headers={
                "Cache-Control": "no-cache",
                "X-Accel-Buffering": "no",
            },
        )

    # ─── Contract 5: close session ───────────────────────────────────────
    @app.post("/v1/sessions/{session_id}/close")
    async def close_session(
        session_id: str,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        try:
            return await orchestrator.close_session(session_id)
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={"code": "session_not_found", "session_id": exc.session_id},
            ) from exc
        except InvalidStateTransition as exc:
            raise HTTPException(
                status_code=409,
                detail={
                    "code": "invalid_state_transition",
                    "session_id": exc.session_id,
                    "current": exc.current,
                    "target": exc.target,
                },
            ) from exc

    # ─── Contract 5: list events ─────────────────────────────────────────
    @app.get("/v1/sessions/{session_id}/events")
    async def list_events(
        session_id: str,
        from_: int = Query(default=1, ge=1, alias="from"),
        to: int = Query(default=2**31 - 1, ge=1),
        limit: int = Query(default=500, ge=1, le=500),
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        try:
            events = await orchestrator.list_events(
                session_id, from_seq=from_, to_seq=to, limit=limit
            )
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={"code": "session_not_found", "session_id": exc.session_id},
            ) from exc
        return {"session_id": session_id, "events": events}

    # ─── S4.T2 (D84): list events as SSE stream (follow mode) ────────────
    @app.get("/v1/sessions/{session_id}/events/stream")
    async def stream_events(
        session_id: str,
        from_: int = Query(default=1, ge=1, alias="from"),
        poll_interval_ms: int = Query(default=500, ge=50, le=5000),
        heartbeat_secs: int = Query(default=15, ge=1, le=120),
        max_idle_polls: int = Query(default=0, ge=0, le=10000),
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> StreamingResponse:
        """SSE tail-follow for session events.

        Emits each event as ``event: event\\ndata: <json>\\n\\n``. Replays all
        events with ``seq >= from`` then polls at ``poll_interval_ms``.
        Heartbeat comments (``: hb``) are emitted every ``heartbeat_secs`` to
        keep the connection alive and surface client disconnects quickly.

        ``max_idle_polls=0`` (default) polls forever. Set a positive value to
        terminate the stream after that many consecutive empty polls — useful
        for "catch up then exit" workflows and for tests running under an
        ASGI transport that buffers the full response body.
        """
        # Validate session up front (fail fast with 404 instead of in-stream).
        try:
            await orchestrator._require_session(session_id)
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={"code": "session_not_found", "session_id": exc.session_id},
            ) from exc

        async def _sse_generator() -> AsyncIterator[str]:
            # D124 (S4.T2 reviewer note): log follow-stream lifecycle for ops
            # visibility. Starlette detects client disconnect via is_disconnected()
            # polling in the response-sender task and raises CancelledError inside
            # our await points — we catch it here, log, and re-raise so Starlette
            # can complete its cleanup.
            logger.info(
                "sse_follow_start session_id=%s from=%s", session_id, from_
            )
            last_seen = from_ - 1
            last_heartbeat = asyncio.get_event_loop().time()
            poll_s = poll_interval_ms / 1000.0
            idle_polls = 0
            try:
                while True:
                    try:
                        events = await orchestrator.list_events(
                            session_id,
                            from_seq=last_seen + 1,
                            limit=500,
                        )
                    except SessionNotFound:
                        # Session deleted mid-stream — emit a terminal error frame
                        # and break rather than raising (can't raise inside a
                        # started StreamingResponse body).
                        logger.info(
                            "sse_follow_session_gone session_id=%s last_seen=%s",
                            session_id,
                            last_seen,
                        )
                        yield (
                            "event: error\n"
                            "data: "
                            + json.dumps(
                                {
                                    "code": "session_not_found",
                                    "session_id": session_id,
                                }
                            )
                            + "\n\n"
                        )
                        return
                    if events:
                        idle_polls = 0
                        for ev in events:
                            payload = json.dumps(ev, ensure_ascii=False, default=str)
                            yield f"event: event\ndata: {payload}\n\n"
                            last_seen = int(ev["seq"])
                    else:
                        idle_polls += 1
                        if max_idle_polls and idle_polls >= max_idle_polls:
                            logger.debug(
                                "sse_follow_idle_exit session_id=%s last_seen=%s",
                                session_id,
                                last_seen,
                            )
                            return
                    now = asyncio.get_event_loop().time()
                    if now - last_heartbeat >= heartbeat_secs:
                        yield ": hb\n\n"
                        last_heartbeat = now
                    await asyncio.sleep(poll_s)
            except asyncio.CancelledError:
                logger.info(
                    "sse_follow_disconnect session_id=%s last_seen=%s",
                    session_id,
                    last_seen,
                )
                raise

        return StreamingResponse(
            _sse_generator(),
            media_type="text/event-stream",
            headers={
                "Cache-Control": "no-cache",
                "X-Accel-Buffering": "no",
            },
        )

    # ─── Contract 5: get session ─────────────────────────────────────────
    @app.get("/v1/sessions/{session_id}")
    async def get_session(
        session_id: str,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        try:
            return await orchestrator.get_session(session_id)
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={"code": "session_not_found", "session_id": exc.session_id},
            ) from exc

    # ─── List sessions (closes D41) ─────────────────────────────────────
    @app.get("/v1/sessions")
    async def list_sessions(
        limit: int = Query(default=50, ge=1, le=500),
        status: str | None = Query(default=None),
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        """List all sessions, newest first."""
        rows = await orchestrator.list_sessions(limit=limit, status=status)
        return {"sessions": rows}

    # ─── Phase 1: Event ingest (ADR-V2-001 REST fallback) ──────────────
    @app.post("/v1/events/ingest")
    async def ingest_event(
        body: EventIngestRequest,
        orchestrator: SessionOrchestrator = Depends(get_orchestrator),
    ) -> dict[str, Any]:
        """Accept an event from L1 EmitEvent REST fallback.

        Validates that the session exists before accepting the event
        to prevent dangling FK rows in session_events.
        """
        # Validate session existence — prevents FK violation + silent failures.
        try:
            await orchestrator.get_session(body.session_id)
        except SessionNotFound as exc:
            raise HTTPException(
                status_code=404,
                detail={
                    "code": "session_not_found",
                    "session_id": exc.session_id,
                },
            ) from exc

        engine: EventEngine = app.state.event_engine
        event = Event(
            session_id=body.session_id,
            event_type=body.event_type,
            payload=body.payload,
            event_id=body.event_id or "",
            metadata=EventMetadata(source=body.source),
        )
        try:
            seq, event_id = await engine.ingest(event)
        except Exception as exc:
            raise HTTPException(
                status_code=400,
                detail={"code": "ingest_failed", "error": str(exc)},
            ) from exc
        return {"seq": seq, "event_id": event_id}

    return app


# ─── Shared handler ─────────────────────────────────────────────────────────


async def _run_create_session(
    orchestrator: SessionOrchestrator,
    body: IntentDispatchRequest,
) -> dict[str, Any]:
    """Call orchestrator.create_session and map upstream errors to HTTP."""
    try:
        return await orchestrator.create_session(
            intent_text=body.intent_text,
            skill_id=body.skill_id,
            runtime_pref=body.runtime_pref,
            user_id=body.user_id,
            intent_id=body.intent_id,
        )
    except ValidationError as exc:
        raise HTTPException(
            status_code=422, detail=_sanitize_errors(exc.errors())
        ) from exc
    except UpstreamError as exc:
        raise _upstream_to_http(exc) from exc
    except L1RuntimeError as exc:
        raise HTTPException(
            status_code=502,
            detail={
                "code": "l1_runtime_error",
                "runtime_id": exc.runtime_id,
                "method": exc.method,
                "detail": exc.detail,
            },
        ) from exc


def _upstream_to_http(exc: UpstreamError) -> HTTPException:
    """Map ``UpstreamError`` into an HTTP status code + payload."""
    if exc.kind == "unavailable":
        return HTTPException(
            status_code=503,
            detail={
                "code": "upstream_unavailable",
                "service": exc.service,
                "detail": exc.detail,
            },
        )
    if exc.kind == "no_policy":
        # 424 Failed Dependency — L3 has no managed-settings version yet.
        return HTTPException(
            status_code=424,
            detail={
                "code": "no_policy",
                "service": exc.service,
                "message": exc.detail
                or "no managed-settings version has been deployed yet",
            },
        )
    # Default: upstream 5xx / unexpected.
    return HTTPException(
        status_code=502,
        detail={
            "code": "upstream_error",
            "service": exc.service,
            "detail": exc.detail,
        },
    )


def _sanitize_errors(errors: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Strip non-JSON-serializable objects from Pydantic error dicts."""
    clean: list[dict[str, Any]] = []
    for err in errors:
        safe: dict[str, Any] = {}
        for key, value in err.items():
            if key == "ctx" and isinstance(value, dict):
                safe[key] = {
                    ctx_key: (
                        str(ctx_val)
                        if isinstance(ctx_val, BaseException)
                        else ctx_val
                    )
                    for ctx_key, ctx_val in value.items()
                }
            elif isinstance(value, BaseException):
                safe[key] = str(value)
            else:
                safe[key] = value
        clean.append(safe)
    return clean
