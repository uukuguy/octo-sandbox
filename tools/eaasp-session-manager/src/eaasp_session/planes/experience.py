"""体验平面 — user-facing conversation API (§3.1).

Handles conversation creation, message relay, and status queries.
L4 uses 'conversations', L3 uses 'sessions' (KD-BH4).
"""

from __future__ import annotations

import uuid

from fastapi import APIRouter, HTTPException, Request

from eaasp_session.models import CreateConversationRequest, SendMessageRequest

router = APIRouter(prefix="/v1/conversations", tags=["conversations"])


@router.post("")
async def create_conversation(req: CreateConversationRequest, request: Request):
    """Create a conversation — resolves intent → creates L3 session."""
    persistence = request.app.state.persistence
    l3_client = request.app.state.l3_client
    conversation_id = f"conv-{uuid.uuid4().hex[:8]}"

    # Resolve skill_id
    skill_id = req.skill_id
    if not skill_id and req.input:
        intent_result = await l3_client.resolve_intent(
            req.input, req.user_id, req.org_unit
        )
        skill_id = intent_result.get("skill_id")
        if not skill_id:
            raise HTTPException(status_code=400, detail="Could not resolve intent to a skill")

    # Create L3 session (three-way handshake via L3)
    l3_result = await l3_client.create_session(
        user_id=req.user_id,
        user_role="user",
        org_unit=req.org_unit,
        skill_id=skill_id,
    )

    session_id = l3_result["session_id"]
    runtime_id = l3_result.get("runtime_id", "")

    # Persist in L4
    persistence.create_session(
        session_id=session_id,
        conversation_id=conversation_id,
        user_id=req.user_id,
        org_unit=req.org_unit,
        skill_id=skill_id,
        runtime_id=runtime_id,
        runtime_endpoint=l3_result.get("runtime_endpoint", ""),
        managed_hooks_digest=l3_result.get("governance_summary", {}).get("managed_hooks_digest", ""),
    )
    persistence.log_event(session_id, "conversation_created", {"conversation_id": conversation_id})

    return {
        "conversation_id": conversation_id,
        "session_id": session_id,
        "skill_name": skill_id,
        "runtime": runtime_id,
    }


@router.post("/{conversation_id}/message")
async def send_message(conversation_id: str, req: SendMessageRequest, request: Request):
    """Send message — proxies through L3 to L1 (KD-BH6)."""
    persistence = request.app.state.persistence
    l3_client = request.app.state.l3_client

    # Find session by conversation_id
    sessions = persistence.list_sessions()
    session = next((s for s in sessions if s["conversation_id"] == conversation_id), None)
    if not session:
        raise HTTPException(status_code=404, detail=f"Conversation not found: {conversation_id}")

    if session["status"] != "active":
        raise HTTPException(status_code=400, detail=f"Conversation not active: {session['status']}")

    result = await l3_client.send_message(session["id"], req.content)
    persistence.log_event(session["id"], "message_sent", {"content_preview": req.content[:50]})

    return result


@router.get("/{conversation_id}")
async def get_conversation(conversation_id: str, request: Request):
    """Get conversation status."""
    persistence = request.app.state.persistence
    sessions = persistence.list_sessions()
    session = next((s for s in sessions if s["conversation_id"] == conversation_id), None)
    if not session:
        raise HTTPException(status_code=404, detail=f"Conversation not found: {conversation_id}")

    return {
        "id": conversation_id,
        "status": session["status"],
        "skill": session["skill_id"],
        "created_at": session["created_at"],
    }


@router.delete("/{conversation_id}")
async def delete_conversation(conversation_id: str, request: Request):
    """Terminate conversation → L3 terminate → L1 cleanup."""
    persistence = request.app.state.persistence
    l3_client = request.app.state.l3_client

    sessions = persistence.list_sessions()
    session = next((s for s in sessions if s["conversation_id"] == conversation_id), None)
    if not session:
        raise HTTPException(status_code=404, detail=f"Conversation not found: {conversation_id}")

    await l3_client.terminate_session(session["id"])
    persistence.update_status(session["id"], "terminated")
    persistence.log_event(session["id"], "conversation_terminated")

    return {"id": conversation_id, "status": "terminated"}
