"""Contract 5 (partial) — Session validate endpoint tests (via FastAPI app)."""

from __future__ import annotations

import pytest
from httpx import AsyncClient


pytestmark = pytest.mark.asyncio


async def _deploy(app: AsyncClient, hooks: list[dict]) -> int:
    resp = await app.put(
        "/v1/policies/managed-hooks",
        json={"version": "v2.0.0-mvp", "hooks": hooks},
    )
    assert resp.status_code == 200, resp.text
    return int(resp.json()["version"])


async def test_validate_returns_hooks_matching_agent_id(app: AsyncClient) -> None:
    await _deploy(
        app,
        [
            {
                "hook_id": "h_global",
                "phase": "PostToolUse",
                "mode": "enforce",
                "agent_id": "*",
                "skill_id": "*",
            },
            {
                "hook_id": "h_threshold",
                "phase": "PreToolUse",
                "mode": "enforce",
                "agent_id": "agent_threshold",
                "skill_id": "*",
            },
            {
                "hook_id": "h_other",
                "phase": "PreToolUse",
                "mode": "enforce",
                "agent_id": "agent_somebody_else",
            },
        ],
    )

    resp = await app.post(
        "/v1/sessions/sess_abc/validate",
        json={"agent_id": "agent_threshold", "skill_id": "sk_threshold_v1"},
    )
    assert resp.status_code == 200
    body = resp.json()
    assert body["managed_settings_version"] == 1
    ids = [h["hook_id"] for h in body["hooks_to_attach"]]
    assert set(ids) == {"h_global", "h_threshold"}


async def test_validate_applies_mode_override(app: AsyncClient) -> None:
    await _deploy(
        app,
        [
            {
                "hook_id": "h_audit",
                "phase": "PostToolUse",
                "mode": "enforce",
                "agent_id": "*",
            }
        ],
    )

    # Flip audit hook to shadow.
    resp = await app.put(
        "/v1/policies/h_audit/mode", json={"mode": "shadow"}
    )
    assert resp.status_code == 200

    resp = await app.post(
        "/v1/sessions/s1/validate",
        json={"agent_id": "agent_threshold"},
    )
    assert resp.status_code == 200
    hooks = resp.json()["hooks_to_attach"]
    assert len(hooks) == 1
    assert hooks[0]["hook_id"] == "h_audit"
    assert hooks[0]["mode"] == "shadow"  # override applied


async def test_validate_404_when_no_policy(app: AsyncClient) -> None:
    resp = await app.post(
        "/v1/sessions/s1/validate",
        json={"agent_id": "agent_threshold"},
    )
    assert resp.status_code == 404
    detail = resp.json()["detail"]
    assert detail["code"] == "no_policy"
