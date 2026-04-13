"""Tests for MCP bridge — tool injection, env resolution, monkey-patch."""

import asyncio
from unittest.mock import MagicMock, patch

from hermes_runtime.mcp_bridge import (
    McpBridge,
    McpToolProxy,
    inject_mcp_tools,
)


class TestMcpToolProxy:
    def test_to_openai_tool(self):
        bridge = McpBridge("test-server", "http://localhost:18090/sse")
        proxy = McpToolProxy(
            name="scada_read_snapshot",
            description="Read telemetry",
            input_schema={
                "type": "object",
                "properties": {"device_id": {"type": "string"}},
                "required": ["device_id"],
            },
            bridge=bridge,
        )
        tool_dict = proxy.to_openai_tool()
        assert tool_dict["type"] == "function"
        assert tool_dict["function"]["name"] == "scada_read_snapshot"
        assert tool_dict["function"]["description"] == "Read telemetry"
        assert "device_id" in tool_dict["function"]["parameters"]["properties"]


class TestInjectMcpTools:
    def test_inject_adds_tools_to_agent(self):
        """inject_mcp_tools adds tool dicts and names to agent."""
        agent = MagicMock()
        agent.tools = []
        agent.valid_tool_names = set()

        bridge = McpBridge("test-server", "http://localhost:18090/sse")
        proxy = McpToolProxy(
            name="my_tool",
            description="A test tool",
            input_schema={"type": "object", "properties": {}},
            bridge=bridge,
        )
        bridge._tools = [proxy]

        # model_tools and run_agent won't be available outside container
        # so we mock them
        mock_model_tools = MagicMock()
        mock_model_tools.handle_function_call = MagicMock(return_value="original")
        mock_run_agent = MagicMock()
        mock_run_agent.handle_function_call = mock_model_tools.handle_function_call

        with patch.dict("sys.modules", {
            "model_tools": mock_model_tools,
            "run_agent": mock_run_agent,
        }):
            inject_mcp_tools(agent, [bridge])

        assert len(agent.tools) == 1
        assert agent.tools[0]["function"]["name"] == "my_tool"
        assert "my_tool" in agent.valid_tool_names

    def test_inject_patches_handle_function_call(self):
        """After injection, MCP tool calls are routed to the proxy."""
        agent = MagicMock()
        agent.tools = []
        agent.valid_tool_names = set()

        bridge = McpBridge("test-server", "http://localhost:18090/sse")

        async def fake_call(name, args):
            return '{"result": "mcp_response"}'

        proxy = McpToolProxy(
            name="mcp_tool",
            description="MCP tool",
            input_schema={"type": "object", "properties": {}},
            bridge=bridge,
        )
        bridge._tools = [proxy]

        original_fn = MagicMock(return_value="original_result")
        mock_model_tools = MagicMock()
        mock_model_tools.handle_function_call = original_fn
        mock_run_agent = MagicMock()
        mock_run_agent.handle_function_call = original_fn

        with patch.dict("sys.modules", {
            "model_tools": mock_model_tools,
            "run_agent": mock_run_agent,
        }):
            inject_mcp_tools(agent, [bridge])

        # The patched function should be installed on both modules
        patched_fn = mock_model_tools.handle_function_call
        assert patched_fn is not original_fn  # was replaced

        # Non-MCP tool should fall through to original
        result = patched_fn("terminal", {"command": "ls"})
        assert result == "original_result"

    def test_empty_bridges_no_patch(self):
        """No bridges → no tools added, no patch."""
        agent = MagicMock()
        agent.tools = []
        agent.valid_tool_names = set()

        inject_mcp_tools(agent, [])
        assert len(agent.tools) == 0


class TestConfigFallback:
    """Test HERMES_* → OPENAI_* fallback chain."""

    def test_hermes_vars_take_priority(self, monkeypatch):
        from hermes_runtime.config import HermesRuntimeConfig

        monkeypatch.setenv("HERMES_API_KEY", "hermes-key")
        monkeypatch.setenv("HERMES_BASE_URL", "http://hermes-url")
        monkeypatch.setenv("HERMES_MODEL", "hermes-model")
        monkeypatch.setenv("OPENAI_API_KEY", "openai-key")
        monkeypatch.setenv("OPENAI_BASE_URL", "http://openai-url")
        monkeypatch.setenv("OPENAI_MODEL_NAME", "openai-model")

        config = HermesRuntimeConfig.from_env()
        assert config.hermes_api_key == "hermes-key"
        assert config.hermes_base_url == "http://hermes-url"
        assert config.hermes_model == "hermes-model"

    def test_falls_back_to_openai(self, monkeypatch):
        from hermes_runtime.config import HermesRuntimeConfig

        # Clear HERMES_* vars
        monkeypatch.delenv("HERMES_API_KEY", raising=False)
        monkeypatch.delenv("HERMES_BASE_URL", raising=False)
        monkeypatch.delenv("HERMES_MODEL", raising=False)
        monkeypatch.setenv("OPENAI_API_KEY", "openai-key")
        monkeypatch.setenv("OPENAI_BASE_URL", "http://openai-url")
        monkeypatch.setenv("OPENAI_MODEL_NAME", "openai-model")

        config = HermesRuntimeConfig.from_env()
        assert config.hermes_api_key == "openai-key"
        assert config.hermes_base_url == "http://openai-url"
        assert config.hermes_model == "openai-model"

    def test_no_vars_uses_defaults(self, monkeypatch):
        from hermes_runtime.config import HermesRuntimeConfig

        monkeypatch.delenv("HERMES_API_KEY", raising=False)
        monkeypatch.delenv("HERMES_BASE_URL", raising=False)
        monkeypatch.delenv("HERMES_MODEL", raising=False)
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
        monkeypatch.delenv("OPENAI_MODEL_NAME", raising=False)

        config = HermesRuntimeConfig.from_env()
        assert config.hermes_api_key == ""
        assert config.hermes_base_url == ""
        assert config.hermes_model == "anthropic/claude-sonnet-4-20250514"
