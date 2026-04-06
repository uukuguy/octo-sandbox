"""Telemetry collector — records and emits EAASP telemetry events.

Event types align with grid-runtime's EaaspEventType:
- session_start, session_end
- send, tool_call, tool_result
- skill_loaded, mcp_connected, mcp_disconnected
- hook_evaluated, stop_evaluated
- error
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class TelemetryEntry:
    """A single telemetry event."""

    event_type: str
    timestamp: float = field(default_factory=time.time)
    session_id: str = ""
    runtime_id: str = ""
    user_id: str = ""
    payload: dict = field(default_factory=dict)
    input_tokens: int = 0
    output_tokens: int = 0
    compute_ms: int = 0


class TelemetryCollector:
    """Collects telemetry events for a session."""

    def __init__(self, session_id: str, runtime_id: str, user_id: str = ""):
        self.session_id = session_id
        self.runtime_id = runtime_id
        self.user_id = user_id
        self._events: list[TelemetryEntry] = []

    def record(
        self,
        event_type: str,
        payload: dict | None = None,
        input_tokens: int = 0,
        output_tokens: int = 0,
        compute_ms: int = 0,
    ) -> None:
        """Record a telemetry event."""
        entry = TelemetryEntry(
            event_type=event_type,
            session_id=self.session_id,
            runtime_id=self.runtime_id,
            user_id=self.user_id,
            payload=payload or {},
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            compute_ms=compute_ms,
        )
        self._events.append(entry)
        logger.debug("Telemetry: %s [%s]", event_type, self.session_id)

    def flush(self) -> list[TelemetryEntry]:
        """Return all events and clear the buffer."""
        events = self._events.copy()
        self._events.clear()
        return events

    def peek(self) -> list[TelemetryEntry]:
        """Return all events without clearing."""
        return self._events.copy()

    @property
    def count(self) -> int:
        return len(self._events)
