"""hermes-agent 消息格式 ↔ EAASP v2 proto 转换."""

from hermes_runtime._fix_proto_imports import fix as _fix_proto_imports

_fix_proto_imports()

from eaasp.runtime.v2 import common_pb2, runtime_pb2  # noqa: E402


def chunk_to_proto(chunk_type: str, content: str, **kwargs) -> runtime_pb2.SendResponse:
    """Convert a hermes response fragment to EAASP v2 SendResponse chunk."""
    return runtime_pb2.SendResponse(
        chunk_type=chunk_type,
        content=content,
        tool_name=kwargs.get("tool_name", ""),
        tool_id=kwargs.get("tool_id", ""),
        is_error=kwargs.get("is_error", False),
    )


def telemetry_to_proto(event: dict) -> runtime_pb2.TelemetryEvent:
    """Convert telemetry dict to v2 TelemetryEvent proto (note: only event_type/payload_json/timestamp)."""
    return runtime_pb2.TelemetryEvent(
        event_type=event.get("event_type", ""),
        payload_json=str(event.get("payload", {})),
        timestamp=event.get("timestamp", ""),
    )


def extract_user_id(payload: common_pb2.SessionPayload) -> str:
    """Extract user_id from the v2 5-block SessionPayload (P5 UserPreferences)."""
    if payload.HasField("user_preferences"):
        return payload.user_preferences.user_id
    return payload.user_id  # fallback to session metadata field


def extract_skill_content(payload: common_pb2.SessionPayload) -> str:
    """Extract the loaded skill prose (P4 SkillInstructions.content) if present."""
    if payload.HasField("skill_instructions"):
        return payload.skill_instructions.content
    return ""


def extract_policy_hooks(payload: common_pb2.SessionPayload) -> list:
    """Extract managed hooks from P1 PolicyContext as plain dicts."""
    if not payload.HasField("policy_context"):
        return []
    return [
        {
            "hook_id": h.hook_id,
            "hook_type": h.hook_type,
            "condition": h.condition,
            "action": h.action,
            "precedence": h.precedence,
            "scope": h.scope,
        }
        for h in payload.policy_context.hooks
    ]
