"""Tests for SessionManager."""

from hermes_runtime.session import SessionManager


def test_session_lifecycle():
    mgr = SessionManager()
    assert mgr.count == 0

    s = mgr.create(user_id="u1", user_role="developer", org_unit="eng")
    assert s.session_id.startswith("hermes-")
    assert mgr.count == 1

    assert mgr.get(s.session_id) is s
    assert mgr.get("nonexistent") is None


def test_session_pause_resume():
    mgr = SessionManager()
    s = mgr.create(user_id="u1", user_role="dev", org_unit="eng")
    assert not s.paused
    assert mgr.pause(s.session_id)
    assert s.paused
    assert mgr.resume(s.session_id)
    assert not s.paused


def test_session_terminate():
    mgr = SessionManager()
    s = mgr.create(user_id="u1", user_role="dev", org_unit="eng")
    sid = s.session_id
    terminated = mgr.terminate(sid)
    assert terminated is s
    assert mgr.count == 0
    assert mgr.get(sid) is None
