"""State manager — session serialization/deserialization.

Uses JSON format with state_format="python-json".
"""

from __future__ import annotations

import json
import logging

from .session import Session

logger = logging.getLogger(__name__)

STATE_FORMAT = "python-json"


def serialize_session(session: Session) -> bytes:
    """Serialize session to bytes for gRPC GetState."""
    return json.dumps(session.to_dict(), ensure_ascii=False).encode("utf-8")


def deserialize_session(data: bytes) -> dict:
    """Deserialize session state from bytes.

    Returns raw dict; caller uses SessionManager.restore() to create Session.
    """
    return json.loads(data.decode("utf-8"))
