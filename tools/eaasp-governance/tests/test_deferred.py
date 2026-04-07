"""Tests for BH-D3/D5/D10 deferred items — 13 tests.

D3: Audit event persistence (4 tests)
D5: Intent resolver enhancement (4 tests)
D10: Policy version history + rollback (5 tests)
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from eaasp_governance.api.intent_gateway import IntentResolver, IntentRule, init_resolver
from eaasp_governance.main import create_app

EXAMPLES_DIR = Path(__file__).resolve().parents[3] / "sdk" / "examples" / "hr-onboarding" / "policies"
CONFIG_DIR = Path(__file__).resolve().parents[1] / "config"


@pytest.fixture
def client():
    app = create_app(
        l2_url="http://mock-l2:8081",
        runtimes_config=str(CONFIG_DIR / "runtimes.yaml"),
    )
    return TestClient(app)


def _deploy_policies(client):
    """Deploy both example policies."""
    for f in ["enterprise.yaml", "bu_hr.yaml"]:
        resp = client.put(
            "/v1/policies/deploy",
            content=(EXAMPLES_DIR / f).read_text(),
            headers={"Content-Type": "application/yaml"},
        )
        assert resp.status_code == 200


# ══════════════════════════════════════════════════════════════
# D3: Audit Event Persistence (4 tests)
# ══════════════════════════════════════════════════════════════

def test_d3_audit_events_ingested(client):
    """Audit events (hook_fired, hook_deny) are stored in telemetry."""
    client.post("/v1/telemetry", json={
        "session_id": "audit-sess-001",
        "events": [
            {"event_type": "tool_call", "payload": {"tool": "file_write"}},
            {"event_type": "hook_fired", "payload": {"rule": "pii-guard", "action": "deny"}},
            {"event_type": "hook_deny", "payload": {"rule": "pii-guard"}},
            {"event_type": "tool_call", "payload": {"tool": "file_read"}},
        ],
    })
    resp = client.get("/v1/telemetry/sessions/audit-sess-001/audit")
    assert resp.status_code == 200
    data = resp.json()
    assert data["total_audit"] == 2  # hook_fired + hook_deny
    assert all(e["event_type"] in ("hook_fired", "hook_deny") for e in data["audit_events"])


def test_d3_audit_filter_by_payload_flag(client):
    """Events with payload.audit=true are included in audit query."""
    client.post("/v1/telemetry", json={
        "session_id": "audit-sess-002",
        "events": [
            {"event_type": "tool_call", "payload": {"tool": "file_write", "audit": True}},
            {"event_type": "tool_call", "payload": {"tool": "file_read"}},
        ],
    })
    resp = client.get("/v1/telemetry/sessions/audit-sess-002/audit")
    data = resp.json()
    assert data["total_audit"] == 1


def test_d3_audit_empty_session(client):
    """Audit query on non-existent session returns empty list."""
    resp = client.get("/v1/telemetry/sessions/nonexistent/audit")
    assert resp.status_code == 200
    assert resp.json()["total_audit"] == 0


def test_d3_audit_event_structure(client):
    """Audit events preserve full payload structure."""
    client.post("/v1/telemetry", json={
        "session_id": "audit-sess-003",
        "events": [
            {"event_type": "hook_fired", "timestamp": "2026-04-07T12:00:00Z",
             "payload": {"rule_id": "pii-guard", "action": "deny", "tool": "file_write"},
             "resource_usage": {"tokens": 50}},
        ],
    })
    resp = client.get("/v1/telemetry/sessions/audit-sess-003/audit")
    event = resp.json()["audit_events"][0]
    assert event["payload"]["rule_id"] == "pii-guard"
    assert event["payload"]["action"] == "deny"


# ══════════════════════════════════════════════════════════════
# D5: Intent Resolver Enhancement (4 tests)
# ══════════════════════════════════════════════════════════════

def test_d5_resolver_multi_keyword_scoring():
    """IntentResolver scores higher for more keyword matches."""
    resolver = IntentResolver()
    resolver.add_rule(IntentRule(
        keywords=["入职", "新员工", "onboarding"],
        skill_id="hr-onboarding",
        confidence=0.9,
    ))
    # 1 keyword match
    skill, conf1 = resolver.resolve("入职")
    assert skill == "hr-onboarding"
    # 2 keyword matches → higher confidence
    skill, conf2 = resolver.resolve("新员工入职")
    assert conf2 > conf1


def test_d5_resolver_best_match_wins():
    """When multiple skills match, highest score wins."""
    resolver = IntentResolver()
    resolver.add_rule(IntentRule(keywords=["入职", "新员工"], skill_id="hr-onboarding", confidence=0.9))
    resolver.add_rule(IntentRule(keywords=["请假", "休假"], skill_id="hr-leave", confidence=0.85))

    skill, _ = resolver.resolve("新员工入职手续")
    assert skill == "hr-onboarding"

    skill, _ = resolver.resolve("请假申请")
    assert skill == "hr-leave"


def test_d5_resolver_yaml_config():
    """IntentResolver loads from YAML config file."""
    config_path = CONFIG_DIR / "intents.yaml"
    if not config_path.exists():
        pytest.skip("intents.yaml not found")

    resolver = init_resolver(config_path=str(config_path))
    skill, conf = resolver.resolve("新员工入职")
    assert skill == "hr-onboarding"
    assert conf > 0


def test_d5_api_uses_resolver(client):
    """POST /v1/intents uses the enhanced resolver."""
    resp = client.post("/v1/intents", json={
        "text": "新员工入职流程",
        "user_id": "test",
    })
    assert resp.status_code == 200
    data = resp.json()
    assert data["skill_id"] == "hr-onboarding"
    assert data["confidence"] > 0


# ══════════════════════════════════════════════════════════════
# D10: Policy Version History + Rollback (5 tests)
# ══════════════════════════════════════════════════════════════

def test_d10_deploy_creates_version(client):
    """Each deploy creates a new version entry."""
    yaml_content = (EXAMPLES_DIR / "enterprise.yaml").read_text()
    resp1 = client.put("/v1/policies/deploy", content=yaml_content,
                       headers={"Content-Type": "application/yaml"})
    assert resp1.json()["version_num"] == 1

    resp2 = client.put("/v1/policies/deploy", content=yaml_content,
                       headers={"Content-Type": "application/yaml"})
    assert resp2.json()["version_num"] == 2


def test_d10_list_versions(client):
    """GET /v1/policies/{id}/versions returns all versions."""
    yaml_content = (EXAMPLES_DIR / "enterprise.yaml").read_text()
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})

    resp = client.get("/v1/policies/enterprise-security-baseline/versions")
    assert resp.status_code == 200
    data = resp.json()
    assert data["current_version"] == 2
    assert len(data["versions"]) == 2
    assert data["versions"][0]["version_num"] == 1
    assert data["versions"][1]["version_num"] == 2


def test_d10_rollback(client):
    """POST /v1/policies/{id}/rollback creates a new version from target."""
    yaml_content = (EXAMPLES_DIR / "enterprise.yaml").read_text()
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})
    # Deploy a different policy (bu_hr) under same id is not possible,
    # so just deploy enterprise twice to get version 2
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})

    # Rollback to version 1
    resp = client.post("/v1/policies/enterprise-security-baseline/rollback?version=1")
    assert resp.status_code == 200
    data = resp.json()
    assert data["rolled_back_to"] == 1
    assert data["new_version_num"] == 3


def test_d10_rollback_invalid_version(client):
    """Rollback to non-existent version returns 400."""
    yaml_content = (EXAMPLES_DIR / "enterprise.yaml").read_text()
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})

    resp = client.post("/v1/policies/enterprise-security-baseline/rollback?version=99")
    assert resp.status_code == 400
    assert "not found" in resp.json()["detail"].lower()


def test_d10_current_after_rollback(client):
    """After rollback, GET /v1/policies/{id} returns the rolled-back version."""
    yaml_content = (EXAMPLES_DIR / "enterprise.yaml").read_text()
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})
    client.put("/v1/policies/deploy", content=yaml_content,
               headers={"Content-Type": "application/yaml"})
    client.post("/v1/policies/enterprise-security-baseline/rollback?version=1")

    resp = client.get("/v1/policies/enterprise-security-baseline")
    assert resp.status_code == 200
    # Current is now version 3 (rollback to 1)
    assert resp.json()["version_num"] == 3
