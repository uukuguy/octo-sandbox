"""Tests for SessionManager."""

from claude_code_runtime.session import Session, SessionManager, SessionState


def test_create_session():
    mgr = SessionManager()
    s = mgr.create(user_id="u1", user_role="dev", org_unit="eng")
    assert s.session_id.startswith("crt-")
    assert s.user_id == "u1"
    assert s.state == SessionState.ACTIVE
    assert mgr.count == 1


def test_get_session():
    mgr = SessionManager()
    s = mgr.create(user_id="u1")
    assert mgr.get(s.session_id) is s
    assert mgr.get("nonexistent") is None


def test_pause_resume():
    mgr = SessionManager()
    s = mgr.create(user_id="u1")
    assert mgr.pause(s.session_id) is True
    assert s.state == SessionState.PAUSED
    assert mgr.pause(s.session_id) is False  # already paused

    assert mgr.resume(s.session_id) is True
    assert s.state == SessionState.ACTIVE
    assert mgr.resume(s.session_id) is False  # already active


def test_terminate():
    mgr = SessionManager()
    s = mgr.create(user_id="u1")
    sid = s.session_id
    terminated = mgr.terminate(sid)
    assert terminated is not None
    assert terminated.state == SessionState.TERMINATED
    assert mgr.get(sid) is None
    assert mgr.count == 0


def test_session_serialization():
    s = Session(
        session_id="crt-abc",
        user_id="u1",
        user_role="dev",
        skills=[{"skill_id": "s1", "name": "Test"}],
    )
    data = s.to_dict()
    assert data["session_id"] == "crt-abc"
    assert data["user_id"] == "u1"
    assert len(data["skills"]) == 1

    restored = Session.from_dict(data)
    assert restored.session_id == "crt-abc"
    assert restored.user_id == "u1"
    assert restored.state == SessionState.ACTIVE


def test_restore_session():
    mgr = SessionManager()
    data = {
        "session_id": "crt-old",
        "user_id": "u1",
        "state": "paused",
        "skills": [],
        "mcp_servers": [],
        "telemetry_events": [],
        "context": {},
    }
    s = mgr.restore(data)
    assert s.session_id == "crt-old"
    assert s.state == SessionState.ACTIVE  # restored sessions become active
    assert mgr.get("crt-old") is s
