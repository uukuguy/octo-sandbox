"""Contract 1 — Policy engine unit tests."""

from __future__ import annotations

import pytest

from eaasp_l3_governance.managed_settings import ManagedSettings
from eaasp_l3_governance.policy_engine import PolicyEngine


pytestmark = pytest.mark.asyncio


def _sample_settings(enforce_count: int = 2, shadow_count: int = 1) -> ManagedSettings:
    hooks: list[dict] = []
    for i in range(enforce_count):
        hooks.append(
            {
                "hook_id": f"h_enforce_{i}",
                "phase": "PostToolUse",
                "mode": "enforce",
                "agent_id": "*",
                "skill_id": "*",
                "handler": "http://audit.local/ingest",
            }
        )
    for i in range(shadow_count):
        hooks.append(
            {
                "hook_id": f"h_shadow_{i}",
                "phase": "PreToolUse",
                "mode": "shadow",
            }
        )
    return ManagedSettings(version="v2.0.0-mvp", hooks=hooks)  # type: ignore[arg-type]


async def test_deploy_creates_version_row(policy_engine: PolicyEngine) -> None:
    result = await policy_engine.deploy(_sample_settings())
    assert result.version == 1
    assert result.hook_count == 3
    assert result.mode_summary == {"enforce": 2, "shadow": 1}
    assert result.created_at  # non-empty ISO string


async def test_deploy_increments_version(policy_engine: PolicyEngine) -> None:
    r1 = await policy_engine.deploy(_sample_settings(1, 0))
    r2 = await policy_engine.deploy(_sample_settings(2, 1))
    r3 = await policy_engine.deploy(_sample_settings(0, 3))
    assert [r1.version, r2.version, r3.version] == [1, 2, 3]


async def test_list_versions_newest_first(policy_engine: PolicyEngine) -> None:
    for _ in range(3):
        await policy_engine.deploy(_sample_settings())

    versions = await policy_engine.list_versions()
    assert [v.version for v in versions] == [3, 2, 1]
    for v in versions:
        assert v.hook_count == 3
        assert v.mode_summary == {"enforce": 2, "shadow": 1}


async def test_list_versions_limit_clamped(policy_engine: PolicyEngine) -> None:
    for _ in range(5):
        await policy_engine.deploy(_sample_settings(1, 0))

    # Oversized limit is silently clamped to the configured maximum (500).
    huge = await policy_engine.list_versions(limit=99999)
    assert len(huge) == 5

    # Zero / negative coerces to default (100).
    defaulted = await policy_engine.list_versions(limit=0)
    assert len(defaulted) == 5


async def test_latest_version_returns_full_payload(policy_engine: PolicyEngine) -> None:
    assert await policy_engine.latest_version() is None

    await policy_engine.deploy(_sample_settings(1, 0))
    await policy_engine.deploy(_sample_settings(2, 2))
    latest = await policy_engine.latest_version()
    assert latest is not None
    assert latest.version == 2
    assert latest.hook_count == 4
    assert isinstance(latest.payload, dict)
    assert "hooks" in latest.payload
    assert len(latest.payload["hooks"]) == 4


async def test_switch_mode_upserts_override(policy_engine: PolicyEngine) -> None:
    override = await policy_engine.switch_mode("h_enforce_0", "shadow")
    assert override.hook_id == "h_enforce_0"
    assert override.mode == "shadow"

    fetched = await policy_engine.get_mode_override("h_enforce_0")
    assert fetched is not None
    assert fetched.mode == "shadow"

    # Flip it back — should not bump version, just overwrite in place.
    flipped = await policy_engine.switch_mode("h_enforce_0", "enforce")
    assert flipped.mode == "enforce"


async def test_switch_mode_rejects_unknown(policy_engine: PolicyEngine) -> None:
    with pytest.raises(ValueError):
        await policy_engine.switch_mode("h_enforce_0", "paused")


async def test_get_mode_override_returns_none_when_unset(
    policy_engine: PolicyEngine,
) -> None:
    assert await policy_engine.get_mode_override("unknown_hook") is None


async def test_duplicate_hook_ids_rejected_at_payload_level() -> None:
    with pytest.raises(ValueError, match="duplicate hook_id"):
        ManagedSettings(
            hooks=[
                {"hook_id": "dup", "phase": "PostToolUse"},  # type: ignore[list-item]
                {"hook_id": "dup", "phase": "PreToolUse"},  # type: ignore[list-item]
            ]
        )
