"""MCP server wrapper tests — tool manifest, call_tool dispatch, error handling."""

from __future__ import annotations

import json
from typing import Any

import pytest

from mcp.types import CallToolRequest, CallToolRequestParams

from eaasp_l2_memory_engine.mcp_server import (
    SERVER_NAME,
    SERVER_VERSION,
    _TOOL_MANIFEST,
    build_server,
)


pytestmark = pytest.mark.asyncio


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

async def _call(db_path: str, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    """Build a fresh server, invoke a tool, return parsed JSON result."""
    server, _ = build_server(db_path)
    return await _call_on(server, tool_name, arguments)


async def _call_on(server, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:  # type: ignore[no-untyped-def]
    """Invoke a tool on an already-built server (shares dispatcher state).

    The MCP SDK handler returns ``ServerResult`` whose ``.root`` holds the
    actual ``CallToolResult`` with ``.content``.
    """
    handler = server.request_handlers[CallToolRequest]
    req = CallToolRequest(
        method="tools/call",
        params=CallToolRequestParams(name=tool_name, arguments=arguments),
    )
    resp = await handler(req)
    result = resp.root  # CallToolResult
    assert len(result.content) >= 1
    return json.loads(result.content[0].text)


# ---------------------------------------------------------------------------
# Identity & manifest (sync)
# ---------------------------------------------------------------------------

def test_server_identity() -> None:
    assert SERVER_NAME == "eaasp-l2-memory"
    assert SERVER_VERSION == "0.1.0"


def test_tool_manifest_exposes_seven_tools() -> None:
    # S2.T3: manifest grew from 6 → 7 tools (added memory_confirm).
    names = {tool.name for tool in _TOOL_MANIFEST}
    expected = {
        "memory_search",
        "memory_read",
        "memory_write_anchor",
        "memory_write_file",
        "memory_list",
        "memory_archive",
        "memory_confirm",
    }
    assert names == expected


def test_tool_manifest_schemas_have_required_fields() -> None:
    by_name = {tool.name: tool for tool in _TOOL_MANIFEST}

    search = by_name["memory_search"].inputSchema
    assert search["required"] == ["query"]

    read = by_name["memory_read"].inputSchema
    assert read["required"] == ["memory_id"]

    anchor = by_name["memory_write_anchor"].inputSchema
    assert set(anchor["required"]) == {"event_id", "session_id", "type"}

    write = by_name["memory_write_file"].inputSchema
    assert set(write["required"]) == {"scope", "category", "content"}

    archive = by_name["memory_archive"].inputSchema
    assert archive["required"] == ["memory_id"]

    # memory_list has no required fields
    lst = by_name["memory_list"].inputSchema
    assert "required" not in lst


def test_build_server_returns_configured_instance(db_path: str) -> None:
    server, resolved = build_server(db_path)
    assert server.name == SERVER_NAME
    assert resolved == db_path


# ---------------------------------------------------------------------------
# call_tool dispatch via McpToolDispatcher (async, needs DB)
# ---------------------------------------------------------------------------

async def test_call_tool_write_file_and_read(db_path: str) -> None:
    """Write a memory file via call_tool, then read it back."""
    server, _ = build_server(db_path)

    # Write
    write_data = await _call_on(server, "memory_write_file", {
        "scope": "test-scope",
        "category": "test-cat",
        "content": "Hello from MCP server",
    })
    assert "memory_id" in write_data
    mid = write_data["memory_id"]

    # Read back on same server (shares lazy dispatcher)
    read_data = await _call_on(server, "memory_read", {"memory_id": mid})
    assert read_data["memory_id"] == mid
    assert read_data["content"] == "Hello from MCP server"
    assert read_data["scope"] == "test-scope"


async def test_call_tool_write_anchor(db_path: str) -> None:
    """Write an evidence anchor via call_tool."""
    data = await _call(db_path, "memory_write_anchor", {
        "event_id": "evt-mcp-001",
        "session_id": "sess-mcp-001",
        "type": "observation",
    })
    assert "anchor_id" in data
    assert data["event_id"] == "evt-mcp-001"


async def test_call_tool_search_empty(db_path: str) -> None:
    """Search on empty DB returns empty hits, not an error."""
    data = await _call(db_path, "memory_search", {"query": "nonexistent", "top_k": 5})
    assert data["hits"] == []


async def test_call_tool_list_empty(db_path: str) -> None:
    """List on empty DB returns empty list."""
    data = await _call(db_path, "memory_list", {})
    assert data["memories"] == []


async def test_call_tool_read_not_found_returns_error_content(db_path: str) -> None:
    """Reading a non-existent memory_id returns error in content, not crash."""
    data = await _call(db_path, "memory_read", {"memory_id": "mem_nonexistent"})
    assert data["error"] == "not_found"


async def test_call_tool_unknown_tool_returns_error_content(db_path: str) -> None:
    """Unknown tool name returns error in content, not crash."""
    data = await _call(db_path, "totally_unknown", {})
    assert data["error"] == "unknown_tool"


async def test_call_tool_archive_round_trip(db_path: str) -> None:
    """Write, archive, verify status change."""
    server, _ = build_server(db_path)

    # Write
    write_data = await _call_on(server, "memory_write_file", {
        "scope": "s",
        "category": "c",
        "content": "to be archived",
    })
    mid = write_data["memory_id"]

    # Archive
    archive_data = await _call_on(server, "memory_archive", {"memory_id": mid})
    assert archive_data["status"] == "archived"
