"""hermes-agent 消息格式 ↔ EAASP proto 转换。"""

from hermes_runtime._fix_proto_imports import fix as _fix_proto_imports

_fix_proto_imports()

from eaasp.runtime.v1 import runtime_pb2  # noqa: E402
from eaasp.common.v1 import common_pb2  # noqa: E402


def chunk_to_proto(chunk_type: str, content: str, **kwargs) -> runtime_pb2.ResponseChunk:
    """Convert a hermes response fragment to EAASP ResponseChunk proto."""
    return runtime_pb2.ResponseChunk(
        chunk_type=chunk_type,
        content=content,
        tool_name=kwargs.get("tool_name", ""),
        tool_id=kwargs.get("tool_id", ""),
        is_error=kwargs.get("is_error", False),
    )


def telemetry_to_proto(event: dict) -> common_pb2.TelemetryEvent:
    """Convert telemetry dict to proto."""
    return common_pb2.TelemetryEvent(
        session_id=event.get("session_id", ""),
        runtime_id=event.get("runtime_id", ""),
        event_type=event.get("event_type", ""),
        timestamp=event.get("timestamp", ""),
        payload_json=str(event.get("payload", {})),
    )
