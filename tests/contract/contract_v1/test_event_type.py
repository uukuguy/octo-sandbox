"""Contract-v1 event_type enumeration lock.

Pins the exact set of event_type values every L1 runtime MUST be able
to emit on the ``Send`` response stream (``chunk_type`` field per
proto/eaasp/runtime/v2/runtime.proto SendResponse). Any future addition
requires a contract-version bump (ADR-V2-017 §2 freeze policy).

Blueprint note (S0.T4): The original blueprint referenced an ``Events``
RPC, which does not exist. Event-type assertions in contract-v1 apply
to the ``chunk_type`` field of ``SendResponse`` plus the
``EventStreamEntry.event_type`` enum consumed by the optional
``EmitEvent`` RPC.
"""

from __future__ import annotations

import pytest

from tests.contract.harness.assertions import EVENT_TYPES_V1, assert_event_type_in

pytestmark = pytest.mark.contract_v1


def test_event_types_v1_set_is_seven_members():
    """EVENT_TYPES_V1 is the contract; lock the cardinality + members."""
    assert len(EVENT_TYPES_V1) == 7
    assert EVENT_TYPES_V1 == frozenset(
        {
            "CHUNK",
            "TOOL_CALL",
            "TOOL_RESULT",
            "STOP",
            "ERROR",
            "HOOK_FIRED",
            "PRE_COMPACT",
        }
    )


def test_chunk_event_is_emitted_for_assistant_text(runtime_config):
    """CHUNK events carry assistant text deltas during streaming.

    Deferred: chunk_type observation requires driving a full Send turn
    past the mock LLM's scripted first tool_call to the terminal text
    response. Harness support for multi-turn drives lands in S0.T6.
    """
    pytest.xfail("D137: multi-turn chunk_type observation deferred to S0.T6")


def test_tool_call_event_precedes_tool_result(runtime_config):
    """TOOL_CALL MUST precede the matching TOOL_RESULT for the same call id.

    Deferred: same multi-turn observation machinery as above.
    """
    pytest.xfail("D137: tool-call ordering observation deferred to S0.T6")


def test_unknown_event_type_not_emitted(runtime_config):
    """Every observed event_type MUST be a member of EVENT_TYPES_V1."""

    def _check(observed: list[str]) -> None:
        for t in observed:
            assert_event_type_in(t)

    # _check remains exported as a contract helper for S0.T6 callers.
    pytest.xfail("D137: event_type whitelist observation deferred to S0.T6")


def test_pre_compact_event_emitted_over_threshold(runtime_config):
    """Per ADR-V2-018, PRE_COMPACT fires when context usage exceeds threshold.

    Deferred: requires feeding the runtime a multi-turn session large
    enough to breach its compaction threshold. Out of scope for S0.T4's
    single-turn scripted probe.
    """
    pytest.xfail("D137: PRE_COMPACT threshold test deferred to Phase 2.5 S1")
