"""MCP SSE Client Bridge — connects hermes-runtime to external MCP servers.

Provides:
- ``McpBridge``: manages SSE connections to MCP servers.
- ``inject_mcp_tools``: registers MCP tools into hermes-agent's tool list
  and patches ``handle_function_call`` to route MCP tool calls.

Phase 0.5 ADR-V2-005: Tool Sandbox Container verification.
"""

from __future__ import annotations

import asyncio
import functools
import json
import logging
import os
from typing import Any

logger = logging.getLogger(__name__)


class McpToolProxy:
    """Holds MCP tool metadata and routes calls to the SSE server."""

    def __init__(self, name: str, description: str, input_schema: dict, bridge: McpBridge):
        self.name = name
        self.description = description
        self.input_schema = input_schema
        self._bridge = bridge

    def to_openai_tool(self) -> dict:
        """Convert to OpenAI function calling format (hermes-agent tool dict)."""
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.input_schema,
            },
        }

    async def call(self, arguments: dict[str, Any]) -> str:
        """Call the MCP tool via SSE and return the result text."""
        return await self._bridge.call_tool(self.name, arguments)


class McpBridge:
    """Manages an SSE connection to one MCP server."""

    def __init__(self, server_name: str, sse_url: str):
        self.server_name = server_name
        self.sse_url = sse_url
        self._tools: list[McpToolProxy] = []
        self._connected = False

    async def connect(self) -> list[McpToolProxy]:
        """Connect to MCP server, list tools, return proxies."""
        try:
            from mcp.client.sse import sse_client
            from mcp import ClientSession

            async with sse_client(self.sse_url) as (read, write):
                async with ClientSession(read, write) as session:
                    await session.initialize()
                    tools_result = await session.list_tools()
                    self._tools = [
                        McpToolProxy(
                            name=t.name,
                            description=t.description or "",
                            input_schema=t.inputSchema if hasattr(t, 'inputSchema') else {},
                            bridge=self,
                        )
                        for t in tools_result.tools
                    ]
                    self._connected = True
                    logger.info(
                        "MCP bridge connected to %s (%s): %d tools",
                        self.server_name,
                        self.sse_url,
                        len(self._tools),
                    )
                    return self._tools
        except Exception as e:
            logger.error("MCP bridge connect failed for %s: %s", self.server_name, e)
            return []

    async def call_tool(self, tool_name: str, arguments: dict[str, Any]) -> str:
        """Call a tool on the MCP server via a fresh SSE connection."""
        try:
            from mcp.client.sse import sse_client
            from mcp import ClientSession

            async with sse_client(self.sse_url) as (read, write):
                async with ClientSession(read, write) as session:
                    await session.initialize()
                    result = await session.call_tool(tool_name, arguments)
                    # Extract text from content blocks
                    texts = []
                    for block in result.content:
                        if hasattr(block, 'text'):
                            texts.append(block.text)
                    return "\n".join(texts) if texts else json.dumps({"status": "ok"})
        except Exception as e:
            logger.error("MCP tool call %s failed: %s", tool_name, e)
            return json.dumps({"error": str(e)})

    @property
    def tools(self) -> list[McpToolProxy]:
        return self._tools


class L2MemoryToolProxy:
    """Routes memory_* tool calls to L2 Memory Engine REST facade.

    L2 doesn't have SSE transport, so we proxy via HTTP POST to
    /tools/{name}/invoke with {"args": {...}} body.
    """

    # The 6 MCP tools exposed by L2 (from mcp_tools.py)
    TOOL_DEFS: list[dict[str, Any]] = [
        {
            "name": "memory_search",
            "description": "Hybrid keyword + time-decay ranked search over memory files.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "top_k": {"type": "integer", "default": 10},
                    "scope": {"type": "string"},
                    "category": {"type": "string"},
                },
                "required": ["query"],
            },
        },
        {
            "name": "memory_read",
            "description": "Read the latest version of a memory file by memory_id.",
            "parameters": {
                "type": "object",
                "properties": {"memory_id": {"type": "string"}},
                "required": ["memory_id"],
            },
        },
        {
            "name": "memory_write_anchor",
            "description": "Append-only write of an evidence anchor.",
            "parameters": {
                "type": "object",
                "properties": {
                    "event_id": {"type": "string"},
                    "session_id": {"type": "string"},
                    "type": {"type": "string"},
                    "data_ref": {"type": "string"},
                    "snapshot_hash": {"type": "string"},
                    "source_system": {"type": "string"},
                },
                "required": ["event_id", "session_id", "type"],
            },
        },
        {
            "name": "memory_write_file",
            "description": "Create a new memory file or bump the version of an existing memory_id.",
            "parameters": {
                "type": "object",
                "properties": {
                    "memory_id": {"type": "string"},
                    "scope": {"type": "string"},
                    "category": {"type": "string"},
                    "content": {"type": "string"},
                    "evidence_refs": {"type": "array", "items": {"type": "string"}},
                    "status": {"type": "string", "enum": ["agent_suggested", "confirmed", "archived"]},
                },
                "required": ["scope", "category", "content"],
            },
        },
        {
            "name": "memory_list",
            "description": "List latest versions of memory files by scope/category/status.",
            "parameters": {
                "type": "object",
                "properties": {
                    "scope": {"type": "string"},
                    "category": {"type": "string"},
                    "status": {"type": "string"},
                    "limit": {"type": "integer", "default": 50},
                },
            },
        },
        {
            "name": "memory_archive",
            "description": "Transition a memory file's status to archived.",
            "parameters": {
                "type": "object",
                "properties": {"memory_id": {"type": "string"}},
                "required": ["memory_id"],
            },
        },
    ]

    def __init__(self, base_url: str | None = None):
        port = os.environ.get("EAASP_L2_PORT", "18085")
        host = os.environ.get("EAASP_L2_HOST", "127.0.0.1")
        self.base_url = (base_url or f"http://{host}:{port}").rstrip("/")
        self.tool_names = {t["name"] for t in self.TOOL_DEFS}

    def get_openai_tools(self) -> list[dict]:
        """Return tool definitions in OpenAI function calling format."""
        return [
            {
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["parameters"],
                },
            }
            for t in self.TOOL_DEFS
        ]

    def call_tool(self, name: str, arguments: dict[str, Any]) -> str:
        """Synchronous HTTP call to L2 REST facade."""
        import httpx

        url = f"{self.base_url}/tools/{name}/invoke"
        try:
            with httpx.Client(trust_env=False, timeout=10.0) as client:
                resp = client.post(url, json={"args": arguments})
                resp.raise_for_status()
                return json.dumps(resp.json())
        except Exception as e:
            logger.error("L2 tool %s call failed: %s", name, e)
            return json.dumps({"error": str(e)})


def inject_mcp_tools(
    agent,
    bridges: list[McpBridge],
    l2_proxy: L2MemoryToolProxy | None = None,
) -> None:
    """Inject MCP tools into hermes-agent and patch handle_function_call.

    Args:
        agent: hermes AIAgent instance (has .tools list, .valid_tool_names set)
        bridges: list of connected McpBridge instances
        l2_proxy: optional L2 Memory Engine REST proxy for memory_* tools
    """
    mcp_tool_map: dict[str, McpToolProxy] = {}

    for bridge in bridges:
        for tool_proxy in bridge.tools:
            # Add tool definition to agent
            agent.tools.append(tool_proxy.to_openai_tool())
            agent.valid_tool_names.add(tool_proxy.name)
            mcp_tool_map[tool_proxy.name] = tool_proxy
            logger.info("Injected MCP tool: %s (from %s)", tool_proxy.name, bridge.server_name)

    # Inject L2 memory tools (REST proxy, not MCP SSE)
    l2_tool_names: set[str] = set()
    if l2_proxy is not None:
        for tool_def in l2_proxy.get_openai_tools():
            name = tool_def["function"]["name"]
            agent.tools.append(tool_def)
            agent.valid_tool_names.add(name)
            l2_tool_names.add(name)
            logger.info("Injected L2 memory tool: %s", name)

    if not mcp_tool_map and not l2_tool_names:
        return

    # Monkey-patch handle_function_call to route MCP tools.
    # IMPORTANT: run_agent.py does `from model_tools import handle_function_call`
    # at module level (line 66), so we must patch BOTH model_tools AND run_agent
    # to ensure the patch takes effect regardless of which reference is used.
    try:
        import model_tools
        import run_agent as _run_agent_mod
    except ImportError:
        logger.warning("model_tools/run_agent not available — MCP tool routing not installed")
        return

    _original = model_tools.handle_function_call

    @functools.wraps(_original)
    def _mcp_aware_handle_function_call(
        function_name: str,
        function_args: dict[str, Any],
        task_id=None,
        tool_call_id=None,
        session_id=None,
        user_task=None,
        enabled_tools=None,
    ) -> str:
        # Route L2 memory tools via REST proxy (sync)
        if l2_proxy is not None and function_name in l2_tool_names:
            logger.info("Routing L2 tool call: %s args=%s", function_name, function_args)
            result = l2_proxy.call_tool(function_name, function_args)
            logger.info("L2 tool %s result: %s", function_name, result[:200])
            return result
        # Route MCP SSE tools (async)
        if function_name in mcp_tool_map:
            proxy = mcp_tool_map[function_name]
            logger.info("Routing MCP tool call: %s args=%s", function_name, function_args)
            # Run async call in sync context (hermes-agent is sync)
            loop = asyncio.new_event_loop()
            try:
                result = loop.run_until_complete(proxy.call(function_args))
            finally:
                loop.close()
            logger.info("MCP tool %s result: %s", function_name, result[:200])
            return result
        return _original(
            function_name, function_args,
            task_id=task_id, tool_call_id=tool_call_id,
            session_id=session_id, user_task=user_task,
            enabled_tools=enabled_tools,
        )

    model_tools.handle_function_call = _mcp_aware_handle_function_call
    # Patch run_agent's local reference too (bound at import time)
    _run_agent_mod.handle_function_call = _mcp_aware_handle_function_call
    logger.info("Installed MCP-aware handle_function_call (tools: %s)", list(mcp_tool_map.keys()))

