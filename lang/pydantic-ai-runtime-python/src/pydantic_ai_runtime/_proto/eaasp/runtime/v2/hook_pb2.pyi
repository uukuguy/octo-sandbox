from eaasp.runtime.v2 import common_pb2 as _common_pb2
from eaasp.runtime.v2 import runtime_pb2 as _runtime_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class HookEvent(_message.Message):
    __slots__ = ("session_id", "request_id", "event_type", "timestamp", "pre_tool_call", "post_tool_result", "stop", "session_start", "session_end", "pre_policy_deploy", "pre_approval", "event_received", "pre_compact")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    PRE_TOOL_CALL_FIELD_NUMBER: _ClassVar[int]
    POST_TOOL_RESULT_FIELD_NUMBER: _ClassVar[int]
    STOP_FIELD_NUMBER: _ClassVar[int]
    SESSION_START_FIELD_NUMBER: _ClassVar[int]
    SESSION_END_FIELD_NUMBER: _ClassVar[int]
    PRE_POLICY_DEPLOY_FIELD_NUMBER: _ClassVar[int]
    PRE_APPROVAL_FIELD_NUMBER: _ClassVar[int]
    EVENT_RECEIVED_FIELD_NUMBER: _ClassVar[int]
    PRE_COMPACT_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    request_id: str
    event_type: _runtime_pb2.HookEventType
    timestamp: str
    pre_tool_call: PreToolCallHook
    post_tool_result: PostToolResultHook
    stop: StopHook
    session_start: SessionStartHook
    session_end: SessionEndHook
    pre_policy_deploy: PrePolicyDeployHook
    pre_approval: PreApprovalHook
    event_received: EventReceivedHook
    pre_compact: PreCompactHook
    def __init__(self, session_id: _Optional[str] = ..., request_id: _Optional[str] = ..., event_type: _Optional[_Union[_runtime_pb2.HookEventType, str]] = ..., timestamp: _Optional[str] = ..., pre_tool_call: _Optional[_Union[PreToolCallHook, _Mapping]] = ..., post_tool_result: _Optional[_Union[PostToolResultHook, _Mapping]] = ..., stop: _Optional[_Union[StopHook, _Mapping]] = ..., session_start: _Optional[_Union[SessionStartHook, _Mapping]] = ..., session_end: _Optional[_Union[SessionEndHook, _Mapping]] = ..., pre_policy_deploy: _Optional[_Union[PrePolicyDeployHook, _Mapping]] = ..., pre_approval: _Optional[_Union[PreApprovalHook, _Mapping]] = ..., event_received: _Optional[_Union[EventReceivedHook, _Mapping]] = ..., pre_compact: _Optional[_Union[PreCompactHook, _Mapping]] = ...) -> None: ...

class PreToolCallHook(_message.Message):
    __slots__ = ("tool_name", "tool_id", "input_json")
    TOOL_NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_ID_FIELD_NUMBER: _ClassVar[int]
    INPUT_JSON_FIELD_NUMBER: _ClassVar[int]
    tool_name: str
    tool_id: str
    input_json: str
    def __init__(self, tool_name: _Optional[str] = ..., tool_id: _Optional[str] = ..., input_json: _Optional[str] = ...) -> None: ...

class PostToolResultHook(_message.Message):
    __slots__ = ("tool_name", "tool_id", "output", "is_error")
    TOOL_NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_ID_FIELD_NUMBER: _ClassVar[int]
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    IS_ERROR_FIELD_NUMBER: _ClassVar[int]
    tool_name: str
    tool_id: str
    output: str
    is_error: bool
    def __init__(self, tool_name: _Optional[str] = ..., tool_id: _Optional[str] = ..., output: _Optional[str] = ..., is_error: bool = ...) -> None: ...

class StopHook(_message.Message):
    __slots__ = ("reason",)
    REASON_FIELD_NUMBER: _ClassVar[int]
    reason: str
    def __init__(self, reason: _Optional[str] = ...) -> None: ...

class SessionStartHook(_message.Message):
    __slots__ = ("user_id", "user_role", "org_unit")
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ROLE_FIELD_NUMBER: _ClassVar[int]
    ORG_UNIT_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    user_role: str
    org_unit: str
    def __init__(self, user_id: _Optional[str] = ..., user_role: _Optional[str] = ..., org_unit: _Optional[str] = ...) -> None: ...

class SessionEndHook(_message.Message):
    __slots__ = ("reason",)
    REASON_FIELD_NUMBER: _ClassVar[int]
    reason: str
    def __init__(self, reason: _Optional[str] = ...) -> None: ...

class PrePolicyDeployHook(_message.Message):
    __slots__ = ("policy_id", "policy_version")
    POLICY_ID_FIELD_NUMBER: _ClassVar[int]
    POLICY_VERSION_FIELD_NUMBER: _ClassVar[int]
    policy_id: str
    policy_version: str
    def __init__(self, policy_id: _Optional[str] = ..., policy_version: _Optional[str] = ...) -> None: ...

class PreApprovalHook(_message.Message):
    __slots__ = ("approval_id", "resource_ref")
    APPROVAL_ID_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_REF_FIELD_NUMBER: _ClassVar[int]
    approval_id: str
    resource_ref: str
    def __init__(self, approval_id: _Optional[str] = ..., resource_ref: _Optional[str] = ...) -> None: ...

class EventReceivedHook(_message.Message):
    __slots__ = ("event",)
    EVENT_FIELD_NUMBER: _ClassVar[int]
    event: _common_pb2.EventContext
    def __init__(self, event: _Optional[_Union[_common_pb2.EventContext, _Mapping]] = ...) -> None: ...

class PreCompactHook(_message.Message):
    __slots__ = ("trigger", "estimated_tokens", "context_window", "usage_pct", "messages_to_compact", "messages_total", "reuses_prior_summary", "prior_summary_count")
    TRIGGER_FIELD_NUMBER: _ClassVar[int]
    ESTIMATED_TOKENS_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_WINDOW_FIELD_NUMBER: _ClassVar[int]
    USAGE_PCT_FIELD_NUMBER: _ClassVar[int]
    MESSAGES_TO_COMPACT_FIELD_NUMBER: _ClassVar[int]
    MESSAGES_TOTAL_FIELD_NUMBER: _ClassVar[int]
    REUSES_PRIOR_SUMMARY_FIELD_NUMBER: _ClassVar[int]
    PRIOR_SUMMARY_COUNT_FIELD_NUMBER: _ClassVar[int]
    trigger: str
    estimated_tokens: int
    context_window: int
    usage_pct: int
    messages_to_compact: int
    messages_total: int
    reuses_prior_summary: bool
    prior_summary_count: int
    def __init__(self, trigger: _Optional[str] = ..., estimated_tokens: _Optional[int] = ..., context_window: _Optional[int] = ..., usage_pct: _Optional[int] = ..., messages_to_compact: _Optional[int] = ..., messages_total: _Optional[int] = ..., reuses_prior_summary: bool = ..., prior_summary_count: _Optional[int] = ...) -> None: ...

class HookResponse(_message.Message):
    __slots__ = ("request_id", "decision", "policy_update", "error")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    DECISION_FIELD_NUMBER: _ClassVar[int]
    POLICY_UPDATE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    decision: HookDecision
    policy_update: PolicyUpdate
    error: ErrorResponse
    def __init__(self, request_id: _Optional[str] = ..., decision: _Optional[_Union[HookDecision, _Mapping]] = ..., policy_update: _Optional[_Union[PolicyUpdate, _Mapping]] = ..., error: _Optional[_Union[ErrorResponse, _Mapping]] = ...) -> None: ...

class HookDecision(_message.Message):
    __slots__ = ("decision", "reason", "mutated_input_json", "precedence")
    DECISION_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    MUTATED_INPUT_JSON_FIELD_NUMBER: _ClassVar[int]
    PRECEDENCE_FIELD_NUMBER: _ClassVar[int]
    decision: str
    reason: str
    mutated_input_json: str
    precedence: int
    def __init__(self, decision: _Optional[str] = ..., reason: _Optional[str] = ..., mutated_input_json: _Optional[str] = ..., precedence: _Optional[int] = ...) -> None: ...

class PolicyUpdate(_message.Message):
    __slots__ = ("policy_id", "policy_json", "action", "timestamp")
    POLICY_ID_FIELD_NUMBER: _ClassVar[int]
    POLICY_JSON_FIELD_NUMBER: _ClassVar[int]
    ACTION_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    policy_id: str
    policy_json: str
    action: str
    timestamp: str
    def __init__(self, policy_id: _Optional[str] = ..., policy_json: _Optional[str] = ..., action: _Optional[str] = ..., timestamp: _Optional[str] = ...) -> None: ...

class ErrorResponse(_message.Message):
    __slots__ = ("code", "message")
    CODE_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    code: str
    message: str
    def __init__(self, code: _Optional[str] = ..., message: _Optional[str] = ...) -> None: ...

class HookEvaluateRequest(_message.Message):
    __slots__ = ("session_id", "event_type", "tool_name", "tool_id", "input_json", "output", "is_error")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    TOOL_NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_ID_FIELD_NUMBER: _ClassVar[int]
    INPUT_JSON_FIELD_NUMBER: _ClassVar[int]
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    IS_ERROR_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    event_type: _runtime_pb2.HookEventType
    tool_name: str
    tool_id: str
    input_json: str
    output: str
    is_error: bool
    def __init__(self, session_id: _Optional[str] = ..., event_type: _Optional[_Union[_runtime_pb2.HookEventType, str]] = ..., tool_name: _Optional[str] = ..., tool_id: _Optional[str] = ..., input_json: _Optional[str] = ..., output: _Optional[str] = ..., is_error: bool = ...) -> None: ...

class HookTelemetryBatch(_message.Message):
    __slots__ = ("session_id", "events")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    EVENTS_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    events: _containers.RepeatedCompositeFieldContainer[HookTelemetryEvent]
    def __init__(self, session_id: _Optional[str] = ..., events: _Optional[_Iterable[_Union[HookTelemetryEvent, _Mapping]]] = ...) -> None: ...

class HookTelemetryEvent(_message.Message):
    __slots__ = ("hook_id", "event_type", "decision", "latency_us", "timestamp")
    HOOK_ID_FIELD_NUMBER: _ClassVar[int]
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    DECISION_FIELD_NUMBER: _ClassVar[int]
    LATENCY_US_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    hook_id: str
    event_type: _runtime_pb2.HookEventType
    decision: str
    latency_us: int
    timestamp: str
    def __init__(self, hook_id: _Optional[str] = ..., event_type: _Optional[_Union[_runtime_pb2.HookEventType, str]] = ..., decision: _Optional[str] = ..., latency_us: _Optional[int] = ..., timestamp: _Optional[str] = ...) -> None: ...

class TelemetryAck(_message.Message):
    __slots__ = ("accepted", "rejected")
    ACCEPTED_FIELD_NUMBER: _ClassVar[int]
    REJECTED_FIELD_NUMBER: _ClassVar[int]
    accepted: int
    rejected: int
    def __init__(self, accepted: _Optional[int] = ..., rejected: _Optional[int] = ...) -> None: ...

class PolicySummaryRequest(_message.Message):
    __slots__ = ("session_id",)
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    def __init__(self, session_id: _Optional[str] = ...) -> None: ...

class PolicySummary(_message.Message):
    __slots__ = ("total_policies", "policies")
    TOTAL_POLICIES_FIELD_NUMBER: _ClassVar[int]
    POLICIES_FIELD_NUMBER: _ClassVar[int]
    total_policies: int
    policies: _containers.RepeatedCompositeFieldContainer[PolicyInfo]
    def __init__(self, total_policies: _Optional[int] = ..., policies: _Optional[_Iterable[_Union[PolicyInfo, _Mapping]]] = ...) -> None: ...

class PolicyInfo(_message.Message):
    __slots__ = ("policy_id", "name", "scope", "hook_type", "enabled", "precedence")
    POLICY_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    SCOPE_FIELD_NUMBER: _ClassVar[int]
    HOOK_TYPE_FIELD_NUMBER: _ClassVar[int]
    ENABLED_FIELD_NUMBER: _ClassVar[int]
    PRECEDENCE_FIELD_NUMBER: _ClassVar[int]
    policy_id: str
    name: str
    scope: str
    hook_type: _runtime_pb2.HookEventType
    enabled: bool
    precedence: int
    def __init__(self, policy_id: _Optional[str] = ..., name: _Optional[str] = ..., scope: _Optional[str] = ..., hook_type: _Optional[_Union[_runtime_pb2.HookEventType, str]] = ..., enabled: bool = ..., precedence: _Optional[int] = ...) -> None: ...
