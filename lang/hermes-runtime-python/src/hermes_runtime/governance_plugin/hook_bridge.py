"""gRPC client for Grid HookBridge sidecar — EvaluateHook one-shot mode."""

import logging

import grpc

from hermes_runtime._fix_proto_imports import fix as _fix_proto_imports

_fix_proto_imports()

from eaasp.common.v1 import common_pb2  # noqa: E402
from eaasp.hook.v1 import hook_pb2, hook_pb2_grpc  # noqa: E402

logger = logging.getLogger(__name__)


class HookBridgeClient:
    """Synchronous gRPC client for HookBridge EvaluateHook."""

    def __init__(self, url: str):
        self._url = url
        self._channel: grpc.Channel | None = None
        self._stub = None

    def _ensure_connected(self):
        if self._channel is None:
            self._channel = grpc.insecure_channel(self._url)
            self._stub = hook_pb2_grpc.HookBridgeServiceStub(self._channel)

    def evaluate_pre_tool_call(
        self, session_id: str, tool_name: str, tool_id: str, input_json: str
    ) -> tuple[str, str, str]:
        """Returns (decision, reason, modified_input). decision: 'allow'|'deny'|'modify'."""
        try:
            self._ensure_connected()
            request = hook_pb2.HookEvaluateRequest(
                session_id=session_id,
                hook_type="pre_tool_call",
                tool_name=tool_name,
                tool_id=tool_id,
                input_json=input_json,
            )
            response = self._stub.EvaluateHook(request, timeout=5.0)
            return response.decision, response.reason, response.modified_input
        except Exception as e:
            logger.warning("HookBridge pre_tool_call failed (allow-on-error): %s", e)
            return "allow", "", ""

    def evaluate_post_tool_result(
        self, session_id: str, tool_name: str, tool_id: str, output: str, is_error: bool
    ) -> tuple[str, str, str]:
        try:
            self._ensure_connected()
            request = hook_pb2.HookEvaluateRequest(
                session_id=session_id,
                hook_type="post_tool_result",
                tool_name=tool_name,
                tool_id=tool_id,
                output=output,
                is_error=is_error,
            )
            response = self._stub.EvaluateHook(request, timeout=5.0)
            return response.decision, response.reason, response.modified_input
        except Exception as e:
            logger.warning("HookBridge post_tool_result failed: %s", e)
            return "allow", "", ""

    def close(self):
        if self._channel:
            self._channel.close()
            self._channel = None
            self._stub = None
