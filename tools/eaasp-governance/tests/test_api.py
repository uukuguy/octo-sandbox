"""Tests for L3 Governance Service — 5 API contracts (12 tests).

Uses FastAPI TestClient for in-process HTTP testing.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from eaasp_governance.main import create_app

# ── Fixtures ────────────────────────────────────────────────

EXAMPLES_DIR = Path(__file__).resolve().parents[3] / "sdk" / "examples" / "hr-onboarding" / "policies"
CONFIG_DIR = Path(__file__).resolve().parents[1] / "config"


@pytest.fixture
def client():
    """Create a test client with mocked L2/L1."""
    app = create_app(
        l2_url="http://mock-l2:8081",
        runtimes_config=str(CONFIG_DIR / "runtimes.yaml"),
    )
    return TestClient(app)


@pytest.fixture
def client_with_policies(client):
    """Deploy both example policies and return the client."""
    for yaml_file in ["enterprise.yaml", "bu_hr.yaml"]:
        yaml_content = (EXAMPLES_DIR / yaml_file).read_text()
        resp = client.put(
            "/v1/policies/deploy",
            content=yaml_content,
            headers={"Content-Type": "application/yaml"},
        )
        assert resp.status_code == 200
    return client


# ── 契约 1: 策略部署 (3 tests) ──────────────────────────────

def test_deploy_policy(client):
    """PUT /v1/policies/deploy compiles and stores a policy."""
    yaml_content = (EXAMPLES_DIR / "enterprise.yaml").read_text()
    resp = client.put(
        "/v1/policies/deploy",
        content=yaml_content,
        headers={"Content-Type": "application/yaml"},
    )
    assert resp.status_code == 200
    data = resp.json()
    assert data["policy_id"] == "enterprise-security-baseline"
    assert data["rules_count"] == 2
    assert len(data["compiled_hooks_digest"]) == 16


def test_list_policies(client_with_policies):
    """GET /v1/policies returns all deployed policies."""
    resp = client_with_policies.get("/v1/policies")
    assert resp.status_code == 200
    policies = resp.json()
    assert len(policies) == 2
    names = {p["name"] for p in policies}
    assert "enterprise-security-baseline" in names
    assert "hr-department-policies" in names


def test_get_policy_detail(client_with_policies):
    """GET /v1/policies/{id} returns full policy with compiled hooks."""
    resp = client_with_policies.get("/v1/policies/enterprise-security-baseline")
    assert resp.status_code == 200
    data = resp.json()
    assert data["rules_count"] == 2
    assert "compiled_hooks_json" in data
    assert "compiled_hooks_digest" in data


# ── 契约 2: 意图网关 (2 tests) ──────────────────────────────

def test_intent_match(client):
    """POST /v1/intents resolves '入职' to hr-onboarding."""
    resp = client.post("/v1/intents", json={
        "text": "新员工张三入职",
        "user_id": "user-001",
        "org_unit": "hr-dept",
    })
    assert resp.status_code == 200
    data = resp.json()
    assert data["skill_id"] == "hr-onboarding"
    assert data["confidence"] > 0.0  # multi-keyword proportional scoring


def test_intent_no_match(client):
    """POST /v1/intents returns null for unknown intents."""
    resp = client.post("/v1/intents", json={
        "text": "天气预报",
        "user_id": "user-001",
    })
    assert resp.status_code == 200
    data = resp.json()
    assert data["skill_id"] is None
    assert data["confidence"] == 0.0


# ── 契约 3: 技能生命周期 (1 test) ───────────────────────────

def test_skill_governance(client_with_policies):
    """GET /v1/skills/{id}/governance returns applicable policies."""
    resp = client_with_policies.get("/v1/skills/hr-onboarding/governance")
    assert resp.status_code == 200
    data = resp.json()
    assert data["skill_id"] == "hr-onboarding"
    assert data["status"] == "active"
    assert len(data["applicable_policies"]) == 2
    assert data["hooks_summary"]["total_hooks"] == 4  # 2 enterprise + 2 bu


# ── 契约 4: 遥测采集 (2 tests) ──────────────────────────────

def test_telemetry_ingest(client):
    """POST /v1/telemetry accepts events."""
    resp = client.post("/v1/telemetry", json={
        "session_id": "sess-test-001",
        "events": [
            {"event_type": "tool_call", "timestamp": "2026-04-07T12:00:00Z",
             "payload": {"tool": "file_write"}, "resource_usage": {"tokens": 100}},
            {"event_type": "hook_fired", "timestamp": "2026-04-07T12:00:01Z",
             "payload": {"rule_id": "pii-guard", "action": "deny"}},
        ],
    })
    assert resp.status_code == 200
    data = resp.json()
    assert data["accepted"] == 2
    assert data["rejected"] == 0


def test_telemetry_query(client):
    """GET /v1/telemetry/sessions/{id} returns ingested events."""
    # Ingest first
    client.post("/v1/telemetry", json={
        "session_id": "sess-query-001",
        "events": [
            {"event_type": "tool_call", "resource_usage": {"tokens": 50}},
            {"event_type": "tool_call", "resource_usage": {"tokens": 30}},
        ],
    })

    resp = client.get("/v1/telemetry/sessions/sess-query-001")
    assert resp.status_code == 200
    data = resp.json()
    assert len(data["events"]) == 2
    assert data["resource_summary"]["tokens"] == 80


# ── 契约 5: 会话控制 (4 tests) ──────────────────────────────

def test_create_session(client_with_policies):
    """POST /v1/sessions creates a session via three-way handshake."""
    resp = client_with_policies.post("/v1/sessions", json={
        "user_id": "user-001",
        "user_role": "hr_specialist",
        "org_unit": "hr-dept",
        "skill_id": "hr-onboarding",
        "runtime_preference": "grid",
    })
    assert resp.status_code == 200
    data = resp.json()
    assert "session_id" in data
    assert data["runtime_id"] == "grid"
    assert data["governance_summary"]["hooks_count"] > 0


def test_get_session_status(client_with_policies):
    """GET /v1/sessions/{id} returns session status."""
    # Create first
    create_resp = client_with_policies.post("/v1/sessions", json={
        "user_id": "user-002",
        "skill_id": "hr-onboarding",
    })
    session_id = create_resp.json()["session_id"]

    resp = client_with_policies.get(f"/v1/sessions/{session_id}")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "active"
    assert data["skill_id"] == "hr-onboarding"


def test_send_message(client_with_policies):
    """POST /v1/sessions/{id}/message forwards to L1."""
    create_resp = client_with_policies.post("/v1/sessions", json={
        "user_id": "user-003",
        "skill_id": "hr-onboarding",
    })
    session_id = create_resp.json()["session_id"]

    resp = client_with_policies.post(
        f"/v1/sessions/{session_id}/message",
        json={"content": "新员工张三入职"},
    )
    assert resp.status_code == 200
    data = resp.json()
    assert len(data["chunks"]) > 0
    assert data["chunks"][0]["chunk_type"] == "text_delta"


def test_terminate_session(client_with_policies):
    """DELETE /v1/sessions/{id} terminates and cleans up."""
    create_resp = client_with_policies.post("/v1/sessions", json={
        "user_id": "user-004",
        "skill_id": "hr-onboarding",
    })
    session_id = create_resp.json()["session_id"]

    resp = client_with_policies.delete(f"/v1/sessions/{session_id}")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "terminated"

    # Verify session is terminated
    get_resp = client_with_policies.get(f"/v1/sessions/{session_id}")
    assert get_resp.json()["status"] == "terminated"
