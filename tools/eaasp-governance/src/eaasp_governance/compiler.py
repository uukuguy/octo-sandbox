"""Policy DSL compiler — YAML → managed_hooks_json (KD-BH3: idempotent output).

Compiles Kubernetes-style PolicyBundle YAML into the managed_hooks_json format
that both L1 runtimes (grid-runtime Rust, claude-code-runtime Python) consume.
"""

from __future__ import annotations

import hashlib
import json
import logging

import yaml
from pydantic import ValidationError

from eaasp_governance.models.policy import PolicyBundle, PolicyRule

logger = logging.getLogger(__name__)

# Event name → hook_type mapping (DSL event → L1 runtime hook_type)
_EVENT_TO_HOOK_TYPE: dict[str, str] = {
    "PreToolUse": "pre_tool_call",
    "PostToolUse": "post_tool_result",
    "Stop": "stop",
    "PreEdit": "pre_edit",
    "PostEdit": "post_edit",
    "PreCommand": "pre_command",
    "PostCommand": "post_command",
    "SessionStart": "session_start",
    "SessionEnd": "session_end",
}


class CompileError(Exception):
    """Raised when policy compilation fails."""


def compile_policy_yaml(yaml_content: str) -> PolicyBundle:
    """Parse and validate a PolicyBundle YAML string.

    Raises CompileError on invalid YAML or schema violations.
    """
    try:
        data = yaml.safe_load(yaml_content)
    except yaml.YAMLError as e:
        raise CompileError(f"Invalid YAML: {e}") from e

    if not isinstance(data, dict):
        raise CompileError("Policy YAML must be a mapping")

    if data.get("kind") != "PolicyBundle":
        raise CompileError(f"Expected kind=PolicyBundle, got {data.get('kind')}")

    try:
        bundle = PolicyBundle.model_validate(data)
    except ValidationError as e:
        raise CompileError(f"Schema validation failed: {e}") from e

    return bundle


def _rule_to_hook_entry(rule: PolicyRule) -> dict:
    """Convert a PolicyRule to a managed_hooks_json rule entry."""
    entry: dict = {
        "id": rule.id,
        "name": rule.name,
        "hook_type": _EVENT_TO_HOOK_TYPE.get(rule.event, rule.event.lower()),
        "action": rule.action,
        "reason": rule.reason,
        "enabled": rule.enabled,
    }
    if rule.match.tool_name:
        entry["tool_pattern"] = rule.match.tool_name
    if rule.match.input_pattern:
        entry["input_pattern"] = rule.match.input_pattern
    if rule.audit:
        entry["audit"] = True
    if rule.config:
        entry["config"] = rule.config
    return entry


def compile_bundle(bundle: PolicyBundle) -> str:
    """Compile a PolicyBundle into managed_hooks_json string.

    Output is idempotent: same input → same output (KD-BH3).
    Rules are sorted by id for deterministic output.
    """
    entries = [_rule_to_hook_entry(r) for r in bundle.rules]
    # Sort by id for idempotent output
    entries.sort(key=lambda e: e["id"])
    result = {"rules": entries}
    return json.dumps(result, ensure_ascii=False, sort_keys=True)


def compile_yaml_to_hooks(yaml_content: str) -> tuple[str, str]:
    """One-shot: YAML string → (managed_hooks_json, digest).

    Returns (json_string, sha256_hex_digest).
    """
    bundle = compile_policy_yaml(yaml_content)
    hooks_json = compile_bundle(bundle)
    digest = hashlib.sha256(hooks_json.encode()).hexdigest()[:16]
    return hooks_json, digest
