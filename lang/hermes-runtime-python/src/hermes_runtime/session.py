"""Session manager for hermes-runtime — tracks active AIAgent instances.

EAASP v2: SessionPayload has 5 priority blocks (P1 PolicyContext / P2 EventContext /
P3 MemoryRef[] / P4 SkillInstructions / P5 UserPreferences). We store the raw
dicts for each block + derived top-level fields for convenient access.
"""

import time
import uuid
from dataclasses import dataclass, field


@dataclass
class HermesSession:
    session_id: str
    # Derived top-level identifiers (pulled from P5 or payload metadata)
    user_id: str = ""
    runtime_id: str = ""
    # 5 priority blocks as plain dicts (None if absent)
    policy_context: dict | None = None       # P1
    event_context: dict | None = None        # P2
    memory_refs: list = field(default_factory=list)   # P3
    skill_instructions: dict | None = None   # P4
    user_preferences: dict | None = None     # P5
    # Runtime state
    skills: list = field(default_factory=list)  # dynamically loaded skills (LoadSkill RPC)
    mcp_servers: list = field(default_factory=list)
    conversation_history: list = field(default_factory=list)
    created_at: str = field(default_factory=lambda: time.strftime("%Y-%m-%dT%H:%M:%SZ"))
    paused: bool = False

    # Trim helpers used by context budget tests.
    def trim_p5(self):
        """P5 UserPreferences — first to be trimmed when budget exceeded."""
        self.user_preferences = None

    def trim_p4(self):
        """P4 SkillInstructions — trimmed after P5."""
        self.skill_instructions = None

    def trim_p3(self):
        """P3 MemoryRefs — trimmed after P4."""
        self.memory_refs = []

    # NOTE: P1 PolicyContext and P2 EventContext MUST NEVER be removed.


class SessionManager:
    def __init__(self):
        self._sessions: dict[str, HermesSession] = {}

    @property
    def count(self) -> int:
        return len(self._sessions)

    def create(
        self,
        *,
        user_id: str = "",
        runtime_id: str = "hermes-runtime",
        policy_context: dict | None = None,
        event_context: dict | None = None,
        memory_refs: list | None = None,
        skill_instructions: dict | None = None,
        user_preferences: dict | None = None,
    ) -> HermesSession:
        sid = f"hermes-{uuid.uuid4().hex[:12]}"
        session = HermesSession(
            session_id=sid,
            user_id=user_id,
            runtime_id=runtime_id,
            policy_context=policy_context,
            event_context=event_context,
            memory_refs=memory_refs or [],
            skill_instructions=skill_instructions,
            user_preferences=user_preferences,
        )
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
