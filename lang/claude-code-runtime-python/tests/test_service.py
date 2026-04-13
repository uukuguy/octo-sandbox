"""Tests for gRPC RuntimeService — EAASP v2.0 unit tests without real SDK.

These cover the 12 MUST methods + 4 OPTIONAL methods + the EmitEvent
placeholder. SessionPayload uses the 5-block priority structure.
"""

import grpc
import pytest

from claude_code_runtime._proto.eaasp.runtime.v2 import (
    common_pb2,
    runtime_pb2,
)
from claude_code_runtime.config import RuntimeConfig
from claude_code_runtime.service import RuntimeServiceImpl


class FakeContext:
    """Minimal fake gRPC context for unit tests."""

    def __init__(self):
        self.code = None
        self.details = None

    def set_code(self, code):
        self.code = code

    def set_details(self, details):
        self.details = details


@pytest.fixture
def config():
    return RuntimeConfig(
        grpc_port=50099,
        runtime_id="test-runtime",
        runtime_name="Test Runtime",
        anthropic_model_name="test-model",
    )


@pytest.fixture
def service(config):
    return RuntimeServiceImpl(config)


@pytest.fixture
def ctx():
    return FakeContext()


def _payload(user_id: str = "u1", hooks=None) -> common_pb2.SessionPayload:
    """Build a minimal v2 SessionPayload with P1 + P5 blocks populated."""
    policy = common_pb2.PolicyContext(
        org_unit="test-org",
        policy_version="v-test",
    )
    if hooks:
        for h in hooks:
            policy.hooks.append(common_pb2.ManagedHook(**h))
    prefs = common_pb2.UserPreferences(user_id=user_id, language="en")
    return common_pb2.SessionPayload(
        policy_context=policy,
        user_preferences=prefs,
        user_id=user_id,
    )


async def _init_session(
    service, ctx, user_id: str = "u1", hooks=None
) -> str:
    req = runtime_pb2.InitializeRequest(payload=_payload(user_id, hooks))
    resp = await service.Initialize(req, ctx)
    assert resp.runtime_id == "test-runtime"
    return resp.session_id


# ── Health / Capabilities ────────────────────────────────────────


@pytest.mark.asyncio
async def test_health(service, ctx):
    resp = await service.Health(common_pb2.Empty(), ctx)
    assert resp.healthy is True
    assert resp.runtime_id == "test-runtime"
    assert "sdk" in resp.checks
    assert "sessions" in resp.checks


@pytest.mark.asyncio
async def test_get_capabilities(service, ctx):
    resp = await service.GetCapabilities(common_pb2.Empty(), ctx)
    assert resp.runtime_id == "test-runtime"
    assert resp.tier == "harness"
    assert resp.model == "test-model"
    assert resp.supports_native_hooks is True
    assert resp.supports_native_mcp is True
    assert resp.supports_native_skills is True
    assert resp.deployment_mode == "per_session"
    assert len(resp.tools) > 0


# ── Initialize & SessionPayload priority blocks ──────────────────


@pytest.mark.asyncio
async def test_initialize(service, ctx):
    sid = await _init_session(service, ctx)
    assert sid.startswith("crt-")
    assert service.session_mgr.get(sid) is not None
    assert service._active_session_id == sid


@pytest.mark.asyncio
async def test_initialize_with_skill_instructions_p4(service, ctx):
    """SessionPayload.skill_instructions (P4) should be auto-loaded."""
    payload = _payload()
    payload.skill_instructions.skill_id = "skill-preflight"
    payload.skill_instructions.name = "Preflight"
    payload.skill_instructions.content = "Always double-check tool output."
    resp = await service.Initialize(
        runtime_pb2.InitializeRequest(payload=payload), ctx
    )
    sid = resp.session_id
    assert service._skills[sid].count == 1
    assert service._skills[sid].get("skill-preflight") is not None


@pytest.mark.asyncio
async def test_initialize_user_id_from_p5(service, ctx):
    """user_id should fall back to UserPreferences.user_id when flat field
    is empty (v2 priority block model)."""
    payload = common_pb2.SessionPayload(
        policy_context=common_pb2.PolicyContext(org_unit="ou-42"),
        user_preferences=common_pb2.UserPreferences(user_id="alice-p5"),
    )
    resp = await service.Initialize(
        runtime_pb2.InitializeRequest(payload=payload), ctx
    )
    session = service.session_mgr.get(resp.session_id)
    assert session is not None
    assert session.user_id == "alice-p5"
    assert session.org_unit == "ou-42"


# ── LoadSkill ────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_load_skill(service, ctx):
    sid = await _init_session(service, ctx)

    skill = common_pb2.SkillInstructions(
        skill_id="s-1",
        name="Test Skill",
        content="Do something carefully.",
    )
    req = runtime_pb2.LoadSkillRequest(session_id=sid, skill=skill)
    resp = await service.LoadSkill(req, ctx)
    assert resp.success is True
    assert service._skills[sid].count == 1


# ── Hook methods (OnToolCall / OnToolResult / OnStop) ────────────


@pytest.mark.asyncio
async def test_on_tool_call_allow(service, ctx):
    sid = await _init_session(service, ctx)
    req = runtime_pb2.ToolCallEvent(
        session_id=sid,
        tool_name="bash",
        tool_id="t-1",
        input_json='{"command": "ls"}',
    )
    resp = await service.OnToolCall(req, ctx)
    assert resp.decision == "allow"


@pytest.mark.asyncio
async def test_on_tool_call_deny_with_managed_hooks(service, ctx):
    """A ManagedHook with 'tool:^bash$;input:rm -rf' condition denies."""
    hooks = [
        {
            "hook_id": "block-rm",
            "hook_type": "pre_tool_call",
            "condition": "tool:^bash$;input:rm -rf;reason:blocked",
            "action": "deny",
            "precedence": 0,
            "scope": "managed",
        }
    ]
    sid = await _init_session(service, ctx, hooks=hooks)

    req = runtime_pb2.ToolCallEvent(
        session_id=sid,
        tool_name="bash",
        tool_id="t-1",
        input_json='{"command": "rm -rf /"}',
    )
    resp = await service.OnToolCall(req, ctx)
    assert resp.decision == "deny"
    assert "blocked" in resp.reason


@pytest.mark.asyncio
async def test_on_tool_result(service, ctx):
    sid = await _init_session(service, ctx)
    req = runtime_pb2.ToolResultEvent(
        session_id=sid,
        tool_name="bash",
        tool_id="t-1",
        output="file.txt",
        is_error=False,
    )
    resp = await service.OnToolResult(req, ctx)
    assert resp.decision == "allow"


@pytest.mark.asyncio
async def test_on_stop_allow(service, ctx):
    sid = await _init_session(service, ctx)
    req = runtime_pb2.StopEvent(session_id=sid)
    resp = await service.OnStop(req, ctx)
    assert resp.decision == "allow"  # v2: allow == stop


@pytest.mark.asyncio
async def test_on_stop_force_continue(service, ctx):
    """Stop hook with deny action forces the agent to continue."""
    hooks = [
        {
            "hook_id": "force-continue",
            "hook_type": "stop",
            "condition": "reason:task incomplete",
            "action": "deny",
            "precedence": 0,
            "scope": "managed",
        }
    ]
    sid = await _init_session(service, ctx, hooks=hooks)
    req = runtime_pb2.StopEvent(session_id=sid)
    resp = await service.OnStop(req, ctx)
    assert resp.decision == "deny"  # v2: deny == force continue
    assert "task incomplete" in resp.reason


# ── MCP connect / disconnect ─────────────────────────────────────


@pytest.mark.asyncio
async def test_connect_disconnect_mcp(service, ctx):
    sid = await _init_session(service, ctx)

    req = runtime_pb2.ConnectMCPRequest(
        session_id=sid,
        servers=[
            runtime_pb2.McpServerConfig(
                name="test-mcp", transport="stdio", command="echo"
            )
        ],
    )
    resp = await service.ConnectMCP(req, ctx)
    assert resp.success is True
    assert "test-mcp" in resp.connected

    disc_resp = await service.DisconnectMcp(
        runtime_pb2.DisconnectMcpRequest(
            session_id=sid, server_name="test-mcp"
        ),
        ctx,
    )
    # DisconnectMcp returns Empty in v2; success is implicit when no error.
    assert disc_resp is not None
    assert ctx.code is None


# ── GetState / RestoreState ──────────────────────────────────────


@pytest.mark.asyncio
async def test_get_state_and_restore(service, ctx):
    sid = await _init_session(service, ctx, user_id="alice")

    state_resp = await service.GetState(common_pb2.Empty(), ctx)
    assert state_resp.session_id == sid
    assert state_resp.state_format == "python-json"
    assert len(state_resp.state_data) > 0

    # Terminate active session, then restore from blob.
    await service.Terminate(common_pb2.Empty(), ctx)
    assert service.session_mgr.get(sid) is None

    restore_empty = await service.RestoreState(state_resp, ctx)
    assert restore_empty is not None
    restored = service.session_mgr.get(sid)
    assert restored is not None
    assert restored.user_id == "alice"
    assert service._active_session_id == sid


# ── Pause / Resume ───────────────────────────────────────────────


@pytest.mark.asyncio
async def test_pause_resume(service, ctx):
    sid = await _init_session(service, ctx)

    pause_resp = await service.PauseSession(common_pb2.Empty(), ctx)
    assert pause_resp.session_id == sid
    assert len(pause_resp.state_data) > 0

    # Rebuild a StateResponse-like arg for Resume (session_id must be set).
    resume_arg = runtime_pb2.StateResponse(session_id=sid)
    resume_empty = await service.ResumeSession(resume_arg, ctx)
    assert resume_empty is not None
    assert ctx.code is None


# ── EmitTelemetry (push semantics) ───────────────────────────────


@pytest.mark.asyncio
async def test_emit_telemetry_push(service, ctx):
    sid = await _init_session(service, ctx)

    # Client pushes telemetry events to the runtime; response is Empty.
    req = runtime_pb2.TelemetryRequest(
        session_id=sid,
        events=[
            runtime_pb2.TelemetryEvent(
                event_type="client_metric",
                payload_json='{"k":"v"}',
                timestamp="0",
            )
        ],
    )
    resp = await service.EmitTelemetry(req, ctx)
    assert isinstance(resp, common_pb2.Empty)
    # Event should have been recorded into the collector
    tc = service._telemetry[sid]
    assert any(e.event_type == "client_metric" for e in tc.peek())


# ── Terminate (Empty-input) ──────────────────────────────────────


@pytest.mark.asyncio
async def test_terminate(service, ctx):
    sid = await _init_session(service, ctx)

    resp = await service.Terminate(common_pb2.Empty(), ctx)
    assert isinstance(resp, common_pb2.Empty)
    assert service.session_mgr.get(sid) is None
    assert sid not in service._hooks
    assert sid not in service._telemetry
    assert sid not in service._skills
    assert service._active_session_id is None


# ── Empty-input methods with no active session ───────────────────


@pytest.mark.asyncio
async def test_get_state_no_active_session(service, ctx):
    await service.GetState(common_pb2.Empty(), ctx)
    assert ctx.code == grpc.StatusCode.NOT_FOUND


# ── EmitEvent placeholder (ADR-V2-001 pending) ───────────────────


@pytest.mark.asyncio
async def test_emit_event_unimplemented(service, ctx):
    req = runtime_pb2.EventStreamEntry(
        session_id="any",
        event_id="e-1",
        event_type=runtime_pb2.SESSION_START,
        payload_json="{}",
    )
    await service.EmitEvent(req, ctx)
    assert ctx.code == grpc.StatusCode.UNIMPLEMENTED
    assert ctx.details == "ADR-V2-001 pending"


# ── D2-py — Initialize wiring for P3 memory_refs + P1 policy_context ──


@pytest.mark.asyncio
async def test_initialize_injects_memory_refs_preamble(service, ctx):
    """Initialize must project P3 memory_refs + P1 policy_context onto the
    Session and the stored dicts must be usable to build a preamble string.

    D2-py scope: this validates the *extraction* step only — it asserts the
    Session carries memory_refs / policy_context in the shape that the Send
    handler then consumes to build the "## Prior memories..." preamble. The
    actual Send-time injection is covered indirectly because Send reads
    straight from these same fields.
    """
    payload = common_pb2.SessionPayload(
        policy_context=common_pb2.PolicyContext(
            org_unit="engineering-dept",
            policy_version="v2.0-20260412",
            hooks=[
                common_pb2.ManagedHook(
                    hook_id="h1",
                    hook_type="pre_tool_call",
                    condition="tool:^bash$",
                    action="deny",
                    precedence=1,
                    scope="managed",
                )
            ],
        ),
        user_preferences=common_pb2.UserPreferences(user_id="alice"),
        user_id="alice",
    )
    payload.memory_refs.add(
        memory_id="mem-1",
        memory_type="fact",
        relevance_score=0.95,
        content="Device XYZ temperature threshold is 75C",
        source_session_id="s-prev",
        created_at="2026-04-10T00:00:00Z",
    )
    payload.memory_refs.add(
        memory_id="mem-2",
        memory_type="preference",
        relevance_score=0.80,
        content="User prefers conservative thresholds",
        source_session_id="s-prev",
        created_at="2026-04-10T00:00:00Z",
    )

    resp = await service.Initialize(
        runtime_pb2.InitializeRequest(payload=payload), ctx
    )
    sid = resp.session_id
    assert sid.startswith("crt-")

    session = service.session_mgr.get(sid)
    assert session is not None

    # P3 memory_refs were projected into session.memory_refs as plain dicts.
    assert len(session.memory_refs) == 2
    mem_ids = [m["memory_id"] for m in session.memory_refs]
    assert mem_ids == ["mem-1", "mem-2"]
    assert session.memory_refs[0]["content"].startswith("Device XYZ")
    assert session.memory_refs[0]["relevance_score"] == pytest.approx(0.95)
    assert session.memory_refs[1]["memory_type"] == "preference"

    # P1 policy_context metadata captured (hooks list + version + org_unit).
    assert session.policy_context is not None
    assert session.policy_context["org_unit"] == "engineering-dept"
    assert session.policy_context["policy_version"] == "v2.0-20260412"
    assert len(session.policy_context["hooks"]) == 1
    assert session.policy_context["hooks"][0]["hook_id"] == "h1"

    # Session defaults — preamble not yet injected until the first Send.
    assert session.preamble_injected is False

    # The preamble string the Send handler will build must contain both
    # memory entries' content substrings. We reconstruct it locally using
    # the same join pattern the handler uses so the test stays in lock-step
    # with the format spec.
    preamble_lines = ["## Prior memories from previous sessions", ""]
    for mref in session.memory_refs:
        preamble_lines.append(
            f"- [{mref.get('memory_type', '')}] {mref.get('content', '')}"
        )
    preamble = "\n".join(preamble_lines) + "\n"
    assert "Prior memories from previous sessions" in preamble
    assert "Device XYZ temperature threshold is 75C" in preamble
    assert "User prefers conservative thresholds" in preamble
    assert "[fact]" in preamble
    assert "[preference]" in preamble


@pytest.mark.asyncio
async def test_initialize_creates_isolated_workspace(service, ctx):
    """Initialize must create an isolated workspace directory for the session.
    L1 Runtime is designed to run in a container; bare-metal isolation prevents
    development environment (.claude/, hooks) from leaking into skill context.
    """
    sid = await _init_session(service, ctx)
    session = service.session_mgr.get(sid)
    assert session is not None
    assert session.workspace is not None
    import os
    assert os.path.isdir(session.workspace), f"workspace must be a directory: {session.workspace}"
    assert "eaasp-workspace" in session.workspace


@pytest.mark.asyncio
async def test_initialize_mcp_deps_not_auto_configured(service, ctx):
    """Initialize must NOT auto-configure MCP servers from env vars.
    MCP configuration is handled by ConnectMCP (Phase 0.75)."""
    import os
    os.environ["EAASP_MCP_SERVER_MOCK_SCADA_CMD"] = "/usr/bin/mock-scada"
    try:
        payload = _payload()
        payload.skill_instructions.skill_id = "cal"
        payload.skill_instructions.content = "calibrate"
        payload.skill_instructions.dependencies.append("mcp:mock-scada")
        resp = await service.Initialize(
            runtime_pb2.InitializeRequest(payload=payload), ctx
        )
        sid = resp.session_id
        session = service.session_mgr.get(sid)
        assert session is not None
        # mcp_servers_config should NOT be set during Initialize
        assert session.mcp_servers_config is None
    finally:
        os.environ.pop("EAASP_MCP_SERVER_MOCK_SCADA_CMD", None)


@pytest.mark.asyncio
async def test_connect_mcp_stdio(service, ctx):
    """ConnectMCP with stdio transport sets session.mcp_servers_config."""
    sid = await _init_session(service, ctx)
    req = runtime_pb2.ConnectMCPRequest(
        session_id=sid,
        servers=[
            runtime_pb2.McpServerConfig(
                name="mock-scada", transport="stdio",
                command="mock-scada", args=["--transport", "stdio"],
            ),
        ],
    )
    resp = await service.ConnectMCP(req, ctx)
    assert resp.success is True
    assert "mock-scada" in resp.connected
    assert resp.failed == []

    session = service.session_mgr.get(sid)
    assert session.mcp_servers_config is not None
    assert "mock-scada" in session.mcp_servers_config
    cfg = session.mcp_servers_config["mock-scada"]
    assert cfg["command"] == "mock-scada"
    assert cfg["args"] == ["--transport", "stdio"]


@pytest.mark.asyncio
async def test_connect_mcp_sse(service, ctx):
    """ConnectMCP with SSE transport sets url in config."""
    sid = await _init_session(service, ctx)
    req = runtime_pb2.ConnectMCPRequest(
        session_id=sid,
        servers=[
            runtime_pb2.McpServerConfig(
                name="memory-sse", transport="sse",
                url="http://127.0.0.1:18086/sse",
            ),
        ],
    )
    resp = await service.ConnectMCP(req, ctx)
    assert resp.success is True
    assert "memory-sse" in resp.connected

    session = service.session_mgr.get(sid)
    assert session.mcp_servers_config["memory-sse"]["url"] == "http://127.0.0.1:18086/sse"


@pytest.mark.asyncio
async def test_connect_mcp_unsupported_transport(service, ctx):
    """ConnectMCP with missing command/url reports failure."""
    sid = await _init_session(service, ctx)
    req = runtime_pb2.ConnectMCPRequest(
        session_id=sid,
        servers=[
            runtime_pb2.McpServerConfig(
                name="bad-server", transport="stdio",
                # No command provided
            ),
        ],
    )
    resp = await service.ConnectMCP(req, ctx)
    assert resp.success is False
    assert "bad-server" in resp.failed


@pytest.mark.asyncio
async def test_sdk_wrapper_sets_bare_mode():
    """SdkWrapper must set CLAUDE_CODE_SIMPLE=1 for L1 Runtime isolation."""
    from claude_code_runtime.sdk_wrapper import SdkWrapper
    config = RuntimeConfig()
    wrapper = SdkWrapper(config)
    opts = wrapper._build_options(system_prompt="test")
    assert opts.env.get("CLAUDE_CODE_SIMPLE") == "1"
