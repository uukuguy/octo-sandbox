"""控制平面 — admin session management and telemetry (§3.3).

Exposes admin-facing endpoints for session listing and telemetry queries.
"""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request

router = APIRouter(prefix="/v1/sessions", tags=["admin-sessions"])


@router.get("")
async def list_sessions(request: Request):
    """List all sessions (admin view)."""
    persistence = request.app.state.persistence
    sessions = persistence.list_sessions()
    return [
        {
            "id": s["id"],
            "user": s["user_id"],
            "skill": s["skill_id"],
            "runtime": s["runtime_id"],
            "status": s["status"],
        }
        for s in sessions
    ]


@router.get("/{session_id}/telemetry")
async def get_session_telemetry(session_id: str, request: Request):
    """Get telemetry summary for a session."""
    persistence = request.app.state.persistence
    session = persistence.get_session(session_id)
    if not session:
        raise HTTPException(status_code=404, detail=f"Session not found: {session_id}")

    telemetry = persistence.get_telemetry(session_id)
    return {
        "tools_called": sum(1 for t in telemetry if t["event_type"] == "tool_call"),
        "hooks_fired": sum(1 for t in telemetry if t["event_type"] == "hook_fired"),
        "tokens_used": 0,
        "duration_ms": 0,
    }
