"""Tests for TelemetryCollector."""

from claude_code_runtime.telemetry import TelemetryCollector


def test_record_and_flush():
    tc = TelemetryCollector(
        session_id="s-1", runtime_id="crt", user_id="u1"
    )
    tc.record("session_start")
    tc.record("send", payload={"content": "hello"})
    assert tc.count == 2

    events = tc.flush()
    assert len(events) == 2
    assert events[0].event_type == "session_start"
    assert events[1].payload == {"content": "hello"}
    assert tc.count == 0  # flushed


def test_peek_does_not_clear():
    tc = TelemetryCollector(session_id="s-1", runtime_id="crt")
    tc.record("test")
    events = tc.peek()
    assert len(events) == 1
    assert tc.count == 1  # not cleared


def test_resource_usage():
    tc = TelemetryCollector(session_id="s-1", runtime_id="crt")
    tc.record(
        "send",
        input_tokens=100,
        output_tokens=200,
        compute_ms=500,
    )
    events = tc.flush()
    assert events[0].input_tokens == 100
    assert events[0].output_tokens == 200
    assert events[0].compute_ms == 500


def test_session_and_runtime_ids():
    tc = TelemetryCollector(
        session_id="s-42", runtime_id="crt-test", user_id="alice"
    )
    tc.record("tool_call")
    event = tc.peek()[0]
    assert event.session_id == "s-42"
    assert event.runtime_id == "crt-test"
    assert event.user_id == "alice"
