"""gRPC RuntimeService implementation — 16-method EAASP L1 contract.

Integrates: SessionManager, HookExecutor, TelemetryCollector, SkillLoader,
SdkWrapper, and mapper for full T1 Harness functionality.
"""

from __future__ import annotations

import logging
import time

import grpc

from ._proto.eaasp.common.v1 import common_pb2
from ._proto.eaasp.runtime.v1 import runtime_pb2, runtime_pb2_grpc
from .config import RuntimeConfig
from .hook_executor import HookExecutor
from .mapper import chunk_to_proto, telemetry_batch_to_proto, telemetry_to_proto
from .sdk_wrapper import SdkWrapper
from .session import SessionManager
from .skill_loader import SkillLoader
from .state_manager import STATE_FORMAT, deserialize_session, serialize_session
from .telemetry import TelemetryCollector

logger = logging.getLogger(__name__)


class RuntimeServiceImpl(runtime_pb2_grpc.RuntimeServiceServicer):
    """EAASP L1 RuntimeService — Python T1 Harness."""

    def __init__(self, config: RuntimeConfig):
        self.config = config
        self.sdk = SdkWrapper(config)
        self.session_mgr = SessionManager()
        # Per-session components indexed by session_id
        self._hooks: dict[str, HookExecutor] = {}
        self._telemetry: dict[str, TelemetryCollector] = {}
        self._skills: dict[str, SkillLoader] = {}
        self._start_time = time.time()

    def _get_or_404(self, session_id: str, context):
        """Get session or set NOT_FOUND on context. Returns session or None."""
        session = self.session_mgr.get(session_id)
        if session is None:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            context.set_details(f"Session {session_id} not found")
        return session

    # ── 1. Health ──

    async def Health(self, request, context):
        return runtime_pb2.HealthStatus(
            healthy=True,
            runtime_id=self.config.runtime_id,
            checks={
                "sdk": "ok",
                "sessions": str(self.session_mgr.count),
                "uptime": f"{time.time() - self._start_time:.0f}s",
            },
        )

    # ── 2. GetCapabilities ──

    async def GetCapabilities(self, request, context):
        return runtime_pb2.CapabilityManifest(
            runtime_id=self.config.runtime_id,
            runtime_name=self.config.runtime_name,
            tier=self.config.tier,
            model=self.config.anthropic_model_name,
            context_window=200000,
            supported_tools=["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
            native_hooks=True,
            native_mcp=True,
            native_skills=True,
            requires_hook_bridge=False,
            cost=runtime_pb2.CostEstimate(
                input_cost_per_1k=0.003,
                output_cost_per_1k=0.015,
            ),
        )

    # ── 3. Initialize ──

    async def Initialize(self, request, context):
        payload = request.payload
        session = self.session_mgr.create(
            user_id=payload.user_id,
            user_role=payload.user_role,
            org_unit=payload.org_unit,
            managed_hooks_json=payload.managed_hooks_json,
            context=dict(payload.context) if payload.context else {},
            hook_bridge_url=payload.hook_bridge_url,
            telemetry_endpoint=payload.telemetry_endpoint,
        )
        sid = session.session_id

        # Initialize per-session components
        hook_exe = HookExecutor()
        hook_exe.load_rules(payload.managed_hooks_json)
        self._hooks[sid] = hook_exe

        self._telemetry[sid] = TelemetryCollector(
            session_id=sid,
            runtime_id=self.config.runtime_id,
            user_id=payload.user_id,
        )
        self._telemetry[sid].record("session_start")

        self._skills[sid] = SkillLoader()

        logger.info("Session initialized: %s (user=%s)", sid, payload.user_id)
        return runtime_pb2.InitializeResponse(session_id=sid)

    # ── 4. Send (streaming) ──

    async def Send(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return

        sid = session.session_id
        message = request.message
        logger.info("Send: session=%s content=%s", sid, message.content[:50])

        tc = self._telemetry.get(sid)
        if tc:
            tc.record("send", payload={"content_len": len(message.content)})

        # Inject skill system prompts if any
        skill_loader = self._skills.get(sid)
        system_prompt = None
        if skill_loader and skill_loader.count > 0:
            system_prompt = skill_loader.all_system_prompt_fragments()

        async for chunk in self.sdk.send_message(
            prompt=message.content, system_prompt=system_prompt
        ):
            yield chunk_to_proto(chunk)

    # ── 5. LoadSkill ──

    async def LoadSkill(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.LoadSkillResponse(
                success=False, error="session not found"
            )

        skill = request.skill
        skill_loader = self._skills.get(session.session_id)
        if skill_loader:
            skill_loader.load(
                skill_id=skill.skill_id,
                name=skill.name,
                frontmatter_yaml=skill.frontmatter_yaml,
                prose=skill.prose,
            )

        session.skills.append({"skill_id": skill.skill_id, "name": skill.name})

        tc = self._telemetry.get(session.session_id)
        if tc:
            tc.record("skill_loaded", payload={"skill_id": skill.skill_id})

        logger.info(
            "Skill loaded: %s in session %s", skill.name, session.session_id
        )
        return runtime_pb2.LoadSkillResponse(success=True)

    # ── 6. OnToolCall ──

    async def OnToolCall(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        if hook_exe:
            decision, reason = hook_exe.evaluate_pre_tool_call(
                request.tool_name, request.input_json
            )
            tc = self._telemetry.get(sid)
            if tc:
                tc.record(
                    "hook_evaluated",
                    payload={
                        "hook_type": "pre_tool_call",
                        "tool": request.tool_name,
                        "decision": decision,
                    },
                )
            return common_pb2.HookDecision(
                decision=decision, reason=reason, modified_input=""
            )

        return common_pb2.HookDecision(
            decision="allow", reason="", modified_input=""
        )

    # ── 7. OnToolResult ──

    async def OnToolResult(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        if hook_exe:
            decision, reason = hook_exe.evaluate_post_tool_result(
                request.tool_name, request.output, request.is_error
            )
            tc = self._telemetry.get(sid)
            if tc:
                tc.record(
                    "hook_evaluated",
                    payload={
                        "hook_type": "post_tool_result",
                        "tool": request.tool_name,
                        "decision": decision,
                    },
                )
            return common_pb2.HookDecision(
                decision=decision, reason=reason, modified_input=""
            )

        return common_pb2.HookDecision(
            decision="allow", reason="", modified_input=""
        )

    # ── 8. OnStop ──

    async def OnStop(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        if hook_exe:
            decision, feedback = hook_exe.evaluate_stop()
            tc = self._telemetry.get(sid)
            if tc:
                tc.record(
                    "stop_evaluated",
                    payload={"decision": decision},
                )
            return common_pb2.StopDecision(
                decision=decision, feedback=feedback
            )

        return common_pb2.StopDecision(decision="complete", feedback="")

    # ── 9. ConnectMcp ──

    async def ConnectMcp(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.ConnectMcpResponse(success=False)

        connected = []
        for server in request.servers:
            session.mcp_servers.append(server.name)
            connected.append(server.name)

        tc = self._telemetry.get(session.session_id)
        if tc:
            tc.record(
                "mcp_connected", payload={"servers": connected}
            )

        return runtime_pb2.ConnectMcpResponse(
            success=True, connected=connected, failed=[]
        )

    # ── 10. DisconnectMcp ──

    async def DisconnectMcp(self, request, context):
        session = self.session_mgr.get(request.session_id)
        if session and request.server_name in session.mcp_servers:
            session.mcp_servers.remove(request.server_name)
            tc = self._telemetry.get(session.session_id)
            if tc:
                tc.record(
                    "mcp_disconnected",
                    payload={"server": request.server_name},
                )
        return runtime_pb2.DisconnectMcpResponse(success=True)

    # ── 11. EmitTelemetry ──

    async def EmitTelemetry(self, request, context):
        tc = self._telemetry.get(request.session_id)
        if tc:
            entries = tc.peek()
            return telemetry_batch_to_proto(entries)
        return common_pb2.TelemetryBatch(events=[])

    # ── 12. GetState ──

    async def GetState(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.SessionState()

        return runtime_pb2.SessionState(
            session_id=session.session_id,
            state_data=serialize_session(session),
            runtime_id=self.config.runtime_id,
            created_at=str(session.created_at),
            state_format=STATE_FORMAT,
        )

    # ── 13. RestoreState ──

    async def RestoreState(self, request, context):
        try:
            data = deserialize_session(request.state_data)
            session = self.session_mgr.restore(data)

            # Re-initialize per-session components
            sid = session.session_id
            hook_exe = HookExecutor()
            hook_exe.load_rules(session.managed_hooks_json)
            self._hooks[sid] = hook_exe
            self._telemetry[sid] = TelemetryCollector(
                session_id=sid,
                runtime_id=self.config.runtime_id,
                user_id=session.user_id,
            )
            self._skills[sid] = SkillLoader()

            return runtime_pb2.InitializeResponse(session_id=sid)
        except Exception as e:
            context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
            context.set_details(str(e))
            return runtime_pb2.InitializeResponse(session_id="")

    # ── 14. PauseSession ──

    async def PauseSession(self, request, context):
        success = self.session_mgr.pause(request.session_id)
        return runtime_pb2.PauseResponse(success=success)

    # ── 15. ResumeSession ──

    async def ResumeSession(self, request, context):
        success = self.session_mgr.resume(request.session_id)
        if success:
            return runtime_pb2.ResumeResponse(
                success=True, session_id=request.session_id
            )
        context.set_code(grpc.StatusCode.NOT_FOUND)
        return runtime_pb2.ResumeResponse(success=False, session_id="")

    # ── 16. Terminate ──

    async def Terminate(self, request, context):
        sid = request.session_id
        telemetry_batch = None

        # Collect final telemetry before terminating
        tc = self._telemetry.pop(sid, None)
        if tc:
            tc.record("session_end")
            entries = tc.flush()
            telemetry_batch = telemetry_batch_to_proto(entries)

        # Clean up per-session components
        self._hooks.pop(sid, None)
        self._skills.pop(sid, None)

        session = self.session_mgr.terminate(sid)
        success = session is not None

        return runtime_pb2.TerminateResponse(
            success=success, final_telemetry=telemetry_batch
        )
