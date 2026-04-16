"""Contract-v1 skill-workflow assertions.

Locks the behaviour of ``workflow.required_tools`` across all L1
runtimes: skill-attached sessions MUST enforce the declared tool set,
reject unknown tools, and tolerate any ordering of permitted calls.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def test_required_tools_enforced_at_send(runtime_config):
    """A skill with required_tools=[A,B] MUST reject calls to tool C."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_tool_order_is_free_within_required_set(runtime_config):
    """Required-tool order is NOT fixed; any permutation MUST be allowed."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_unknown_tool_rejects_with_error_event(runtime_config):
    """Calling a tool not in required_tools MUST surface ERROR (not STOP)."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_skill_unloaded_between_sessions(runtime_config):
    """Session-scoped skill MUST NOT leak to the next session in the runtime."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_load_skill_after_initialize_is_idempotent(runtime_config):
    """Re-loading the same skill_id MUST be a no-op, not an error."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")
