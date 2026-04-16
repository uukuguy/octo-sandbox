"""W2.T6 — Skill-extraction E2E smoke through nanobot → mock_openai_server.

Verifies that the nanobot-runtime AgentSession drives the skill-extraction
workflow end-to-end using a mock OpenAI-compatible server (pytest-httpx),
asserting the event sequence emitted matches the expected TOOL_CALL /
TOOL_RESULT / CHUNK / STOP pattern.

Design:
- No live LLM or gRPC server. AgentSession is exercised in-process.
- MockOpenAIServer (pytest-httpx respx) returns canned OAI responses for the
  4 required_tools (memory_search / memory_read / memory_write_anchor /
  memory_write_file) and a final text STOP.
- StubToolExecutor returns canned JSON matching fixture expectations.
- Hook scripts are NOT invoked here — hook wiring belongs to future S3 work.
  (W2.T6 scope: event sequence + mock LLM round-trip only.)

Fixture reuse: skill_extraction_input_trace.json from
claude-code-runtime-python/tests/fixtures/ is the canonical shared fixture;
we load it by path for determinism.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any
from unittest.mock import AsyncMock, MagicMock

import pytest

from nanobot_runtime.provider import OpenAICompatProvider
from nanobot_runtime.session import AgentSession, EventType

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURE_PATH = (
    REPO_ROOT
    / "lang"
    / "claude-code-runtime-python"
    / "tests"
    / "fixtures"
    / "skill_extraction_input_trace.json"
)

REQUIRED_TOOLS = {
    "memory_search",
    "memory_read",
    "memory_write_anchor",
    "memory_write_file",
}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _tool_call_response(tool_calls: list[dict[str, Any]]) -> dict[str, Any]:
    """Build an OAI chat response that requests tool calls."""
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


def _text_response(content: str) -> dict[str, Any]:
    """Build an OAI chat response with plain text content."""
    return {"choices": [{"message": {"role": "assistant", "content": content}}]}


def _tc(tool_id: str, name: str, args: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": tool_id,
        "type": "function",
        "function": {"name": name, "arguments": json.dumps(args)},
    }


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def fixture_trace() -> dict[str, Any]:
    return json.loads(FIXTURE_PATH.read_text())


@pytest.fixture
def mock_provider() -> MagicMock:
    """Mock provider with canned responses for 4-tool skill-extraction workflow."""
    p = MagicMock(spec=OpenAICompatProvider)
    p.chat = AsyncMock(
        side_effect=[
            # Turn 1: agent calls memory_search
            _tool_call_response(
                [_tc("tc-search", "memory_search", {"query": "cluster_threshold_xfmr042_v1", "top_k": 1})]
            ),
            # Turn 2: agent calls memory_read
            _tool_call_response(
                [_tc("tc-read", "memory_read", {"memory_id": "mem_cluster_threshold_xfmr042_v1"})]
            ),
            # Turn 3: agent calls memory_write_anchor
            _tool_call_response(
                [
                    _tc(
                        "tc-anchor",
                        "memory_write_anchor",
                        {
                            "event_id": "evt_extract_sess_20260415_001",
                            "session_id": "sess_20260415_001",
                            "type": "skill_extraction_source",
                            "data_ref": "cluster_threshold_xfmr042_v1",
                            "source_system": "skill-extraction",
                        },
                    )
                ]
            ),
            # Turn 4: agent calls memory_write_file
            _tool_call_response(
                [
                    _tc(
                        "tc-write",
                        "memory_write_file",
                        {
                            "scope": "org:eaasp-mvp",
                            "category": "skill_draft",
                            "content": json.dumps({"frontmatter_yaml": "---\n---\n", "prose": "# Draft"}),
                            "evidence_refs": ["anc_skill_src_sess_20260415_001"],
                            "status": "agent_suggested",
                        },
                    )
                ]
            ),
            # Turn 5: agent stops with final text
            _text_response(
                json.dumps(
                    {
                        "draft_memory_id": "mem_skill_draft_sess_20260415_001_v1",
                        "source_cluster_id": "cluster_threshold_xfmr042_v1",
                        "suggested_skill_id": "threshold-calibration-variant-A",
                        "evidence_anchor_id": "anc_skill_src_sess_20260415_001",
                        "event_count": 6,
                        "analysis_summary": "Replay of threshold-calibration 6-event cluster.",
                        "confidence_score": 0.87,
                    }
                )
            ),
        ]
    )
    p.aclose = AsyncMock()
    return p


class _SkillExtractionToolExecutor:
    """Canned tool executor for the 4 required skill-extraction tools."""

    def __init__(self, cluster_id: str, session_id: str) -> None:
        self.cluster_id = cluster_id
        self.session_id = session_id
        self.calls: list[tuple[str, dict[str, Any]]] = []

    async def execute(self, tool_name: str, tool_input: dict[str, Any]) -> str:
        self.calls.append((tool_name, tool_input))
        if tool_name == "memory_search":
            return json.dumps(
                {
                    "hits": [
                        {
                            "memory_id": f"mem_{self.cluster_id}",
                            "relevance": 0.93,
                            "scope": "org:eaasp-mvp",
                        }
                    ]
                }
            )
        if tool_name == "memory_read":
            return json.dumps(
                {
                    "memory_id": tool_input.get("memory_id", ""),
                    "version": 1,
                    "status": "agent_suggested",
                    "content": "{}",
                }
            )
        if tool_name == "memory_write_anchor":
            return json.dumps(
                {
                    "anchor_id": f"anc_skill_src_{self.session_id}",
                    "event_id": tool_input.get("event_id", ""),
                    "session_id": self.session_id,
                    "type": tool_input.get("type", ""),
                    "created_at": 1713168020000,
                }
            )
        if tool_name == "memory_write_file":
            return json.dumps(
                {
                    "memory_id": f"mem_skill_draft_{self.session_id}_v1",
                    "version": 1,
                    "scope": tool_input.get("scope", ""),
                    "category": tool_input.get("category", ""),
                    "status": tool_input.get("status", "agent_suggested"),
                }
            )
        return json.dumps({"tool": tool_name, "args": tool_input})


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


async def test_fixture_loads_correctly(fixture_trace: dict[str, Any]) -> None:
    """Shared fixture is readable and has expected schema."""
    assert fixture_trace["schema_version"] == 1
    assert fixture_trace["cluster_id"] == "cluster_threshold_xfmr042_v1"
    assert fixture_trace["session_id"] == "sess_20260415_001"
    assert len(fixture_trace["events"]) == 6


async def test_skill_extraction_event_sequence(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """AgentSession emits 4×(TOOL_CALL+TOOL_RESULT) + CHUNK + STOP for skill-extraction."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    events = [ev async for ev in session.run("Extract skill from cluster")]

    types = [e.event_type for e in events]

    # Expected: 4 tool round-trips (TOOL_CALL + TOOL_RESULT each) + CHUNK + STOP
    expected = [
        EventType.TOOL_CALL, EventType.TOOL_RESULT,
        EventType.TOOL_CALL, EventType.TOOL_RESULT,
        EventType.TOOL_CALL, EventType.TOOL_RESULT,
        EventType.TOOL_CALL, EventType.TOOL_RESULT,
        EventType.CHUNK, EventType.STOP,
    ]
    assert types == expected, f"Got event sequence: {types}"


async def test_required_tools_all_called(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """All 4 SKILL.md required_tools are exercised in one session.run()."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    _ = [ev async for ev in session.run("Extract skill from cluster")]

    called = {name for name, _ in executor.calls}
    assert called == REQUIRED_TOOLS, f"Missing tools: {REQUIRED_TOOLS - called}"


async def test_tool_call_ordering(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """Tools are called in SKILL.md-mandated order: search→read→write_anchor→write_file."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    _ = [ev async for ev in session.run("Extract skill from cluster")]

    call_order = [name for name, _ in executor.calls]
    assert call_order == [
        "memory_search",
        "memory_read",
        "memory_write_anchor",
        "memory_write_file",
    ], f"Actual order: {call_order}"


async def test_stop_event_content_is_json_with_draft_id(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """Final STOP event content parses as JSON containing draft_memory_id."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    events = [ev async for ev in session.run("Extract skill from cluster")]

    stop_events = [e for e in events if e.event_type == EventType.STOP]
    assert len(stop_events) == 1
    output = json.loads(stop_events[0].content)
    assert output.get("draft_memory_id"), "draft_memory_id must be non-empty in STOP content"
    assert output.get("evidence_anchor_id"), "evidence_anchor_id must be non-empty in STOP content"


async def test_no_error_events_emitted(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """Happy-path run emits zero ERROR events."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    events = [ev async for ev in session.run("Extract skill from cluster")]

    error_events = [e for e in events if e.event_type == EventType.ERROR]
    assert not error_events, f"Unexpected ERROR events: {[e.content for e in error_events]}"


async def test_n14_skill_submit_draft_never_called(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """N14: skill_submit_draft must never be called by the skill-extraction workflow."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    _ = [ev async for ev in session.run("Extract skill from cluster")]

    called = {name for name, _ in executor.calls}
    assert "skill_submit_draft" not in called, (
        "N14 violation: skill_submit_draft must never be called automatically"
    )


async def test_provider_called_five_times(
    mock_provider: MagicMock, fixture_trace: dict[str, Any]
) -> None:
    """Provider.chat() is called exactly 5 times: 4 tool rounds + 1 final text."""
    executor = _SkillExtractionToolExecutor(
        cluster_id=fixture_trace["cluster_id"],
        session_id=fixture_trace["session_id"],
    )
    session = AgentSession(
        provider=mock_provider,
        tool_executor=executor,
        session_id=fixture_trace["session_id"],
    )
    _ = [ev async for ev in session.run("Extract skill from cluster")]

    assert mock_provider.chat.call_count == 5, (
        f"Expected 5 chat() calls (4 tool rounds + stop), got {mock_provider.chat.call_count}"
    )
