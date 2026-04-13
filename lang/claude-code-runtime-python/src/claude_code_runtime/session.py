"""Session management for claude-code-runtime."""

from __future__ import annotations

import logging
import time
import uuid
from dataclasses import dataclass, field
from enum import Enum

logger = logging.getLogger(__name__)


class SessionState(Enum):
    ACTIVE = "active"
    PAUSED = "paused"
    TERMINATED = "terminated"


@dataclass
class Session:
    """Represents an active runtime session."""

    session_id: str
    user_id: str
    user_role: str = ""
    org_unit: str = ""
    managed_hooks_json: str = ""
    state: SessionState = SessionState.ACTIVE
    created_at: float = field(default_factory=time.time)

    # Runtime state
    skills: list[dict] = field(default_factory=list)
    mcp_servers: list[str] = field(default_factory=list)
    mcp_cwd: str | None = None  # temp dir with .mcp.json for SDK auto-discovery
    telemetry_events: list[dict] = field(default_factory=list)
    context: dict[str, str] = field(default_factory=dict)
    hook_bridge_url: str = ""
    telemetry_endpoint: str = ""

    # D2-py — P3 memory_refs from SessionPayload (v2.0 Phase 0 S4.T2)
    memory_refs: list[dict] = field(default_factory=list)
    # D2-py — P1 policy_context from SessionPayload (metadata only, no hook
    # execution at this layer; HookExecutor continues to run off
    # managed_hooks_json)
    policy_context: dict | None = None
    # D2-py — tracks whether the cross-session memory preamble has already
    # been prepended to the system prompt for this session. The preamble is
    # injected only on the first Send to avoid duplication across turns.
    preamble_injected: bool = False

    def to_dict(self) -> dict:
        """Serialize session to dict for state persistence."""
        return {
            "session_id": self.session_id,
            "user_id": self.user_id,
            "user_role": self.user_role,
            "org_unit": self.org_unit,
            "managed_hooks_json": self.managed_hooks_json,
            "state": self.state.value,
            "created_at": self.created_at,
            "skills": self.skills,
            "mcp_servers": self.mcp_servers,
            "telemetry_events": self.telemetry_events,
            "context": self.context,
            "memory_refs": self.memory_refs,
            "policy_context": self.policy_context,
            "preamble_injected": self.preamble_injected,
        }

    @classmethod
    def from_dict(cls, data: dict) -> Session:
        """Deserialize session from dict."""
        state_str = data.get("state", "active")
        return cls(
            session_id=data.get("session_id", ""),
            user_id=data.get("user_id", ""),
            user_role=data.get("user_role", ""),
            org_unit=data.get("org_unit", ""),
            managed_hooks_json=data.get("managed_hooks_json", ""),
            state=SessionState(state_str),
            created_at=data.get("created_at", time.time()),
            skills=data.get("skills", []),
            mcp_servers=data.get("mcp_servers", []),
            telemetry_events=data.get("telemetry_events", []),
            context=data.get("context", {}),
            memory_refs=data.get("memory_refs", []),
            policy_context=data.get("policy_context"),
            preamble_injected=data.get("preamble_injected", False),
        )


class SessionManager:
    """Manages runtime sessions."""

    def __init__(self):
        self._sessions: dict[str, Session] = {}

    def create(
        self,
        user_id: str,
        user_role: str = "",
        org_unit: str = "",
        managed_hooks_json: str = "",
        context: dict[str, str] | None = None,
        hook_bridge_url: str = "",
        telemetry_endpoint: str = "",
        memory_refs: list[dict] | None = None,
        policy_context: dict | None = None,
    ) -> Session:
        """Create a new session."""
        session_id = f"crt-{uuid.uuid4().hex[:12]}"
        session = Session(
            session_id=session_id,
            user_id=user_id,
            user_role=user_role,
            org_unit=org_unit,
            managed_hooks_json=managed_hooks_json,
            context=context or {},
            hook_bridge_url=hook_bridge_url,
            telemetry_endpoint=telemetry_endpoint,
            memory_refs=memory_refs or [],
            policy_context=policy_context,
        )
        self._sessions[session_id] = session
        logger.info("Session created: %s (user=%s)", session_id, user_id)
        return session

    def get(self, session_id: str) -> Session | None:
        """Get session by ID."""
        return self._sessions.get(session_id)

    def pause(self, session_id: str) -> bool:
        """Pause a session."""
        session = self._sessions.get(session_id)
        if session and session.state == SessionState.ACTIVE:
            session.state = SessionState.PAUSED
            return True
        return False

    def resume(self, session_id: str) -> bool:
        """Resume a paused session."""
        session = self._sessions.get(session_id)
        if session and session.state == SessionState.PAUSED:
            session.state = SessionState.ACTIVE
            return True
        return False

    def terminate(self, session_id: str) -> Session | None:
        """Terminate and remove a session."""
        session = self._sessions.pop(session_id, None)
        if session:
            session.state = SessionState.TERMINATED
            logger.info("Session terminated: %s", session_id)
        return session

    def restore(self, data: dict) -> Session:
        """Restore a session from serialized state."""
        session = Session.from_dict(data)
        if not session.session_id:
            session.session_id = f"crt-restored-{uuid.uuid4().hex[:8]}"
        session.state = SessionState.ACTIVE
        self._sessions[session.session_id] = session
        logger.info("Session restored: %s", session.session_id)
        return session

    @property
    def count(self) -> int:
        return len(self._sessions)
