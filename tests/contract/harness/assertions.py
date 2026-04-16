"""Shared contract-assertion helpers.

Keep this module intentionally small — the bulk of contract semantics
live in the test case bodies under ``contract_v1/``. Helpers here are
limited to schema constants and small utility predicates that appear in
more than one test file.
"""

from __future__ import annotations

from typing import Any

# Canonical event_type set per contract-v1.
# Source of truth: proto/eaasp/runtime/v2/common.proto EventType enum
# + ADR-V2-018 (PRE_COMPACT). Any test that enumerates event types MUST
# import from this constant rather than repeat the set inline.
EVENT_TYPES_V1: frozenset[str] = frozenset(
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

# Canonical hook event set per ADR-V2-006 §2.
HOOK_EVENTS_V1: frozenset[str] = frozenset({"PreToolUse", "PostToolUse", "Stop"})


def assert_event_type_in(actual: str, allowed: frozenset[str] = EVENT_TYPES_V1) -> None:
    """Assert ``actual`` is a member of ``allowed``.

    Args:
        actual: Event-type string emitted by the runtime under test.
        allowed: Set of permitted values (defaults to :data:`EVENT_TYPES_V1`).
    """
    assert actual in allowed, f"event_type {actual!r} not in {sorted(allowed)}"


def assert_hook_envelope_required_fields(
    envelope: dict[str, Any], scope: str
) -> None:
    """Assert the hook envelope carries the required ADR-V2-006 §2 fields.

    Required keys are dispatched by ``scope``:

    * ``"PreToolUse"``  — ``event``, ``session_id``, ``skill_id``,
      ``tool_name``, ``tool_args``, ``created_at``
    * ``"PostToolUse"`` — ``event``, ``session_id``, ``skill_id``,
      ``tool_name``, ``tool_result``, ``is_error``, ``created_at``
    * ``"Stop"``        — ``event``, ``session_id``, ``skill_id``,
      ``draft_memory_id``, ``evidence_anchor_id``, ``created_at``

    Args:
        envelope: Parsed JSON hook envelope.
        scope: One of :data:`HOOK_EVENTS_V1`.

    Raises:
        AssertionError: On any missing field. Messages include the full
            envelope for forensic inspection.
    """
    assert scope in HOOK_EVENTS_V1, f"unknown hook scope {scope!r}"
    required = {
        "PreToolUse": (
            "event",
            "session_id",
            "skill_id",
            "tool_name",
            "tool_args",
            "created_at",
        ),
        "PostToolUse": (
            "event",
            "session_id",
            "skill_id",
            "tool_name",
            "tool_result",
            "is_error",
            "created_at",
        ),
        "Stop": (
            "event",
            "session_id",
            "skill_id",
            "draft_memory_id",
            "evidence_anchor_id",
            "created_at",
        ),
    }[scope]
    missing = [k for k in required if k not in envelope]
    assert not missing, (
        f"envelope for scope {scope!r} missing fields {missing}; got keys "
        f"{sorted(envelope.keys())}"
    )
    assert envelope.get("event") == scope, (
        f"envelope.event = {envelope.get('event')!r}, expected {scope!r}"
    )


def assert_grid_env_vars_present(env: dict[str, str], scope: str) -> None:
    """Assert that ADR-V2-006 §3 GRID_* env vars are populated.

    Per §3 the runtime MUST always set the four env vars:
    ``GRID_SESSION_ID``, ``GRID_TOOL_NAME``, ``GRID_SKILL_ID``, ``GRID_EVENT``.

    For ``scope="Stop"`` ``GRID_TOOL_NAME`` may be empty string, but the
    variable MUST still be present.
    """
    assert scope in HOOK_EVENTS_V1, f"unknown hook scope {scope!r}"
    required_keys = ("GRID_SESSION_ID", "GRID_TOOL_NAME", "GRID_SKILL_ID", "GRID_EVENT")
    missing = [k for k in required_keys if k not in env]
    assert not missing, (
        f"hook env missing GRID_* vars {missing} for scope {scope!r}; "
        f"got keys {sorted(env.keys())}"
    )
    assert env["GRID_EVENT"] == scope, (
        f"GRID_EVENT = {env['GRID_EVENT']!r}, expected {scope!r}"
    )
