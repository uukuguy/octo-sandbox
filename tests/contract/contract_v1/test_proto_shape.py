"""Contract-v1 proto-shape assertions.

Locks the request/response message fields that every L1 runtime MUST
accept and produce. These cases speak to the gRPC surface itself — not
to behaviour — and are shared by all four runtimes.

RED policy (S0.T2): every case skips with NotImplementedError-flavoured
``runtime_config`` until S0.T4/T5 backfill the concrete configs.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def test_initialize_request_accepts_required_fields(runtime_config):
    """InitializeRequest MUST accept session_id, policy_context, capabilities."""
    # Driven by S0.T4 fixtures; RED until then.
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_initialize_response_returns_session_id_echo(runtime_config):
    """InitializeResponse MUST echo session_id and signal ready-state."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_send_request_accepts_user_message(runtime_config):
    """SendRequest MUST carry a UserMessage with role+content fields."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_send_request_accepts_tool_result(runtime_config):
    """SendRequest MUST accept tool_result payloads (ToolResultAck path)."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_events_stream_yields_eventstreamentry(runtime_config):
    """Events stream MUST yield EventStreamEntry with event_type + payload."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_events_stream_emits_stop_at_turn_end(runtime_config):
    """A terminal STOP event MUST close the stream for the current turn."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_close_request_accepts_session_id(runtime_config):
    """CloseRequest MUST accept session_id; CloseResponse returns Empty."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_state_endpoint_returns_state_response(runtime_config):
    """GetState MUST return StateResponse with current turn + status."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_health_returns_health_response(runtime_config):
    """Health MUST return HealthResponse with status=SERVING."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_capabilities_advertises_supported_scopes(runtime_config):
    """Capabilities MUST enumerate supported HookEventType values."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")
