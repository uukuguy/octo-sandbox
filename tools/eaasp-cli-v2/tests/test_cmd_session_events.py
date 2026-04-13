"""Tests for `eaasp session events` command."""

from __future__ import annotations

import json
from unittest.mock import patch

from typer.testing import CliRunner

from eaasp_cli_v2.main import app

runner = CliRunner()

_MOCK_EVENTS = {
    "session_id": "sess_001",
    "events": [
        {
            "seq": 1,
            "event_type": "SESSION_START",
            "payload": {"runtime_id": "grid-runtime"},
            "created_at": 1700000000,
            "cluster_id": None,
        },
        {
            "seq": 2,
            "event_type": "PRE_TOOL_USE",
            "payload": {"tool_name": "scada_read"},
            "created_at": 1700000001,
            "cluster_id": "c-abc12345",
        },
        {
            "seq": 3,
            "event_type": "STOP",
            "payload": {"reason": "complete"},
            "created_at": 1700000005,
            "cluster_id": "c-abc12345",
        },
    ],
}


async def _mock_fetch_events(*args, **kwargs):
    """Async mock that returns events directly."""
    return _MOCK_EVENTS


async def _mock_fetch_empty(*args, **kwargs):
    return {"session_id": "sess_empty", "events": []}


def test_session_events_table_format():
    """session events <id> should list events in table format."""
    with patch(
        "eaasp_cli_v2.cmd_session._fetch_events",
        new=_mock_fetch_events,
    ):
        result = runner.invoke(app, ["session", "events", "sess_001"])
        assert result.exit_code == 0, result.output
        assert "SESSION_START" in result.output
        assert "PRE_TOOL_USE" in result.output
        assert "STOP" in result.output


def test_session_events_json_format():
    """session events <id> --format json should output raw JSON."""
    with patch(
        "eaasp_cli_v2.cmd_session._fetch_events",
        new=_mock_fetch_events,
    ):
        result = runner.invoke(
            app, ["session", "events", "sess_001", "--format", "json"]
        )
        assert result.exit_code == 0, result.output
        parsed = json.loads(result.output)
        assert len(parsed["events"]) == 3
        assert parsed["events"][0]["event_type"] == "SESSION_START"


def test_session_events_empty():
    """session events for a session with no events."""
    with patch(
        "eaasp_cli_v2.cmd_session._fetch_events",
        new=_mock_fetch_empty,
    ):
        result = runner.invoke(app, ["session", "events", "sess_empty"])
        assert result.exit_code == 0, result.output
        assert "No events" in result.output
