"""Grid Governance plugin for hermes-agent — HookBridge relay."""

import functools
import json
import logging
import os

logger = logging.getLogger(__name__)

# Module-level state — set during register(), used by hooks
_hook_bridge = None
_session_id = ""  # set per-session by hermes-runtime adapter
_original_handle_function_call = None


def set_session_id(sid: str):
    """Called by hermes-runtime adapter to set current session context."""
    global _session_id
    _session_id = sid


def register(ctx):
    """Hermes plugin registration — called by PluginManager."""
    bridge_url = os.getenv("HOOK_BRIDGE_URL", "")
    if not bridge_url:
        logger.info("grid-governance: HOOK_BRIDGE_URL not set, running in audit-only mode")

    # Register hooks for observability/telemetry
    ctx.register_hook("pre_tool_call", _on_pre_tool_call)
    ctx.register_hook("post_tool_call", _on_post_tool_call)
    ctx.register_hook("on_session_start", _on_session_start)
    ctx.register_hook("on_session_end", _on_session_end)

    # Monkey-patch handle_function_call for deny/modify support
    if bridge_url:
        from .hook_bridge import HookBridgeClient

        global _hook_bridge
        _hook_bridge = HookBridgeClient(bridge_url)
        _install_tool_call_interceptor()
        logger.info("grid-governance: HookBridge connected at %s", bridge_url)


def _install_tool_call_interceptor():
    """Wrap model_tools.handle_function_call to check HookBridge decisions."""
    import model_tools

    global _original_handle_function_call
    _original_handle_function_call = model_tools.handle_function_call

    @functools.wraps(_original_handle_function_call)
    def _intercepted_handle_function_call(
        function_name, function_args, task_id=None, **kwargs
    ):
        if _hook_bridge is None:
            return _original_handle_function_call(
                function_name, function_args, task_id=task_id, **kwargs
            )

        # Pre-tool-call governance check
        input_json = json.dumps(function_args, ensure_ascii=False)
        tool_id = kwargs.get("tool_call_id", "") or ""
        decision, reason, modified_input = _hook_bridge.evaluate_pre_tool_call(
            session_id=_session_id,
            tool_name=function_name,
            tool_id=tool_id,
            input_json=input_json,
        )

        if decision == "deny":
            logger.warning(
                "grid-governance DENIED tool call: %s reason=%s", function_name, reason
            )
            return json.dumps(
                {"error": f"[Grid Governance] Tool call denied: {reason}"},
                ensure_ascii=False,
            )

        if decision == "modify" and modified_input:
            try:
                function_args = json.loads(modified_input)
            except json.JSONDecodeError:
                pass

        return _original_handle_function_call(
            function_name, function_args, task_id=task_id, **kwargs
        )

    model_tools.handle_function_call = _intercepted_handle_function_call


# ── Hook callbacks (observability only, return value ignored by hermes) ──


def _on_pre_tool_call(**kwargs):
    logger.debug("grid-governance pre_tool_call: %s", kwargs.get("tool_name"))


def _on_post_tool_call(**kwargs):
    logger.debug("grid-governance post_tool_call: %s", kwargs.get("tool_name"))


def _on_session_start(**kwargs):
    logger.info("grid-governance session_start")


def _on_session_end(**kwargs):
    logger.info("grid-governance session_end")
    global _hook_bridge
    if _hook_bridge:
        _hook_bridge.close()
