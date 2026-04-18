"""S3.T3 — StdioMcpClient unit tests.

Tests use a fake MCP server script (echo-based) to verify:
- initialize handshake
- tools/list parsing → McpToolSpec
- close() kills subprocess
- to_oai_schema() produces correct OAI tool schema shape

No real MCP server required.
"""
from __future__ import annotations

import json
import sys
import textwrap

import pytest

from nanobot_runtime.mcp_client import McpToolSpec, StdioMcpClient


# ── Fake MCP server ───────────────────────────────────────────────────────────

FAKE_MCP_SCRIPT = textwrap.dedent("""\
    import sys
    import json

    # Read initialize request
    line = sys.stdin.readline()
    req = json.loads(line)
    resp = {"jsonrpc": "2.0", "id": req["id"], "result": {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "serverInfo": {"name": "fake-mcp", "version": "0.1"},
    }}
    sys.stdout.write(json.dumps(resp) + "\\n")
    sys.stdout.flush()

    # Read notifications/initialized (no response needed)
    line = sys.stdin.readline()

    # Read tools/list request
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    req = json.loads(line)
    resp = {"jsonrpc": "2.0", "id": req["id"], "result": {
        "tools": [
            {
                "name": "memory_search",
                "description": "Search memory for relevant entries",
                "inputSchema": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"],
                },
            },
            {
                "name": "memory_write",
                "description": "Write a memory entry",
                "inputSchema": {
                    "type": "object",
                    "properties": {"content": {"type": "string"}},
                },
            },
        ]
    }}
    sys.stdout.write(json.dumps(resp) + "\\n")
    sys.stdout.flush()
    sys.exit(0)
""")

EMPTY_TOOLS_SCRIPT = textwrap.dedent("""\
    import sys, json
    line = sys.stdin.readline()
    req = json.loads(line)
    resp = {"jsonrpc": "2.0", "id": req["id"], "result": {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {}}}
    sys.stdout.write(json.dumps(resp) + "\\n")
    sys.stdout.flush()
    sys.stdin.readline()  # notifications/initialized
    line = sys.stdin.readline()
    req = json.loads(line)
    resp = {"jsonrpc": "2.0", "id": req["id"], "result": {"tools": []}}
    sys.stdout.write(json.dumps(resp) + "\\n")
    sys.stdout.flush()
    sys.exit(0)
""")


def _python_inline_cmd(script: str) -> list[str]:
    return [sys.executable, "-c", script]


# ── Tests ─────────────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_list_tools_returns_two_tools():
    cmd = _python_inline_cmd(FAKE_MCP_SCRIPT)
    async with StdioMcpClient(cmd=cmd, server_name="fake") as client:
        tools = await client.list_tools()
    assert len(tools) == 2
    assert tools[0].name == "memory_search"
    assert tools[1].name == "memory_write"


@pytest.mark.asyncio
async def test_list_tools_sets_server_name():
    cmd = _python_inline_cmd(FAKE_MCP_SCRIPT)
    async with StdioMcpClient(cmd=cmd, server_name="my-server") as client:
        tools = await client.list_tools()
    assert all(t.server_name == "my-server" for t in tools)


@pytest.mark.asyncio
async def test_list_tools_empty_server():
    cmd = _python_inline_cmd(EMPTY_TOOLS_SCRIPT)
    async with StdioMcpClient(cmd=cmd, server_name="empty") as client:
        tools = await client.list_tools()
    assert tools == []


@pytest.mark.asyncio
async def test_close_terminates_subprocess():
    import asyncio
    cmd = _python_inline_cmd(FAKE_MCP_SCRIPT)
    client = StdioMcpClient(cmd=cmd, server_name="test")
    await client.start()
    proc = client._proc
    assert proc is not None
    await client.list_tools()
    await client.close()
    # After close the process should have exited
    await asyncio.sleep(0.05)
    assert proc.returncode is not None


def test_to_oai_schema_shape():
    spec = McpToolSpec(
        name="memory_search",
        description="Search memory",
        parameters={"type": "object", "properties": {"q": {"type": "string"}}},
        server_name="test",
    )
    schema = spec.to_oai_schema()
    assert schema["type"] == "function"
    assert schema["function"]["name"] == "memory_search"
    assert schema["function"]["description"] == "Search memory"
    assert "properties" in schema["function"]["parameters"]


def test_to_oai_schema_empty_parameters():
    spec = McpToolSpec(name="ping", description="Ping", server_name="x")
    schema = spec.to_oai_schema()
    # Should default to empty object schema, not crash
    assert schema["function"]["parameters"]["type"] == "object"


@pytest.mark.asyncio
async def test_double_close_is_safe():
    cmd = _python_inline_cmd(FAKE_MCP_SCRIPT)
    client = StdioMcpClient(cmd=cmd, server_name="test")
    await client.start()
    await client.list_tools()
    await client.close()
    await client.close()  # should not raise
