"""Tests for L2Client + L3Client wrappers (respx-mocked)."""

from __future__ import annotations

import httpx
import pytest
import respx

from eaasp_l4_orchestration.handshake import L2Client, L3Client, UpstreamError

L2_BASE = "http://l2.test"
L3_BASE = "http://l3.test"


# ─── L2 ─────────────────────────────────────────────────────────────────────


@respx.mock
async def test_l2_search_memory_happy_path() -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        return_value=httpx.Response(
            200,
            json={
                "hits": [
                    {"memory_id": "m1", "memory_type": "anchor", "score": 0.9},
                    {"memory_id": "m2", "memory_type": "file", "score": 0.7},
                ]
            },
        )
    )
    async with httpx.AsyncClient() as client:
        l2 = L2Client(client, base_url=L2_BASE)
        hits = await l2.search_memory(query="hello", top_k=5)
    assert len(hits) == 2
    assert hits[0]["memory_id"] == "m1"


@respx.mock
async def test_l2_search_memory_connect_error() -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        side_effect=httpx.ConnectError("refused")
    )
    async with httpx.AsyncClient() as client:
        l2 = L2Client(client, base_url=L2_BASE)
        with pytest.raises(UpstreamError) as exc_info:
            await l2.search_memory(query="hello")
    assert exc_info.value.service == "l2"
    assert exc_info.value.kind == "unavailable"


@respx.mock
async def test_l2_search_memory_server_error() -> None:
    respx.post(f"{L2_BASE}/api/v1/memory/search").mock(
        return_value=httpx.Response(500, text="boom")
    )
    async with httpx.AsyncClient() as client:
        l2 = L2Client(client, base_url=L2_BASE)
        with pytest.raises(UpstreamError) as exc_info:
            await l2.search_memory(query="hello")
    assert exc_info.value.service == "l2"
    assert exc_info.value.kind == "error"


# ─── L3 ─────────────────────────────────────────────────────────────────────


@respx.mock
async def test_l3_validate_session_happy_path() -> None:
    respx.post(f"{L3_BASE}/v1/sessions/sess_x/validate").mock(
        return_value=httpx.Response(
            200,
            json={
                "session_id": "sess_x",
                "hooks_to_attach": [
                    {"hook_id": "h1", "phase": "PreToolUse", "mode": "enforce"}
                ],
                "managed_settings_version": 7,
                "validated_at": "2026-04-12 00:00:00",
                "runtime_tier": "strict",
            },
        )
    )
    async with httpx.AsyncClient() as client:
        l3 = L3Client(client, base_url=L3_BASE)
        out = await l3.validate_session(
            session_id="sess_x",
            skill_id="skill.hr",
            runtime_tier="strict",
            agent_id="agent-a",
        )
    assert out["managed_settings_version"] == 7
    assert out["hooks_to_attach"][0]["hook_id"] == "h1"


@respx.mock
async def test_l3_validate_session_404_no_policy() -> None:
    respx.post(f"{L3_BASE}/v1/sessions/sess_x/validate").mock(
        return_value=httpx.Response(
            404, json={"detail": {"code": "no_policy", "message": "empty"}}
        )
    )
    async with httpx.AsyncClient() as client:
        l3 = L3Client(client, base_url=L3_BASE)
        with pytest.raises(UpstreamError) as exc_info:
            await l3.validate_session(
                session_id="sess_x", skill_id="skill.hr", runtime_tier="strict"
            )
    assert exc_info.value.kind == "no_policy"


@respx.mock
async def test_l3_validate_session_503_error() -> None:
    respx.post(f"{L3_BASE}/v1/sessions/sess_x/validate").mock(
        return_value=httpx.Response(503, text="unavailable")
    )
    async with httpx.AsyncClient() as client:
        l3 = L3Client(client, base_url=L3_BASE)
        with pytest.raises(UpstreamError) as exc_info:
            await l3.validate_session(
                session_id="sess_x", skill_id="skill.hr", runtime_tier="strict"
            )
    assert exc_info.value.kind == "error"


@respx.mock
async def test_l3_validate_session_connect_error() -> None:
    respx.post(f"{L3_BASE}/v1/sessions/sess_x/validate").mock(
        side_effect=httpx.ConnectError("no route")
    )
    async with httpx.AsyncClient() as client:
        l3 = L3Client(client, base_url=L3_BASE)
        with pytest.raises(UpstreamError) as exc_info:
            await l3.validate_session(
                session_id="sess_x", skill_id="skill.hr", runtime_tier="strict"
            )
    assert exc_info.value.kind == "unavailable"
