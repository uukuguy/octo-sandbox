"""Tests for SessionOrchestrator (three-way handshake + send_message)."""

from __future__ import annotations

import httpx
import pytest
import respx

from eaasp_l4_orchestration.event_stream import SessionEventStream
from eaasp_l4_orchestration.handshake import L2Client, L3Client, UpstreamError
from eaasp_l4_orchestration.session_orchestrator import (
    SessionNotFound,
    SessionOrchestrator,
)

L2_BASE = "http://l2.test"
L3_BASE = "http://l3.test"


async def _make_orchestrator(
    tmp_db_path: str, http_client: httpx.AsyncClient
) -> SessionOrchestrator:
    l2 = L2Client(http_client, base_url=L2_BASE)
    l3 = L3Client(http_client, base_url=L3_BASE)
    stream = SessionEventStream(tmp_db_path)
    return SessionOrchestrator(tmp_db_path, l2=l2, l3=l3, event_stream=stream)


@respx.mock
async def test_create_session_happy_path(tmp_db_path: str) -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        return_value=httpx.Response(
            200,
            json={
                "hits": [
                    {"memory_id": "m1", "memory_type": "anchor", "score": 0.9},
                    {"memory_id": "m2", "memory_type": "file", "score": 0.6},
                ]
            },
        )
    )
    respx.post(url__regex=rf"{L3_BASE}/v1/sessions/.*/validate").mock(
        return_value=httpx.Response(
            200,
            json={
                "session_id": "placeholder",
                "hooks_to_attach": [
                    {"hook_id": "h1", "phase": "PreToolUse", "mode": "enforce"}
                ],
                "managed_settings_version": 3,
                "validated_at": "2026-04-12 01:00:00",
                "runtime_tier": "strict",
            },
        )
    )

    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        out = await orch.create_session(
            intent_text="hire a new SDE",
            skill_id="skill.hr.onboard",
            runtime_pref="strict",
            user_id="u-1",
        )

    assert out["session_id"].startswith("sess_")
    assert out["status"] == "created"
    payload = out["payload"]
    assert len(payload["memory_refs"]) == 2
    assert payload["memory_refs"][0]["memory_id"] == "m1"
    assert len(payload["policy_context"]["hooks"]) == 1
    assert payload["policy_context"]["policy_version"] == "3"
    # Sessions row persisted as "created".
    fetched = await orch.get_session(out["session_id"])
    assert fetched["status"] == "created"
    # Boot events present: SESSION_CREATED + RUNTIME_INITIALIZE_STUBBED.
    events = await orch.list_events(out["session_id"])
    types = [e["event_type"] for e in events]
    assert "SESSION_CREATED" in types
    assert "RUNTIME_INITIALIZE_STUBBED" in types
    # N3 (reviewer): enforce boot-event ordering — SESSION_CREATED must
    # always land at a lower seq than RUNTIME_INITIALIZE_STUBBED so that
    # consumers replaying the stream see handshake completion before the
    # runtime stub marker.
    seq_created = next(
        e["seq"] for e in events if e["event_type"] == "SESSION_CREATED"
    )
    seq_init = next(
        e["seq"] for e in events if e["event_type"] == "RUNTIME_INITIALIZE_STUBBED"
    )
    assert seq_created < seq_init


@respx.mock
async def test_create_session_l2_unavailable(tmp_db_path: str) -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        side_effect=httpx.ConnectError("no l2")
    )
    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        with pytest.raises(UpstreamError) as exc_info:
            await orch.create_session(
                intent_text="x",
                skill_id="skill.s",
                runtime_pref="strict",
            )
    assert exc_info.value.service == "l2"
    assert exc_info.value.kind == "unavailable"


@respx.mock
async def test_create_session_l3_no_policy(tmp_db_path: str) -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        return_value=httpx.Response(200, json={"hits": []})
    )
    respx.post(url__regex=rf"{L3_BASE}/v1/sessions/.*/validate").mock(
        return_value=httpx.Response(
            404, json={"detail": {"code": "no_policy", "message": "empty"}}
        )
    )
    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        with pytest.raises(UpstreamError) as exc_info:
            await orch.create_session(
                intent_text="x",
                skill_id="skill.s",
                runtime_pref="strict",
            )
    assert exc_info.value.service == "l3"
    assert exc_info.value.kind == "no_policy"


@respx.mock
async def test_send_message_happy_path(tmp_db_path: str) -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        return_value=httpx.Response(200, json={"hits": []})
    )
    respx.post(url__regex=rf"{L3_BASE}/v1/sessions/.*/validate").mock(
        return_value=httpx.Response(
            200,
            json={
                "session_id": "placeholder",
                "hooks_to_attach": [],
                "managed_settings_version": 1,
                "validated_at": "2026-04-12 01:00:00",
                "runtime_tier": "strict",
            },
        )
    )
    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        created = await orch.create_session(
            intent_text="x", skill_id="skill.s", runtime_pref="strict"
        )
        sid = created["session_id"]
        result = await orch.send_message(sid, "hello world")

    assert result["session_id"] == sid
    assert result["seq"] > 0
    assert any(e["event_type"] == "USER_MESSAGE" for e in result["events"])

    events = await orch.list_events(sid)
    types = [e["event_type"] for e in events]
    assert "USER_MESSAGE" in types
    assert "RUNTIME_SEND_STUBBED" in types


async def test_send_message_unknown_session_raises(tmp_db_path: str) -> None:
    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        with pytest.raises(SessionNotFound):
            await orch.send_message("sess_nope", "hi")


async def test_get_session_unknown_raises(tmp_db_path: str) -> None:
    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        with pytest.raises(SessionNotFound):
            await orch.get_session("sess_nope")


async def test_list_events_unknown_raises(tmp_db_path: str) -> None:
    async with httpx.AsyncClient() as client:
        orch = await _make_orchestrator(tmp_db_path, client)
        with pytest.raises(SessionNotFound):
            await orch.list_events("sess_nope")
