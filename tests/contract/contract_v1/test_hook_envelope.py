"""Contract-v1 hook-envelope assertions — ADR-V2-006 §2/§3.

THIS IS THE SUITE THAT DRIVES D120.

Per ADR-V2-006:

* §2 — hooks MUST receive a canonical JSON envelope on stdin with
  ``event``, ``session_id``, ``skill_id``, ``created_at`` plus scope-
  specific fields (``tool_args`` / ``tool_result``+``is_error`` /
  ``draft_memory_id``+``evidence_anchor_id``).
* §3 — hooks MUST see four env vars populated: ``GRID_SESSION_ID``,
  ``GRID_TOOL_NAME``, ``GRID_SKILL_ID``, ``GRID_EVENT``.

Python runtime (claude-code-runtime) is already compliant — verified in
Phase 2 S3.T5. Rust runtime (grid-runtime) has envelope-mode support
via `HookContext::with_event` (S0.T3 / D120). S0.T4 wires the tests
against a real probe-skill subprocess.

If the probe-skill does not fire an expected hook for this runtime, the
test xfails with D136 rather than falsely passing.
"""

from __future__ import annotations

import pytest

from tests.contract.harness.assertions import (
    assert_grid_env_vars_present,
    assert_hook_envelope_required_fields,
)

pytestmark = pytest.mark.contract_v1


# ---------------------------------------------------------------------------
# ADR-V2-006 §2 — stdin envelope schema
# ---------------------------------------------------------------------------


def test_pre_tool_use_envelope_has_all_required_fields(trigger_pre_tool_use_hook):
    envelope, _env = trigger_pre_tool_use_hook()
    assert_hook_envelope_required_fields(envelope, "PreToolUse")
    assert isinstance(envelope["tool_args"], dict)
    assert isinstance(envelope["tool_name"], str) and envelope["tool_name"]


def test_post_tool_use_envelope_has_all_required_fields(trigger_post_tool_use_hook):
    envelope, _env = trigger_post_tool_use_hook()
    assert_hook_envelope_required_fields(envelope, "PostToolUse")
    assert isinstance(envelope["tool_result"], str)
    assert isinstance(envelope["is_error"], bool)


def test_stop_envelope_has_all_required_fields(trigger_stop_hook):
    envelope, _env = trigger_stop_hook()
    assert_hook_envelope_required_fields(envelope, "Stop")
    # §2.3: optional fields MUST be empty string, not null/missing.
    assert envelope["draft_memory_id"] == "" or isinstance(
        envelope["draft_memory_id"], str
    )
    assert envelope["evidence_anchor_id"] == "" or isinstance(
        envelope["evidence_anchor_id"], str
    )


# ---------------------------------------------------------------------------
# ADR-V2-006 §3 — environment variables
# ---------------------------------------------------------------------------


def test_pre_tool_use_sets_grid_env_vars(trigger_pre_tool_use_hook):
    _envelope, env = trigger_pre_tool_use_hook()
    assert_grid_env_vars_present(env, "PreToolUse")
    assert env["GRID_TOOL_NAME"]  # non-empty for Pre


def test_stop_sets_grid_env_vars_with_empty_tool_name(trigger_stop_hook):
    _envelope, env = trigger_stop_hook()
    assert_grid_env_vars_present(env, "Stop")
    # §3 explicitly allows empty GRID_TOOL_NAME for Stop scope.
    assert env["GRID_TOOL_NAME"] == ""
