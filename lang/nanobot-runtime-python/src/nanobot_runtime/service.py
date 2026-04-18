"""W2.T4: 16 gRPC RuntimeService methods for nanobot-runtime.

Thin OAI-compatible runtime. Wraps AgentSession for Send.
All hook dispatch stubs return allow — wired in W2.T5/T6.
"""
from __future__ import annotations

import logging
import os
import uuid
from datetime import datetime, timezone

logger = logging.getLogger(__name__)

import grpc

from nanobot_runtime._proto.eaasp.runtime.v2 import (
    common_pb2,
    runtime_pb2,
    runtime_pb2_grpc,
)
from nanobot_runtime.provider import OpenAICompatProvider
from nanobot_runtime.session import AgentSession, EventType

_RUNTIME_ID = "eaasp-nanobot-runtime"
_DEPLOYMENT_MODE = os.environ.get("EAASP_DEPLOYMENT_MODE", "shared")


class NanobotRuntimeService(runtime_pb2_grpc.RuntimeServiceServicer):
    def __init__(self) -> None:
        self._sessions: dict[str, AgentSession] = {}
        self._active_session_id: str | None = None

    def _make_provider(self) -> OpenAICompatProvider:
        return OpenAICompatProvider(
            base_url=os.environ.get("OPENAI_BASE_URL", "https://api.openai.com"),
            api_key=os.environ.get("OPENAI_API_KEY", ""),
            model=os.environ.get("OPENAI_MODEL_NAME", "gpt-4o-mini"),
        )

    def _resolve_active(self, context) -> str | None:
        if self._active_session_id and self._active_session_id in self._sessions:
            return self._active_session_id
        context.set_code(grpc.StatusCode.FAILED_PRECONDITION)
        context.set_details("no active session; call Initialize first")
        return None

    async def Initialize(self, request, context):
        payload = request.payload
        sid = payload.session_id if payload.session_id else str(uuid.uuid4())
        provider = self._make_provider()
        self._sessions[sid] = AgentSession(provider=provider, session_id=sid)
        self._active_session_id = sid
        return runtime_pb2.InitializeResponse(
            session_id=sid,
            runtime_id=_RUNTIME_ID,
        )

    async def Send(self, request, context):
        sid = request.session_id or self._active_session_id
        if not sid or sid not in self._sessions:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            context.set_details(f"session {sid!r} not found")
            return
        session = self._sessions[sid]
        content = request.message.content if request.message else ""
        try:
            async for event in session.run(content):
                if event.event_type == EventType.CHUNK:
                    yield runtime_pb2.SendResponse(
                        chunk_type="text",
                        content=event.content,
                    )
                elif event.event_type == EventType.TOOL_CALL:
                    yield runtime_pb2.SendResponse(
                        chunk_type="tool_call",
                        tool_name=event.tool_name,
                        tool_id=event.tool_call_id,
                        content=event.content,
                    )
                elif event.event_type == EventType.TOOL_RESULT:
                    yield runtime_pb2.SendResponse(
                        chunk_type="tool_result",
                        tool_name=event.tool_name,
                        tool_id=event.tool_call_id,
                        content=event.content,
                        is_error=event.is_error,
                    )
                elif event.event_type == EventType.STOP:
                    yield runtime_pb2.SendResponse(
                        chunk_type="done",
                        content=event.content,
                    )
                elif event.event_type == EventType.ERROR:
                    yield runtime_pb2.SendResponse(
                        chunk_type="error",
                        content=event.content,
                        is_error=True,
                    )
        except Exception as exc:
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(exc))

    async def LoadSkill(self, request, context):
        return runtime_pb2.LoadSkillResponse(success=True, error="")

    async def OnToolCall(self, request, context):
        return runtime_pb2.ToolCallAck(decision="allow", mutated_input_json="", reason="")

    async def OnToolResult(self, request, context):
        return runtime_pb2.ToolResultAck(decision="allow", reason="")

    async def OnStop(self, request, context):
        return runtime_pb2.StopAck(decision="allow", reason="")

    async def GetState(self, request, context):
        sid = self._resolve_active(context)
        if sid is None:
            return runtime_pb2.StateResponse()
        return runtime_pb2.StateResponse(
            session_id=sid,
            state_data=b"",
            runtime_id=_RUNTIME_ID,
            state_format="nanobot-stub-v1",
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    async def ConnectMCP(self, request, context):
        from nanobot_runtime.mcp_client import StdioMcpClient

        connected: list[str] = []
        failed: list[str] = []

        for server_spec in request.servers:
            name = server_spec.name
            cmd_str = server_spec.command  # e.g. "npx -y @some/mcp-server"
            cmd = cmd_str.split() if cmd_str else []
            if not cmd:
                failed.append(name)
                continue
            try:
                client = StdioMcpClient(cmd=cmd, server_name=name)
                await client.start()
                tools = await client.list_tools()
                # Store client for lifecycle management (terminate on session close)
                if not hasattr(self, "_mcp_clients"):
                    self._mcp_clients: dict[str, StdioMcpClient] = {}
                self._mcp_clients[name] = client
                # Inject discovered tools into active session
                if self._active_session_id and self._active_session_id in self._sessions:
                    sess = self._sessions[self._active_session_id]
                    oai_tools = [t.to_oai_schema() for t in tools]
                    sess.tools = list(sess.tools) + oai_tools
                connected.append(name)
            except Exception as exc:
                logger.warning("ConnectMCP: failed to connect %s: %s", name, exc)
                failed.append(name)

        success = len(failed) == 0
        return runtime_pb2.ConnectMCPResponse(
            success=success,
            connected=connected,
            failed=failed,
        )

    async def EmitTelemetry(self, request, context):
        return common_pb2.Empty()

    async def GetCapabilities(self, request, context):
        return runtime_pb2.Capabilities(
            runtime_id=_RUNTIME_ID,
            model=os.environ.get("OPENAI_MODEL_NAME", ""),
            context_window=0,
            tools=[],
            supports_native_hooks=False,
            supports_native_mcp=False,
            supports_native_skills=False,
            cost_per_1k_tokens=0.0,
            credential_mode=0,  # CredentialMode.DIRECT = 0
            strengths=["oai-compat"],
            limitations=["stub-hooks"],
            tier="aligned",
            deployment_mode=_DEPLOYMENT_MODE,
        )

    async def Terminate(self, request, context):
        sid = self._active_session_id
        if sid and sid in self._sessions:
            sess = self._sessions.pop(sid)
            await sess.provider.aclose()
            self._active_session_id = None
        # Close all MCP subprocess clients
        for client in getattr(self, "_mcp_clients", {}).values():
            try:
                await client.close()
            except Exception:
                pass
        self._mcp_clients = {}
        return common_pb2.Empty()

    async def RestoreState(self, request, context):
        self._active_session_id = request.session_id
        return common_pb2.Empty()

    async def Health(self, request, context):
        return runtime_pb2.HealthResponse(
            healthy=True,
            runtime_id=_RUNTIME_ID,
            checks={},
        )

    async def DisconnectMcp(self, request, context):
        return common_pb2.Empty()

    async def PauseSession(self, request, context):
        sid = self._resolve_active(context)
        if sid is None:
            return runtime_pb2.StateResponse()
        return runtime_pb2.StateResponse(
            session_id=sid,
            state_data=b"",
            runtime_id=_RUNTIME_ID,
            state_format="nanobot-stub-v1",
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    async def ResumeSession(self, request, context):
        self._active_session_id = request.session_id
        return common_pb2.Empty()

    async def EmitEvent(self, request, context):
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details("ADR-V2-001 pending")
        return common_pb2.Empty()
