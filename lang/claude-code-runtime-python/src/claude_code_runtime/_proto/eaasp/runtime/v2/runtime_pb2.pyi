from eaasp.runtime.v2 import common_pb2 as _common_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class HookEventType(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    SESSION_START: _ClassVar[HookEventType]
    USER_PROMPT_SUBMIT: _ClassVar[HookEventType]
    PRE_TOOL_USE: _ClassVar[HookEventType]
    POST_TOOL_USE: _ClassVar[HookEventType]
    POST_TOOL_USE_FAILURE: _ClassVar[HookEventType]
    PERMISSION_REQUEST: _ClassVar[HookEventType]
    STOP: _ClassVar[HookEventType]
    SUBAGENT_STOP: _ClassVar[HookEventType]
    PRE_COMPACT: _ClassVar[HookEventType]
    PRE_POLICY_DEPLOY: _ClassVar[HookEventType]
    PRE_APPROVAL: _ClassVar[HookEventType]
    EVENT_RECEIVED: _ClassVar[HookEventType]
    PRE_SESSION_CREATE: _ClassVar[HookEventType]
    POST_SESSION_END: _ClassVar[HookEventType]
SESSION_START: HookEventType
USER_PROMPT_SUBMIT: HookEventType
PRE_TOOL_USE: HookEventType
POST_TOOL_USE: HookEventType
POST_TOOL_USE_FAILURE: HookEventType
PERMISSION_REQUEST: HookEventType
STOP: HookEventType
SUBAGENT_STOP: HookEventType
PRE_COMPACT: HookEventType
PRE_POLICY_DEPLOY: HookEventType
PRE_APPROVAL: HookEventType
EVENT_RECEIVED: HookEventType
PRE_SESSION_CREATE: HookEventType
POST_SESSION_END: HookEventType

class InitializeRequest(_message.Message):
    __slots__ = ("payload",)
    PAYLOAD_FIELD_NUMBER: _ClassVar[int]
    payload: _common_pb2.SessionPayload
    def __init__(self, payload: _Optional[_Union[_common_pb2.SessionPayload, _Mapping]] = ...) -> None: ...

class InitializeResponse(_message.Message):
    __slots__ = ("session_id", "runtime_id")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    RUNTIME_ID_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    runtime_id: str
    def __init__(self, session_id: _Optional[str] = ..., runtime_id: _Optional[str] = ...) -> None: ...

class SendRequest(_message.Message):
    __slots__ = ("session_id", "message")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    message: UserMessage
    def __init__(self, session_id: _Optional[str] = ..., message: _Optional[_Union[UserMessage, _Mapping]] = ...) -> None: ...

class UserMessage(_message.Message):
    __slots__ = ("content", "message_type", "metadata")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_TYPE_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    content: str
    message_type: str
    metadata: _containers.ScalarMap[str, str]
    def __init__(self, content: _Optional[str] = ..., message_type: _Optional[str] = ..., metadata: _Optional[_Mapping[str, str]] = ...) -> None: ...

class SendResponse(_message.Message):
    __slots__ = ("chunk_type", "content", "tool_name", "tool_id", "is_error", "error")
    CHUNK_TYPE_FIELD_NUMBER: _ClassVar[int]
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    TOOL_NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_ID_FIELD_NUMBER: _ClassVar[int]
    IS_ERROR_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    chunk_type: str
    content: str
    tool_name: str
    tool_id: str
    is_error: bool
    error: _common_pb2.RuntimeError
    def __init__(self, chunk_type: _Optional[str] = ..., content: _Optional[str] = ..., tool_name: _Optional[str] = ..., tool_id: _Optional[str] = ..., is_error: bool = ..., error: _Optional[_Union[_common_pb2.RuntimeError, _Mapping]] = ...) -> None: ...

class LoadSkillRequest(_message.Message):
    __slots__ = ("session_id", "skill")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    SKILL_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    skill: _common_pb2.SkillInstructions
    def __init__(self, session_id: _Optional[str] = ..., skill: _Optional[_Union[_common_pb2.SkillInstructions, _Mapping]] = ...) -> None: ...

class LoadSkillResponse(_message.Message):
    __slots__ = ("success", "error")
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    success: bool
    error: str
    def __init__(self, success: bool = ..., error: _Optional[str] = ...) -> None: ...

class ToolCallEvent(_message.Message):
    __slots__ = ("session_id", "tool_name", "tool_id", "input_json")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    TOOL_NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_ID_FIELD_NUMBER: _ClassVar[int]
    INPUT_JSON_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    tool_name: str
    tool_id: str
    input_json: str
    def __init__(self, session_id: _Optional[str] = ..., tool_name: _Optional[str] = ..., tool_id: _Optional[str] = ..., input_json: _Optional[str] = ...) -> None: ...

class ToolCallAck(_message.Message):
    __slots__ = ("decision", "mutated_input_json", "reason")
    DECISION_FIELD_NUMBER: _ClassVar[int]
    MUTATED_INPUT_JSON_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    decision: str
    mutated_input_json: str
    reason: str
    def __init__(self, decision: _Optional[str] = ..., mutated_input_json: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class ToolResultEvent(_message.Message):
    __slots__ = ("session_id", "tool_name", "tool_id", "output", "is_error")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    TOOL_NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_ID_FIELD_NUMBER: _ClassVar[int]
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    IS_ERROR_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    tool_name: str
    tool_id: str
    output: str
    is_error: bool
    def __init__(self, session_id: _Optional[str] = ..., tool_name: _Optional[str] = ..., tool_id: _Optional[str] = ..., output: _Optional[str] = ..., is_error: bool = ...) -> None: ...

class ToolResultAck(_message.Message):
    __slots__ = ("decision", "reason")
    DECISION_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    decision: str
    reason: str
    def __init__(self, decision: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class StopEvent(_message.Message):
    __slots__ = ("session_id", "reason")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    reason: str
    def __init__(self, session_id: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class StopAck(_message.Message):
    __slots__ = ("decision", "reason")
    DECISION_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    decision: str
    reason: str
    def __init__(self, decision: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class StateResponse(_message.Message):
    __slots__ = ("session_id", "state_data", "runtime_id", "state_format", "created_at")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    STATE_DATA_FIELD_NUMBER: _ClassVar[int]
    RUNTIME_ID_FIELD_NUMBER: _ClassVar[int]
    STATE_FORMAT_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    state_data: bytes
    runtime_id: str
    state_format: str
    created_at: str
    def __init__(self, session_id: _Optional[str] = ..., state_data: _Optional[bytes] = ..., runtime_id: _Optional[str] = ..., state_format: _Optional[str] = ..., created_at: _Optional[str] = ...) -> None: ...

class ConnectMCPRequest(_message.Message):
    __slots__ = ("session_id", "servers")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    SERVERS_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    servers: _containers.RepeatedCompositeFieldContainer[McpServerConfig]
    def __init__(self, session_id: _Optional[str] = ..., servers: _Optional[_Iterable[_Union[McpServerConfig, _Mapping]]] = ...) -> None: ...

class ConnectMCPResponse(_message.Message):
    __slots__ = ("success", "connected", "failed")
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    CONNECTED_FIELD_NUMBER: _ClassVar[int]
    FAILED_FIELD_NUMBER: _ClassVar[int]
    success: bool
    connected: _containers.RepeatedScalarFieldContainer[str]
    failed: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, success: bool = ..., connected: _Optional[_Iterable[str]] = ..., failed: _Optional[_Iterable[str]] = ...) -> None: ...

class McpServerConfig(_message.Message):
    __slots__ = ("name", "transport", "command", "args", "url", "env")
    class EnvEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    NAME_FIELD_NUMBER: _ClassVar[int]
    TRANSPORT_FIELD_NUMBER: _ClassVar[int]
    COMMAND_FIELD_NUMBER: _ClassVar[int]
    ARGS_FIELD_NUMBER: _ClassVar[int]
    URL_FIELD_NUMBER: _ClassVar[int]
    ENV_FIELD_NUMBER: _ClassVar[int]
    name: str
    transport: str
    command: str
    args: _containers.RepeatedScalarFieldContainer[str]
    url: str
    env: _containers.ScalarMap[str, str]
    def __init__(self, name: _Optional[str] = ..., transport: _Optional[str] = ..., command: _Optional[str] = ..., args: _Optional[_Iterable[str]] = ..., url: _Optional[str] = ..., env: _Optional[_Mapping[str, str]] = ...) -> None: ...

class DisconnectMcpRequest(_message.Message):
    __slots__ = ("session_id", "server_name")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    SERVER_NAME_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    server_name: str
    def __init__(self, session_id: _Optional[str] = ..., server_name: _Optional[str] = ...) -> None: ...

class TelemetryRequest(_message.Message):
    __slots__ = ("session_id", "events")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    EVENTS_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    events: _containers.RepeatedCompositeFieldContainer[TelemetryEvent]
    def __init__(self, session_id: _Optional[str] = ..., events: _Optional[_Iterable[_Union[TelemetryEvent, _Mapping]]] = ...) -> None: ...

class TelemetryEvent(_message.Message):
    __slots__ = ("event_type", "payload_json", "timestamp")
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    PAYLOAD_JSON_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    event_type: str
    payload_json: str
    timestamp: str
    def __init__(self, event_type: _Optional[str] = ..., payload_json: _Optional[str] = ..., timestamp: _Optional[str] = ...) -> None: ...

class HealthResponse(_message.Message):
    __slots__ = ("healthy", "runtime_id", "checks")
    class ChecksEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    HEALTHY_FIELD_NUMBER: _ClassVar[int]
    RUNTIME_ID_FIELD_NUMBER: _ClassVar[int]
    CHECKS_FIELD_NUMBER: _ClassVar[int]
    healthy: bool
    runtime_id: str
    checks: _containers.ScalarMap[str, str]
    def __init__(self, healthy: bool = ..., runtime_id: _Optional[str] = ..., checks: _Optional[_Mapping[str, str]] = ...) -> None: ...

class Capabilities(_message.Message):
    __slots__ = ("runtime_id", "model", "context_window", "tools", "supports_native_hooks", "supports_native_mcp", "supports_native_skills", "cost_per_1k_tokens", "credential_mode", "strengths", "limitations", "tier", "deployment_mode")
    class CredentialMode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        DIRECT: _ClassVar[Capabilities.CredentialMode]
        PROXY: _ClassVar[Capabilities.CredentialMode]
        BRIDGE_INJECTED: _ClassVar[Capabilities.CredentialMode]
    DIRECT: Capabilities.CredentialMode
    PROXY: Capabilities.CredentialMode
    BRIDGE_INJECTED: Capabilities.CredentialMode
    RUNTIME_ID_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_WINDOW_FIELD_NUMBER: _ClassVar[int]
    TOOLS_FIELD_NUMBER: _ClassVar[int]
    SUPPORTS_NATIVE_HOOKS_FIELD_NUMBER: _ClassVar[int]
    SUPPORTS_NATIVE_MCP_FIELD_NUMBER: _ClassVar[int]
    SUPPORTS_NATIVE_SKILLS_FIELD_NUMBER: _ClassVar[int]
    COST_PER_1K_TOKENS_FIELD_NUMBER: _ClassVar[int]
    CREDENTIAL_MODE_FIELD_NUMBER: _ClassVar[int]
    STRENGTHS_FIELD_NUMBER: _ClassVar[int]
    LIMITATIONS_FIELD_NUMBER: _ClassVar[int]
    TIER_FIELD_NUMBER: _ClassVar[int]
    DEPLOYMENT_MODE_FIELD_NUMBER: _ClassVar[int]
    runtime_id: str
    model: str
    context_window: int
    tools: _containers.RepeatedScalarFieldContainer[str]
    supports_native_hooks: bool
    supports_native_mcp: bool
    supports_native_skills: bool
    cost_per_1k_tokens: float
    credential_mode: Capabilities.CredentialMode
    strengths: _containers.RepeatedScalarFieldContainer[str]
    limitations: _containers.RepeatedScalarFieldContainer[str]
    tier: str
    deployment_mode: str
    def __init__(self, runtime_id: _Optional[str] = ..., model: _Optional[str] = ..., context_window: _Optional[int] = ..., tools: _Optional[_Iterable[str]] = ..., supports_native_hooks: bool = ..., supports_native_mcp: bool = ..., supports_native_skills: bool = ..., cost_per_1k_tokens: _Optional[float] = ..., credential_mode: _Optional[_Union[Capabilities.CredentialMode, str]] = ..., strengths: _Optional[_Iterable[str]] = ..., limitations: _Optional[_Iterable[str]] = ..., tier: _Optional[str] = ..., deployment_mode: _Optional[str] = ...) -> None: ...

class EventStreamEntry(_message.Message):
    __slots__ = ("session_id", "event_id", "event_type", "payload_json", "timestamp")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    EVENT_ID_FIELD_NUMBER: _ClassVar[int]
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    PAYLOAD_JSON_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    event_id: str
    event_type: HookEventType
    payload_json: str
    timestamp: str
    def __init__(self, session_id: _Optional[str] = ..., event_id: _Optional[str] = ..., event_type: _Optional[_Union[HookEventType, str]] = ..., payload_json: _Optional[str] = ..., timestamp: _Optional[str] = ...) -> None: ...
