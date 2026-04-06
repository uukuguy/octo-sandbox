"""Hook executor — T1 local hook evaluation (no HookBridge needed).

Evaluates hooks against managed-settings rules loaded from SessionPayload.
Implements deny-always-wins policy (aligned with Rust InProcessHookBridge).
"""

from __future__ import annotations

import json
import logging
import re
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class HookRule:
    """A single hook evaluation rule from managed-settings."""

    rule_id: str
    name: str
    hook_type: str  # "pre_tool_call" | "post_tool_result" | "stop"
    action: str  # "allow" | "deny" | "modify"
    reason: str = ""
    tool_pattern: str = ""  # regex pattern for tool name
    input_pattern: str = ""  # regex pattern for input content
    enabled: bool = True


class HookExecutor:
    """T1 local hook evaluation engine.

    Evaluates hooks against rules loaded from managed_hooks_json.
    Deny-always-wins: if any rule returns deny, final result is deny.
    """

    def __init__(self):
        self._rules: list[HookRule] = []

    def load_rules(self, managed_hooks_json: str) -> int:
        """Load hook rules from managed-settings JSON.

        Returns number of rules loaded.
        """
        if not managed_hooks_json or managed_hooks_json in ("{}", ""):
            return 0

        try:
            data = json.loads(managed_hooks_json)
        except json.JSONDecodeError:
            logger.warning("Invalid managed_hooks_json, skipping")
            return 0

        rules = data.get("rules", [])
        for r in rules:
            self._rules.append(
                HookRule(
                    rule_id=r.get("id", ""),
                    name=r.get("name", ""),
                    hook_type=r.get("hook_type", ""),
                    action=r.get("action", "allow"),
                    reason=r.get("reason", ""),
                    tool_pattern=r.get("tool_pattern", ""),
                    input_pattern=r.get("input_pattern", ""),
                    enabled=r.get("enabled", True),
                )
            )

        loaded = len(rules)
        logger.info("Loaded %d hook rules", loaded)
        return loaded

    def evaluate_pre_tool_call(
        self, tool_name: str, input_json: str
    ) -> tuple[str, str]:
        """Evaluate pre-tool-call hook.

        Returns (decision, reason) where decision is "allow"|"deny"|"modify".
        """
        return self._evaluate("pre_tool_call", tool_name, input_json)

    def evaluate_post_tool_result(
        self, tool_name: str, output: str, is_error: bool
    ) -> tuple[str, str]:
        """Evaluate post-tool-result hook."""
        return self._evaluate("post_tool_result", tool_name, output)

    def evaluate_stop(self) -> tuple[str, str]:
        """Evaluate stop hook.

        Returns ("complete", "") or ("continue", feedback).
        """
        for rule in self._rules:
            if not rule.enabled or rule.hook_type != "stop":
                continue
            if rule.action == "deny":
                return "continue", rule.reason
        return "complete", ""

    def _evaluate(
        self, hook_type: str, tool_name: str, content: str
    ) -> tuple[str, str]:
        """Core evaluation logic. Deny-always-wins."""
        result_action = "allow"
        result_reason = ""

        for rule in self._rules:
            if not rule.enabled or rule.hook_type != hook_type:
                continue

            if not self._matches(rule, tool_name, content):
                continue

            logger.debug(
                "Rule matched: %s (%s) -> %s",
                rule.name,
                rule.rule_id,
                rule.action,
            )

            if rule.action == "deny":
                # Deny always wins (EAASP §10.8)
                return "deny", rule.reason
            elif rule.action == "modify":
                result_action = "modify"
                result_reason = rule.reason

        return result_action, result_reason

    def _matches(self, rule: HookRule, tool_name: str, content: str) -> bool:
        """Check if a rule matches the given tool/content."""
        if rule.tool_pattern:
            if not re.search(rule.tool_pattern, tool_name):
                return False

        if rule.input_pattern:
            if not re.search(rule.input_pattern, content):
                return False

        # If no patterns specified, rule matches everything
        return True

    @property
    def rule_count(self) -> int:
        return len(self._rules)
