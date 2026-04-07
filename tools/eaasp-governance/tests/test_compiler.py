"""Tests for policy DSL compiler and merger — 8 tests."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from eaasp_governance.compiler import (
    CompileError,
    compile_bundle,
    compile_policy_yaml,
    compile_yaml_to_hooks,
)
from eaasp_governance.merger import merge_by_scope
from eaasp_governance.models.policy import PolicyBundle

# ── Fixtures ────────────────────────────────────────────────

EXAMPLES_DIR = Path(__file__).resolve().parents[3] / "sdk" / "examples" / "hr-onboarding" / "policies"


def _load_yaml(name: str) -> str:
    return (EXAMPLES_DIR / name).read_text()


# ── Test 1: Parse enterprise policy YAML ────────────────────

def test_parse_enterprise_yaml():
    """Enterprise policy YAML parses into a valid PolicyBundle."""
    bundle = compile_policy_yaml(_load_yaml("enterprise.yaml"))
    assert isinstance(bundle, PolicyBundle)
    assert bundle.metadata.name == "enterprise-security-baseline"
    assert bundle.metadata.scope == "enterprise"
    assert len(bundle.rules) == 2
    assert bundle.rules[0].id == "pii-guard"
    assert bundle.rules[0].action == "deny"
    assert bundle.rules[1].id == "audit-all-writes"
    assert bundle.rules[1].audit is True


# ── Test 2: Parse bu_hr policy YAML ─────────────────────────

def test_parse_bu_hr_yaml():
    """BU-level HR policy YAML parses correctly."""
    bundle = compile_policy_yaml(_load_yaml("bu_hr.yaml"))
    assert bundle.metadata.scope == "bu"
    assert bundle.metadata.org_unit == "hr-dept"
    assert len(bundle.rules) == 2
    assert bundle.rules[0].id == "checklist-enforcement"
    assert bundle.rules[0].event == "Stop"
    assert bundle.rules[1].id == "bash-deny"


# ── Test 3: Compile to managed_hooks_json ───────────────────

def test_compile_to_hooks_json():
    """Compiler produces valid managed_hooks_json consumable by HookExecutor."""
    bundle = compile_policy_yaml(_load_yaml("enterprise.yaml"))
    hooks_json = compile_bundle(bundle)
    data = json.loads(hooks_json)
    assert "rules" in data
    assert len(data["rules"]) == 2
    # Sorted by id
    assert data["rules"][0]["id"] == "audit-all-writes"
    assert data["rules"][1]["id"] == "pii-guard"
    # Check hook_type mapping
    assert data["rules"][1]["hook_type"] == "pre_tool_call"
    assert data["rules"][0]["hook_type"] == "post_tool_result"


# ── Test 4: Idempotent compilation (KD-BH3) ────────────────

def test_compile_idempotent():
    """Same input always produces same output (KD-BH3)."""
    yaml_content = _load_yaml("enterprise.yaml")
    json1, digest1 = compile_yaml_to_hooks(yaml_content)
    json2, digest2 = compile_yaml_to_hooks(yaml_content)
    assert json1 == json2
    assert digest1 == digest2


# ── Test 5: Invalid YAML raises CompileError ────────────────

def test_invalid_yaml_raises():
    """Invalid YAML content raises CompileError."""
    with pytest.raises(CompileError, match="Invalid YAML"):
        compile_policy_yaml("  bad: yaml: [unclosed")


# ── Test 6: Wrong kind raises CompileError ──────────────────

def test_wrong_kind_raises():
    """YAML with wrong kind raises CompileError."""
    yaml_content = """
apiVersion: eaasp.io/v1
kind: WrongKind
metadata:
  name: test
rules: []
"""
    with pytest.raises(CompileError, match="Expected kind=PolicyBundle"):
        compile_policy_yaml(yaml_content)


# ── Test 7: Merger — deny-always-wins across scopes ─────────

def test_merger_deny_always_wins():
    """Merge rules from multiple scopes; deny from any scope persists."""
    # User scope: allow file_write
    user_hooks = json.dumps({"rules": [
        {"id": "user-allow", "hook_type": "pre_tool_call",
         "action": "allow", "tool_pattern": "^file_write$"},
    ]})
    # Enterprise scope: deny file_write with PII
    enterprise_hooks = json.dumps({"rules": [
        {"id": "pii-guard", "hook_type": "pre_tool_call",
         "action": "deny", "tool_pattern": "^file_write$",
         "input_pattern": "\\d{3}-\\d{2}-\\d{4}"},
    ]})

    merged = merge_by_scope(managed=enterprise_hooks, user=user_hooks)
    data = json.loads(merged)
    assert len(data["rules"]) == 2
    # Both rules present — HookExecutor evaluates deny-always-wins at runtime
    ids = {r["id"] for r in data["rules"]}
    assert "pii-guard" in ids
    assert "user-allow" in ids


# ── Test 8: Merger — same-id override by higher priority ────

def test_merger_same_id_override():
    """Higher-priority scope overrides lower-priority rule with same id."""
    user_hooks = json.dumps({"rules": [
        {"id": "pii-guard", "hook_type": "pre_tool_call",
         "action": "allow", "reason": "user override"},
    ]})
    managed_hooks = json.dumps({"rules": [
        {"id": "pii-guard", "hook_type": "pre_tool_call",
         "action": "deny", "reason": "enterprise policy"},
    ]})

    merged = merge_by_scope(managed=managed_hooks, user=user_hooks)
    data = json.loads(merged)
    # Only one rule with id "pii-guard" — enterprise wins
    assert len(data["rules"]) == 1
    assert data["rules"][0]["action"] == "deny"
    assert data["rules"][0]["reason"] == "enterprise policy"
