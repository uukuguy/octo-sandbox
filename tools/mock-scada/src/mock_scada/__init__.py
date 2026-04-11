"""Mock SCADA MCP stdio server for EAASP v2.0 Phase 0 e2e verification.

Exposes two tools:
- scada_read_snapshot(device_id, time_window): returns deterministic telemetry.
- scada_write(...): always fails; exists so PreToolUse hooks can block it.
"""

from .snapshots import (
    SAMPLE_DEVICE_IDS,
    SCADA_WRITE_ERROR_MARKER,
    build_snapshot,
    snapshot_hash,
)

__all__ = [
    "SAMPLE_DEVICE_IDS",
    "SCADA_WRITE_ERROR_MARKER",
    "build_snapshot",
    "snapshot_hash",
]
