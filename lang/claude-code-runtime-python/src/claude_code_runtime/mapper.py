"""Mapper — converts between SDK types and gRPC proto types."""

from __future__ import annotations

import json

from ._proto.eaasp.common.v1 import common_pb2
from ._proto.eaasp.runtime.v1 import runtime_pb2
from .sdk_wrapper import ChunkEvent
from .telemetry import TelemetryEntry


def chunk_to_proto(chunk: ChunkEvent) -> runtime_pb2.ResponseChunk:
    """Convert SDK ChunkEvent to gRPC ResponseChunk."""
    return runtime_pb2.ResponseChunk(
        chunk_type=chunk.chunk_type,
        content=chunk.content,
        tool_name=chunk.tool_name,
        tool_id=chunk.tool_id,
        is_error=chunk.is_error,
    )


def telemetry_to_proto(entry: TelemetryEntry) -> common_pb2.TelemetryEvent:
    """Convert TelemetryEntry to gRPC TelemetryEvent."""
    return common_pb2.TelemetryEvent(
        session_id=entry.session_id,
        runtime_id=entry.runtime_id,
        user_id=entry.user_id,
        event_type=entry.event_type,
        timestamp=str(entry.timestamp),
        payload_json=json.dumps(entry.payload) if entry.payload else "",
        resource_usage=common_pb2.ResourceUsage(
            input_tokens=entry.input_tokens,
            output_tokens=entry.output_tokens,
            compute_ms=entry.compute_ms,
        ),
    )


def telemetry_batch_to_proto(
    entries: list[TelemetryEntry],
) -> common_pb2.TelemetryBatch:
    """Convert list of TelemetryEntry to gRPC TelemetryBatch."""
    return common_pb2.TelemetryBatch(
        events=[telemetry_to_proto(e) for e in entries]
    )
