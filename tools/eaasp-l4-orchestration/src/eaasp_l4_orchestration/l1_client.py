"""gRPC client for L1 Runtime — wraps ``RuntimeServiceStub``.

Phase 0.5 S1.T2: Provides ``L1RuntimeClient`` used by ``SessionOrchestrator``
to call real L1 runtimes via gRPC instead of writing stubbed events.

Key design:
- Channel is created lazily per endpoint (one per runtime_id mapping).
- ``initialize`` returns a ``SessionHandle`` dict.
- ``send`` is an async generator yielding ``SendResponse`` chunks (server-stream).
- ``terminate`` uses the implicit-session pattern (no session_id in proto).
- All errors are wrapped in ``L1RuntimeError`` for uniform upstream handling.
"""

from __future__ import annotations

import os
from collections.abc import AsyncIterator
from typing import Any

import grpc

from ._proto.eaasp.runtime.v2 import common_pb2, runtime_pb2, runtime_pb2_grpc

# Default L1 runtime gRPC endpoint.
L1_GRID_RUNTIME_DEFAULT = os.environ.get(
    "EAASP_L1_GRID_RUNTIME_URL", "127.0.0.1:50051"
)
L1_CLAUDE_CODE_RUNTIME_DEFAULT = os.environ.get(
    "EAASP_L1_CLAUDE_CODE_RUNTIME_URL", "127.0.0.1:50052"
)
L1_HERMES_RUNTIME_DEFAULT = os.environ.get(
    "EAASP_L1_HERMES_RUNTIME_URL", "127.0.0.1:50053"
)
L1_NANOBOT_RUNTIME_DEFAULT = os.environ.get(
    "EAASP_L1_NANOBOT_RUNTIME_URL", "127.0.0.1:50054"
)
L1_GOOSE_RUNTIME_DEFAULT = os.environ.get(
    "EAASP_L1_GOOSE_RUNTIME_URL", "127.0.0.1:50063"
)

# runtime_id → default gRPC endpoint mapping.
RUNTIME_ENDPOINTS: dict[str, str] = {
    "grid-runtime": L1_GRID_RUNTIME_DEFAULT,
    "claude-code-runtime": L1_CLAUDE_CODE_RUNTIME_DEFAULT,
    "hermes-runtime": L1_HERMES_RUNTIME_DEFAULT,
    "nanobot-runtime": L1_NANOBOT_RUNTIME_DEFAULT,
    "goose-runtime": L1_GOOSE_RUNTIME_DEFAULT,
}


class L1RuntimeError(Exception):
    """Raised when an L1 gRPC call fails."""

    def __init__(self, runtime_id: str, method: str, detail: str) -> None:
        self.runtime_id = runtime_id
        self.method = method
        self.detail = detail
        super().__init__(f"L1 {runtime_id}.{method}: {detail}")


class L1RuntimeClient:
    """Thin wrapper around a gRPC channel to one L1 runtime."""

    def __init__(self, endpoint: str, runtime_id: str = "unknown") -> None:
        self._endpoint = endpoint
        self._runtime_id = runtime_id
        self._channel: grpc.aio.Channel | None = None
        self._stub: runtime_pb2_grpc.RuntimeServiceStub | None = None

    async def _ensure_channel(self) -> runtime_pb2_grpc.RuntimeServiceStub:
        if self._stub is None:
            self._channel = grpc.aio.insecure_channel(self._endpoint)
            self._stub = runtime_pb2_grpc.RuntimeServiceStub(self._channel)
        return self._stub

    async def close(self) -> None:
        if self._channel is not None:
            await self._channel.close()
            self._channel = None
            self._stub = None

    # ── Initialize ──────────────────────────────────────────────────────────

    async def initialize(
        self, payload_dict: dict[str, Any]
    ) -> dict[str, str]:
        """Call L1 ``Initialize(SessionPayload)`` → ``{session_id, runtime_id}``.

        ``payload_dict`` is the JSON-serializable dict built by
        ``context_assembly.build_session_payload``. We convert it to a proto
        ``SessionPayload`` message before sending.
        """
        stub = await self._ensure_channel()
        proto_payload = _dict_to_session_payload(payload_dict)
        request = runtime_pb2.InitializeRequest(payload=proto_payload)

        try:
            response: runtime_pb2.InitializeResponse = await stub.Initialize(
                request, timeout=30.0
            )
        except grpc.aio.AioRpcError as exc:
            raise L1RuntimeError(
                self._runtime_id,
                "Initialize",
                f"gRPC {exc.code()}: {exc.details()}",
            ) from exc

        return {
            "session_id": response.session_id,
            "runtime_id": response.runtime_id,
        }

    # ── Send (server-streaming) ─────────────────────────────────────────────

    async def send(
        self,
        session_id: str,
        content: str,
        message_type: str = "text",
    ) -> AsyncIterator[dict[str, Any]]:
        """Call L1 ``Send(SendRequest)`` → stream of response chunks.

        Yields dicts with keys: chunk_type, content, tool_name, tool_id,
        is_error, error (if present).
        """
        stub = await self._ensure_channel()
        user_msg = runtime_pb2.UserMessage(
            content=content,
            message_type=message_type,
        )
        request = runtime_pb2.SendRequest(
            session_id=session_id,
            message=user_msg,
        )

        try:
            stream = stub.Send(request, timeout=300.0)
            async for chunk in stream:
                yield _send_response_to_dict(chunk)
        except grpc.aio.AioRpcError as exc:
            raise L1RuntimeError(
                self._runtime_id,
                "Send",
                f"gRPC {exc.code()}: {exc.details()}",
            ) from exc

    # ── ConnectMCP ──────────────────────────────────────────────────────────

    async def connect_mcp(
        self,
        session_id: str,
        servers: list[dict[str, Any]],
    ) -> dict[str, Any]:
        """Call L1 ``ConnectMCP`` to attach MCP servers to a session.

        Args:
            session_id: Target session ID (L1's session_id).
            servers: List of server config dicts with keys:
                name, transport, command?, args?, url?, env?

        Returns:
            ``{"success": bool, "connected": [str], "failed": [str]}``

        Raises:
            L1RuntimeError: On gRPC failure.
        """
        proto_servers = []
        for s in servers:
            cfg = runtime_pb2.McpServerConfig(
                name=s.get("name", ""),
                transport=s.get("transport", "stdio"),
            )
            if s.get("command"):
                cfg.command = s["command"]
            if s.get("args"):
                cfg.args.extend(s["args"])
            if s.get("url"):
                cfg.url = s["url"]
            if s.get("env"):
                for k, v in s["env"].items():
                    cfg.env[k] = v
            proto_servers.append(cfg)

        request = runtime_pb2.ConnectMCPRequest(
            session_id=session_id,
            servers=proto_servers,
        )
        try:
            stub = await self._ensure_channel()
            resp = await stub.ConnectMCP(request, timeout=30.0)
            return {
                "success": resp.success,
                "connected": list(resp.connected),
                "failed": list(resp.failed),
            }
        except grpc.aio.AioRpcError as exc:
            raise L1RuntimeError(
                self._runtime_id,
                "ConnectMCP",
                f"gRPC {exc.code()}: {exc.details()}",
            ) from exc

    # ── Terminate ───────────────────────────────────────────────────────────

    async def terminate(self) -> None:
        """Call L1 ``Terminate(Empty)``."""
        stub = await self._ensure_channel()
        try:
            await stub.Terminate(common_pb2.Empty(), timeout=10.0)
        except grpc.aio.AioRpcError as exc:
            raise L1RuntimeError(
                self._runtime_id,
                "Terminate",
                f"gRPC {exc.code()}: {exc.details()}",
            ) from exc


# ── Factory ────────────────────────────────────────────────────────────────

def create_l1_client(runtime_id: str) -> L1RuntimeClient:
    """Resolve runtime_id to endpoint and return a client.

    Endpoint resolution order:
    1. ``EAASP_L1_{RUNTIME_ID_UPPER}_URL`` env var
    2. ``RUNTIME_ENDPOINTS`` built-in default
    """
    env_key = f"EAASP_L1_{runtime_id.upper().replace('-', '_')}_URL"
    endpoint = os.environ.get(env_key) or RUNTIME_ENDPOINTS.get(runtime_id)
    if endpoint is None:
        raise L1RuntimeError(
            runtime_id,
            "resolve",
            f"no endpoint configured (set {env_key} or add to RUNTIME_ENDPOINTS)",
        )
    return L1RuntimeClient(endpoint, runtime_id=runtime_id)


# ── Proto conversion helpers ───────────────────────────────────────────────


def _dict_to_session_payload(d: dict[str, Any]) -> common_pb2.SessionPayload:
    """Convert a JSON dict → proto SessionPayload.

    Only fills fields that the dict actually contains. Missing fields get
    proto defaults (empty string / empty repeated / false).
    """
    payload = common_pb2.SessionPayload()

    # Session metadata
    payload.session_id = str(d.get("session_id", ""))
    payload.user_id = str(d.get("user_id", ""))
    payload.runtime_id = str(d.get("runtime_id", ""))
    payload.created_at = str(d.get("created_at", ""))

    # Budget flags
    payload.allow_trim_p5 = bool(d.get("allow_trim_p5", True))
    payload.allow_trim_p4 = bool(d.get("allow_trim_p4", False))
    payload.allow_trim_p3 = bool(d.get("allow_trim_p3", False))

    # P1 — PolicyContext
    pc = d.get("policy_context") or {}
    if pc:
        payload.policy_context.org_unit = str(pc.get("org_unit", ""))
        payload.policy_context.policy_version = str(pc.get("policy_version", ""))
        payload.policy_context.deploy_timestamp = str(pc.get("deploy_timestamp", ""))
        for k, v in (pc.get("quotas") or {}).items():
            payload.policy_context.quotas[str(k)] = str(v)
        for hook_dict in pc.get("hooks") or []:
            hook = payload.policy_context.hooks.add()
            hook.hook_id = str(hook_dict.get("hook_id", ""))
            hook.hook_type = str(hook_dict.get("hook_type", ""))
            hook.condition = str(hook_dict.get("condition", ""))
            hook.action = str(hook_dict.get("action", ""))
            hook.precedence = int(hook_dict.get("precedence", 0))
            hook.scope = str(hook_dict.get("scope", ""))

    # P2 — EventContext
    ec = d.get("event_context") or {}
    if ec:
        payload.event_context.event_id = str(ec.get("event_id", ""))
        payload.event_context.event_type = str(ec.get("event_type", ""))
        payload.event_context.severity = str(ec.get("severity", ""))
        payload.event_context.source = str(ec.get("source", ""))
        payload.event_context.payload_json = str(ec.get("payload_json", ""))
        payload.event_context.timestamp = str(ec.get("timestamp", ""))

    # P3 — MemoryRefs
    for ref_dict in d.get("memory_refs") or []:
        ref = payload.memory_refs.add()
        ref.memory_id = str(ref_dict.get("memory_id", ""))
        ref.memory_type = str(ref_dict.get("memory_type", ""))
        ref.relevance_score = float(ref_dict.get("relevance_score", 0.0))
        ref.content = str(ref_dict.get("content") or ref_dict.get("summary", ""))
        ref.source_session_id = str(ref_dict.get("source_session_id", ""))
        ref.created_at = str(ref_dict.get("created_at", ""))

    # P4 — SkillInstructions
    si = d.get("skill_instructions") or {}
    if si:
        payload.skill_instructions.skill_id = str(si.get("skill_id", ""))
        payload.skill_instructions.name = str(si.get("name", ""))
        payload.skill_instructions.content = str(si.get("content", ""))
        for hook_dict in si.get("frontmatter_hooks") or []:
            hook = payload.skill_instructions.frontmatter_hooks.add()
            # Map skill scoped hook fields → proto ScopedHook fields:
            #   name → hook_id
            #   type (command/prompt) → hook_type
            #   command or prompt → action
            #   scope (PreToolUse/PostToolUse/Stop) → condition
            hook.hook_id = str(hook_dict.get("name", hook_dict.get("hook_id", "")))
            hook.hook_type = str(hook_dict.get("type", hook_dict.get("hook_type", "")))
            hook.condition = str(hook_dict.get("scope", hook_dict.get("condition", "")))
            hook.action = str(
                hook_dict.get("command", "")
                or hook_dict.get("prompt", "")
                or hook_dict.get("action", "")
            )
            hook.precedence = int(hook_dict.get("precedence", 0))
        for dep in si.get("dependencies") or []:
            payload.skill_instructions.dependencies.append(str(dep))
        # D87 L1 metadata: required_tools propagate to L1 runtime.
        for tool in si.get("required_tools") or []:
            payload.skill_instructions.required_tools.append(str(tool))

    # P5 — UserPreferences
    up = d.get("user_preferences") or {}
    if up:
        payload.user_preferences.user_id = str(up.get("user_id", ""))
        payload.user_preferences.language = str(up.get("language", ""))
        payload.user_preferences.timezone = str(up.get("timezone", ""))
        for k, v in (up.get("prefs") or {}).items():
            payload.user_preferences.prefs[str(k)] = str(v)

    return payload


def _send_response_to_dict(chunk: runtime_pb2.SendResponse) -> dict[str, Any]:
    """Convert a proto SendResponse to a plain dict."""
    result: dict[str, Any] = {
        "chunk_type": chunk.chunk_type,
        "content": chunk.content,
    }
    if chunk.tool_name:
        result["tool_name"] = chunk.tool_name
    if chunk.tool_id:
        result["tool_id"] = chunk.tool_id
    if chunk.is_error:
        result["is_error"] = True
    if chunk.HasField("error"):
        result["error"] = {
            "code": chunk.error.code,
            "message": chunk.error.message,
        }
    return result
