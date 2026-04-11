"""Mock SCADA MCP stdio server.

Exposes two tools to any MCP-capable runtime (grid-runtime, claude-code-runtime):

- `scada_read_snapshot(device_id, time_window="5m")` — returns deterministic
  telemetry (3 samples + baseline).
- `scada_write(device_id, field, value)` — always fails with a marker error.
  The threshold-calibration skill's PreToolUse hook blocks it before we even
  get here; this is belt-and-suspenders for the e2e test.

Run via stdio transport (the `mcp-scada` console script):

    uv run mock-scada

The runtime's MCP client spawns this as a subprocess and exchanges
newline-delimited JSON-RPC over stdin/stdout.
"""

from __future__ import annotations

import asyncio
import json
from typing import Any

from mcp.server import NotificationOptions, Server
from mcp.server.models import InitializationOptions
from mcp.server.stdio import stdio_server
from mcp.types import TextContent, Tool

from .snapshots import SCADA_WRITE_ERROR_MARKER, build_snapshot, snapshot_hash

SERVER_NAME = "mock-scada"
SERVER_VERSION = "0.1.0"

_TOOL_MANIFEST: list[Tool] = [
    Tool(
        name="scada_read_snapshot",
        description=(
            "Read the latest SCADA telemetry snapshot for a device. "
            "Returns deterministic temperature/load/dissolved-gas samples "
            "suitable for threshold calibration (read-only, safe to call)."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "Device identifier (e.g. xfmr-042, brk-17).",
                },
                "time_window": {
                    "type": "string",
                    "description": "Lookback window, e.g. '5m', '1h'. Defaults to '5m'.",
                    "default": "5m",
                },
            },
            "required": ["device_id"],
        },
    ),
    Tool(
        name="scada_write",
        description=(
            "MUST NOT be called. Any attempt to write to SCADA from an agent "
            "is blocked by the threshold-calibration skill's PreToolUse hook. "
            "This endpoint exists only so hook denial can be exercised end-to-end."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "device_id": {"type": "string"},
                "field": {"type": "string"},
                "value": {},
            },
            "required": ["device_id", "field", "value"],
        },
    ),
]


def _handle_scada_read_snapshot(args: dict[str, Any]) -> dict[str, Any]:
    device_id = args.get("device_id")
    if not isinstance(device_id, str) or not device_id:
        raise ValueError("device_id (non-empty string) is required")
    time_window = args.get("time_window", "5m")
    if not isinstance(time_window, str) or not time_window:
        time_window = "5m"
    snapshot = build_snapshot(device_id, time_window)
    snapshot["snapshot_hash"] = snapshot_hash(snapshot)
    return snapshot


def _handle_scada_write(args: dict[str, Any]) -> dict[str, Any]:
    raise RuntimeError(
        f"{SCADA_WRITE_ERROR_MARKER}: scada_write is blocked; "
        f"args={json.dumps(args, sort_keys=True)}"
    )


def build_server() -> Server:
    """Build the MCP server with tool handlers wired in."""
    server: Server = Server(SERVER_NAME)

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return list(_TOOL_MANIFEST)

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        if name == "scada_read_snapshot":
            result = _handle_scada_read_snapshot(arguments or {})
        elif name == "scada_write":
            result = _handle_scada_write(arguments or {})
        else:
            raise ValueError(f"unknown tool: {name}")
        return [
            TextContent(
                type="text",
                text=json.dumps(result, sort_keys=True, separators=(",", ":")),
            )
        ]

    return server


async def _serve() -> None:
    server = build_server()
    init_options = InitializationOptions(
        server_name=SERVER_NAME,
        server_version=SERVER_VERSION,
        capabilities=server.get_capabilities(
            notification_options=NotificationOptions(),
            experimental_capabilities={},
        ),
    )
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, init_options)


def run() -> None:
    """Console-script entry point; blocks until stdio streams close."""
    asyncio.run(_serve())


if __name__ == "__main__":
    run()
