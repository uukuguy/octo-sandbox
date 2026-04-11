"""Tests for SessionEventStream append + list_events + FK enforcement."""

from __future__ import annotations

import aiosqlite
import pytest

from eaasp_l4_orchestration.event_stream import SessionEventStream


async def test_append_returns_monotonic_seq_across_sessions(
    tmp_db_path: str, seed_session
) -> None:
    """seq is a global autoincrement — events in different sessions still
    produce strictly increasing seq values."""
    a = await seed_session("sess_aaa")
    b = await seed_session("sess_bbb")
    stream = SessionEventStream(tmp_db_path)

    s1 = await stream.append(a, "SESSION_CREATED", {"payload": 1})
    s2 = await stream.append(b, "SESSION_CREATED", {"payload": 2})
    s3 = await stream.append(a, "USER_MESSAGE", {"content": "hi"})

    assert s1 < s2 < s3


async def test_list_events_returns_ascending_seq(
    tmp_db_path: str, seed_session
) -> None:
    sid = await seed_session("sess_list")
    stream = SessionEventStream(tmp_db_path)
    s1 = await stream.append(sid, "A", {})
    s2 = await stream.append(sid, "B", {})
    s3 = await stream.append(sid, "C", {})

    events = await stream.list_events(sid)
    seqs = [e["seq"] for e in events]
    assert seqs == sorted(seqs)
    assert [e["event_type"] for e in events] == ["A", "B", "C"]
    assert [e["seq"] for e in events] == [s1, s2, s3]


async def test_list_events_range_filter(tmp_db_path: str, seed_session) -> None:
    sid = await seed_session("sess_range")
    stream = SessionEventStream(tmp_db_path)
    s1 = await stream.append(sid, "E1", {})
    s2 = await stream.append(sid, "E2", {})
    s3 = await stream.append(sid, "E3", {})
    s4 = await stream.append(sid, "E4", {})

    events = await stream.list_events(sid, from_seq=s2, to_seq=s3)
    assert [e["event_type"] for e in events] == ["E2", "E3"]
    # Sanity — bounds are inclusive and ignore s1/s4.
    assert all(s2 <= e["seq"] <= s3 for e in events)
    _ = (s1, s4)  # silence unused warnings


async def test_list_events_limit(tmp_db_path: str, seed_session) -> None:
    sid = await seed_session("sess_limit")
    stream = SessionEventStream(tmp_db_path)
    await stream.append(sid, "X", {})
    await stream.append(sid, "Y", {})
    await stream.append(sid, "Z", {})

    events = await stream.list_events(sid, limit=1)
    assert len(events) == 1
    assert events[0]["event_type"] == "X"


async def test_append_unknown_session_raises_fk(tmp_db_path: str) -> None:
    stream = SessionEventStream(tmp_db_path)
    with pytest.raises(aiosqlite.IntegrityError):
        await stream.append("sess_does_not_exist", "BOOM", {})
