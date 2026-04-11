from __future__ import annotations

import json

import pytest

from mock_scada.server import (
    SERVER_NAME,
    SERVER_VERSION,
    _TOOL_MANIFEST,
    _handle_scada_read_snapshot,
    _handle_scada_write,
    build_server,
)
from mock_scada.snapshots import SCADA_WRITE_ERROR_MARKER


def test_server_identity() -> None:
    assert SERVER_NAME == "mock-scada"
    assert SERVER_VERSION == "0.1.0"


def test_tool_manifest_exposes_both_tools() -> None:
    names = {tool.name for tool in _TOOL_MANIFEST}
    assert names == {"scada_read_snapshot", "scada_write"}


def test_tool_manifest_schemas_mark_required_fields() -> None:
    by_name = {tool.name: tool for tool in _TOOL_MANIFEST}

    read_schema = by_name["scada_read_snapshot"].inputSchema
    assert read_schema["type"] == "object"
    assert read_schema["required"] == ["device_id"]
    assert "device_id" in read_schema["properties"]
    assert "time_window" in read_schema["properties"]

    write_schema = by_name["scada_write"].inputSchema
    assert set(write_schema["required"]) == {"device_id", "field", "value"}


def test_read_snapshot_returns_telemetry_with_hash() -> None:
    result = _handle_scada_read_snapshot({"device_id": "xfmr-042", "time_window": "5m"})
    assert result["device_id"] == "xfmr-042"
    assert result["sample_count"] >= 1
    assert isinstance(result["snapshot_hash"], str)
    assert len(result["snapshot_hash"]) == 64  # sha256 hex


def test_read_snapshot_defaults_time_window_when_missing_or_empty() -> None:
    result = _handle_scada_read_snapshot({"device_id": "xfmr-042"})
    assert result["time_window"] == "5m"

    result2 = _handle_scada_read_snapshot({"device_id": "xfmr-042", "time_window": ""})
    assert result2["time_window"] == "5m"


def test_read_snapshot_rejects_missing_device_id() -> None:
    with pytest.raises(ValueError, match="device_id"):
        _handle_scada_read_snapshot({})
    with pytest.raises(ValueError, match="device_id"):
        _handle_scada_read_snapshot({"device_id": ""})
    with pytest.raises(ValueError, match="device_id"):
        _handle_scada_read_snapshot({"device_id": 42})  # type: ignore[dict-item]


def test_scada_write_always_fails_with_marker() -> None:
    with pytest.raises(RuntimeError) as exc_info:
        _handle_scada_write(
            {"device_id": "xfmr-042", "field": "setpoint", "value": 1}
        )
    assert SCADA_WRITE_ERROR_MARKER in str(exc_info.value)
    # Error body must preserve args so callers/tests can assert on them.
    payload = str(exc_info.value).split("args=", 1)[1]
    parsed = json.loads(payload)
    assert parsed == {"device_id": "xfmr-042", "field": "setpoint", "value": 1}


def test_build_server_returns_configured_instance() -> None:
    server = build_server()
    assert server.name == SERVER_NAME
