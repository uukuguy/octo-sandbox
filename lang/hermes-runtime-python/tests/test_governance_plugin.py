"""Tests for governance_plugin — HookBridge client + monkey-patch interceptor."""

import json
import sys
import types
from unittest.mock import MagicMock


def test_hook_bridge_client_allow_on_error():
    """HookBridge 连接失败时 fallback 到 allow。"""
    from hermes_runtime.governance_plugin.hook_bridge import HookBridgeClient

    client = HookBridgeClient("localhost:99999")
    decision, reason, modified = client.evaluate_pre_tool_call(
        "s1", "terminal", "t1", '{"command": "ls"}'
    )
    assert decision == "allow"


def test_interceptor_deny():
    """Monkey-patch 拦截器在 deny 时返回 error JSON。"""
    # Create a fake model_tools module
    fake_model_tools = types.ModuleType("model_tools")
    fake_model_tools.handle_function_call = MagicMock(return_value='{"ok": true}')
    sys.modules["model_tools"] = fake_model_tools

    try:
        import hermes_runtime.governance_plugin as gp

        mock_bridge = MagicMock()
        mock_bridge.evaluate_pre_tool_call.return_value = ("deny", "blocked by policy", "")

        gp._hook_bridge = mock_bridge
        gp._original_handle_function_call = fake_model_tools.handle_function_call
        gp.set_session_id("test-session")

        # Install interceptor (rewrites model_tools.handle_function_call)
        gp._install_tool_call_interceptor()

        result = fake_model_tools.handle_function_call(
            "terminal", {"command": "rm -rf /"}
        )

        parsed = json.loads(result)
        assert "denied" in parsed["error"].lower()
        # Original function should NOT have been called
        gp._original_handle_function_call.assert_not_called()
    finally:
        del sys.modules["model_tools"]


def test_interceptor_modify():
    """Monkey-patch 拦截器在 modify 时替换参数。"""
    fake_model_tools = types.ModuleType("model_tools")
    original_fn = MagicMock(return_value='{"ok": true}')
    fake_model_tools.handle_function_call = original_fn
    sys.modules["model_tools"] = fake_model_tools

    try:
        import hermes_runtime.governance_plugin as gp

        mock_bridge = MagicMock()
        modified_args = json.dumps({"command": "ls -la"})
        mock_bridge.evaluate_pre_tool_call.return_value = ("modify", "", modified_args)

        gp._hook_bridge = mock_bridge
        gp._original_handle_function_call = original_fn
        gp.set_session_id("test-session")

        gp._install_tool_call_interceptor()
        fake_model_tools.handle_function_call("terminal", {"command": "rm -rf /"})

        # Verify original function was called with modified args
        call_args = original_fn.call_args
        assert call_args[0][1] == {"command": "ls -la"}
    finally:
        del sys.modules["model_tools"]
