"""Tests for EventInterceptor — extracts events from L1 response chunks."""

from __future__ import annotations

from eaasp_l4_orchestration.event_interceptor import EventInterceptor


def test_extract_tool_call_start():
    interceptor = EventInterceptor()
    chunk = {
        "chunk_type": "tool_call_start",
        "tool_name": "scada_read",
        "arguments": {"id": "T-001"},
    }
    event = interceptor.extract_from_chunk("s1", chunk, runtime_id="grid-runtime")
    assert event is not None
    assert event.event_type == "PRE_TOOL_USE"
    assert event.payload["tool_name"] == "scada_read"
    assert event.payload["arguments"] == {"id": "T-001"}
    assert "interceptor:grid-runtime" in event.metadata.source


def test_extract_tool_result_success():
    interceptor = EventInterceptor()
    chunk = {
        "chunk_type": "tool_result",
        "tool_name": "scada_read",
        "content": '{"temp": 85}',
        "is_error": False,
    }
    event = interceptor.extract_from_chunk("s1", chunk)
    assert event is not None
    assert event.event_type == "POST_TOOL_USE"
    assert event.payload["tool_name"] == "scada_read"
    assert not event.payload["is_error"]


def test_extract_tool_result_failure():
    interceptor = EventInterceptor()
    chunk = {
        "chunk_type": "tool_result",
        "tool_name": "scada_read",
        "content": "error",
        "is_error": True,
    }
    event = interceptor.extract_from_chunk("s1", chunk)
    assert event is not None
    assert event.event_type == "POST_TOOL_USE_FAILURE"
    assert event.payload["is_error"]


def test_extract_done():
    interceptor = EventInterceptor()
    chunk = {
        "chunk_type": "done",
        "content": "",
        "response_text": "Calibration complete.",
    }
    event = interceptor.extract_from_chunk("s1", chunk)
    assert event is not None
    assert event.event_type == "STOP"
    assert event.payload["reason"] == "complete"
    assert event.payload["response_text"] == "Calibration complete."


def test_extract_text_delta_returns_none():
    interceptor = EventInterceptor()
    chunk = {"chunk_type": "text_delta", "content": "hello"}
    event = interceptor.extract_from_chunk("s1", chunk)
    assert event is None


def test_extract_unknown_chunk_type_returns_none():
    interceptor = EventInterceptor()
    chunk = {"chunk_type": "thinking", "content": "hmm"}
    event = interceptor.extract_from_chunk("s1", chunk)
    assert event is None


def test_create_session_start():
    interceptor = EventInterceptor()
    event = interceptor.create_session_start("s1", "grid-runtime")
    assert event.event_type == "SESSION_START"
    assert event.session_id == "s1"
    assert event.payload["runtime_id"] == "grid-runtime"
    assert event.metadata.source == "interceptor:grid-runtime"


def test_create_session_end():
    interceptor = EventInterceptor()
    event = interceptor.create_session_end("s1")
    assert event.event_type == "POST_SESSION_END"
    assert event.session_id == "s1"
    assert event.metadata.source == "interceptor:orchestrator"
