"""契约 3: 技能生命周期 API (§8.3).

GET /v1/skills/{id}/governance — skill governance status
"""

from __future__ import annotations

from fastapi import APIRouter, Request

router = APIRouter(prefix="/v1/skills", tags=["skills"])


@router.get("/{skill_id}/governance")
async def get_skill_governance(skill_id: str, request: Request):
    """Return governance info for a skill (applicable policies, hooks summary)."""
    store = request.app.state.policy_store

    applicable_policies = []
    total_hooks = 0

    for versions in store.values():
        if versions:
            policy = versions[-1]  # current version
            applicable_policies.append({
                "id": policy["id"],
                "scope": policy["scope"],
                "rules_count": policy["rules_count"],
            })
            total_hooks += policy["rules_count"]

    return {
        "skill_id": skill_id,
        "status": "active",
        "applicable_policies": applicable_policies,
        "hooks_summary": {
            "total_hooks": total_hooks,
            "policy_count": len(applicable_policies),
        },
    }
