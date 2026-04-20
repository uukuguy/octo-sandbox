"""16 gRPC RuntimeService methods for pydantic-ai-runtime."""
from __future__ import annotations

import logging
import os
import uuid
from datetime import datetime, timezone

logger = logging.getLogger(__name__)

import grpc

from pydantic_ai_runtime._proto.eaasp.runtime.v2 import (
    common_pb2,
    runtime_pb2,
    runtime_pb2_grpc,
)
from pydantic_ai_runtime.provider import make_provider
from pydantic_ai_runtime.session import AgentSession, EventType

_RUNTIME_ID = "eaasp-pydantic-ai-runtime"
_DEPLOYMENT_MODE = os.environ.get("EAASP_DEPLOYMENT_MODE", "shared")


class PydanticAiRuntimeService(runtime_pb2_grpc.RuntimeServiceServicer):
    def __init__(self) -> None:
        self._sessions: dict[str, AgentSession] = {}
        self._active_session_id: str | None = None

    def _resolve_active(self, context) -> str | None:
        if self._active_session_id and self._active_session_id in self._sessions:
            return self._active_session_id
        context.set_code(grpc.StatusCode.FAILED_PRECONDITION)
        context.set_details("no active session; call Initialize first")
        return None

    async def Initialize(self, request, context):
        payload = request.payload
        sid = payload.session_id if payload.session_id else str(uuid.uuid4())
        provider = make_provider()
        self._sessions[sid] = AgentSession(provider=provider, session_id=sid)
        self._active_session_id = sid
        return runtime_pb2.InitializeResponse(session_id=sid, runtime_id=_RUNTIME_ID)

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
                # ADR-V2-021: chunk_type is the proto ChunkType enum (int on wire).
                if event.event_type == EventType.CHUNK:
                    yield runtime_pb2.SendResponse(
                        chunk_type=common_pb2.CHUNK_TYPE_TEXT_DELTA,
                        content=event.content,
                    )
                elif event.event_type == EventType.TOOL_CALL:
                    yield runtime_pb2.SendResponse(
                        chunk_type=common_pb2.CHUNK_TYPE_TOOL_START,
                        tool_name=event.tool_name,
                        tool_id=event.tool_call_id,
                        content=event.content,
                    )
                elif event.event_type == EventType.TOOL_RESULT:
                    yield runtime_pb2.SendResponse(
                        chunk_type=common_pb2.CHUNK_TYPE_TOOL_RESULT,
                        tool_name=event.tool_name,
                        tool_id=event.tool_call_id,
                        content=event.content,
                        is_error=event.is_error,
                    )
                elif event.event_type == EventType.STOP:
                    yield runtime_pb2.SendResponse(
                        chunk_type=common_pb2.CHUNK_TYPE_DONE,
                        content=event.content,
                    )
                elif event.event_type == EventType.ERROR:
                    yield runtime_pb2.SendResponse(
                        chunk_type=common_pb2.CHUNK_TYPE_ERROR,
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
            state_format="pydantic-ai-stub-v1",
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    async def ConnectMCP(self, request, context):
        return runtime_pb2.ConnectMCPResponse(success=True, connected=[], failed=[])

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
            credential_mode=0,
            strengths=["pydantic-ai", "oai-compat"],
            limitations=["stub-hooks", "stub-mcp"],
            tier="aligned",
            deployment_mode=_DEPLOYMENT_MODE,
        )

    async def Terminate(self, request, context):
        sid = self._active_session_id
        if sid and sid in self._sessions:
            sess = self._sessions.pop(sid)
            await sess.provider.aclose()
            self._active_session_id = None
        return common_pb2.Empty()

    async def RestoreState(self, request, context):
        self._active_session_id = request.session_id
        return common_pb2.Empty()

    async def Health(self, request, context):
        return runtime_pb2.HealthResponse(healthy=True, runtime_id=_RUNTIME_ID, checks={})

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
            state_format="pydantic-ai-stub-v1",
            created_at=datetime.now(timezone.utc).isoformat(),
        )

    async def ResumeSession(self, request, context):
        self._active_session_id = request.session_id
        return common_pb2.Empty()

    async def EmitEvent(self, request, context):
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details("ADR-V2-001 pending")
        return common_pb2.Empty()
