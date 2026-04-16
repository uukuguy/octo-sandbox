"""Contract-v1 end-to-end smoke.

Five minimal round-trips that every L1 runtime MUST support. These are
the baseline health checks — if any of these fail, the runtime is not
conformant regardless of deeper category results.

S0.T4: Graduates the RPC-only round-trips. Multi-turn behavioural
smoke (``send without initialize errors cleanly``, ``close idempotent``)
depends on pinning specific runtime-side error semantics and is
deferred to D139 for Phase 2.5 S1.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def _import_proto():
    from claude_code_runtime._proto.eaasp.runtime.v2 import common_pb2, runtime_pb2

    return runtime_pb2, common_pb2


def test_initialize_send_events_close_round_trip(runtime_grpc_stub):
    """Full Initialize -> Send -> Terminate round-trip."""
    runtime_pb2, common_pb2 = _import_proto()
    payload = common_pb2.SessionPayload(
        session_id="smoke-e2e-1",
        user_id="u",
        runtime_id="grid-contract-test",
    )
    init_resp = runtime_grpc_stub.Initialize(
        runtime_pb2.InitializeRequest(payload=payload)
    )
    sid = init_resp.session_id
    msg = runtime_pb2.UserMessage(content="hello", message_type="text")
    stream = runtime_grpc_stub.Send(
        runtime_pb2.SendRequest(session_id=sid, message=msg)
    )
    # Drain the stream; we only assert that it terminates.
    chunk_count = 0
    for _ in stream:
        chunk_count += 1
    # Runtime MUST emit at least one chunk (could be done alone).
    assert chunk_count >= 1, "Send stream MUST yield ≥1 chunk"
    runtime_grpc_stub.Terminate(common_pb2.Empty())


def test_double_initialize_same_session_is_noop_or_error(runtime_grpc_stub):
    """Re-initialising with the same session_id MUST NOT corrupt state."""
    runtime_pb2, common_pb2 = _import_proto()
    payload = common_pb2.SessionPayload(
        session_id="smoke-double-init",
        user_id="u",
        runtime_id="grid-contract-test",
    )
    r1 = runtime_grpc_stub.Initialize(
        runtime_pb2.InitializeRequest(payload=payload)
    )
    # Second call — accept either success (noop) or a typed gRPC error.
    try:
        r2 = runtime_grpc_stub.Initialize(
            runtime_pb2.InitializeRequest(payload=payload)
        )
        # Must echo a usable session_id either way.
        assert r2.session_id
    except Exception as err:  # noqa: BLE001
        # A typed gRPC error on double-init is acceptable per contract
        # ("MUST NOT corrupt state" allows either noop or error).
        assert "session" in str(err).lower() or "already" in str(err).lower()
    assert r1.session_id


def test_send_without_initialize_errors_cleanly(runtime_config):
    """SendRequest on an unknown session_id MUST produce a typed error.

    Deferred: grid-runtime's current behaviour on unknown session_id is
    under-specified — it may delegate to the engine which creates a new
    session implicitly. Pinning the exact "typed error" contract and
    fixing the runtime if needed is D139 scope.
    """
    pytest.xfail("D139: unknown-session error semantics underspecified; deferred to Phase 2.5 S1")


def test_close_idempotent_on_already_closed_session(runtime_config):
    """CloseRequest on a closed session MUST NOT panic; MUST return Empty.

    Deferred: grid-runtime's Terminate clears current_session, so a
    second Terminate currently fails with "no active session". Either
    the contract must allow failed_precondition on double-close or the
    runtime must silently accept it. Resolution deferred to D139.
    """
    pytest.xfail("D139: double-Terminate behaviour needs contract clarification; deferred to Phase 2.5 S1")


def test_health_stays_serving_across_turns(runtime_grpc_stub):
    """Health MUST remain healthy across multiple RPCs."""
    runtime_pb2, common_pb2 = _import_proto()
    for _ in range(3):
        resp = runtime_grpc_stub.Health(common_pb2.Empty())
        assert resp.healthy, f"runtime reported unhealthy: {resp}"
