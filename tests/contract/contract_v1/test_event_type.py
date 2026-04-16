"""Contract-v1 event_type enumeration lock.

Pins the exact set of event_type values every L1 runtime MUST be able
to emit on the ``Events`` stream. Any future addition requires a
contract-version bump (ADR-V2-017 §2 freeze policy).
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
    """CHUNK events carry assistant text deltas during streaming."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_tool_call_event_precedes_tool_result(runtime_config):
    """TOOL_CALL MUST precede the matching TOOL_RESULT for the same call id."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_unknown_event_type_not_emitted(runtime_config):
    """Every observed event_type MUST be a member of EVENT_TYPES_V1."""

    def _check(observed: list[str]) -> None:
        for t in observed:
            assert_event_type_in(t)

    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring; _check is contract helper")


def test_pre_compact_event_emitted_over_threshold(runtime_config):
    """Per ADR-V2-018, PRE_COMPACT fires when context usage exceeds threshold."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")
