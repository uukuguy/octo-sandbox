"""Contract-v1 skill-workflow assertions.

Locks the behaviour of ``workflow.required_tools`` across all L1
runtimes: skill-attached sessions MUST enforce the declared tool set,
reject unknown tools, and tolerate any ordering of permitted calls.

S0.T4: required_tools enforcement observation requires an LLM that
emits controllable sequences of tool_use calls (some in-set, some
out-of-set). Our mock OpenAI server scripts a single file_write call
— enough for hook-envelope tests but not for multi-call ordering /
rejection assertions. Deferring 5/5 → D138 for Phase 2.5 S1 when the
mock gains scriptable deny cases.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def test_required_tools_enforced_at_send(runtime_config):
    pytest.xfail("D138: required_tools enforcement needs scriptable deny path in mock LLM; deferred to Phase 2.5 S1")


def test_tool_order_is_free_within_required_set(runtime_config):
    pytest.xfail("D138: multi-tool permutation tests need multi-call mock; deferred to Phase 2.5 S1")


def test_unknown_tool_rejects_with_error_event(runtime_config):
    pytest.xfail("D138: unknown-tool rejection needs scriptable mock LLM; deferred to Phase 2.5 S1")


def test_skill_unloaded_between_sessions(runtime_config):
    pytest.xfail("D138: cross-session skill isolation needs two-session harness; deferred to Phase 2.5 S1")


def test_load_skill_after_initialize_is_idempotent(runtime_config):
    pytest.xfail("D138: idempotent LoadSkill needs skill reload observability; deferred to Phase 2.5 S1")
