"""REST API tests — GET /tools, POST /tools/{name}/invoke, /api/v1/memory/*."""

from __future__ import annotations

import pytest
from httpx import AsyncClient


pytestmark = pytest.mark.asyncio


async def test_health(app: AsyncClient) -> None:
    resp = await app.get("/health")
    assert resp.status_code == 200
    assert resp.json() == {"status": "ok"}


async def test_list_tools_manifest(app: AsyncClient) -> None:
    resp = await app.get("/tools")
    assert resp.status_code == 200
    tools = resp.json()["tools"]
    names = {t["name"] for t in tools}
    # S2.T3: manifest now exposes 7 tools (added memory_confirm).
    assert names == {
        "memory_search",
        "memory_read",
        "memory_write_anchor",
        "memory_write_file",
        "memory_list",
        "memory_archive",
        "memory_confirm",
    }


async def test_invoke_write_and_search_via_rest(app: AsyncClient) -> None:
    write_resp = await app.post(
        "/tools/memory_write_file/invoke",
        json={
            "args": {
                "scope": "user:alice",
                "category": "threshold",
                "content": "salary_floor calibrated to 50000",
            }
        },
    )
    assert write_resp.status_code == 200
    memory_id = write_resp.json()["memory_id"]

    search_resp = await app.post(
        "/api/v1/memory/search",
        json={"query": "salary_floor"},
    )
    assert search_resp.status_code == 200
    hits = search_resp.json()["hits"]
    assert len(hits) == 1
    assert hits[0]["memory"]["memory_id"] == memory_id


async def test_rest_invoke_not_found_returns_404(app: AsyncClient) -> None:
    resp = await app.post(
        "/tools/memory_read/invoke",
        json={"args": {"memory_id": "mem_missing"}},
    )
    assert resp.status_code == 404


async def test_rest_invoke_unknown_tool_returns_400(app: AsyncClient) -> None:
    resp = await app.post("/tools/memory_bogus/invoke", json={"args": {}})
    assert resp.status_code == 400


async def test_anchors_endpoint_by_event(app: AsyncClient) -> None:
    for _ in range(2):
        await app.post(
            "/tools/memory_write_anchor/invoke",
            json={
                "args": {
                    "event_id": "evt_abc",
                    "session_id": "sess1",
                    "type": "tool_result",
                }
            },
        )
    await app.post(
        "/tools/memory_write_anchor/invoke",
        json={
            "args": {
                "event_id": "evt_other",
                "session_id": "sess1",
                "type": "tool_result",
            }
        },
    )

    resp = await app.get("/api/v1/memory/anchors", params={"event_id": "evt_abc"})
    assert resp.status_code == 200
    anchors = resp.json()["anchors"]
    assert len(anchors) == 2
    assert all(a["event_id"] == "evt_abc" for a in anchors)
