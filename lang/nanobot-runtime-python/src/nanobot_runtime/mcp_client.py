"""Minimal stdio JSON-RPC MCP client for nanobot-runtime (S3.T3).

Launches an MCP server subprocess, performs JSON-RPC initialize + tools/list,
and exposes the discovered tools as OAI-compatible tool schemas.

Protocol: MCP JSON-RPC 2.0 over stdio (newline-delimited).
"""
from __future__ import annotations

import asyncio
import json
import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)

_RPC_ID_COUNTER = 0


def _next_id() -> int:
    global _RPC_ID_COUNTER
    _RPC_ID_COUNTER += 1
    return _RPC_ID_COUNTER


@dataclass
class McpToolSpec:
    """A tool discovered from an MCP server, in OAI-compatible format."""
    name: str
    description: str
    parameters: dict[str, Any] = field(default_factory=dict)
    server_name: str = ""

    def to_oai_schema(self) -> dict[str, Any]:
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters or {"type": "object", "properties": {}},
            },
        }


class StdioMcpClient:
    """JSON-RPC 2.0 MCP client over subprocess stdio."""

    def __init__(self, cmd: list[str], server_name: str = "") -> None:
        self.cmd = cmd
        self.server_name = server_name or (cmd[0] if cmd else "mcp")
        self._proc: asyncio.subprocess.Process | None = None

    async def start(self) -> None:
        self._proc = await asyncio.create_subprocess_exec(
            *self.cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await self._send_rpc("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "nanobot-runtime", "version": "1.0"},
        })
        init_resp = await self._read_response()
        if "error" in init_resp:
            raise RuntimeError(f"MCP initialize failed: {init_resp['error']}")
        await self._send_notification("notifications/initialized", {})

    async def list_tools(self) -> list[McpToolSpec]:
        await self._send_rpc("tools/list", {})
        resp = await self._read_response()
        if "error" in resp:
            logger.warning("tools/list error from %s: %s", self.server_name, resp["error"])
            return []
        tools_raw = resp.get("result", {}).get("tools", [])
        return [
            McpToolSpec(
                name=t.get("name", ""),
                description=t.get("description", ""),
                parameters=t.get("inputSchema", {}),
                server_name=self.server_name,
            )
            for t in tools_raw
        ]

    async def close(self) -> None:
        if self._proc is not None:
            try:
                self._proc.stdin.close()  # type: ignore[union-attr]
                await asyncio.wait_for(self._proc.wait(), timeout=5.0)
            except Exception:
                try:
                    self._proc.kill()
                    await self._proc.wait()
                except Exception:
                    pass
            self._proc = None

    async def __aenter__(self) -> "StdioMcpClient":
        await self.start()
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()

    async def _send_rpc(self, method: str, params: dict[str, Any]) -> None:
        msg = {"jsonrpc": "2.0", "id": _next_id(), "method": method, "params": params}
        await self._write_line(json.dumps(msg))

    async def _send_notification(self, method: str, params: dict[str, Any]) -> None:
        msg = {"jsonrpc": "2.0", "method": method, "params": params}
        await self._write_line(json.dumps(msg))

    async def _write_line(self, line: str) -> None:
        assert self._proc is not None and self._proc.stdin is not None
        self._proc.stdin.write((line + "\n").encode())
        await self._proc.stdin.drain()

    async def _read_response(self, timeout: float = 10.0) -> dict[str, Any]:
        assert self._proc is not None and self._proc.stdout is not None
        try:
            line = await asyncio.wait_for(
                self._proc.stdout.readline(),
                timeout=timeout,
            )
        except asyncio.TimeoutError:
            return {"error": {"code": -32000, "message": "read timeout"}}
        if not line:
            return {"error": {"code": -32001, "message": "EOF from MCP server"}}
        try:
            return json.loads(line.decode())
        except json.JSONDecodeError as exc:
            return {"error": {"code": -32700, "message": f"parse error: {exc}"}}
