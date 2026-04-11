"""Tests for SessionManager (EAASP v2: 5-block SessionPayload)."""

from hermes_runtime.session import SessionManager


def test_session_lifecycle():
    mgr = SessionManager()
    assert mgr.count == 0

    s = mgr.create(
        user_id="u1",
        policy_context={"org_unit": "eng", "hooks": []},
        user_preferences={"user_id": "u1", "language": "en", "prefs": {}, "timezone": "UTC"},
    )
    assert s.session_id.startswith("hermes-")
    assert mgr.count == 1

    assert mgr.get(s.session_id) is s
    assert mgr.get("nonexistent") is None

    # P1 populated, P2 absent by default
    assert s.policy_context is not None
    assert s.event_context is None
    assert s.user_preferences["user_id"] == "u1"


def test_session_pause_resume():
    mgr = SessionManager()
    s = mgr.create(user_id="u1")
    assert not s.paused
    assert mgr.pause(s.session_id)
    assert s.paused
    assert mgr.resume(s.session_id)
    assert not s.paused


def test_session_terminate():
    mgr = SessionManager()
    s = mgr.create(user_id="u1")
    sid = s.session_id
    terminated = mgr.terminate(sid)
    assert terminated is s
    assert mgr.count == 0
    assert mgr.get(sid) is None
