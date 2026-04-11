from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class PolicyContext(_message.Message):
    __slots__ = ("hooks", "org_unit", "policy_version", "quotas", "deploy_timestamp")
    class QuotasEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    HOOKS_FIELD_NUMBER: _ClassVar[int]
    ORG_UNIT_FIELD_NUMBER: _ClassVar[int]
    POLICY_VERSION_FIELD_NUMBER: _ClassVar[int]
    QUOTAS_FIELD_NUMBER: _ClassVar[int]
    DEPLOY_TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    hooks: _containers.RepeatedCompositeFieldContainer[ManagedHook]
    org_unit: str
    policy_version: str
    quotas: _containers.ScalarMap[str, str]
    deploy_timestamp: str
    def __init__(self, hooks: _Optional[_Iterable[_Union[ManagedHook, _Mapping]]] = ..., org_unit: _Optional[str] = ..., policy_version: _Optional[str] = ..., quotas: _Optional[_Mapping[str, str]] = ..., deploy_timestamp: _Optional[str] = ...) -> None: ...

class ManagedHook(_message.Message):
    __slots__ = ("hook_id", "hook_type", "condition", "action", "precedence", "scope")
    HOOK_ID_FIELD_NUMBER: _ClassVar[int]
    HOOK_TYPE_FIELD_NUMBER: _ClassVar[int]
    CONDITION_FIELD_NUMBER: _ClassVar[int]
    ACTION_FIELD_NUMBER: _ClassVar[int]
    PRECEDENCE_FIELD_NUMBER: _ClassVar[int]
    SCOPE_FIELD_NUMBER: _ClassVar[int]
    hook_id: str
    hook_type: str
    condition: str
    action: str
    precedence: int
    scope: str
    def __init__(self, hook_id: _Optional[str] = ..., hook_type: _Optional[str] = ..., condition: _Optional[str] = ..., action: _Optional[str] = ..., precedence: _Optional[int] = ..., scope: _Optional[str] = ...) -> None: ...

class EventContext(_message.Message):
    __slots__ = ("event_id", "event_type", "severity", "source", "payload_json", "timestamp")
    EVENT_ID_FIELD_NUMBER: _ClassVar[int]
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    SEVERITY_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    PAYLOAD_JSON_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    event_id: str
    event_type: str
    severity: str
    source: str
    payload_json: str
    timestamp: str
    def __init__(self, event_id: _Optional[str] = ..., event_type: _Optional[str] = ..., severity: _Optional[str] = ..., source: _Optional[str] = ..., payload_json: _Optional[str] = ..., timestamp: _Optional[str] = ...) -> None: ...

class MemoryRef(_message.Message):
    __slots__ = ("memory_id", "memory_type", "relevance_score", "content", "source_session_id", "created_at", "tags")
    class TagsEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    MEMORY_ID_FIELD_NUMBER: _ClassVar[int]
    MEMORY_TYPE_FIELD_NUMBER: _ClassVar[int]
    RELEVANCE_SCORE_FIELD_NUMBER: _ClassVar[int]
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    SOURCE_SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    TAGS_FIELD_NUMBER: _ClassVar[int]
    memory_id: str
    memory_type: str
    relevance_score: float
    content: str
    source_session_id: str
    created_at: str
    tags: _containers.ScalarMap[str, str]
    def __init__(self, memory_id: _Optional[str] = ..., memory_type: _Optional[str] = ..., relevance_score: _Optional[float] = ..., content: _Optional[str] = ..., source_session_id: _Optional[str] = ..., created_at: _Optional[str] = ..., tags: _Optional[_Mapping[str, str]] = ...) -> None: ...

class SkillInstructions(_message.Message):
    __slots__ = ("skill_id", "name", "content", "frontmatter_hooks", "metadata")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    SKILL_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    FRONTMATTER_HOOKS_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    skill_id: str
    name: str
    content: str
    frontmatter_hooks: _containers.RepeatedCompositeFieldContainer[ScopedHook]
    metadata: _containers.ScalarMap[str, str]
    def __init__(self, skill_id: _Optional[str] = ..., name: _Optional[str] = ..., content: _Optional[str] = ..., frontmatter_hooks: _Optional[_Iterable[_Union[ScopedHook, _Mapping]]] = ..., metadata: _Optional[_Mapping[str, str]] = ...) -> None: ...

class ScopedHook(_message.Message):
    __slots__ = ("hook_id", "hook_type", "condition", "action", "precedence")
    HOOK_ID_FIELD_NUMBER: _ClassVar[int]
    HOOK_TYPE_FIELD_NUMBER: _ClassVar[int]
    CONDITION_FIELD_NUMBER: _ClassVar[int]
    ACTION_FIELD_NUMBER: _ClassVar[int]
    PRECEDENCE_FIELD_NUMBER: _ClassVar[int]
    hook_id: str
    hook_type: str
    condition: str
    action: str
    precedence: int
    def __init__(self, hook_id: _Optional[str] = ..., hook_type: _Optional[str] = ..., condition: _Optional[str] = ..., action: _Optional[str] = ..., precedence: _Optional[int] = ...) -> None: ...

class UserPreferences(_message.Message):
    __slots__ = ("user_id", "prefs", "language", "timezone")
    class PrefsEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    PREFS_FIELD_NUMBER: _ClassVar[int]
    LANGUAGE_FIELD_NUMBER: _ClassVar[int]
    TIMEZONE_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    prefs: _containers.ScalarMap[str, str]
    language: str
    timezone: str
    def __init__(self, user_id: _Optional[str] = ..., prefs: _Optional[_Mapping[str, str]] = ..., language: _Optional[str] = ..., timezone: _Optional[str] = ...) -> None: ...

class SessionPayload(_message.Message):
    __slots__ = ("policy_context", "event_context", "memory_refs", "skill_instructions", "user_preferences", "allow_trim_p5", "allow_trim_p4", "allow_trim_p3", "session_id", "user_id", "runtime_id", "created_at")
    POLICY_CONTEXT_FIELD_NUMBER: _ClassVar[int]
    EVENT_CONTEXT_FIELD_NUMBER: _ClassVar[int]
    MEMORY_REFS_FIELD_NUMBER: _ClassVar[int]
    SKILL_INSTRUCTIONS_FIELD_NUMBER: _ClassVar[int]
    USER_PREFERENCES_FIELD_NUMBER: _ClassVar[int]
    ALLOW_TRIM_P5_FIELD_NUMBER: _ClassVar[int]
    ALLOW_TRIM_P4_FIELD_NUMBER: _ClassVar[int]
    ALLOW_TRIM_P3_FIELD_NUMBER: _ClassVar[int]
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    RUNTIME_ID_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    policy_context: PolicyContext
    event_context: EventContext
    memory_refs: _containers.RepeatedCompositeFieldContainer[MemoryRef]
    skill_instructions: SkillInstructions
    user_preferences: UserPreferences
    allow_trim_p5: bool
    allow_trim_p4: bool
    allow_trim_p3: bool
    session_id: str
    user_id: str
    runtime_id: str
    created_at: str
    def __init__(self, policy_context: _Optional[_Union[PolicyContext, _Mapping]] = ..., event_context: _Optional[_Union[EventContext, _Mapping]] = ..., memory_refs: _Optional[_Iterable[_Union[MemoryRef, _Mapping]]] = ..., skill_instructions: _Optional[_Union[SkillInstructions, _Mapping]] = ..., user_preferences: _Optional[_Union[UserPreferences, _Mapping]] = ..., allow_trim_p5: bool = ..., allow_trim_p4: bool = ..., allow_trim_p3: bool = ..., session_id: _Optional[str] = ..., user_id: _Optional[str] = ..., runtime_id: _Optional[str] = ..., created_at: _Optional[str] = ...) -> None: ...

class Empty(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class EvidenceAnchor(_message.Message):
    __slots__ = ("anchor_id", "data_ref", "snapshot_hash", "created_at", "produced_by")
    ANCHOR_ID_FIELD_NUMBER: _ClassVar[int]
    DATA_REF_FIELD_NUMBER: _ClassVar[int]
    SNAPSHOT_HASH_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    PRODUCED_BY_FIELD_NUMBER: _ClassVar[int]
    anchor_id: str
    data_ref: str
    snapshot_hash: str
    created_at: str
    produced_by: str
    def __init__(self, anchor_id: _Optional[str] = ..., data_ref: _Optional[str] = ..., snapshot_hash: _Optional[str] = ..., created_at: _Optional[str] = ..., produced_by: _Optional[str] = ...) -> None: ...

class RuntimeError(_message.Message):
    __slots__ = ("code", "message", "details")
    class DetailsEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    CODE_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    DETAILS_FIELD_NUMBER: _ClassVar[int]
    code: str
    message: str
    details: _containers.ScalarMap[str, str]
    def __init__(self, code: _Optional[str] = ..., message: _Optional[str] = ..., details: _Optional[_Mapping[str, str]] = ...) -> None: ...
