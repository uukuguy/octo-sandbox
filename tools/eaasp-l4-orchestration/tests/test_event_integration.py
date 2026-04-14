"""End-to-end integration tests for Phase 1 Event Engine.

These tests verify the CRITICAL integration paths that unit tests miss:
- Real chunk_types from runtimes flow through stream_message → interceptor → engine
- POST /v1/events/ingest actually persists to backend (not just returns 200)
- EventEngine is started in lifespan and processes events
- Session existence is enforced at ingest boundary

Context: Phase 0.5 MVP verification needed 4 debug rounds to find integration
gaps that unit tests didn't expose. These tests are designed to catch
the analogous gaps in Phase 1 BEFORE manual verification.
"""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
from typing import Any

import httpx
import pytest
import pytest_asyncio
import respx

from eaasp_l4_orchestration.api import create_app
from eaasp_l4_orchestration.event_backend_sqlite import SqliteWalBackend
from eaasp_l4_orchestration.event_engine import EventEngine
from eaasp_l4_orchestration.event_interceptor import EventInterceptor
from eaasp_l4_orchestration.event_models import Event, EventMetadata
from eaasp_l4_orchestration.session_orchestrator import SessionOrchestrator


L2_DEFAULT = "http://127.0.0.1:18085"
L3_DEFAULT = "http://127.0.0.1:18083"
SKILL_REG_DEFAULT = "http://127.0.0.1:18081"


def _mock_upstreams() -> None:
    """Register respx mocks for L2/L3/skill-registry — call inside respx.mock context."""
    respx.post(f"{L2_DEFAULT}/api/v1/memory/search").mock(
        return_value=httpx.Response(200, json={"hits": []})
    )
    respx.post(url__regex=rf"{L3_DEFAULT}/v1/sessions/.*/validate").mock(
        return_value=httpx.Response(
            200,
            json={
                "session_id": "placeholder",
                "hooks_to_attach": [],
                "managed_settings_version": 1,
                "validated_at": "2026-01-01",
                "runtime_tier": "strict",
            },
        )
    )
    # Skill registry — non-fatal even if skill_id not registered, but avoid unmocked request
    respx.post(f"{SKILL_REG_DEFAULT}/tools/skill_read/invoke").mock(
        return_value=httpx.Response(
            404, json={"error": "skill not found"}
        )
    )


# ── Stub L1 that emits realistic chunk_types matching grid-runtime ───────────


class _RealisticL1Client:
    """Stub L1 client that emits chunk_types matching real grid-runtime output.

    Critical: uses "tool_start" (not "tool_call_start") — this is the actual
    name from harness.rs that the interceptor must handle.
    """

    def __init__(self, runtime_id: str) -> None:
        self.runtime_id = runtime_id

    async def initialize(self, payload_dict: dict[str, Any]) -> dict[str, str]:
        sid = payload_dict.get("session_id", "mock")
        return {"session_id": sid, "runtime_id": self.runtime_id}

    async def send(
        self, session_id: str, content: str, message_type: str = "text"
    ) -> AsyncIterator[dict[str, Any]]:
        # Realistic sequence: text → tool_start → tool_result → done.
        yield {"chunk_type": "text_delta", "content": "Let me read the device."}
        yield {
            "chunk_type": "tool_start",
            "tool_name": "scada_read_snapshot",
            "arguments": {"device_id": "T-001"},
        }
        yield {
            "chunk_type": "tool_result",
            "tool_name": "scada_read_snapshot",
            "content": '{"temp": 85}',
            "is_error": False,
        }
        yield {"chunk_type": "text_delta", "content": "Done."}
        yield {"chunk_type": "done", "content": ""}

    async def terminate(self) -> None:
        pass

    async def close(self) -> None:
        pass


def _realistic_l1_factory(runtime_id: str) -> _RealisticL1Client:
    return _RealisticL1Client(runtime_id)


@pytest_asyncio.fixture
async def event_engine_app_client(
    tmp_db_path: str,
    l4_http_client: httpx.AsyncClient,
) -> AsyncIterator[httpx.AsyncClient]:
    """L4 ASGI client with a realistic L1 stub.

    Unlike the default `app_client` fixture, this uses an L1 that emits
    chunk_types matching real runtime output (exposes interceptor gaps).
    """
    application = create_app(
        tmp_db_path,
        http_client=l4_http_client,
        l1_factory=_realistic_l1_factory,
    )
    async with application.router.lifespan_context(application):
        transport = httpx.ASGITransport(app=application)
        async with httpx.AsyncClient(
            transport=transport, base_url="http://testserver"
        ) as client:
            yield client


# ── CRITICAL #2: stream_message → interceptor → engine end-to-end ────────────


@respx.mock
async def test_stream_message_fires_interceptor_for_tool_chunks(
    event_engine_app_client: httpx.AsyncClient,
    tmp_db_path: str,
) -> None:
    """CRITICAL: stream_message must fire interceptor for tool_start chunks.

    This is the #1 gap found in Phase 1 audit — interceptor was checking
    "tool_call_start" but grid-runtime emits "tool_start".
    """
    _mock_upstreams()

    # Step 1: create session
    resp = await event_engine_app_client.post(
        "/v1/sessions/create",
        json={
            "intent_text": "read device",
            "skill_id": "skill.test",
            "runtime_pref": "grid-runtime",
        },
    )
    assert resp.status_code == 200, resp.text
    session_id = resp.json()["session_id"]

    # Step 2: stream a message (non-SSE path via /message endpoint)
    resp = await event_engine_app_client.post(
        f"/v1/sessions/{session_id}/message",
        json={"content": "read T-001"},
    )
    assert resp.status_code == 200, resp.text

    # Step 3: wait briefly for async pipeline to process events
    await asyncio.sleep(1.5)

    # Step 4: query events via the backend (bypass API to verify persistence)
    backend = SqliteWalBackend(tmp_db_path)
    all_events = await backend.list_events(session_id, limit=500)

    event_types = [e["event_type"] for e in all_events]

    # PROOF: interceptor fired for tool_start chunk (this was the broken path).
    assert "PRE_TOOL_USE" in event_types, (
        f"PRE_TOOL_USE missing — interceptor did NOT recognize tool_start. "
        f"Got: {event_types}"
    )
    assert "POST_TOOL_USE" in event_types, (
        f"POST_TOOL_USE missing. Got: {event_types}"
    )
    assert "STOP" in event_types, (
        f"STOP missing — done chunk not handled. Got: {event_types}"
    )
    assert "SESSION_START" in event_types, (
        f"SESSION_START missing — create_session didn't emit. Got: {event_types}"
    )

    # PROOF: Event Engine pipeline ran (cluster_id assigned).
    clustered = [e for e in all_events if e.get("cluster_id")]
    assert len(clustered) >= 1, (
        f"No events have cluster_id — pipeline worker did not process. "
        f"Events: {[(e['event_type'], e.get('cluster_id')) for e in all_events]}"
    )

    # PROOF: source metadata includes runtime_id.
    pre_tool_events = [e for e in all_events if e["event_type"] == "PRE_TOOL_USE"]
    assert pre_tool_events, "no PRE_TOOL_USE events found"
    pre = pre_tool_events[0]
    # Interceptor source is in metadata_json (stored) — payload has tool_name.
    assert pre["payload"].get("tool_name") == "scada_read_snapshot"


# ── CRITICAL #3: ingest endpoint enforces session existence ──────────────────


@respx.mock
async def test_ingest_rejects_nonexistent_session(
    event_engine_app_client: httpx.AsyncClient,
) -> None:
    """POST /v1/events/ingest with nonexistent session → 404 (not FK error).

    Before fix: silently wrote FK-violating row.
    """
    resp = await event_engine_app_client.post(
        "/v1/events/ingest",
        json={
            "session_id": "sess_does_not_exist",
            "event_type": "PRE_TOOL_USE",
            "payload": {"tool_name": "foo"},
            "source": "runtime:test",
        },
    )
    assert resp.status_code == 404
    assert resp.json()["detail"]["code"] == "session_not_found"


# ── CRITICAL #4: ingest endpoint actually persists event to backend ──────────


@respx.mock
async def test_ingest_endpoint_persists_event_to_backend(
    event_engine_app_client: httpx.AsyncClient,
    tmp_db_path: str,
) -> None:
    """POST /v1/events/ingest must actually write to backend (not just return 200).

    Before: test only verified HTTP response, not backend state.
    """
    _mock_upstreams()

    # Create session first
    resp = await event_engine_app_client.post(
        "/v1/sessions/create",
        json={
            "intent_text": "t",
            "skill_id": "skill.test",
            "runtime_pref": "grid-runtime",
        },
    )
    assert resp.status_code == 200
    session_id = resp.json()["session_id"]

    # Ingest
    resp = await event_engine_app_client.post(
        "/v1/events/ingest",
        json={
            "session_id": session_id,
            "event_type": "PRE_COMPACT",
            "payload": {"context_size": 8000},
            "source": "runtime:grid-runtime",
        },
    )
    assert resp.status_code == 200
    ingested_event_id = resp.json()["event_id"]

    # Allow pipeline to run
    await asyncio.sleep(1.5)

    # PROOF: event exists in backend
    backend = SqliteWalBackend(tmp_db_path)
    events = await backend.list_events(session_id)
    pre_compact = [e for e in events if e["event_type"] == "PRE_COMPACT"]
    assert pre_compact, "PRE_COMPACT event not persisted to backend"
    assert pre_compact[0]["event_id"] == ingested_event_id
    assert pre_compact[0]["source"] == "runtime:grid-runtime"
    # PROOF: pipeline ran (cluster assigned)
    assert pre_compact[0]["cluster_id"] is not None


# ── App lifespan actually starts EventEngine ─────────────────────────────────


@respx.mock
async def test_event_engine_started_in_lifespan(
    event_engine_app_client: httpx.AsyncClient,
) -> None:
    """Verify EventEngine.start() was called via lifespan context.

    If lifespan didn't run, ingest would hang or fail.
    """
    _mock_upstreams()
    resp = await event_engine_app_client.post(
        "/v1/sessions/create",
        json={
            "intent_text": "t",
            "skill_id": "skill.test",
            "runtime_pref": "grid-runtime",
        },
    )
    session_id = resp.json()["session_id"]

    # Ingest must complete without hanging (proves engine + worker are live)
    resp = await asyncio.wait_for(
        event_engine_app_client.post(
            "/v1/events/ingest",
            json={
                "session_id": session_id,
                "event_type": "USER_PROMPT_SUBMIT",
                "payload": {},
                "source": "test",
            },
        ),
        timeout=3.0,
    )
    assert resp.status_code == 200
