"""REST API status-code and happy-path tests."""

from __future__ import annotations

import asyncio
import json

import pytest
from httpx import AsyncClient


pytestmark = pytest.mark.asyncio


async def test_health(app: AsyncClient) -> None:
    resp = await app.get("/health")
    assert resp.status_code == 200
    assert resp.json() == {"status": "ok"}


async def test_policy_deploy_happy_path(app: AsyncClient) -> None:
    payload = {
        "version": "v2.0.0-mvp",
        "hooks": [
            {
                "hook_id": "h1",
                "phase": "PostToolUse",
                "mode": "enforce",
                "agent_id": "*",
            },
            {"hook_id": "h2", "phase": "PreToolUse", "mode": "shadow"},
        ],
    }
    resp = await app.put("/v1/policies/managed-hooks", json=payload)
    assert resp.status_code == 200
    body = resp.json()
    assert body["version"] == 1
    assert body["hook_count"] == 2
    assert body["mode_summary"] == {"enforce": 1, "shadow": 1}

    # And the version is listable.
    resp = await app.get("/v1/policies/versions")
    assert resp.status_code == 200
    versions = resp.json()["versions"]
    assert len(versions) == 1
    assert versions[0]["version"] == 1


async def test_policy_deploy_rejects_duplicate_hook_ids(app: AsyncClient) -> None:
    resp = await app.put(
        "/v1/policies/managed-hooks",
        json={
            "hooks": [
                {"hook_id": "dup", "phase": "PreToolUse"},
                {"hook_id": "dup", "phase": "PostToolUse"},
            ]
        },
    )
    assert resp.status_code == 422


async def test_policy_deploy_rejects_invalid_mode(app: AsyncClient) -> None:
    resp = await app.put(
        "/v1/policies/managed-hooks",
        json={
            "hooks": [
                {"hook_id": "h1", "phase": "PreToolUse", "mode": "broken"}
            ]
        },
    )
    assert resp.status_code == 422


async def test_mode_switch_rejects_invalid_mode(app: AsyncClient) -> None:
    resp = await app.put("/v1/policies/h1/mode", json={"mode": "paused"})
    assert resp.status_code == 422


async def test_telemetry_ingest_and_query(app: AsyncClient) -> None:
    ingest_resp = await app.post(
        "/v1/telemetry/events",
        json={
            "session_id": "sess_abc",
            "agent_id": "agent_threshold",
            "hook_id": "h_audit",
            "phase": "PostToolUse",
            "payload": {"tool": "bash", "exit": 0},
        },
    )
    assert ingest_resp.status_code == 200
    body = ingest_resp.json()
    assert body["event_id"].startswith("tel_")

    query_resp = await app.get(
        "/v1/telemetry/events", params={"session_id": "sess_abc"}
    )
    assert query_resp.status_code == 200
    events = query_resp.json()["events"]
    assert len(events) == 1
    assert events[0]["payload"] == {"tool": "bash", "exit": 0}


async def test_telemetry_ingest_missing_session_id_returns_422(
    app: AsyncClient,
) -> None:
    resp = await app.post(
        "/v1/telemetry/events",
        json={"payload": {"tool": "bash"}},
    )
    assert resp.status_code == 422


async def test_telemetry_query_limit_validation(app: AsyncClient) -> None:
    # FastAPI Query(le=500) turns oversize limits into 422 at the edge.
    resp = await app.get("/v1/telemetry/events", params={"limit": 99999})
    assert resp.status_code == 422


async def test_duplicate_hook_id_422_body_is_json_serializable(
    app: AsyncClient,
) -> None:
    """R1: `_sanitize_errors()` must strip raw ValueError from Pydantic ctx.

    Ensures the 422 response body from a `hook_id` uniqueness violation is
    fully JSON-decodable end-to-end (the Pydantic v2 ``ValidationError.ctx``
    can embed a raw ``ValueError`` that ``JSONResponse`` cannot serialize).
    """
    resp = await app.put(
        "/v1/policies/managed-hooks",
        json={
            "hooks": [
                {"hook_id": "same", "phase": "PreToolUse"},
                {"hook_id": "same", "phase": "PostToolUse"},
            ]
        },
    )
    assert resp.status_code == 422
    # Full round-trip through response content to prove serializer survived.
    body = resp.json()
    detail = body["detail"]
    assert isinstance(detail, list) and detail, "sanitized detail must be a list"
    # Must be re-encodable — proves no BaseException leaked into ctx.
    json.dumps(detail)
    # And the sanitized ctx (if any) must be a plain dict of strings/scalars.
    for err in detail:
        ctx = err.get("ctx")
        if ctx is not None:
            assert isinstance(ctx, dict)
            for val in ctx.values():
                assert not isinstance(val, BaseException)


async def test_concurrent_deploys_produce_distinct_versions(
    app: AsyncClient,
) -> None:
    """R2: BEGIN IMMEDIATE must serialize concurrent deploy() calls cleanly.

    Fires 10 deploys concurrently against a single DB and asserts every
    returned version number is unique and strictly monotonically contiguous
    — no dropped rows, no ``database is locked`` leaks out to the caller.
    """
    async def _deploy(i: int) -> int:
        resp = await app.put(
            "/v1/policies/managed-hooks",
            json={
                "version": f"v-concurrent-{i}",
                "hooks": [
                    {"hook_id": f"c{i}", "phase": "PreToolUse", "mode": "shadow"}
                ],
            },
        )
        assert resp.status_code == 200, resp.text
        return resp.json()["version"]

    results = await asyncio.gather(*(_deploy(i) for i in range(10)))

    assert len(results) == 10
    assert len(set(results)) == 10, f"duplicate versions: {results}"
    assert sorted(results) == list(
        range(min(results), min(results) + 10)
    ), f"non-contiguous versions: {sorted(results)}"
