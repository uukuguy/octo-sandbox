"""gRPC RuntimeService implementation — EAASP v2.0 L1 contract.

Implements the 16-method RuntimeService (12 MUST + 4 OPTIONAL +
1 PLACEHOLDER) over the single ``eaasp.runtime.v2`` proto package.

Tier: T1 Harness (native hooks + MCP + skills via claude-agent-sdk).

Key v2 differences vs v1:
- SessionPayload is a 5-block structured priority stack (P1→P5):
    P1 PolicyContext, P2 EventContext, P3 MemoryRefs,
    P4 SkillInstructions, P5 UserPreferences.
- Lifecycle methods (GetState / Terminate / Pause / Resume / Health /
  GetCapabilities / EmitEvent) take common_pb2.Empty. In the per_session
  deployment model each runtime instance typically owns a single session,
  so we route Empty-input calls to the most recently initialised session
  (``_active_session_id``). Multi-session callers that need to target a
  specific session should explicitly Initialize→...→Terminate each one.
- EmitEvent is a PLACEHOLDER (ADR-V2-001). We raise UNIMPLEMENTED with
  ``"ADR-V2-001 pending"`` as the gRPC details string.
"""

from __future__ import annotations

import json
import logging
import time

import grpc

from ._proto.eaasp.runtime.v2 import common_pb2, runtime_pb2, runtime_pb2_grpc
from .config import RuntimeConfig
from .hook_executor import HookExecutor
from .mapper import chunk_to_proto, telemetry_batch_to_proto
from .sdk_wrapper import SdkWrapper
from .session import SessionManager
from .skill_loader import SkillLoader
from .state_manager import STATE_FORMAT, deserialize_session, serialize_session
from .telemetry import TelemetryCollector

logger = logging.getLogger(__name__)


def _managed_hooks_to_rules_json(policy_context) -> str:
    """Convert a PolicyContext.hooks list into the legacy rules-JSON format.

    HookExecutor was designed around a ``{"rules": [...]}`` JSON document
    loaded from v1 managed_hooks_json. To stay source-compatible we project
    v2 ManagedHook messages back into that shape.

    The v2 ManagedHook condition field is a CEL/DSL expression (deferred in
    Phase 0). For the MVP we translate two conventions seen in tests and
    HookExecutor into the rule format:

    - ``hook_type == "pre_tool_call"`` with ``action == "deny"`` and a
      ``condition`` of the form ``"tool:^bash$;input:rm -rf"`` -> compiled
      into tool_pattern / input_pattern.
    - ``hook_type == "stop"`` / ``action == "deny"`` -> force-continue stop.

    This mapping is intentionally narrow; richer expression evaluation
    lands in Phase 1.
    """
    if not policy_context or not policy_context.hooks:
        return ""

    rules = []
    for idx, h in enumerate(policy_context.hooks):
        rule = {
            "id": h.hook_id or f"rule-{idx}",
            "name": h.hook_id or f"rule-{idx}",
            "hook_type": h.hook_type or "pre_tool_call",
            "action": h.action or "allow",
            "reason": h.hook_id or "",
            "tool_pattern": "",
            "input_pattern": "",
            "enabled": True,
        }
        # Parse simple "tool:<regex>;input:<substr>" condition sugar.
        for part in (h.condition or "").split(";"):
            part = part.strip()
            if part.startswith("tool:"):
                rule["tool_pattern"] = part[len("tool:") :].strip()
            elif part.startswith("input:"):
                rule["input_pattern"] = part[len("input:") :].strip()
            elif part.startswith("reason:"):
                rule["reason"] = part[len("reason:") :].strip()
        rules.append(rule)
    return json.dumps({"rules": rules})


def _extract_user_id(payload) -> str:
    """Pull user id from either the flat session_id block or P5."""
    if payload.user_id:
        return payload.user_id
    if payload.user_preferences and payload.user_preferences.user_id:
        return payload.user_preferences.user_id
    # Also check free-form prefs map (spec §8 allows carrying user_id here)
    prefs = (
        dict(payload.user_preferences.prefs)
        if payload.user_preferences and payload.user_preferences.prefs
        else {}
    )
    return prefs.get("user_id", "")


class RuntimeServiceImpl(runtime_pb2_grpc.RuntimeServiceServicer):
    """EAASP v2.0 L1 RuntimeService — Python T1 Harness."""

    def __init__(self, config: RuntimeConfig):
        self.config = config
        self.sdk = SdkWrapper(config)
        self.session_mgr = SessionManager()
        # Per-session components indexed by session_id
        self._hooks: dict[str, HookExecutor] = {}
        self._telemetry: dict[str, TelemetryCollector] = {}
        self._skills: dict[str, SkillLoader] = {}
        # Active session for Empty-input lifecycle methods (per_session tier)
        self._active_session_id: str | None = None
        self._start_time = time.time()

    # ── helpers ──

    def _get_or_404(self, session_id: str, context):
        session = self.session_mgr.get(session_id)
        if session is None:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            context.set_details(f"Session {session_id} not found")
        return session

    def _resolve_active(self, context) -> str | None:
        """Return the active session_id or flag NOT_FOUND on the context."""
        if self._active_session_id and self.session_mgr.get(
            self._active_session_id
        ):
            return self._active_session_id
        context.set_code(grpc.StatusCode.NOT_FOUND)
        context.set_details("no active session; call Initialize first")
        return None

    # ── 12 MUST methods ──────────────────────────────────────────

    # 1. Initialize
    async def Initialize(self, request, context):
        payload = request.payload

        user_id = _extract_user_id(payload)
        org_unit = (
            payload.policy_context.org_unit if payload.policy_context else ""
        )
        managed_hooks_json = _managed_hooks_to_rules_json(payload.policy_context)

        # Cache the P4 skill if the orchestrator pre-populated one.
        context_dict: dict[str, str] = {}
        if payload.policy_context and payload.policy_context.policy_version:
            context_dict["policy_version"] = payload.policy_context.policy_version
        if payload.event_context and payload.event_context.event_id:
            context_dict["event_id"] = payload.event_context.event_id
            context_dict["event_type"] = payload.event_context.event_type

        session = self.session_mgr.create(
            user_id=user_id,
            user_role="",  # v2 drops explicit role; carried by P1 if needed
            org_unit=org_unit,
            managed_hooks_json=managed_hooks_json,
            context=context_dict,
            hook_bridge_url="",
            telemetry_endpoint="",
        )
        sid = session.session_id

        hook_exe = HookExecutor()
        hook_exe.load_rules(managed_hooks_json)
        self._hooks[sid] = hook_exe

        self._telemetry[sid] = TelemetryCollector(
            session_id=sid,
            runtime_id=self.config.runtime_id,
            user_id=user_id,
        )
        self._telemetry[sid].record("session_start")

        skill_loader = SkillLoader()
        # Eagerly materialise the P4 skill (if any) into the session.
        skill = payload.skill_instructions
        if skill and (skill.skill_id or skill.content):
            skill_loader.load(
                skill_id=skill.skill_id,
                name=skill.name,
                frontmatter_yaml="",  # v2 passes structured scoped_hooks
                prose=skill.content,
            )
            session.skills.append(
                {"skill_id": skill.skill_id, "name": skill.name}
            )
        self._skills[sid] = skill_loader

        # Ingest P3 memory refs into session context for telemetry provenance
        if payload.memory_refs:
            for mref in payload.memory_refs:
                session.telemetry_events.append(
                    {
                        "type": "memory_ref_injected",
                        "memory_id": mref.memory_id,
                        "memory_type": mref.memory_type,
                    }
                )

        self._active_session_id = sid
        logger.info("Session initialized: %s (user=%s)", sid, user_id)
        return runtime_pb2.InitializeResponse(
            session_id=sid,
            runtime_id=self.config.runtime_id,
        )

    # 2. Send (server-streaming)
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

        skill_loader = self._skills.get(sid)
        system_prompt = None
        if skill_loader and skill_loader.count > 0:
            system_prompt = skill_loader.all_system_prompt_fragments()

        async for chunk in self.sdk.send_message(
            prompt=message.content, system_prompt=system_prompt
        ):
            yield chunk_to_proto(chunk)

    # 3. LoadSkill
    async def LoadSkill(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.LoadSkillResponse(
                success=False, error="session not found"
            )

        skill = request.skill
        skill_loader = self._skills.get(session.session_id)
        if skill_loader is not None:
            skill_loader.load(
                skill_id=skill.skill_id,
                name=skill.name,
                frontmatter_yaml="",
                prose=skill.content,
            )

        session.skills.append(
            {"skill_id": skill.skill_id, "name": skill.name}
        )

        tc = self._telemetry.get(session.session_id)
        if tc:
            tc.record(
                "skill_loaded",
                payload={"skill_id": skill.skill_id},
            )

        logger.info(
            "Skill loaded: %s in session %s", skill.name, session.session_id
        )
        return runtime_pb2.LoadSkillResponse(success=True)

    # 4. OnToolCall
    async def OnToolCall(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        if hook_exe is not None:
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
            return runtime_pb2.ToolCallAck(
                decision=decision, mutated_input_json="", reason=reason
            )

        return runtime_pb2.ToolCallAck(
            decision="allow", mutated_input_json="", reason=""
        )

    # 5. OnToolResult
    async def OnToolResult(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        if hook_exe is not None:
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
            return runtime_pb2.ToolResultAck(decision=decision, reason=reason)

        return runtime_pb2.ToolResultAck(decision="allow", reason="")

    # 6. OnStop
    async def OnStop(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        if hook_exe is not None:
            decision, feedback = hook_exe.evaluate_stop()
            tc = self._telemetry.get(sid)
            if tc:
                tc.record("stop_evaluated", payload={"decision": decision})
            # v1 used "complete"/"continue"; map to v2 allow/deny semantics
            # (allow = stop, deny = force continue).
            ack_decision = "allow" if decision == "complete" else "deny"
            return runtime_pb2.StopAck(decision=ack_decision, reason=feedback)

        return runtime_pb2.StopAck(decision="allow", reason="")

    # 7. GetState (Empty-input → StateResponse)
    async def GetState(self, request, context):
        sid = self._resolve_active(context)
        if sid is None:
            return runtime_pb2.StateResponse()
        session = self.session_mgr.get(sid)
        if session is None:
            return runtime_pb2.StateResponse()

        return runtime_pb2.StateResponse(
            session_id=session.session_id,
            state_data=serialize_session(session),
            runtime_id=self.config.runtime_id,
            state_format=STATE_FORMAT,
            created_at=str(session.created_at),
        )

    # 8. ConnectMCP
    async def ConnectMCP(self, request, context):
        session = self._get_or_404(request.session_id, context)
        if session is None:
            return runtime_pb2.ConnectMCPResponse(success=False)

        connected: list[str] = []
        for server in request.servers:
            session.mcp_servers.append(server.name)
            connected.append(server.name)

        tc = self._telemetry.get(session.session_id)
        if tc:
            tc.record("mcp_connected", payload={"servers": connected})

        return runtime_pb2.ConnectMCPResponse(
            success=True, connected=connected, failed=[]
        )

    # 9. EmitTelemetry (client → runtime push; returns Empty)
    async def EmitTelemetry(self, request, context):
        tc = self._telemetry.get(request.session_id)
        if tc is not None:
            for ev in request.events:
                try:
                    payload = (
                        json.loads(ev.payload_json) if ev.payload_json else {}
                    )
                except json.JSONDecodeError:
                    payload = {"raw": ev.payload_json}
                tc.record(ev.event_type, payload=payload)
        return common_pb2.Empty()

    # 10. GetCapabilities
    async def GetCapabilities(self, request, context):
        return runtime_pb2.Capabilities(
            runtime_id=self.config.runtime_id,
            model=self.config.anthropic_model_name or "",
            context_window=200000,
            tools=["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
            supports_native_hooks=True,
            supports_native_mcp=True,
            supports_native_skills=True,
            cost_per_1k_tokens=0.003,
            credential_mode=runtime_pb2.Capabilities.DIRECT,
            strengths=["native-hooks", "claude-agent-sdk"],
            limitations=["subprocess-cli-roundtrip"],
            tier=self.config.tier or "harness",
            deployment_mode="per_session",
        )

    # 11. Terminate
    async def Terminate(self, request, context):
        sid = self._resolve_active(context)
        if sid is None:
            return common_pb2.Empty()
        self._teardown_session(sid)
        return common_pb2.Empty()

    # 12. RestoreState
    async def RestoreState(self, request, context):
        try:
            data = deserialize_session(request.state_data)
            session = self.session_mgr.restore(data)
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
            self._active_session_id = sid
            return common_pb2.Empty()
        except Exception as e:  # noqa: BLE001
            context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
            context.set_details(str(e))
            return common_pb2.Empty()

    # ── 4 OPTIONAL methods ───────────────────────────────────────

    async def Health(self, request, context):
        return runtime_pb2.HealthResponse(
            healthy=True,
            runtime_id=self.config.runtime_id,
            checks={
                "sdk": "ok",
                "sessions": str(self.session_mgr.count),
                "uptime": f"{time.time() - self._start_time:.0f}s",
            },
        )

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
        return common_pb2.Empty()

    async def PauseSession(self, request, context):
        sid = self._resolve_active(context)
        if sid is None:
            return runtime_pb2.StateResponse()
        if not self.session_mgr.pause(sid):
            context.set_code(grpc.StatusCode.FAILED_PRECONDITION)
            context.set_details("session not active")
            return runtime_pb2.StateResponse()

        session = self.session_mgr.get(sid)
        return runtime_pb2.StateResponse(
            session_id=sid,
            state_data=serialize_session(session) if session else b"",
            runtime_id=self.config.runtime_id,
            state_format=STATE_FORMAT,
            created_at=str(session.created_at) if session else "",
        )

    async def ResumeSession(self, request, context):
        # Resume can arrive with either an in-memory paused session or an
        # opaque state_data blob to reinflate.
        if request.state_data:
            await self.RestoreState(request, context)
        target = request.session_id or self._active_session_id or ""
        if target and self.session_mgr.resume(target):
            self._active_session_id = target
            return common_pb2.Empty()
        context.set_code(grpc.StatusCode.NOT_FOUND)
        context.set_details("session not paused/available")
        return common_pb2.Empty()

    # ── PLACEHOLDER: EmitEvent (ADR-V2-001 pending) ──────────────

    async def EmitEvent(self, request, context):
        context.set_code(grpc.StatusCode.UNIMPLEMENTED)
        context.set_details("ADR-V2-001 pending")
        return common_pb2.Empty()

    # ── internal ─────────────────────────────────────────────────

    def _teardown_session(self, sid: str) -> None:
        tc = self._telemetry.pop(sid, None)
        if tc is not None:
            tc.record("session_end")
            # Flush for side-effects / external endpoint (best effort).
            telemetry_batch_to_proto(tc.flush(), session_id=sid)
        self._hooks.pop(sid, None)
        self._skills.pop(sid, None)
        self.session_mgr.terminate(sid)
        if self._active_session_id == sid:
            self._active_session_id = None
