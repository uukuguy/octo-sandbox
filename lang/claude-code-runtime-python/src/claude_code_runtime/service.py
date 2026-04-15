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
from .hook_substitution import (
    HookSubstitutionError,
    HookVars,
    substitute_scoped_hooks,
)
from .l2_memory_client import L2MemoryClient
from .mapper import chunk_to_proto, telemetry_batch_to_proto
from .scoped_command_executor import ScopedCommandExecutor, ScopedHookBundle
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
        # S3.T5 — per-session scoped hook executor + substituted-hook bundle.
        # Populated in Initialize when P4 frontmatter_hooks is non-empty.
        self._scoped_bundles: dict[str, ScopedHookBundle] = {}
        self._scoped_executors: dict[str, ScopedCommandExecutor] = {}
        # Active session for Empty-input lifecycle methods (per_session tier)
        self._active_session_id: str | None = None
        self._start_time = time.time()
        # L2 Memory Engine client for tool execution evidence writes
        self._l2_client = L2MemoryClient()

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

        # M1 (reviewer R2) — proto3 submessage presence uses HasField, NOT
        # truthy fallback. Accessing an unset singular submessage returns a
        # default instance that is always truthy in google.protobuf, so the
        # previous `if payload.policy_context:` branch would always enter.
        has_policy_context = payload.HasField("policy_context")
        has_event_context = payload.HasField("event_context")

        org_unit = payload.policy_context.org_unit if has_policy_context else ""
        # NOTE: empty fallback is "" (not "[]"). HookExecutor.load_rules treats
        # ""/None/"{}" as zero rules; "[]" would parse as a list and fail
        # `data.get("rules", [])` with AttributeError. Reviewer M1 introduced
        # the wrong fallback string and the certifier caught it (S4.T2 E2E run).
        managed_hooks_json = (
            _managed_hooks_to_rules_json(payload.policy_context)
            if has_policy_context
            else ""
        )

        # Cache the P4 skill if the orchestrator pre-populated one.
        context_dict: dict[str, str] = {}
        if has_policy_context and payload.policy_context.policy_version:
            context_dict["policy_version"] = payload.policy_context.policy_version
        if has_event_context and payload.event_context.event_id:
            context_dict["event_id"] = payload.event_context.event_id
            context_dict["event_type"] = payload.event_context.event_type

        # D2-py — Extract P3 memory_refs into a plain-dict projection that
        # SessionManager.create() can persist on the Session dataclass.
        memory_refs_list = [
            {
                "memory_id": m.memory_id,
                "memory_type": m.memory_type,
                "relevance_score": m.relevance_score,
                "content": m.content,
                "source_session_id": m.source_session_id,
                "created_at": m.created_at,
                "tags": dict(m.tags) if m.tags else {},
            }
            for m in payload.memory_refs
        ]

        # D2-py — Extract P1 policy_context metadata (read-only; hook
        # execution still happens via HookExecutor + managed_hooks_json).
        policy_context_dict: dict | None = None
        if has_policy_context:
            pc = payload.policy_context
            policy_context_dict = {
                "org_unit": pc.org_unit,
                "policy_version": pc.policy_version,
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

        session = self.session_mgr.create(
            user_id=user_id,
            user_role="",  # v2 drops explicit role; carried by P1 if needed
            org_unit=org_unit,
            managed_hooks_json=managed_hooks_json,
            context=context_dict,
            hook_bridge_url="",
            telemetry_endpoint="",
            memory_refs=memory_refs_list,
            policy_context=policy_context_dict,
        )
        sid = session.session_id

        # D1-py — Log policy_context metadata (hooks count / org_unit /
        # policy_version). Mirrors the Rust harness D1 log line so the
        # certifier / verify script can assert both runtimes emit it.
        if policy_context_dict is not None:
            logger.info(
                "GridHarness(py): policy_context metadata "
                "session_id=%s org_unit=%s policy_version=%s hooks_count=%d (D1)",
                sid,
                policy_context_dict.get("org_unit", ""),
                policy_context_dict.get("policy_version", ""),
                len(policy_context_dict.get("hooks", [])),
            )
        logger.info(
            "GridHarness(py): memory_refs injected session_id=%s count=%d (D2)",
            sid,
            len(memory_refs_list),
        )

        hook_exe = HookExecutor()
        hook_exe.load_rules(managed_hooks_json)

        # S3.T2 — Load P4 scoped hooks from skill frontmatter into the
        # same HookExecutor. Scoped hooks use tool_pattern/input_pattern
        # matching just like P1 managed hooks (deny-always-wins).
        skill_for_hooks = payload.skill_instructions
        if skill_for_hooks and skill_for_hooks.frontmatter_hooks:
            scoped_rules = self._scoped_hooks_to_rules(
                skill_for_hooks.frontmatter_hooks
            )
            if scoped_rules:
                hook_exe.load_rules(json.dumps({"rules": scoped_rules}))
                logger.info(
                    "Loaded %d scoped hooks from P4 skill frontmatter "
                    "session_id=%s",
                    len(scoped_rules),
                    sid,
                )

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

        # MCP server configuration is now handled by ConnectMCP (Phase 0.75).
        # L4 will call ConnectMCP after Initialize to wire MCP servers.
        has_si = payload.HasField("skill_instructions")
        skill_deps = list(payload.skill_instructions.dependencies) if has_si else []
        mcp_deps = [d for d in skill_deps if d.startswith("mcp:")]
        if mcp_deps:
            logger.info(
                "Initialize: session=%s has %d MCP dependencies "
                "(will be connected via ConnectMCP): %s",
                session.session_id, len(mcp_deps), mcp_deps,
            )

        # Create isolated runtime workspace for this session.
        # Uses EAASP_RUNTIME_WORKSPACE (platform-level base dir) or falls back
        # to tempdir. In production, this is a container-internal mount point.
        import os
        base_workspace = os.environ.get("EAASP_RUNTIME_WORKSPACE", "")
        if base_workspace:
            workspace = os.path.join(base_workspace, sid)
        else:
            import tempfile
            workspace = tempfile.mkdtemp(prefix=f"eaasp-workspace-{sid}-")
        os.makedirs(workspace, exist_ok=True)
        session.workspace = workspace
        logger.info("Runtime workspace: %s", workspace)

        # S3.T5 (G3) — Build HookVars, substitute ${SKILL_DIR}/etc, and store
        # a per-session ScopedCommandExecutor + ScopedHookBundle. This runs
        # AFTER workspace setup so SKILL_DIR materialization can land under
        # {workspace}/skill/ for inline skill content. Mirrors Rust harness
        # G1. Per-hook fail-open: if substitution fails for one hook, only
        # that hook is skipped; others still register.
        if skill_for_hooks and skill_for_hooks.frontmatter_hooks:
            import pathlib

            skill_id_for_vars = skill_for_hooks.skill_id or ""
            skill_dir: str | None = None

            # Resolution order (ADR-V2-006 §5):
            # 1) Inline skill content → materialize to {workspace}/skill/SKILL.md
            # 2) EAASP_SKILL_CACHE_DIR/{skill_id} if directory exists
            # 3) Leave None (hooks referencing ${SKILL_DIR} will be skipped)
            if skill_for_hooks.content:
                try:
                    materialized = pathlib.Path(workspace) / "skill"
                    materialized.mkdir(parents=True, exist_ok=True)
                    (materialized / "SKILL.md").write_text(
                        skill_for_hooks.content, encoding="utf-8"
                    )
                    skill_dir = str(materialized)
                except OSError as e:
                    logger.error(
                        "Scoped hooks: SkillDir materialize failed session_id=%s "
                        "error=%s (hooks referencing ${SKILL_DIR} will be skipped)",
                        sid,
                        e,
                    )
                    skill_dir = None
            if skill_dir is None:
                cache_root = os.environ.get("EAASP_SKILL_CACHE_DIR", "")
                if cache_root and skill_id_for_vars:
                    candidate = pathlib.Path(cache_root) / skill_id_for_vars
                    if candidate.exists():
                        skill_dir = str(candidate)

            hook_vars = HookVars(
                skill_dir=skill_dir,
                session_dir=workspace,
                runtime_dir=os.environ.get("EAASP_RUNTIME_DIR", "") or None,
            )

            # Project proto ScopedHook -> dict shape expected by
            # substitute_scoped_hooks (keys: command/prompt). We pass
            # ``action`` as ``command`` so the substitutor touches it, then
            # restore the ``action`` key on the resolved hook.
            substituted: list[dict] = []
            for h in skill_for_hooks.frontmatter_hooks:
                raw: dict = {
                    "hook_id": h.hook_id,
                    "hook_type": h.hook_type,
                    "condition": h.condition,
                    "action": h.action,
                    "precedence": h.precedence,
                }
                sub_input = dict(raw)
                sub_input["command"] = sub_input["action"]
                try:
                    resolved = substitute_scoped_hooks([sub_input], hook_vars)
                    resolved_cmd = resolved[0].get("command", raw["action"])
                    resolved_hook = dict(raw)
                    resolved_hook["action"] = resolved_cmd
                    substituted.append(resolved_hook)
                except HookSubstitutionError as e:
                    logger.warning(
                        "Scoped hook %s skipped: %s — %s "
                        "(error_kind=%s action=skip)",
                        raw["hook_id"] or "?",
                        type(e).__name__,
                        e,
                        type(e).__name__.replace("Error", "").lower(),
                    )

            bundle = ScopedHookBundle.from_hooks(substituted)
            self._scoped_bundles[sid] = bundle
            self._scoped_executors[sid] = ScopedCommandExecutor(timeout_secs=5.0)
            logger.info(
                "Scoped command executor ready session_id=%s count=%d "
                "(pre=%d post=%d stop=%d) skill_dir=%s",
                sid,
                len(substituted),
                len(bundle.pre),
                len(bundle.post),
                len(bundle.stop),
                skill_dir or "<unresolved>",
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

        # D2-py — On the FIRST Send of a session, prepend a system-prompt
        # preamble built from P3 memory_refs so the underlying claude-agent
        # sees prior-session context. Subsequent Sends reuse the SDK's own
        # conversation state, so we must NOT re-inject (avoid duplication).
        #
        # Injection strategy: option 1 from the blueprint — prepend to the
        # system_prompt string passed to ClaudeAgentOptions.system_prompt.
        # This is the simplest integration point: SdkWrapper already wires
        # system_prompt straight through to ClaudeAgentOptions, and the
        # claude-agent-sdk `query()` call does not accept a pre-seeded
        # history list, so a leading system-role message would have no
        # delivery vehicle. Prepending to system_prompt keeps the preamble
        # in the same "out-of-band" channel the SDK already honors.
        if not session.preamble_injected and session.memory_refs:
            memory_preamble_lines = [
                "## Prior memories from previous sessions",
                "",
            ]
            for mref in session.memory_refs:
                memory_preamble_lines.append(
                    f"- [{mref.get('memory_type', '')}] {mref.get('content', '')}"
                )
            memory_preamble = "\n".join(memory_preamble_lines) + "\n"

            if system_prompt:
                system_prompt = memory_preamble + "\n" + system_prompt
            else:
                system_prompt = memory_preamble
            session.preamble_injected = True
            logger.info(
                "Injected memory_refs preamble into system_prompt "
                "session_id=%s memory_refs=%d",
                sid,
                len(session.memory_refs),
            )

        # Pass MCP servers + isolated workspace to SDK.
        mcp_servers = getattr(session, "mcp_servers_config", None)
        workspace = getattr(session, "workspace", None)
        async for chunk in self.sdk.send_message(
            prompt=message.content, system_prompt=system_prompt,
            mcp_servers=mcp_servers, cwd=workspace,
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

        # S3.T5 (G3) — Scoped PreToolUse hook dispatch. Runs BEFORE the
        # pattern-based HookExecutor so a scoped deny short-circuits. Uses
        # ADR-V2-006 §2.1 envelope + §3 env vars. Fail-open per §7.
        scoped_decision = await self._dispatch_scoped_pre_tool_use(
            sid, request.tool_name, request.input_json
        )
        if scoped_decision is not None and scoped_decision.action == "deny":
            tc = self._telemetry.get(sid)
            if tc:
                tc.record(
                    "hook_evaluated",
                    payload={
                        "hook_type": "pre_tool_call",
                        "tool": request.tool_name,
                        "decision": "deny",
                        "source": "scoped",
                    },
                )
            return runtime_pb2.ToolCallAck(
                decision="deny",
                mutated_input_json="",
                reason=scoped_decision.reason or "denied by scoped hook",
            )

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

        # S3.T5 (G3) — Scoped PostToolUse dispatch BEFORE pattern-based
        # HookExecutor. Deny wins and short-circuits, but L2 evidence writes
        # below are skipped in the deny path.
        scoped_decision = await self._dispatch_scoped_post_tool_use(
            sid, request.tool_name, request.output, request.is_error
        )
        if scoped_decision is not None and scoped_decision.action == "deny":
            tc = self._telemetry.get(sid)
            if tc:
                tc.record(
                    "hook_evaluated",
                    payload={
                        "hook_type": "post_tool_result",
                        "tool": request.tool_name,
                        "decision": "deny",
                        "source": "scoped",
                    },
                )
            return runtime_pb2.ToolResultAck(
                decision="deny",
                reason=scoped_decision.reason or "denied by scoped hook",
            )

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
        else:
            decision = "allow"
            reason = ""

        # Fire-and-forget: write tool execution evidence to L2 Memory Engine.
        # Only for successful (non-error) tool calls. L2 failure is non-fatal.
        if self._l2_client and not request.is_error:
            event_id = f"tool-{request.tool_name}-{int(time.time() * 1000)}"
            # Truncate output to avoid oversized payloads
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

            # Write memory file so memory_search (FTS5) can find this evidence.
            # Anchors alone are not searchable — only memory_files have FTS index.
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

        return runtime_pb2.ToolResultAck(decision=decision, reason=reason)

    # 6. OnStop
    async def OnStop(self, request, context):
        sid = request.session_id
        hook_exe = self._hooks.get(sid)

        # S3.T5 (G3) — Scoped Stop dispatch BEFORE pattern-based HookExecutor.
        # On deny, emit a force-continue ack with the hook's reason as
        # feedback; this mirrors the "continue" semantics the v1 HookExecutor
        # uses and aligns with the Rust ScopedStopHookBridge
        # InjectAndContinue behavior.
        scoped_decision = await self._dispatch_scoped_stop(sid)
        if scoped_decision is not None and scoped_decision.action == "deny":
            tc = self._telemetry.get(sid)
            if tc:
                tc.record(
                    "stop_evaluated",
                    payload={"decision": "deny", "source": "scoped"},
                )
            return runtime_pb2.StopAck(
                decision="deny",
                reason=scoped_decision.reason or "stop denied by scoped hook",
            )

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
        failed: list[str] = []

        # Build mcp_servers_config dict in SDK-expected format.
        # Initialize from existing config (may have been set earlier).
        mcp_config = session.mcp_servers_config or {}

        for server in request.servers:
            try:
                if server.transport == "stdio" and server.command:
                    entry: dict = {"command": server.command, "args": list(server.args)}
                    if server.env:
                        entry["env"] = dict(server.env)
                    mcp_config[server.name] = entry
                elif server.transport in ("sse", "streamable-http") and server.url:
                    mcp_config[server.name] = {"url": server.url}
                    if server.env:
                        mcp_config[server.name]["env"] = dict(server.env)
                else:
                    logger.warning(
                        "ConnectMCP: unsupported config for %s "
                        "(transport=%s, command=%s, url=%s)",
                        server.name, server.transport, server.command, server.url,
                    )
                    failed.append(server.name)
                    continue
                connected.append(server.name)
                session.mcp_servers.append(server.name)
            except Exception as e:
                logger.warning("ConnectMCP: failed to configure %s: %s", server.name, e)
                failed.append(server.name)

        session.mcp_servers_config = mcp_config if mcp_config else None
        logger.info(
            "ConnectMCP: session=%s connected=%s failed=%s",
            session.session_id, connected, failed,
        )

        tc = self._telemetry.get(session.session_id)
        if tc:
            tc.record("mcp_connected", payload={"connected": connected, "failed": failed})

        return runtime_pb2.ConnectMCPResponse(
            success=len(failed) == 0,
            connected=connected,
            failed=failed,
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

    # S3.T5 (G3) — scoped-hook dispatch helpers. Each returns None when the
    # session has no scoped hooks for this point, a ScopedHookDecision
    # otherwise. First matching hook that denies short-circuits the chain
    # ("deny wins"). Precedence ordering is handled by ScopedHookBundle.matching.

    async def _dispatch_scoped_pre_tool_use(
        self, sid: str, tool_name: str, input_json: str
    ):
        """Run all matching PreToolUse scoped hooks. Return first deny, else allow."""
        bundle = self._scoped_bundles.get(sid)
        executor = self._scoped_executors.get(sid)
        if bundle is None or executor is None:
            return None
        hooks = bundle.matching("PreToolUse", tool_name)
        if not hooks:
            return None

        from datetime import datetime, timezone

        try:
            tool_args = json.loads(input_json) if input_json else {}
        except (json.JSONDecodeError, ValueError):
            tool_args = {}
        if not isinstance(tool_args, dict):
            tool_args = {"raw": tool_args}

        skill_id = ""
        loader = self._skills.get(sid)
        if loader is not None:
            skill_id = loader.first_skill_id()

        envelope = {
            "event": "PreToolUse",
            "session_id": sid,
            "skill_id": skill_id,
            "tool_name": tool_name,
            "tool_args": tool_args,
            "created_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        }
        env_extras = {
            "GRID_SESSION_ID": sid,
            "GRID_TOOL_NAME": tool_name,
            "GRID_SKILL_ID": skill_id,
            "GRID_EVENT": "PreToolUse",
        }

        last_decision = None
        for hook in hooks:
            env_per_hook = dict(env_extras)
            # Thread hook_id into envelope for log-correlation only
            # (ADR §2.5 forward-compat: unknown keys are ignored by hooks).
            env_with_id = dict(envelope)
            env_with_id["hook_id"] = hook.get("hook_id", "")
            decision = await executor.execute(
                hook.get("action", ""), env_with_id, env_per_hook
            )
            last_decision = decision
            if decision.action == "deny":
                return decision
        return last_decision

    async def _dispatch_scoped_post_tool_use(
        self, sid: str, tool_name: str, output: str, is_error: bool
    ):
        """Run all matching PostToolUse scoped hooks. Return first deny, else allow."""
        bundle = self._scoped_bundles.get(sid)
        executor = self._scoped_executors.get(sid)
        if bundle is None or executor is None:
            return None
        hooks = bundle.matching("PostToolUse", tool_name)
        if not hooks:
            return None

        from datetime import datetime, timezone

        # ADR §2.2: tool_result must be a string (serialized tool output).
        if not isinstance(output, str):
            try:
                tool_result = json.dumps(output, default=str)
            except (TypeError, ValueError):
                tool_result = str(output)
        else:
            tool_result = output

        skill_id = ""
        loader = self._skills.get(sid)
        if loader is not None:
            skill_id = loader.first_skill_id()

        envelope = {
            "event": "PostToolUse",
            "session_id": sid,
            "skill_id": skill_id,
            "tool_name": tool_name,
            "tool_result": tool_result,
            "is_error": bool(is_error),
            "created_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        }
        env_extras = {
            "GRID_SESSION_ID": sid,
            "GRID_TOOL_NAME": tool_name,
            "GRID_SKILL_ID": skill_id,
            "GRID_EVENT": "PostToolUse",
        }

        last_decision = None
        for hook in hooks:
            env_with_id = dict(envelope)
            env_with_id["hook_id"] = hook.get("hook_id", "")
            decision = await executor.execute(
                hook.get("action", ""), env_with_id, dict(env_extras)
            )
            last_decision = decision
            if decision.action == "deny":
                return decision
        return last_decision

    async def _dispatch_scoped_stop(self, sid: str):
        """Run all Stop scoped hooks. Return first deny, else allow."""
        bundle = self._scoped_bundles.get(sid)
        executor = self._scoped_executors.get(sid)
        if bundle is None or executor is None:
            return None
        hooks = bundle.matching("Stop")
        if not hooks:
            return None

        from datetime import datetime, timezone

        skill_id = ""
        loader = self._skills.get(sid)
        if loader is not None:
            skill_id = loader.first_skill_id()

        # Per ADR §2.3, draft_memory_id / evidence_anchor_id are optional; we
        # MUST emit empty string when absent (not null, not missing).
        envelope = {
            "event": "Stop",
            "session_id": sid,
            "skill_id": skill_id,
            "draft_memory_id": "",
            "evidence_anchor_id": "",
            "created_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        }
        env_extras = {
            "GRID_SESSION_ID": sid,
            "GRID_TOOL_NAME": "",
            "GRID_SKILL_ID": skill_id,
            "GRID_EVENT": "Stop",
        }

        last_decision = None
        for hook in hooks:
            env_with_id = dict(envelope)
            env_with_id["hook_id"] = hook.get("hook_id", "")
            decision = await executor.execute(
                hook.get("action", ""), env_with_id, dict(env_extras)
            )
            last_decision = decision
            if decision.action == "deny":
                return decision
        return last_decision

    @staticmethod
    def _scoped_hooks_to_rules(frontmatter_hooks) -> list[dict]:
        """Convert P4 frontmatter ScopedHook messages to HookExecutor rule dicts.

        Maps scoped hook fields to the rule format consumed by
        HookExecutor.load_rules(). The ``action`` field in the proto is a
        shell command; we derive the hook action (allow/deny) from its
        content:
        - Commands containing "exit 2" or the literal "deny" → deny rule
        - Everything else → allow rule (informational only)

        The ``condition`` field is mapped to ``tool_pattern`` using regex
        conversion: trailing ``*`` becomes ``.*`` for regex matching.
        """
        rules: list[dict] = []
        for idx, h in enumerate(frontmatter_hooks):
            hook_id = h.hook_id or f"scoped-{idx}"

            # Map condition glob to regex tool_pattern
            condition = h.condition or ""
            if condition.endswith("*"):
                tool_pattern = "^" + condition[:-1] + ".*"
            elif condition and condition != "*":
                tool_pattern = "^" + condition + "$"
            else:
                tool_pattern = ""  # match all

            # Derive action from command content
            cmd = h.action or ""
            is_deny = "exit 2" in cmd or "deny" in cmd.lower()

            # Map hook_type to HookExecutor's naming
            hook_type = h.hook_type or ""
            type_map = {
                "PreToolUse": "pre_tool_call",
                "pre_tool_call": "pre_tool_call",
                "PostToolUse": "post_tool_result",
                "post_tool_result": "post_tool_result",
                "Stop": "stop",
                "stop": "stop",
            }
            mapped_type = type_map.get(hook_type, hook_type)

            rules.append(
                {
                    "id": hook_id,
                    "name": hook_id,
                    "hook_type": mapped_type,
                    "action": "deny" if is_deny else "allow",
                    "reason": f"Scoped hook: {hook_id}",
                    "tool_pattern": tool_pattern,
                    "input_pattern": "",
                    "enabled": True,
                }
            )
        return rules

    def _teardown_session(self, sid: str) -> None:
        tc = self._telemetry.pop(sid, None)
        if tc is not None:
            tc.record("session_end")
            # Flush for side-effects / external endpoint (best effort).
            telemetry_batch_to_proto(tc.flush(), session_id=sid)
        self._hooks.pop(sid, None)
        self._skills.pop(sid, None)
        # S3.T5 — per-session scoped executor + bundle are discarded on
        # teardown. SkillDir materialization on disk is NOT cleaned here;
        # that sweep is tracked as D118 (ADR-V2-006 §9).
        self._scoped_bundles.pop(sid, None)
        self._scoped_executors.pop(sid, None)
        self.session_mgr.terminate(sid)
        if self._active_session_id == sid:
            self._active_session_id = None
