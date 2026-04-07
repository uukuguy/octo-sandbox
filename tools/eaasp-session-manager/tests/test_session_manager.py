"""Tests for L4 Session Manager — 10 tests.

Uses mock L3 client to test four-plane architecture in isolation.
"""

from __future__ import annotations

import uuid
from unittest.mock import AsyncMock

import pytest
from fastapi.testclient import TestClient

from eaasp_session.main import create_app
from eaasp_session.planes.persistence import PersistencePlane

# ── Mock L3 Client ──────────────────────────────────────────


class MockL3Client:
    """Mock L3 Governance client for testing."""

    def __init__(self):
        self._sessions: dict[str, dict] = {}

    async def create_session(self, user_id, user_role, org_unit, skill_id, runtime_preference=None):
        session_id = f"sess-{uuid.uuid4().hex[:8]}"
        self._sessions[session_id] = {
            "session_id": session_id,
            "runtime_id": "grid",
            "runtime_endpoint": "localhost:50051",
            "governance_summary": {
                "hooks_count": 3,
                "scope_chain": ["managed", "skill"],
                "managed_hooks_digest": "abc123",
            },
        }
        return self._sessions[session_id]

    async def send_message(self, session_id, content):
        return {
            "chunks": [
                {"chunk_type": "text_delta", "content": f"Mock response for: {content[:30]}"},
                {"chunk_type": "done", "content": ""},
            ],
        }

    async def get_session(self, session_id):
        return self._sessions.get(session_id, {"status": "not_found"})

    async def terminate_session(self, session_id):
        return {"session_id": session_id, "status": "terminated"}

    async def resolve_intent(self, text, user_id, org_unit):
        if "入职" in text or "onboarding" in text.lower():
            return {"skill_id": "hr-onboarding", "confidence": 0.9}
        return {"skill_id": None, "confidence": 0.0}


# ── Fixtures ────────────────────────────────────────────────

@pytest.fixture
def client():
    """Create a test client with mock L3."""
    app = create_app(l3_url="http://mock:8083", db_path=":memory:")
    app.state.l3_client = MockL3Client()
    return TestClient(app)


def _create_conversation(client, user_id="user-001", skill_id="hr-onboarding"):
    """Helper: create a conversation and return the response."""
    resp = client.post("/v1/conversations", json={
        "user_id": user_id,
        "org_unit": "hr-dept",
        "skill_id": skill_id,
    })
    assert resp.status_code == 200
    return resp.json()


# ── Test 1: Create conversation ─────────────────────────────

def test_create_conversation(client):
    """POST /v1/conversations creates session via L3."""
    data = _create_conversation(client)
    assert "conversation_id" in data
    assert "session_id" in data
    assert data["runtime"] == "grid"
    assert data["skill_name"] == "hr-onboarding"


# ── Test 2: Create with intent resolution ───────────────────

def test_create_with_intent(client):
    """POST /v1/conversations with input resolves intent to skill."""
    resp = client.post("/v1/conversations", json={
        "user_id": "user-002",
        "org_unit": "hr-dept",
        "input": "新员工张三入职",
    })
    assert resp.status_code == 200
    data = resp.json()
    assert data["skill_name"] == "hr-onboarding"


# ── Test 3: Send message ────────────────────────────────────

def test_send_message(client):
    """POST /v1/conversations/{id}/message forwards to L3."""
    conv = _create_conversation(client)
    resp = client.post(
        f"/v1/conversations/{conv['conversation_id']}/message",
        json={"content": "开始入职流程"},
    )
    assert resp.status_code == 200
    data = resp.json()
    assert len(data["chunks"]) > 0


# ── Test 4: Get conversation ────────────────────────────────

def test_get_conversation(client):
    """GET /v1/conversations/{id} returns status."""
    conv = _create_conversation(client)
    resp = client.get(f"/v1/conversations/{conv['conversation_id']}")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "active"
    assert data["skill"] == "hr-onboarding"


# ── Test 5: Delete conversation ─────────────────────────────

def test_delete_conversation(client):
    """DELETE /v1/conversations/{id} terminates session."""
    conv = _create_conversation(client)
    resp = client.delete(f"/v1/conversations/{conv['conversation_id']}")
    assert resp.status_code == 200
    assert resp.json()["status"] == "terminated"

    # Verify terminated
    get_resp = client.get(f"/v1/conversations/{conv['conversation_id']}")
    assert get_resp.json()["status"] == "terminated"


# ── Test 6: Admin list sessions ─────────────────────────────

def test_admin_list_sessions(client):
    """GET /v1/sessions lists all sessions (admin view)."""
    _create_conversation(client, user_id="user-a")
    _create_conversation(client, user_id="user-b")

    resp = client.get("/v1/sessions")
    assert resp.status_code == 200
    sessions = resp.json()
    assert len(sessions) == 2


# ── Test 7: Admin session telemetry ─────────────────────────

def test_admin_session_telemetry(client):
    """GET /v1/sessions/{id}/telemetry returns summary."""
    conv = _create_conversation(client)
    session_id = conv["session_id"]

    resp = client.get(f"/v1/sessions/{session_id}/telemetry")
    assert resp.status_code == 200
    data = resp.json()
    assert "tools_called" in data
    assert "hooks_fired" in data


# ── Test 8: Persistence — SQLite roundtrip ──────────────────

def test_persistence_roundtrip():
    """PersistencePlane stores and retrieves sessions correctly."""
    db = PersistencePlane(db_path=":memory:")
    db.create_session(
        session_id="sess-001",
        conversation_id="conv-001",
        user_id="user-test",
        org_unit="hr",
        skill_id="hr-onboarding",
        runtime_id="grid",
    )

    session = db.get_session("sess-001")
    assert session is not None
    assert session["user_id"] == "user-test"
    assert session["status"] == "active"

    db.update_status("sess-001", "terminated")
    session = db.get_session("sess-001")
    assert session["status"] == "terminated"
    assert session["terminated_at"] is not None
    db.close()


# ── Test 9: Persistence — execution log ─────────────────────

def test_persistence_execution_log():
    """PersistencePlane logs and retrieves execution events."""
    db = PersistencePlane(db_path=":memory:")
    db.create_session("sess-002", "conv-002", "user-x", "hr", "hr-onboarding")
    db.log_event("sess-002", "intent_dispatch", {"skill": "hr-onboarding"})
    db.log_event("sess-002", "message_sent", {"content": "hello"})

    sessions = db.list_sessions()
    assert len(sessions) == 1
    db.close()


# ── Test 10: Health endpoint ────────────────────────────────

def test_health(client):
    """GET /health returns service status."""
    resp = client.get("/health")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "ok"
    assert data["service"] == "eaasp-session-manager"
