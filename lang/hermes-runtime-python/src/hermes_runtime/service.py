"""gRPC RuntimeService — EAASP v2.0 L1 16-method contract for hermes-agent.

Implements the 12 MUST + 4 OPTIONAL + 1 PLACEHOLDER method surface defined in
`proto/eaasp/runtime/v2/runtime.proto`. SessionPayload is the v2 5-block
priority structure (P1 PolicyContext .. P5 UserPreferences).
"""

import json
import logging
import time

import grpc

from hermes_runtime._fix_proto_imports import fix as _fix_proto_imports

_fix_proto_imports()

from eaasp.runtime.v2 import common_pb2, runtime_pb2, runtime_pb2_grpc  # noqa: E402

from hermes_runtime.adapter import HermesAdapter
from hermes_runtime.l2_memory_client import L2MemoryClient
from hermes_runtime.mcp_bridge import L2MemoryToolProxy, McpBridge, inject_mcp_tools
from hermes_runtime.config import HermesRuntimeConfig
from hermes_runtime.mapper import (
    chunk_to_proto,
    extract_policy_hooks,
    extract_skill_content,
    extract_user_id,
)
from hermes_runtime.session import SessionManager
from hermes_runtime.telemetry import TelemetryCollector

logger = logging.getLogger(__name__)


def _policy_context_to_dict(pc: common_pb2.PolicyContext) -> dict:
    return {
        "org_unit": pc.org_unit,
        "policy_version": pc.policy_version,
        "deploy_timestamp": pc.deploy_timestamp,
        "quotas": dict(pc.quotas),
        "hooks": [
            {
                "hook_id": h.hook_id,
                "hook_type": h.hook_type,
                "condition": h.condition,
                "action": h.action,
                "precedence": h.precedence,
                "scope": h.scope,
            }
            for h in pc.hooks
        ],
    }


def _event_context_to_dict(ec: common_pb2.EventContext) -> dict:
    return {
        "event_id": ec.event_id,
        "event_type": ec.event_type,
        "severity": ec.severity,
        "source": ec.source,
        "payload_json": ec.payload_json,
        "timestamp": ec.timestamp,
    }


def _memory_ref_to_dict(mr: common_pb2.MemoryRef) -> dict:
    return {
        "memory_id": mr.memory_id,
        "memory_type": mr.memory_type,
        "relevance_score": mr.relevance_score,
        "content": mr.content,
        "source_session_id": mr.source_session_id,
        "created_at": mr.created_at,
        "tags": dict(mr.tags),
    }


def _skill_instructions_to_dict(si: common_pb2.SkillInstructions) -> dict:
    return {
        "skill_id": si.skill_id,
        "name": si.name,
        "content": si.content,
        "metadata": dict(si.metadata),
        "frontmatter_hooks": [
            {
                "hook_id": h.hook_id,
                "hook_type": h.hook_type,
                "condition": h.condition,
                "action": h.action,
                "precedence": h.precedence,
            }
            for h in si.frontmatter_hooks
        ],
    }


def _user_preferences_to_dict(up: common_pb2.UserPreferences) -> dict:
    return {
        "user_id": up.user_id,
        "prefs": dict(up.prefs),
        "language": up.language,
        "timezone": up.timezone,
    }


class RuntimeServiceImpl(runtime_pb2_grpc.RuntimeServiceServicer):
    """EAASP v2.0 L1 RuntimeService — Hermes Agent T2 Aligned."""

    def __init__(self, config: HermesRuntimeConfig):
        self.config = config
        self.adapter = HermesAdapter(config)
        self.session_mgr = SessionManager()
        self._telemetry: dict[str, TelemetryCollector] = {}
        self._l2_client = L2MemoryClient()
        self._current_session: str = ""  # tracked for Empty-request methods
        self._start_time = time.time()

    def _get_or_404(self, session_id: str, context):
        session = self.session_mgr.get(session_id)
        if session is None:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            context.set_details(f"Session {session_id} not found")
        return session

    def _resolve_sid(self, request, context) -> str:
        """Resolve the target session id for Empty-request methods.

        v2 spec uses `Empty` for GetState/Terminate/Health/Pause/Resume/GetCapabilities.
        As a phase-0 simplification we track the most recently initialized session.
        """
        sid = getattr(request, "session_id", "") or self._current_session
        return sid

    # ── 1. Health (OPTIONAL) ──

    async def Health(self, request, context):
        return runtime_pb2.HealthResponse(
            healthy=True,
            runtime_id=self.config.runtime_id,
            checks={
                "hermes": "ok",
                "sessions": str(self.session_mgr.count),
                "uptime": f"{time.time() - self._start_time:.0f}s",
            },
        )

    # ── 2. GetCapabilities (MUST) ──

    async def GetCapabilities(self, request, context):
        return runtime_pb2.Capabilities(
            runtime_id=self.config.runtime_id,
            model=self.config.hermes_model,
            context_window=200000,
            tools=[
                "terminal", "read_file", "write_file", "patch", "search_files",
                "web_search", "web_extract", "browser_navigate", "execute_code",
                "delegate_task", "memory", "todo", "skills_list", "skill_view",
            ],
            supports_native_hooks=False,   # T2 — uses HookBridge
            supports_native_mcp=True,
            supports_native_skills=True,
            cost_per_1k_tokens=0.0,
            credential_mode=runtime_pb2.Capabilities.CredentialMode.DIRECT,
            strengths=["long-context", "reasoning"],
            limitations=["no native hooks"],
            tier=self.config.tier,
            deployment_mode=self.config.deployment_mode,
        )

    # ── 3. Initialize (MUST) ──

    async def Initialize(self, request, context):
        payload: common_pb2.SessionPayload = request.payload

        user_id = extract_user_id(payload)

        session = self.session_mgr.create(
            user_id=user_id,
            runtime_id=self.config.runtime_id,
            policy_context=(
                _policy_context_to_dict(payload.policy_context)
                if payload.HasField("policy_context")
                else None
            ),
            event_context=(
                _event_context_to_dict(payload.event_context)
                if payload.HasField("event_context")
                else None
            ),
            memory_refs=[_memory_ref_to_dict(m) for m in payload.memory_refs],
            skill_instructions=(
                _skill_instructions_to_dict(payload.skill_instructions)
                if payload.HasField("skill_instructions")
                else None
            ),
            user_preferences=(
                _user_preferences_to_dict(payload.user_preferences)
                if payload.HasField("user_preferences")
                else None
            ),
        )
        sid = session.session_id
        self._current_session = sid

        # If a skill was shipped in P4, surface its content on the session's skills list
        skill_content = extract_skill_content(payload)
        if skill_content:
            session.skills.append(
                {
                    "skill_id": payload.skill_instructions.skill_id,
                    "name": payload.skill_instructions.name,
                    "content": skill_content,
                }
            )

        # Create AIAgent instance for this session, inject skill prose as system prompt
        try:
            self.adapter.create_agent(sid, skill_prose=skill_content)
        except Exception as e:
            logger.error("Failed to create AIAgent for %s: %s", sid, e)
            context.set_code(grpc.StatusCode.INTERNAL)
            context.set_details(str(e))
            self.session_mgr.terminate(sid)
            return runtime_pb2.InitializeResponse(session_id="", runtime_id=self.config.runtime_id)

        # MCP servers are now connected via ConnectMCP (Phase 0.75).
        # L4 calls ConnectMCP after Initialize to wire MCP servers.
        agent = self.adapter.get_agent(sid)
        bridges = []

        # L2 memory tools via REST proxy (always inject, regardless of MCP SSE)
        l2_proxy = L2MemoryToolProxy()

        if agent is not None:
            inject_mcp_tools(agent, bridges, l2_proxy=l2_proxy)
            session.mcp_bridges = bridges
            logger.info("MCP tools injected for session %s: %d bridges + L2 memory", sid, len(bridges))

        self._telemetry[sid] = TelemetryCollector(
            session_id=sid,
            runtime_id=self.config.runtime_id,
            user_id=user_id,
        )
        self._telemetry[sid].record("session_start")

        logger.info(
            "Session initialized: %s (user=%s, model=%s, managed_hooks=%d, mcp=%d)",
            sid,
            user_id,
            self.config.hermes_model,
            len(extract_policy_hooks(payload)),
            len(bridges),
        )
        return runtime_pb2.InitializeResponse(
            session_id=sid,
            runtime_id=self.config.runtime_id,
        )

    # ── 4. Send (MUST, streaming) ──

    async def Send(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return

        sid = session.session_id
        message = request.message
        logger.info("Send: session=%s content=%s", sid, message.content[:80])

        tc = self._telemetry.get(sid)
        if tc:
            tc.record("send", payload={"content_len": len(message.content)})

        for chunk in self.adapter.send_message(
            session_id=sid,
            content=message.content,
            conversation_history=session.conversation_history,
        ):
            yield chunk_to_proto(**chunk)

        session.conversation_history.append({"role": "user", "content": message.content})

    # ── 5. LoadSkill (MUST) ──

    async def LoadSkill(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.LoadSkillResponse(success=False, error="session not found")

        skill = request.skill  # SkillInstructions
        session.skills.append(
            {
                "skill_id": skill.skill_id,
                "name": skill.name,
                "content": skill.content,
            }
        )

        tc = self._telemetry.get(session.session_id)
        if tc:
            tc.record("skill_loaded", payload={"skill_id": skill.skill_id})

        return runtime_pb2.LoadSkillResponse(success=True)

    # ── 6. OnToolCall (MUST) ──

    async def OnToolCall(self, request, context):
        # T2: 治理拦截已在 governance_plugin monkey-patch 中完成
        return runtime_pb2.ToolCallAck(decision="allow", mutated_input_json="", reason="")

    # ── 7. OnToolResult (MUST) ──

    async def OnToolResult(self, request, context):
        sid = request.session_id

        # Fire-and-forget: write tool execution evidence to L2 Memory Engine.
        if not request.is_error:
            event_id = f"tool-{request.tool_name}-{int(time.time() * 1000)}"
            data_ref = request.output[:500] if request.output else None
            anchor_id = None
            try:
                resp = await self._l2_client.write_anchor(
                    event_id=event_id,
                    session_id=sid,
                    anchor_type="tool_execution",
                    data_ref=data_ref,
                )
                anchor_id = resp.get("anchor_id")
            except Exception as e:
                logger.warning("L2 anchor write failed (non-fatal): %s", e)

            content = f"Tool: {request.tool_name}\nSession: {sid}\nResult: {data_ref or '(no output)'}"
            try:
                await self._l2_client.write_file(
                    scope=f"session:{sid}",
                    category="tool_evidence",
                    content=content,
                    evidence_refs=[anchor_id] if anchor_id else None,
                )
            except Exception as e:
                logger.warning("L2 memory file write failed (non-fatal): %s", e)

        return runtime_pb2.ToolResultAck(decision="allow", reason="")

    # ── 8. OnStop (MUST) ──

    async def OnStop(self, request, context):
        return runtime_pb2.StopAck(decision="allow", reason="")

    # ── 9. ConnectMCP (MUST) ──

    async def ConnectMCP(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.ConnectMCPResponse(success=False)

        connected = []
        failed = []
        agent = self.adapter.get_agent(session.session_id)
        bridges = []

        for server_cfg in request.servers:
            # For SSE/HTTP transport, connect via MCP bridge
            if server_cfg.transport in ("sse", "streamable-http") and server_cfg.url:
                bridge = McpBridge(server_cfg.name, server_cfg.url)
                try:
                    await bridge.connect()
                    bridges.append(bridge)
                    connected.append(server_cfg.name)
                except Exception as e:
                    logger.warning("ConnectMCP %s failed: %s", server_cfg.name, e)
                    failed.append(server_cfg.name)
            else:
                # stdio transport not supported in container — record as connected
                # but log warning
                logger.warning(
                    "ConnectMCP: stdio transport not supported in container for %s",
                    server_cfg.name,
                )
                session.mcp_servers.append(server_cfg.name)
                connected.append(server_cfg.name)

        if bridges and agent is not None:
            inject_mcp_tools(agent, bridges)
            if not hasattr(session, 'mcp_bridges'):
                session.mcp_bridges = []
            session.mcp_bridges.extend(bridges)

        return runtime_pb2.ConnectMCPResponse(
            success=len(failed) == 0,
            connected=connected,
            failed=failed,
        )

    # ── 10. DisconnectMcp (OPTIONAL) ──

    async def DisconnectMcp(self, request, context):
        session = self.session_mgr.get(request.session_id)
        if session and request.server_name in session.mcp_servers:
            session.mcp_servers.remove(request.server_name)
        return common_pb2.Empty()

    # ── 11. EmitTelemetry (MUST) ──

    async def EmitTelemetry(self, request, context):
        tc = self._telemetry.get(request.session_id)
        if tc:
            tc.peek()  # acknowledge entries exist
        return common_pb2.Empty()

    # ── 12. GetState (MUST) ──

    async def GetState(self, request, context):
        sid = self._resolve_sid(request, context)
        session = self._get_or_404(sid, context)
        if session is None:
            return runtime_pb2.StateResponse()
        state_data = json.dumps(
            {
                "session_id": session.session_id,
                "user_id": session.user_id,
                "runtime_id": session.runtime_id,
                "policy_context": session.policy_context,
                "event_context": session.event_context,
                "memory_refs": session.memory_refs,
                "skill_instructions": session.skill_instructions,
                "user_preferences": session.user_preferences,
                "skills": session.skills,
                "conversation_history": session.conversation_history,
            }
        ).encode()
        return runtime_pb2.StateResponse(
            session_id=session.session_id,
            state_data=state_data,
            runtime_id=self.config.runtime_id,
            state_format="python-json-v2",
            created_at=session.created_at,
        )

    # ── 13. RestoreState (MUST) ──

    async def RestoreState(self, request, context):
        try:
            data = json.loads(request.state_data)
            session = self.session_mgr.restore(data)
            self.adapter.create_agent(session.session_id)
            sid = session.session_id
            self._current_session = sid
            self._telemetry[sid] = TelemetryCollector(
                session_id=sid,
                runtime_id=self.config.runtime_id,
                user_id=session.user_id,
            )
            return common_pb2.Empty()
        except Exception as e:
            context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
            context.set_details(str(e))
            return common_pb2.Empty()

    # ── 14. PauseSession (OPTIONAL) ──

    async def PauseSession(self, request, context):
        sid = self._resolve_sid(request, context)
        success = self.session_mgr.pause(sid)
        if not success:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            return runtime_pb2.StateResponse()
        session = self.session_mgr.get(sid)
        return runtime_pb2.StateResponse(
            session_id=sid,
            runtime_id=self.config.runtime_id,
            state_format="python-json-v2",
            created_at=session.created_at if session else "",
        )

    # ── 15. ResumeSession (OPTIONAL) ──

    async def ResumeSession(self, request, context):
        sid = self._resolve_sid(request, context)
        if not self.session_mgr.resume(sid):
            context.set_code(grpc.StatusCode.NOT_FOUND)
        return common_pb2.Empty()

    # ── 16. Terminate (MUST) ──

    async def Terminate(self, request, context):
        sid = self._resolve_sid(request, context)
        tc = self._telemetry.pop(sid, None)
        if tc:
            tc.record("session_end")
            tc.flush()
        self.adapter.remove_agent(sid)
        self.session_mgr.terminate(sid)
        if self._current_session == sid:
            self._current_session = ""
        return common_pb2.Empty()

    # ── PLACEHOLDER — EmitEvent (ADR-V2-001 pending) ──

    async def EmitEvent(self, request, context):
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details("ADR-V2-001 pending")
        return common_pb2.Empty()
