"""契约 4: 遥测采集 API (§8.4).

POST /v1/telemetry                    — ingest telemetry events
GET  /v1/telemetry/sessions/{id}      — query session telemetry
GET  /v1/telemetry/sessions/{id}/audit — query audit events only (BH-D3)
"""

from __future__ import annotations

from fastapi import APIRouter, Request
from pydantic import BaseModel

router = APIRouter(prefix="/v1/telemetry", tags=["telemetry"])


class TelemetryEvent(BaseModel):
    event_type: str
    timestamp: str = ""
    payload: dict = {}
    resource_usage: dict = {}


class TelemetryIngest(BaseModel):
    session_id: str
    events: list[TelemetryEvent]


@router.post("")
async def ingest_telemetry(req: TelemetryIngest, request: Request):
    """Accept telemetry events from L1 runtimes."""
    store = request.app.state.telemetry_store
    if req.session_id not in store:
        store[req.session_id] = []

    accepted = 0
    for event in req.events:
        store[req.session_id].append(event.model_dump())
        accepted += 1

    return {"accepted": accepted, "rejected": 0}


@router.get("/sessions/{session_id}")
async def get_session_telemetry(session_id: str, request: Request):
    """Query telemetry events for a session."""
    store = request.app.state.telemetry_store
    events = store.get(session_id, [])

    resource_summary = {}
    for e in events:
        if e.get("resource_usage"):
            for k, v in e["resource_usage"].items():
                if isinstance(v, (int, float)):
                    resource_summary[k] = resource_summary.get(k, 0) + v

    return {
        "session_id": session_id,
        "events": events,
        "resource_summary": resource_summary,
    }


# Audit event types for filtering
_AUDIT_EVENT_TYPES = {"hook_fired", "hook_deny", "hook_allow", "audit", "tool_audit"}


@router.get("/sessions/{session_id}/audit")
async def get_session_audit(session_id: str, request: Request):
    """Query audit-specific events for a session (BH-D3).

    Filters telemetry events to return only audit-related entries:
    hook_fired, hook_deny, hook_allow, audit, tool_audit.
    """
    store = request.app.state.telemetry_store
    events = store.get(session_id, [])

    audit_events = [
        e for e in events
        if e.get("event_type") in _AUDIT_EVENT_TYPES
        or e.get("payload", {}).get("audit") is True
    ]

    return {
        "session_id": session_id,
        "audit_events": audit_events,
        "total_audit": len(audit_events),
    }
