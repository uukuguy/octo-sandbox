"""Unit tests for SdkWrapper — focus on ChunkEvent emission (D86).

D86: Anthropic SDK emits `ToolResultBlock` inside `UserMessage.content`
(message_parser.py lines 74-81) and occasionally inside
`AssistantMessage.content`. The pre-fix wrapper only iterated AssistantMessage
blocks but had no UserMessage branch at all, so tool-result blocks were
silently dropped and the downstream EAASP POST_TOOL_USE hook never fired.
"""

from __future__ import annotations

import pytest
from claude_agent_sdk import (
    AssistantMessage,
    ResultMessage,
    TextBlock,
    ToolResultBlock,
    ToolUseBlock,
    UserMessage,
)

from claude_code_runtime.config import RuntimeConfig
from claude_code_runtime.sdk_wrapper import ChunkEvent, SdkWrapper


@pytest.fixture
def config() -> RuntimeConfig:
    return RuntimeConfig(
        grpc_port=50099,
        runtime_id="test-runtime",
        runtime_name="Test Runtime",
        anthropic_model_name="test-model",
    )


@pytest.fixture
def wrapper(config: RuntimeConfig) -> SdkWrapper:
    return SdkWrapper(config)


def _fake_query_factory(messages: list):
    """Build a replacement for `claude_agent_sdk.query` that yields the
    provided messages verbatim. Matches the real async-generator shape of
    `query(prompt=..., options=...)`.
    """

    async def _fake_query(prompt: str, options):  # noqa: ARG001
        for msg in messages:
            yield msg

    return _fake_query


async def _collect(aiter) -> list[ChunkEvent]:
    events: list[ChunkEvent] = []
    async for evt in aiter:
        events.append(evt)
    return events


# ── D86: UserMessage ToolResultBlock forwarding ───────────────────


@pytest.mark.asyncio
async def test_sdk_wrapper_emits_tool_result_chunk(
    wrapper: SdkWrapper, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A ToolResultBlock arriving inside a UserMessage must be surfaced as a
    chunk_type='tool_result' ChunkEvent carrying the original tool_use_id."""
    user_msg = UserMessage(
        content=[
            ToolResultBlock(
                tool_use_id="tool-use-42",
                content="42 files indexed",
                is_error=False,
            )
        ]
    )
    result_msg = ResultMessage(
        subtype="success",
        duration_ms=1,
        duration_api_ms=1,
        is_error=False,
        num_turns=1,
        session_id="sess-1",
        total_cost_usd=0.0,
    )

    monkeypatch.setattr(
        "claude_code_runtime.sdk_wrapper.query",
        _fake_query_factory([user_msg, result_msg]),
    )

    events = await _collect(wrapper.send_message(prompt="hi"))

    tool_result_events = [e for e in events if e.chunk_type == "tool_result"]
    assert len(tool_result_events) == 1, (
        f"expected exactly one tool_result chunk, got {[e.chunk_type for e in events]}"
    )
    tr = tool_result_events[0]
    assert tr.tool_id == "tool-use-42"
    assert tr.content == "42 files indexed"
    assert tr.is_error is False


@pytest.mark.asyncio
async def test_sdk_wrapper_tool_result_sequence_after_tool_use(
    wrapper: SdkWrapper, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The classic agentic loop: assistant emits ToolUseBlock(id='t1'), then
    the SDK yields a UserMessage carrying ToolResultBlock(tool_use_id='t1').
    Both must flow through as chunks in order with matching tool_id, so the
    gRPC consumer can correlate POST_TOOL_USE to its PRE_TOOL_USE partner."""
    assistant_msg = AssistantMessage(
        content=[
            TextBlock(text="Let me check the index first."),
            ToolUseBlock(id="t1", name="search_memory", input={"query": "logs"}),
        ],
        model="claude-sonnet-test",
    )
    user_msg = UserMessage(
        content=[
            ToolResultBlock(
                tool_use_id="t1",
                content="found 3 matches",
                is_error=False,
            )
        ]
    )
    result_msg = ResultMessage(
        subtype="success",
        duration_ms=5,
        duration_api_ms=4,
        is_error=False,
        num_turns=1,
        session_id="sess-2",
        total_cost_usd=0.0,
    )

    monkeypatch.setattr(
        "claude_code_runtime.sdk_wrapper.query",
        _fake_query_factory([assistant_msg, user_msg, result_msg]),
    )

    events = await _collect(wrapper.send_message(prompt="hi"))
    chunk_types = [e.chunk_type for e in events]

    # Order contract: text_delta → tool_start → tool_result → done
    assert chunk_types == ["text_delta", "tool_start", "tool_result", "done"], (
        f"unexpected chunk order: {chunk_types}"
    )

    tool_start = events[1]
    tool_result = events[2]
    assert tool_start.tool_id == "t1"
    assert tool_start.tool_name == "search_memory"
    assert tool_result.tool_id == "t1", "tool_result.tool_id must echo the tool_use id"
    assert tool_result.content == "found 3 matches"
    assert tool_result.is_error is False


# ── Robustness: defensive coverage for SDK ToolResultBlock shapes ──


@pytest.mark.asyncio
async def test_sdk_wrapper_tool_result_block_with_list_content(
    wrapper: SdkWrapper, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Anthropic SDK permits `ToolResultBlock.content` to be `list[dict]`
    (structured tool output). The wrapper must not crash and must project
    it to a string so the gRPC proto stays single-typed."""
    user_msg = UserMessage(
        content=[
            ToolResultBlock(
                tool_use_id="t-list",
                content=[{"type": "text", "text": "line-1"}],
                is_error=None,  # Anthropic SDK allows None → treat as False
            )
        ]
    )
    monkeypatch.setattr(
        "claude_code_runtime.sdk_wrapper.query",
        _fake_query_factory([user_msg]),
    )

    events = await _collect(wrapper.send_message(prompt="hi"))
    assert len(events) == 1
    evt = events[0]
    assert evt.chunk_type == "tool_result"
    assert evt.tool_id == "t-list"
    assert isinstance(evt.content, str)
    assert "line-1" in evt.content
    assert evt.is_error is False


@pytest.mark.asyncio
async def test_sdk_wrapper_user_message_with_string_content_ignored(
    wrapper: SdkWrapper, monkeypatch: pytest.MonkeyPatch
) -> None:
    """UserMessage.content can also be a plain `str` (the original user
    prompt echo). That is not a tool result, so the wrapper must not emit
    anything for it."""
    user_msg = UserMessage(content="hello world, original prompt")
    result_msg = ResultMessage(
        subtype="success",
        duration_ms=1,
        duration_api_ms=1,
        is_error=False,
        num_turns=1,
        session_id="sess-3",
        total_cost_usd=0.0,
    )
    monkeypatch.setattr(
        "claude_code_runtime.sdk_wrapper.query",
        _fake_query_factory([user_msg, result_msg]),
    )

    events = await _collect(wrapper.send_message(prompt="hi"))
    # Only the ResultMessage 'done' chunk — no tool_result emitted.
    assert [e.chunk_type for e in events] == ["done"]


# ── D85: STOP/done chunk carries accumulated final response_text ──────


@pytest.mark.asyncio
async def test_sdk_wrapper_done_chunk_carries_final_response_text(
    wrapper: SdkWrapper, monkeypatch: pytest.MonkeyPatch
) -> None:
    """D85 (S1.T5): the terminal 'done' ChunkEvent must carry the
    concatenated text of all assistant TextBlocks produced during the
    turn. Downstream L4 STOP consumers read response_text directly from
    this chunk's `content` field; without the accumulation fix the done
    chunk carries an empty string even when the assistant replied."""
    assistant_msg = AssistantMessage(
        content=[
            TextBlock(text="Hello"),
            TextBlock(text=" world"),
        ],
        model="claude-sonnet-test",
    )
    result_msg = ResultMessage(
        subtype="success",
        duration_ms=1,
        duration_api_ms=1,
        is_error=False,
        num_turns=1,
        session_id="sess-d85",
        total_cost_usd=0.0,
    )

    monkeypatch.setattr(
        "claude_code_runtime.sdk_wrapper.query",
        _fake_query_factory([assistant_msg, result_msg]),
    )

    events = await _collect(wrapper.send_message(prompt="hi"))
    # Order: text_delta × 2 → done
    assert [e.chunk_type for e in events] == ["text_delta", "text_delta", "done"]

    done_event = events[-1]
    assert done_event.chunk_type == "done"
    # Final response text is the concatenation of every TextBlock yielded
    # in this turn. No separator — mirrors Rust `ChatMessage::text_content`.
    assert done_event.content == "Hello world"
    assert done_event.is_error is False


@pytest.mark.asyncio
async def test_sdk_wrapper_done_chunk_empty_when_no_text_blocks(
    wrapper: SdkWrapper, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Defensive path: a tool-only assistant turn (no TextBlocks at all)
    must still fire the terminal 'done' chunk, but its content stays an
    empty string. No panic, no KeyError."""
    assistant_msg = AssistantMessage(
        content=[ToolUseBlock(id="t-only", name="search", input={"q": "x"})],
        model="claude-sonnet-test",
    )
    result_msg = ResultMessage(
        subtype="success",
        duration_ms=1,
        duration_api_ms=1,
        is_error=False,
        num_turns=1,
        session_id="sess-d85-empty",
        total_cost_usd=0.0,
    )
    monkeypatch.setattr(
        "claude_code_runtime.sdk_wrapper.query",
        _fake_query_factory([assistant_msg, result_msg]),
    )

    events = await _collect(wrapper.send_message(prompt="hi"))
    assert [e.chunk_type for e in events] == ["tool_start", "done"]
    assert events[-1].content == ""
