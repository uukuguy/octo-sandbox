"""SessionPayload assembly helpers — P1..P5 block builder.

MVP scope: produces a JSON-serializable dict matching the SessionPayload proto
shape. Budget flags default per blueprint:

- ``allow_trim_p5=True``  — user preferences are the first to go
- ``allow_trim_p4=False`` — skill instructions are critical
- ``allow_trim_p3=False`` — memory refs are critical in MVP
"""

from __future__ import annotations

from typing import Any


def build_session_payload(
    *,
    session_id: str,
    user_id: str,
    runtime_id: str,
    policy_context: dict[str, Any],
    event_context: dict[str, Any] | None,
    memory_refs: list[dict[str, Any]],
    skill_instructions: dict[str, Any],
    user_preferences: dict[str, Any],
    created_at: int,
) -> dict[str, Any]:
    """Assemble a five-block SessionPayload dict.

    ``memory_refs`` comes from L2 ``/api/v1/memory/search`` hits — the helper
    normalizes each hit into the MemoryRef shape so downstream consumers can
    rely on the keys being present.
    """
    normalized_refs = [_normalize_memory_ref(hit) for hit in (memory_refs or [])]

    return {
        "session_id": session_id,
        "runtime_id": runtime_id,
        "created_at": created_at,
        # P1 — PolicyContext (from L3 validate response).
        "policy_context": _normalize_policy_context(policy_context),
        # P2 — EventContext (currently empty; D32 will backfill from L2 anchors).
        "event_context": event_context or {},
        # P3 — MemoryRefs (from L2 hybrid search).
        "memory_refs": normalized_refs,
        # P4 — SkillInstructions (resolved from L2 registry in later phases).
        "skill_instructions": skill_instructions or {},
        # P5 — UserPreferences.
        "user_preferences": user_preferences or {"user_id": user_id, "prefs": {}},
        # Budget trim flags — P5 first, P4/P3 locked in MVP.
        "allow_trim_p5": True,
        "allow_trim_p4": False,
        "allow_trim_p3": False,
    }


def _normalize_memory_ref(hit: dict[str, Any]) -> dict[str, Any]:
    """Map an L2 search hit into the MemoryRef dict shape.

    L2 hits vary in shape across versions; fall back to sensible defaults so
    the payload is always well-formed.
    """
    return {
        "memory_id": str(hit.get("memory_id") or hit.get("id") or ""),
        "memory_type": str(hit.get("memory_type") or hit.get("category") or ""),
        "relevance_score": float(
            hit.get("relevance_score") or hit.get("score") or 0.0
        ),
        "summary": str(hit.get("summary") or hit.get("content") or ""),
    }


def _normalize_policy_context(raw: dict[str, Any]) -> dict[str, Any]:
    """Ensure PolicyContext always has the expected keys for downstream code."""
    return {
        "hooks": list(raw.get("hooks") or []),
        "policy_version": str(raw.get("policy_version") or ""),
        "deploy_timestamp": str(raw.get("deploy_timestamp") or ""),
        "org_unit": str(raw.get("org_unit") or ""),
        "quotas": dict(raw.get("quotas") or {}),
    }
