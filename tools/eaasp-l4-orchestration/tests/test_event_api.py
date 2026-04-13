"""Tests for Phase 1 Event Engine API endpoints."""

from __future__ import annotations

import httpx
import respx

L2_DEFAULT = "http://127.0.0.1:18085"
L3_DEFAULT = "http://127.0.0.1:18083"


@respx.mock
async def test_ingest_endpoint_accepts_event(
    app_client: httpx.AsyncClient,
) -> None:
    """POST /v1/events/ingest should persist and return event_id."""
    # Mock L2/L3 for session creation.
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

    # Create a session first.
    resp = await app_client.post(
        "/v1/sessions/create",
        json={
            "intent_text": "test",
            "skill_id": "skill.test",
            "runtime_pref": "grid-runtime",
        },
    )
    assert resp.status_code == 200
    session_id = resp.json()["session_id"]

    # Ingest an event via the new endpoint.
    resp = await app_client.post(
        "/v1/events/ingest",
        json={
            "session_id": session_id,
            "event_type": "PRE_TOOL_USE",
            "payload": {"tool_name": "scada_read"},
            "source": "runtime:grid-runtime",
        },
    )
    assert resp.status_code == 200
    data = resp.json()
    assert "event_id" in data
    assert "seq" in data
    assert data["seq"] >= 1


async def test_ingest_endpoint_missing_session_id(
    app_client: httpx.AsyncClient,
) -> None:
    """POST /v1/events/ingest with empty session_id should return 422."""
    resp = await app_client.post(
        "/v1/events/ingest",
        json={
            "session_id": "",
            "event_type": "STOP",
            "payload": {},
        },
    )
    assert resp.status_code == 422  # Pydantic validation


async def test_health_still_works(app_client: httpx.AsyncClient) -> None:
    """Smoke test: /health should still return OK after Event Engine wiring."""
    resp = await app_client.get("/health")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"
