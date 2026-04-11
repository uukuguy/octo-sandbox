"""Mapper — converts between SDK types and EAASP v2 gRPC proto types.

v2 notes:
- SendResponse replaces v1 ResponseChunk (adds structured RuntimeError field).
- TelemetryEvent is flattened: only event_type / payload_json / timestamp.
  Per-session metadata (session_id / runtime_id / user_id) is moved up to the
  enclosing TelemetryRequest, and resource usage is encoded inside
  payload_json for forward-compat with the 5-block SessionPayload layout.
"""

from __future__ import annotations

import json

from ._proto.eaasp.runtime.v2 import common_pb2, runtime_pb2
from .sdk_wrapper import ChunkEvent
from .telemetry import TelemetryEntry


def chunk_to_proto(chunk: ChunkEvent) -> runtime_pb2.SendResponse:
    """Convert SDK ChunkEvent to v2 SendResponse."""
    resp = runtime_pb2.SendResponse(
        chunk_type=chunk.chunk_type,
        content=chunk.content,
        tool_name=chunk.tool_name,
        tool_id=chunk.tool_id,
        is_error=chunk.is_error,
    )
    if chunk.is_error and chunk.content:
        resp.error.CopyFrom(
            common_pb2.RuntimeError(code="sdk_error", message=chunk.content)
        )
    return resp


def _telemetry_payload(entry: TelemetryEntry) -> dict:
    """Flatten TelemetryEntry side-data into a JSON payload dict."""
    payload: dict = dict(entry.payload or {})
    if entry.input_tokens:
        payload["input_tokens"] = entry.input_tokens
    if entry.output_tokens:
        payload["output_tokens"] = entry.output_tokens
    if entry.compute_ms:
        payload["compute_ms"] = entry.compute_ms
    return payload


def telemetry_to_proto(entry: TelemetryEntry) -> runtime_pb2.TelemetryEvent:
    """Convert a TelemetryEntry to a v2 TelemetryEvent."""
    payload = _telemetry_payload(entry)
    return runtime_pb2.TelemetryEvent(
        event_type=entry.event_type,
        payload_json=json.dumps(payload, ensure_ascii=False) if payload else "",
        timestamp=str(entry.timestamp),
    )


def telemetry_batch_to_proto(
    entries: list[TelemetryEntry],
    session_id: str = "",
) -> runtime_pb2.TelemetryRequest:
    """Convert a list of TelemetryEntry into a v2 TelemetryRequest bundle.

    In v2, batches are addressed via TelemetryRequest (which carries
    session_id), not a bare TelemetryBatch.
    """
    return runtime_pb2.TelemetryRequest(
        session_id=session_id,
        events=[telemetry_to_proto(e) for e in entries],
    )
