"""Contract-v1 MCP-bridge assertions.

Runtimes that accept ``ConnectMCPRequest`` MUST round-trip McpCall /
McpResult messages with identical semantics regardless of underlying
transport (stdio, SSE, streamable-HTTP).
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def test_connect_mcp_accepts_stdio_server_config(runtime_config):
    """ConnectMCPRequest MUST accept an McpServerConfig with stdio transport."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_mcp_call_round_trips_tool_arguments(runtime_config):
    """A call with tool_args MUST return a result that preserves them end-to-end."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_mcp_call_timeout_surfaces_as_error_event(runtime_config):
    """If the MCP server hangs past the timeout, runtime MUST emit ERROR event."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_mcp_disconnect_releases_server_slot(runtime_config):
    """DisconnectMcpRequest MUST allow a subsequent re-connect with the same id."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_mcp_error_propagates_to_tool_result(runtime_config):
    """MCP server-side errors MUST surface in a TOOL_RESULT with is_error=true."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")
