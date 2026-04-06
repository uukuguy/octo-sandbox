"""Tests for gRPC RuntimeService — unit tests without real SDK calls."""

import json

import grpc
import pytest

from claude_code_runtime._proto.eaasp.common.v1 import common_pb2
from claude_code_runtime._proto.eaasp.runtime.v1 import runtime_pb2
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


async def _init_session(service, ctx, user_id="u1", hooks_json=""):
    """Helper to initialize a session and return session_id."""
    req = runtime_pb2.InitializeRequest(
        payload=runtime_pb2.SessionPayload(
            user_id=user_id,
            managed_hooks_json=hooks_json,
        )
    )
    resp = await service.Initialize(req, ctx)
    return resp.session_id


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
    assert resp.runtime_name == "Test Runtime"
    assert resp.tier == "harness"
    assert resp.model == "test-model"
    assert resp.native_hooks is True
    assert resp.requires_hook_bridge is False
    assert len(resp.supported_tools) > 0


@pytest.mark.asyncio
async def test_initialize(service, ctx):
    sid = await _init_session(service, ctx)
    assert sid.startswith("crt-")
    assert service.session_mgr.get(sid) is not None


@pytest.mark.asyncio
async def test_load_skill(service, ctx):
    sid = await _init_session(service, ctx)

    req = runtime_pb2.LoadSkillRequest(
        session_id=sid,
        skill=runtime_pb2.SkillContent(
            skill_id="s-1",
            name="Test Skill",
            frontmatter_yaml="---\nname: test\n---",
            prose="Do something.",
        ),
    )
    resp = await service.LoadSkill(req, ctx)
    assert resp.success is True
    assert service._skills[sid].count == 1


@pytest.mark.asyncio
async def test_on_tool_call_allow(service, ctx):
    sid = await _init_session(service, ctx)
    req = common_pb2.ToolCallEvent(
        session_id=sid,
        tool_name="bash",
        tool_id="t-1",
        input_json='{"command": "ls"}',
    )
    resp = await service.OnToolCall(req, ctx)
    assert resp.decision == "allow"


@pytest.mark.asyncio
async def test_on_tool_call_deny_with_hooks(service, ctx):
    """Test that managed hooks can deny tool calls."""
    hooks = json.dumps({
        "rules": [
            {
                "id": "r-1",
                "name": "block-rm",
                "hook_type": "pre_tool_call",
                "action": "deny",
                "reason": "blocked",
                "tool_pattern": "^bash$",
                "input_pattern": "rm -rf",
                "enabled": True,
            }
        ]
    })
    sid = await _init_session(service, ctx, hooks_json=hooks)

    req = common_pb2.ToolCallEvent(
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
    req = common_pb2.ToolResultEvent(
        session_id=sid,
        tool_name="bash",
        tool_id="t-1",
        output="file.txt",
        is_error=False,
    )
    resp = await service.OnToolResult(req, ctx)
    assert resp.decision == "allow"


@pytest.mark.asyncio
async def test_on_stop(service, ctx):
    sid = await _init_session(service, ctx)
    req = common_pb2.StopRequest(session_id=sid)
    resp = await service.OnStop(req, ctx)
    assert resp.decision == "complete"


@pytest.mark.asyncio
async def test_on_stop_continue_with_hooks(service, ctx):
    """Test stop hook with force-continue rule."""
    hooks = json.dumps({
        "rules": [
            {
                "id": "r-stop",
                "name": "force-continue",
                "hook_type": "stop",
                "action": "deny",
                "reason": "task incomplete",
                "enabled": True,
            }
        ]
    })
    sid = await _init_session(service, ctx, hooks_json=hooks)
    req = common_pb2.StopRequest(session_id=sid)
    resp = await service.OnStop(req, ctx)
    assert resp.decision == "continue"
    assert "incomplete" in resp.feedback


@pytest.mark.asyncio
async def test_connect_disconnect_mcp(service, ctx):
    sid = await _init_session(service, ctx)

    req = runtime_pb2.ConnectMcpRequest(
        session_id=sid,
        servers=[
            runtime_pb2.McpServerConfig(
                name="test-mcp", transport="stdio", command="echo"
            )
        ],
    )
    resp = await service.ConnectMcp(req, ctx)
    assert resp.success is True
    assert "test-mcp" in resp.connected

    disc_resp = await service.DisconnectMcp(
        runtime_pb2.DisconnectMcpRequest(
            session_id=sid, server_name="test-mcp"
        ),
        ctx,
    )
    assert disc_resp.success is True


@pytest.mark.asyncio
async def test_get_state_and_restore(service, ctx):
    sid = await _init_session(service, ctx, user_id="alice")

    state_resp = await service.GetState(
        runtime_pb2.GetStateRequest(session_id=sid), ctx
    )
    assert state_resp.session_id == sid
    assert state_resp.state_format == "python-json"
    assert len(state_resp.state_data) > 0

    # Terminate original, then restore
    await service.Terminate(
        runtime_pb2.TerminateRequest(session_id=sid), ctx
    )
    assert service.session_mgr.get(sid) is None

    restore_resp = await service.RestoreState(state_resp, ctx)
    assert restore_resp.session_id == sid
    restored = service.session_mgr.get(sid)
    assert restored is not None
    assert restored.user_id == "alice"


@pytest.mark.asyncio
async def test_pause_resume(service, ctx):
    sid = await _init_session(service, ctx)

    pause_resp = await service.PauseSession(
        runtime_pb2.PauseRequest(session_id=sid), ctx
    )
    assert pause_resp.success is True

    resume_resp = await service.ResumeSession(
        runtime_pb2.ResumeRequest(session_id=sid), ctx
    )
    assert resume_resp.success is True


@pytest.mark.asyncio
async def test_emit_telemetry(service, ctx):
    sid = await _init_session(service, ctx)
    # session_start is auto-recorded on Initialize

    resp = await service.EmitTelemetry(
        runtime_pb2.EmitTelemetryRequest(session_id=sid), ctx
    )
    assert len(resp.events) >= 1  # at least session_start
    assert resp.events[0].event_type == "session_start"


@pytest.mark.asyncio
async def test_terminate(service, ctx):
    sid = await _init_session(service, ctx)

    resp = await service.Terminate(
        runtime_pb2.TerminateRequest(session_id=sid), ctx
    )
    assert resp.success is True
    assert resp.final_telemetry is not None
    assert service.session_mgr.get(sid) is None
    # Per-session components cleaned up
    assert sid not in service._hooks
    assert sid not in service._telemetry
    assert sid not in service._skills


@pytest.mark.asyncio
async def test_session_not_found(service, ctx):
    await service.GetState(
        runtime_pb2.GetStateRequest(session_id="nonexistent"), ctx
    )
    assert ctx.code == grpc.StatusCode.NOT_FOUND
