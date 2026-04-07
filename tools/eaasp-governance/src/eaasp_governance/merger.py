"""Policy merger — four-scope hierarchical merge with deny-always-wins (KD-BH2).

Scope precedence (§4.5):
  managed (enterprise) > skill-scoped > project > user

Merge strategy:
- All rules from all scopes are collected into one flat list
- Evaluation uses deny-always-wins: if any rule denies, final result is deny
- Disabled rules are preserved but skipped during evaluation
"""

from __future__ import annotations

import json
import logging

logger = logging.getLogger(__name__)

# Scope priority (higher number = higher priority)
_SCOPE_PRIORITY: dict[str, int] = {
    "user": 0,
    "project": 1,
    "skill": 2,
    "managed": 3,
    # Alternative names
    "team": 0,
    "department": 1,
    "bu": 2,
    "enterprise": 3,
}


def merge_hooks(
    *layers: str | None,
    scope_order: list[str] | None = None,
) -> str:
    """Merge multiple managed_hooks_json strings into one.

    Args:
        *layers: JSON strings in ascending priority order
                 (first = lowest priority, last = highest).
                 None or empty strings are skipped.
        scope_order: Optional explicit scope labels for each layer
                     (for logging/audit purposes).

    Returns:
        Merged managed_hooks_json string with all rules combined.
    """
    all_rules: list[dict] = []
    seen_ids: set[str] = set()

    for i, layer_json in enumerate(layers):
        if not layer_json or layer_json in ("{}", ""):
            continue

        try:
            data = json.loads(layer_json)
        except json.JSONDecodeError:
            scope = (scope_order[i] if scope_order and i < len(scope_order)
                     else f"layer-{i}")
            logger.warning("Invalid JSON in %s, skipping", scope)
            continue

        rules = data.get("rules", [])
        for rule in rules:
            rule_id = rule.get("id", "")
            if rule_id in seen_ids:
                # Higher priority layer overrides lower priority rule with same id
                all_rules = [r for r in all_rules if r.get("id") != rule_id]
            seen_ids.add(rule_id)
            all_rules.append(rule)

    # Sort by id for deterministic output
    all_rules.sort(key=lambda r: r.get("id", ""))
    return json.dumps({"rules": all_rules}, ensure_ascii=False, sort_keys=True)


def merge_by_scope(
    managed: str | None = None,
    skill: str | None = None,
    project: str | None = None,
    user: str | None = None,
) -> str:
    """Merge hooks from four named scopes (§4.5 precedence).

    Layers in ascending priority: user < project < skill < managed.
    Same-id rules in higher-priority scope override lower ones.
    """
    return merge_hooks(
        user, project, skill, managed,
        scope_order=["user", "project", "skill", "managed"],
    )
