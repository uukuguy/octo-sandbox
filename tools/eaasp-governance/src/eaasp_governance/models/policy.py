"""Policy DSL models — Kubernetes-style YAML schema (KD-BH1).

Aligned with EAASP spec v1.7 §4.5 and §10.
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, Field


class RuleMatch(BaseModel):
    """Matching criteria for a policy rule."""

    tool_name: str = ""  # regex pattern
    input_pattern: str = ""  # regex pattern for input content


class PolicyRule(BaseModel):
    """A single policy rule in the DSL.

    Maps to one entry in compiled managed_hooks_json.
    """

    id: str
    name: str
    description: str = ""
    event: Literal[
        "PreToolUse", "PostToolUse", "Stop",
        "PreEdit", "PostEdit", "PreCommand", "PostCommand",
        "SessionStart", "SessionEnd",
    ]
    handler_type: Literal["command", "http", "prompt", "agent"] = "command"
    match: RuleMatch = Field(default_factory=RuleMatch)
    action: Literal["allow", "deny"] = "deny"
    reason: str = ""
    severity: Literal["critical", "high", "medium", "low", "info"] = "medium"
    audit: bool = False
    enabled: bool = True
    config: dict[str, Any] = Field(default_factory=dict)


class PolicyMetadata(BaseModel):
    """PolicyBundle metadata block."""

    name: str
    scope: Literal["enterprise", "bu", "department", "team"] = "enterprise"
    org_unit: str = ""
    version: str = "1.0.0"


class PolicyBundle(BaseModel):
    """Top-level policy bundle — maps to one YAML file.

    apiVersion: eaasp.io/v1
    kind: PolicyBundle
    """

    apiVersion: str = "eaasp.io/v1"
    kind: Literal["PolicyBundle"] = "PolicyBundle"
    metadata: PolicyMetadata
    rules: list[PolicyRule] = Field(default_factory=list)
