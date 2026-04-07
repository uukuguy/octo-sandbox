"""E2E: 5 API 契约冒烟测试 (5 tests).

Tests the L3 governance service contracts end-to-end via TestClient.
"""

from __future__ import annotations

import pytest

from tests.e2e.helpers import create_session


# ── 契约 1: 策略部署 ─────────────────────────────────────────

@pytest.mark.e2e
@pytest.mark.mock_llm
def test_contract_policy_deploy(l3_client):
    """契约1: policies are deployed and queryable."""
    resp = l3_client.get("/v1/policies")
    assert resp.status_code == 200
    policies = resp.json()
    assert len(policies) >= 2
    scopes = {p["scope"] for p in policies}
    assert "enterprise" in scopes
    assert "bu" in scopes


# ── 契约 2: 意图网关 ─────────────────────────────────────────

@pytest.mark.e2e
@pytest.mark.mock_llm
def test_contract_intent_gateway(l3_client):
    """契约2: intent resolution maps text to skill."""
    resp = l3_client.post("/v1/intents", json={
        "text": "新员工入职",
        "user_id": "e2e-user",
        "org_unit": "hr-dept",
    })
    assert resp.status_code == 200
    data = resp.json()
    assert data["skill_id"] == "hr-onboarding"
    assert data["confidence"] > 0.0  # multi-keyword proportional scoring


# ── 契约 3: 技能生命周期 ─────────────────────────────────────

@pytest.mark.e2e
@pytest.mark.mock_llm
def test_contract_skill_governance(l3_client):
    """契约3: skill governance reports applicable policies."""
    resp = l3_client.get("/v1/skills/hr-onboarding/governance")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "active"
    assert data["hooks_summary"]["total_hooks"] >= 4


# ── 契约 4: 遥测采集 ─────────────────────────────────────────

@pytest.mark.e2e
@pytest.mark.mock_llm
def test_contract_telemetry(l3_client):
    """契约4: telemetry events are accepted and queryable."""
    # Ingest
    resp = l3_client.post("/v1/telemetry", json={
        "session_id": "e2e-sess-001",
        "events": [
            {"event_type": "tool_call", "payload": {"tool": "file_write"}},
            {"event_type": "hook_fired", "payload": {"rule": "pii-guard", "action": "deny"}},
        ],
    })
    assert resp.status_code == 200
    assert resp.json()["accepted"] == 2

    # Query
    resp = l3_client.get("/v1/telemetry/sessions/e2e-sess-001")
    assert resp.status_code == 200
    assert len(resp.json()["events"]) == 2


# ── 契约 5: 会话控制 ─────────────────────────────────────────

@pytest.mark.e2e
@pytest.mark.mock_llm
def test_contract_session_control(l3_client):
    """契约5: session lifecycle (create → query → terminate)."""
    # Create
    session = create_session(l3_client)
    session_id = session["session_id"]
    assert session["runtime_id"] == "grid"
    assert session["governance_summary"]["hooks_count"] > 0

    # Query
    resp = l3_client.get(f"/v1/sessions/{session_id}")
    assert resp.status_code == 200
    assert resp.json()["status"] == "active"

    # Terminate
    resp = l3_client.delete(f"/v1/sessions/{session_id}")
    assert resp.status_code == 200
    assert resp.json()["status"] == "terminated"
