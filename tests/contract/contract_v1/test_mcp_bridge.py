"""Contract-v1 MCP-bridge assertions.

Runtimes that accept ``ConnectMCPRequest`` MUST round-trip McpCall /
McpResult messages with identical semantics regardless of underlying
transport (stdio, SSE, streamable-HTTP).

S0.T4: MCP bridge assertions require an ephemeral MCP server
subprocess + reachable tool. Deferring 5/5 to D137 — tracked for
Phase 2.5 S1.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def test_connect_mcp_accepts_stdio_server_config(runtime_config):
    """ConnectMCPRequest MUST accept an McpServerConfig with stdio transport."""
    pytest.xfail("D137: MCP bridge suite requires ephemeral MCP server fixture; deferred to Phase 2.5 S1")


def test_mcp_call_round_trips_tool_arguments(runtime_config):
    pytest.xfail("D137: MCP call round-trip requires live MCP server; deferred to Phase 2.5 S1")


def test_mcp_call_timeout_surfaces_as_error_event(runtime_config):
    pytest.xfail("D137: MCP timeout behaviour requires controllable MCP server; deferred to Phase 2.5 S1")


def test_mcp_disconnect_releases_server_slot(runtime_config):
    pytest.xfail("D137: MCP disconnect + reconnect requires live MCP server; deferred to Phase 2.5 S1")


def test_mcp_error_propagates_to_tool_result(runtime_config):
    pytest.xfail("D137: MCP error propagation requires fault-injecting MCP server; deferred to Phase 2.5 S1")
