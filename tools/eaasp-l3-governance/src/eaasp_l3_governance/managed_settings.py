"""Pydantic models for the pre-compiled ``managed-settings.json`` payload.

MVP scope: we *accept* a pre-compiled JSON payload (no DSL compilation here).
The only shape invariants enforced are:

1. ``hooks`` is a list.
2. Each hook has a unique ``hook_id`` (string) within one version.
3. Each hook declares a ``mode`` in ``{'enforce', 'shadow'}``.
4. Optional ``agent_id`` / ``skill_id`` fields are used by the session
   ``/validate`` endpoint for simple wildcard match.

Anything else in the payload is preserved verbatim (``extras``) so that L1
runtimes that understand richer hook definitions keep receiving the data
they need — L3 MVP is deliberately permissive.
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

HookMode = Literal["enforce", "shadow"]
_VALID_MODES: set[str] = {"enforce", "shadow"}


class ManagedHook(BaseModel):
    """A single managed hook definition inside managed-settings.json."""

    model_config = ConfigDict(extra="allow")

    hook_id: str = Field(..., min_length=1)
    phase: str = Field(..., min_length=1)  # PreToolUse / PostToolUse / Stop / ...
    mode: HookMode = "enforce"
    agent_id: str | None = None  # "*" or exact match for session validate
    skill_id: str | None = None  # "*" or exact match for session validate
    handler: str | None = None  # free-form URI (http://…, python:pkg.mod:fn, …)


class ManagedSettings(BaseModel):
    """Top-level managed-settings.json shape.

    ``extras`` captures unknown top-level keys so a newer client payload
    round-trips cleanly through persistence.
    """

    model_config = ConfigDict(extra="allow")

    version: str | None = None  # caller-supplied semver, distinct from DB version
    hooks: list[ManagedHook] = Field(default_factory=list)

    @model_validator(mode="after")
    def _unique_hook_ids(self) -> "ManagedSettings":
        seen: set[str] = set()
        for hook in self.hooks:
            if hook.hook_id in seen:
                raise ValueError(f"duplicate hook_id in managed-settings: {hook.hook_id}")
            seen.add(hook.hook_id)
        return self

    def mode_summary(self) -> dict[str, int]:
        """Aggregate {'enforce': n, 'shadow': m}."""
        out: dict[str, int] = {"enforce": 0, "shadow": 0}
        for hook in self.hooks:
            out[hook.mode] = out.get(hook.mode, 0) + 1
        return out


def ensure_mode(mode: str) -> HookMode:
    """Defense-in-depth: validate mode even when coming from JSON/path params.

    Pydantic will usually reject invalid values before this helper is
    reached, but callers that accept raw strings (e.g. path params) must
    still funnel through here per reviewer note M4.
    """
    if mode not in _VALID_MODES:
        raise ValueError(f"mode must be 'enforce' or 'shadow', got {mode!r}")
    return mode  # type: ignore[return-value]


def hook_matches(
    hook: dict[str, Any] | ManagedHook,
    agent_id: str | None,
    skill_id: str | None,
) -> bool:
    """Simple wildcard matcher used by /sessions/{id}/validate.

    Matching rules (MVP):
    - ``None`` or ``"*"`` on the hook side → matches everything.
    - Otherwise, exact string match.
    - Missing field on the request side with a specific constraint on the
      hook side → does NOT match (the hook is more specific than the req).
    """
    h_agent = hook.get("agent_id") if isinstance(hook, dict) else hook.agent_id
    h_skill = hook.get("skill_id") if isinstance(hook, dict) else hook.skill_id

    if not _wildcard_match(h_agent, agent_id):
        return False
    if not _wildcard_match(h_skill, skill_id):
        return False
    return True


def _wildcard_match(hook_value: str | None, req_value: str | None) -> bool:
    if hook_value is None or hook_value == "*":
        return True
    if req_value is None:
        return False
    return hook_value == req_value
