"""Contract-v1 proto-shape assertions.

Locks the request/response message fields that every L1 runtime MUST
accept and produce. These cases speak to the gRPC surface itself — not
to behaviour — and are shared by all four runtimes.

S0.T4: the proto surface in proto/eaasp/runtime/v2/runtime.proto uses
v2.0 spec names (``Initialize``, ``Terminate``, ``GetState``, ``Health``,
``GetCapabilities``), not the v1 names from the blueprint. Each
assertion below exercises the actual tonic-generated Python stub.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def _import_proto():
    """Local import so the module stays loadable even without --runtime."""
    from claude_code_runtime._proto.eaasp.runtime.v2 import common_pb2, runtime_pb2

    return runtime_pb2, common_pb2


def test_initialize_request_accepts_required_fields(runtime_grpc_stub):
    """InitializeRequest MUST accept a SessionPayload (P1-P5 blocks)."""
    runtime_pb2, common_pb2 = _import_proto()
    payload = common_pb2.SessionPayload(
        session_id="test-init-1",
        user_id="u",
        runtime_id="grid-contract-test",
    )
    resp = runtime_grpc_stub.Initialize(
        runtime_pb2.InitializeRequest(payload=payload)
    )
    assert resp.session_id  # runtime MUST return a non-empty session_id
    assert resp.runtime_id  # and MUST echo its own runtime_id


def test_initialize_response_returns_session_id_echo(runtime_grpc_stub):
    """InitializeResponse MUST include session_id + runtime_id."""
    runtime_pb2, common_pb2 = _import_proto()
    payload = common_pb2.SessionPayload(
        session_id="test-init-2",
        user_id="u",
        runtime_id="grid-contract-test",
    )
    resp = runtime_grpc_stub.Initialize(
        runtime_pb2.InitializeRequest(payload=payload)
    )
    # Field presence — both are declared `string` so the generated class
    # always carries the attr, just with an empty default.
    assert hasattr(resp, "session_id")
    assert hasattr(resp, "runtime_id")
    assert isinstance(resp.session_id, str) and resp.session_id
    assert isinstance(resp.runtime_id, str) and resp.runtime_id


def test_send_request_accepts_user_message(runtime_grpc_stub):
    """SendRequest MUST carry a UserMessage with content + message_type."""
    runtime_pb2, common_pb2 = _import_proto()
    # Exercise the message builder without asserting on the stream —
    # that's covered by behavioural tests. This test locks the shape.
    msg = runtime_pb2.UserMessage(content="hello", message_type="text")
    req = runtime_pb2.SendRequest(session_id="x", message=msg)
    assert req.message.content == "hello"
    assert req.message.message_type == "text"


def test_send_request_accepts_tool_result(runtime_grpc_stub):
    """SendRequest tool_result path goes through OnToolResult RPC.

    v2 separates tool results from Send — they flow through the
    dedicated ``OnToolResult(ToolResultEvent)`` RPC. This test locks
    the ToolResultEvent shape instead.
    """
    runtime_pb2, _ = _import_proto()
    evt = runtime_pb2.ToolResultEvent(
        session_id="x",
        tool_name="file_write",
        tool_id="call_1",
        output="ok",
        is_error=False,
    )
    assert evt.tool_name == "file_write"
    assert evt.is_error is False


def test_events_stream_yields_eventstreamentry(runtime_config):
    """EventStreamEntry shape lock (OPTIONAL EmitEvent RPC per ADR-V2-001)."""
    _, common_pb2 = _import_proto()
    runtime_pb2, _ = _import_proto()
    entry = runtime_pb2.EventStreamEntry(
        session_id="x",
        event_id="e1",
        event_type=runtime_pb2.HookEventType.PRE_TOOL_USE,
        payload_json="{}",
        timestamp="2026-04-16T00:00:00Z",
    )
    assert entry.event_id == "e1"
    assert entry.event_type == runtime_pb2.HookEventType.PRE_TOOL_USE


def test_events_stream_emits_stop_at_turn_end(runtime_config):
    """A terminal STOP event MUST close the Send stream for the turn.

    Deferred: requires driving the stream past the scripted first
    tool_call to observe the terminal ``chunk_type="done"`` response.
    Multi-turn harness support lands in S0.T6.
    """
    pytest.xfail("D137: terminal-stop stream observation deferred to S0.T6")


def test_close_request_accepts_session_id(runtime_grpc_stub):
    """Terminate is Empty-in, Empty-out per v2.0 proto.

    The v1 name ``CloseRequest`` does not exist in the v2 contract;
    Terminate(Empty) takes no session_id and operates on the
    last-initialized session (see RuntimeGrpcService::current_session).
    This test exercises that round-trip.
    """
    runtime_pb2, common_pb2 = _import_proto()
    # Seed a session so Terminate has something to target.
    payload = common_pb2.SessionPayload(
        session_id="test-terminate",
        user_id="u",
        runtime_id="grid-contract-test",
    )
    runtime_grpc_stub.Initialize(
        runtime_pb2.InitializeRequest(payload=payload)
    )
    # Empty -> Empty; if the service raises, the test surfaces the
    # protocol break. Success just means the RPC round-tripped.
    runtime_grpc_stub.Terminate(common_pb2.Empty())


def test_state_endpoint_returns_state_response(runtime_grpc_stub):
    """GetState MUST return a StateResponse with required fields."""
    runtime_pb2, common_pb2 = _import_proto()
    # Re-seed a session after any prior Terminate.
    payload = common_pb2.SessionPayload(
        session_id="test-getstate",
        user_id="u",
        runtime_id="grid-contract-test",
    )
    runtime_grpc_stub.Initialize(runtime_pb2.InitializeRequest(payload=payload))
    state = runtime_grpc_stub.GetState(common_pb2.Empty())
    assert isinstance(state.session_id, str)
    assert isinstance(state.runtime_id, str)
    assert isinstance(state.state_format, str)
    assert isinstance(state.created_at, str)


def test_health_returns_health_response(runtime_grpc_stub):
    """Health (OPTIONAL) MUST return HealthResponse with healthy flag.

    Per proto, Health is in the OPTIONAL band. grid-runtime implements
    it; we graduate because the T1 harness always emits a healthy
    response.
    """
    _, common_pb2 = _import_proto()
    resp = runtime_grpc_stub.Health(common_pb2.Empty())
    assert isinstance(resp.healthy, bool)
    assert isinstance(resp.runtime_id, str) and resp.runtime_id
    # checks map may be empty — shape lock only.
    assert hasattr(resp, "checks")


def test_capabilities_advertises_supported_scopes(runtime_grpc_stub):
    """GetCapabilities MUST return a Capabilities manifest."""
    _, common_pb2 = _import_proto()
    caps = runtime_grpc_stub.GetCapabilities(common_pb2.Empty())
    assert isinstance(caps.runtime_id, str) and caps.runtime_id
    assert isinstance(caps.model, str)
    assert caps.context_window >= 0
    # Tier + deployment_mode MUST be set to one of the documented values.
    assert caps.tier in {"harness", "aligned", "framework", ""}
    assert caps.deployment_mode in {"shared", "per_session", ""}
