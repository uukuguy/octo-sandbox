"""Session manager for hermes-runtime — tracks active AIAgent instances."""

import time
import uuid
from dataclasses import dataclass, field


@dataclass
class HermesSession:
    session_id: str
    user_id: str
    user_role: str
    org_unit: str
    managed_hooks_json: str = ""
    context: dict = field(default_factory=dict)
    hook_bridge_url: str = ""
    telemetry_endpoint: str = ""
    skills: list = field(default_factory=list)
    mcp_servers: list = field(default_factory=list)
    conversation_history: list = field(default_factory=list)
    created_at: str = field(default_factory=lambda: time.strftime("%Y-%m-%dT%H:%M:%SZ"))
    paused: bool = False


class SessionManager:
    def __init__(self):
        self._sessions: dict[str, HermesSession] = {}

    @property
    def count(self) -> int:
        return len(self._sessions)

    def create(self, **kwargs) -> HermesSession:
        sid = f"hermes-{uuid.uuid4().hex[:12]}"
        session = HermesSession(session_id=sid, **kwargs)
        self._sessions[sid] = session
        return session

    def get(self, session_id: str) -> HermesSession | None:
        return self._sessions.get(session_id)

    def terminate(self, session_id: str) -> HermesSession | None:
        return self._sessions.pop(session_id, None)

    def pause(self, session_id: str) -> bool:
        s = self._sessions.get(session_id)
        if s:
            s.paused = True
            return True
        return False

    def resume(self, session_id: str) -> bool:
        s = self._sessions.get(session_id)
        if s and s.paused:
            s.paused = False
            return True
        return False

    def restore(self, data: dict) -> HermesSession:
        session = HermesSession(**data)
        self._sessions[session.session_id] = session
        return session
