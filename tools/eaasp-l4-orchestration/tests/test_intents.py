"""Contract 2 — /v1/intents/dispatch smoke tests."""

from __future__ import annotations

import httpx
import respx

L2_DEFAULT = "http://127.0.0.1:8085"
L3_DEFAULT = "http://127.0.0.1:8083"


@respx.mock
async def test_dispatch_intent_happy_path(app_client: httpx.AsyncClient) -> None:
    respx.post(f"{L2_DEFAULT}/api/v1/memory/search").mock(
        return_value=httpx.Response(
            200,
            json={"hits": [{"memory_id": "m1", "memory_type": "anchor"}]},
        )
    )
    respx.post(url__regex=rf"{L3_DEFAULT}/v1/sessions/.*/validate").mock(
        return_value=httpx.Response(
            200,
            json={
                "session_id": "placeholder",
                "hooks_to_attach": [],
                "managed_settings_version": 1,
                "validated_at": "2026-04-12 02:00:00",
                "runtime_tier": "strict",
            },
        )
    )

    resp = await app_client.post(
        "/v1/intents/dispatch",
        json={
            "intent_text": "please onboard a new hire",
            "skill_id": "skill.hr.onboard",
            "runtime_pref": "strict",
            "user_id": "u-1",
            "intent_id": "intent-42",
        },
    )
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["session_id"].startswith("sess_")
    assert body["status"] == "created"
    # Same payload shape as /v1/sessions/create.
    assert "memory_refs" in body["payload"]
    assert "policy_context" in body["payload"]
    assert "skill_instructions" in body["payload"]


async def test_dispatch_intent_missing_skill_id_422(
    app_client: httpx.AsyncClient,
) -> None:
    resp = await app_client.post(
        "/v1/intents/dispatch",
        json={"intent_text": "x", "runtime_pref": "strict"},
    )
    assert resp.status_code == 422
