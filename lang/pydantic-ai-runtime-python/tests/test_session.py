"""Session tests — multi-turn agent loop with event emission.

Mirrors lang/nanobot-runtime-python/tests/test_session.py but drives
PydanticAiProvider (MagicMock spec) instead of OpenAICompatProvider.
The AgentSession contract is identical across both runtimes.
"""
from __future__ import annotations

import json
import stat
from unittest.mock import AsyncMock, MagicMock

import pytest

from pydantic_ai_runtime.provider import PydanticAiProvider
from pydantic_ai_runtime.session import AgentSession, EventType


# ---------------------------------------------------------------------------
# Helpers (copied from nanobot tests — OAI response shape is identical)
# ---------------------------------------------------------------------------

def _make_text_response(content: str) -> dict:
    return {"choices": [{"message": {"role": "assistant", "content": content}}]}


def _make_tc(id: str, name: str, args: dict) -> dict:
    return {
        "id": id,
        "type": "function",
        "function": {"name": name, "arguments": json.dumps(args)},
    }


def _make_tool_call_response(tool_calls: list[dict]) -> dict:
    return {
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": None,
                    "tool_calls": tool_calls,
                }
            }
        ]
    }


@pytest.fixture
def mock_provider():
    p = MagicMock(spec=PydanticAiProvider)
    p.chat = AsyncMock()
    return p


# ---------------------------------------------------------------------------
# Core agent-loop tests
# ---------------------------------------------------------------------------

async def test_pure_text_response_emits_chunk_and_stop(mock_provider):
    mock_provider.chat.return_value = _make_text_response("Hello, world!")

    session = AgentSession(provider=mock_provider)
    events = [ev async for ev in session.run("hi")]

    types = [e.event_type for e in events]
    assert types == [EventType.CHUNK, EventType.STOP]
    assert events[0].content == "Hello, world!"
    assert events[1].content == "Hello, world!"


async def test_single_tool_call_emits_expected_sequence(mock_provider):
    tc = _make_tc("tc-1", "get_weather", {"city": "Tokyo"})
    mock_provider.chat.side_effect = [
        _make_tool_call_response([tc]),
        _make_text_response("It's sunny in Tokyo."),
    ]

    session = AgentSession(provider=mock_provider)
    events = [ev async for ev in session.run("What's the weather?")]

    types = [e.event_type for e in events]
    assert types == [EventType.TOOL_CALL, EventType.TOOL_RESULT, EventType.CHUNK, EventType.STOP]

    tool_call_ev = events[0]
    assert tool_call_ev.tool_name == "get_weather"
    assert tool_call_ev.tool_call_id == "tc-1"
    assert tool_call_ev.tool_input == {"city": "Tokyo"}

    tool_result_ev = events[1]
    assert tool_result_ev.tool_call_id == "tc-1"
    assert tool_result_ev.tool_name == "get_weather"

    # Second chat() call must include the tool result message
    second_call_messages = mock_provider.chat.call_args_list[1][1]["messages"]
    roles = [m["role"] for m in second_call_messages]
    assert "tool" in roles


async def test_multi_turn_two_tool_call_rounds(mock_provider):
    tc1 = _make_tc("tc-a", "search", {"q": "rust"})
    tc2 = _make_tc("tc-b", "summarize", {"text": "..."})
    mock_provider.chat.side_effect = [
        _make_tool_call_response([tc1]),
        _make_tool_call_response([tc2]),
        _make_text_response("Done."),
    ]

    session = AgentSession(provider=mock_provider)
    events = [ev async for ev in session.run("Do research")]

    types = [e.event_type for e in events]
    assert types == [
        EventType.TOOL_CALL, EventType.TOOL_RESULT,
        EventType.TOOL_CALL, EventType.TOOL_RESULT,
        EventType.CHUNK, EventType.STOP,
    ]
    assert mock_provider.chat.call_count == 3


async def test_max_turns_exceeded_emits_error(mock_provider):
    """Provider always returns tool-call response → session exhausts max_turns → ERROR."""
    # Unlimited tool-call responses (never terminates on its own)
    tc = _make_tc("tc-loop", "infinite_tool", {})
    mock_provider.chat.return_value = _make_tool_call_response([tc])

    session = AgentSession(provider=mock_provider, max_turns=3)
    events = [ev async for ev in session.run("loop forever")]

    # Expect 3 rounds of TOOL_CALL+TOOL_RESULT, then ERROR
    types = [e.event_type for e in events]
    assert types.count(EventType.TOOL_CALL) == 3
    assert types.count(EventType.TOOL_RESULT) == 3
    assert types[-1] == EventType.ERROR
    assert events[-1].is_error is True
    assert "max_turns=3" in events[-1].content


async def test_provider_exception_emits_error_event(mock_provider):
    """Exception from provider.chat → single ERROR event, then session stops."""
    mock_provider.chat.side_effect = Exception("provider timeout")

    session = AgentSession(provider=mock_provider)
    events = [ev async for ev in session.run("this will fail")]

    assert len(events) == 1
    assert events[0].event_type == EventType.ERROR
    assert events[0].is_error is True
    assert "provider timeout" in events[0].content


# ---------------------------------------------------------------------------
# Stop-hook tests (parity with nanobot)
# ---------------------------------------------------------------------------

async def test_stop_hook_allow_fires_and_emits_stop(mock_provider, tmp_path):
    """Stop hook exit-0 → allow: HOOK_FIRED(allow) + STOP emitted."""
    hook_script = tmp_path / "stop_hook.sh"
    hook_script.write_text("#!/bin/sh\nexit 0\n")
    hook_script.chmod(hook_script.stat().st_mode | stat.S_IEXEC)

    mock_provider.chat.return_value = _make_text_response("All done.")

    session = AgentSession(
        provider=mock_provider,
        stop_hooks=[str(hook_script)],
    )
    events = [ev async for ev in session.run("finish")]

    types = [e.event_type for e in events]
    assert EventType.HOOK_FIRED in types
    assert EventType.STOP in types
    hook_ev = next(e for e in events if e.event_type == EventType.HOOK_FIRED)
    assert hook_ev.hook_event == "Stop"
    assert hook_ev.hook_decision == "allow"


async def test_stop_hook_deny_loops_back(mock_provider, tmp_path):
    """Stop hook exit-2 → deny: loop re-enters (provider.chat called > 1)."""
    deny_hook = tmp_path / "deny.sh"
    deny_hook.write_text("#!/bin/sh\nexit 2\n")
    deny_hook.chmod(deny_hook.stat().st_mode | stat.S_IEXEC)

    mock_provider.chat.side_effect = [
        _make_text_response("draft output"),
        _make_text_response("revised output"),
    ]

    session = AgentSession(
        provider=mock_provider,
        stop_hooks=[str(deny_hook)],
        max_turns=5,
    )
    events = [ev async for ev in session.run("finish")]

    hook_events = [e for e in events if e.event_type == EventType.HOOK_FIRED]
    assert any(e.hook_decision == "deny" for e in hook_events)
    # Provider called more than once due to re-entry after deny
    assert mock_provider.chat.call_count >= 2


# ---------------------------------------------------------------------------
# Event-type contract lock (chunk_type parallel — string stability)
# ---------------------------------------------------------------------------

def test_event_type_string_values_are_stable():
    """EventType strings are consumed downstream verbatim — lock them.

    Parallel to nanobot/grid chunk_type contract (ADR-V2-021): these strings
    cross the runtime boundary as event_type on AgentEvent and any refactor
    that changes them silently breaks downstream consumers.
    """
    assert EventType.CHUNK == "CHUNK"
    assert EventType.TOOL_CALL == "TOOL_CALL"
    assert EventType.TOOL_RESULT == "TOOL_RESULT"
    assert EventType.STOP == "STOP"
    assert EventType.ERROR == "ERROR"
    assert EventType.HOOK_FIRED == "HOOK_FIRED"
