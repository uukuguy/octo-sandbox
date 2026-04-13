"""EAASP L2 Memory Engine — MCP Server (stdio / SSE transport).

Wraps the 6 memory tools as a proper MCP server so L1 runtimes can
connect via ConnectMCP instead of using the REST facade.

Run via stdio transport (the ``eaasp-l2-memory`` console script):

    uv run eaasp-l2-memory

The runtime's MCP client spawns this as a subprocess and exchanges
newline-delimited JSON-RPC over stdin/stdout.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import sys
from typing import Any

from mcp.server import NotificationOptions, Server
from mcp.server.models import InitializationOptions
from mcp.server.stdio import stdio_server
from mcp.types import TextContent, Tool

from .anchors import AnchorStore
from .db import init_db
from .files import MemoryFileStore
from .index import HybridIndex
from .mcp_tools import MCP_TOOL_MANIFEST, McpToolDispatcher, ToolError

logging.basicConfig(
    level=logging.INFO,
    format="[eaasp-l2-memory] %(message)s",
    stream=sys.stderr,
)
_log = logging.getLogger("eaasp-l2-memory")

SERVER_NAME = "eaasp-l2-memory"
SERVER_VERSION = "0.1.0"

_DEFAULT_DB = os.environ.get("EAASP_L2_DB_PATH") or os.environ.get(
    "EAASP_MEMORY_DB", "./data/memory.db"
)

# Convert ToolManifestEntry → mcp.types.Tool for the MCP SDK.
_TOOL_MANIFEST: list[Tool] = [
    Tool(
        name=entry.name,
        description=entry.description,
        inputSchema=entry.input_schema,
    )
    for entry in MCP_TOOL_MANIFEST
]


def build_server(db_path: str | None = None) -> tuple[Server, str]:
    """Build an MCP Server with all 6 memory tools wired in.

    Returns ``(server, resolved_db_path)`` so callers (tests, CLI) can
    inspect the DB path that will be used.
    """
    db = db_path or _DEFAULT_DB
    server: Server = Server(SERVER_NAME)

    # Lazy-init: the dispatcher is created on first tool call so that
    # ``init_db`` can run inside the async context of the server loop.
    _dispatcher: dict[str, McpToolDispatcher | None] = {"instance": None}

    async def _get_dispatcher() -> McpToolDispatcher:
        if _dispatcher["instance"] is None:
            await init_db(db)
            _dispatcher["instance"] = McpToolDispatcher(
                AnchorStore(db),
                MemoryFileStore(db),
                HybridIndex(db),
            )
        return _dispatcher["instance"]

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return list(_TOOL_MANIFEST)

    @server.call_tool()
    async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
        _log.info("call_tool: %s args=%s", name, arguments)
        dispatcher = await _get_dispatcher()
        try:
            result = await dispatcher.invoke(name, arguments or {})
        except ToolError as exc:
            # Return error as content so the LLM sees it rather than
            # crashing the MCP channel.
            error_payload = {"error": exc.code, "message": exc.message}
            return [
                TextContent(
                    type="text",
                    text=json.dumps(error_payload, sort_keys=True),
                )
            ]
        return [
            TextContent(
                type="text",
                text=json.dumps(result, sort_keys=True, separators=(",", ":")),
            )
        ]

    return server, db


async def _serve_stdio(db_path: str | None = None) -> None:
    _log.info("Starting %s v%s (stdio transport)", SERVER_NAME, SERVER_VERSION)
    server, _ = build_server(db_path)
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


def _serve_sse(host: str, port: int, db_path: str | None = None) -> None:
    """Run as SSE transport (network MCP for container-to-container)."""
    from starlette.applications import Starlette
    from starlette.responses import Response
    from starlette.routing import Mount, Route

    from mcp.server.sse import SseServerTransport

    _log.info(
        "Starting %s v%s (SSE transport on %s:%d)",
        SERVER_NAME,
        SERVER_VERSION,
        host,
        port,
    )

    sse = SseServerTransport("/messages/")
    server, _ = build_server(db_path)
    init_options = InitializationOptions(
        server_name=SERVER_NAME,
        server_version=SERVER_VERSION,
        capabilities=server.get_capabilities(
            notification_options=NotificationOptions(),
            experimental_capabilities={},
        ),
    )

    async def handle_sse(request):  # type: ignore[no-untyped-def]
        async with sse.connect_sse(
            request.scope,
            request.receive,
            request._send,
        ) as streams:
            await server.run(streams[0], streams[1], init_options)
        return Response()

    starlette_routes = [
        Route("/sse", endpoint=handle_sse, methods=["GET"]),
        Mount("/messages/", app=sse.handle_post_message),
    ]
    app = Starlette(routes=starlette_routes)

    import uvicorn

    uvicorn.run(app, host=host, port=port, log_level="info")


def run() -> None:
    """Console-script entry point.

    Usage:
        eaasp-l2-memory                         # stdio (default)
        eaasp-l2-memory --transport sse         # SSE on 127.0.0.1:18086
        eaasp-l2-memory --transport sse --port 19000 --host 0.0.0.0
    """
    import argparse

    parser = argparse.ArgumentParser(description="EAASP L2 Memory Engine MCP Server")
    parser.add_argument(
        "--transport",
        choices=["stdio", "sse"],
        default="stdio",
        help="MCP transport mode (default: stdio)",
    )
    parser.add_argument("--host", default="127.0.0.1", help="SSE bind host")
    parser.add_argument("--port", type=int, default=18086, help="SSE bind port")
    parser.add_argument("--db", default=None, help="SQLite DB path override")
    args = parser.parse_args()

    if args.transport == "sse":
        _serve_sse(args.host, args.port, args.db)
    else:
        asyncio.run(_serve_stdio(args.db))


if __name__ == "__main__":
    run()
