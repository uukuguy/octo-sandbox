"""Tests for HookExecutor."""

import json

from claude_code_runtime.hook_executor import HookExecutor


def _make_rules_json(*rules):
    return json.dumps({"rules": list(rules)})


def _deny_rm_rf():
    return {
        "id": "r-1",
        "name": "block-rm-rf",
        "hook_type": "pre_tool_call",
        "action": "deny",
        "reason": "destructive command blocked",
        "tool_pattern": "^bash$",
        "input_pattern": "rm -rf",
        "enabled": True,
    }


def _allow_all():
    return {
        "id": "r-2",
        "name": "allow-all",
        "hook_type": "pre_tool_call",
        "action": "allow",
        "enabled": True,
    }


def _stop_continue():
    return {
        "id": "r-3",
        "name": "force-continue",
        "hook_type": "stop",
        "action": "deny",
        "reason": "task incomplete",
        "enabled": True,
    }


def test_empty_executor_allows_all():
    exe = HookExecutor()
    decision, reason = exe.evaluate_pre_tool_call("bash", '{"command": "ls"}')
    assert decision == "allow"


def test_load_rules():
    exe = HookExecutor()
    count = exe.load_rules(_make_rules_json(_deny_rm_rf(), _allow_all()))
    assert count == 2
    assert exe.rule_count == 2


def test_deny_matching_tool():
    exe = HookExecutor()
    exe.load_rules(_make_rules_json(_deny_rm_rf()))
    decision, reason = exe.evaluate_pre_tool_call(
        "bash", '{"command": "rm -rf /"}'
    )
    assert decision == "deny"
    assert "destructive" in reason


def test_allow_non_matching_tool():
    exe = HookExecutor()
    exe.load_rules(_make_rules_json(_deny_rm_rf()))
    decision, _ = exe.evaluate_pre_tool_call("bash", '{"command": "ls -la"}')
    assert decision == "allow"


def test_deny_always_wins():
    exe = HookExecutor()
    exe.load_rules(_make_rules_json(_allow_all(), _deny_rm_rf()))
    decision, _ = exe.evaluate_pre_tool_call(
        "bash", '{"command": "rm -rf /tmp"}'
    )
    assert decision == "deny"


def test_different_tool_name():
    exe = HookExecutor()
    exe.load_rules(_make_rules_json(_deny_rm_rf()))
    decision, _ = exe.evaluate_pre_tool_call("read_file", "/etc/passwd")
    assert decision == "allow"


def test_disabled_rule_skipped():
    exe = HookExecutor()
    rule = _deny_rm_rf()
    rule["enabled"] = False
    exe.load_rules(_make_rules_json(rule))
    decision, _ = exe.evaluate_pre_tool_call(
        "bash", '{"command": "rm -rf /"}'
    )
    assert decision == "allow"


def test_stop_continue():
    exe = HookExecutor()
    exe.load_rules(_make_rules_json(_stop_continue()))
    decision, feedback = exe.evaluate_stop()
    assert decision == "continue"
    assert "incomplete" in feedback


def test_stop_complete_no_rules():
    exe = HookExecutor()
    decision, _ = exe.evaluate_stop()
    assert decision == "complete"


def test_invalid_json():
    exe = HookExecutor()
    count = exe.load_rules("not valid json")
    assert count == 0


def test_empty_json():
    exe = HookExecutor()
    count = exe.load_rules("{}")
    assert count == 0
