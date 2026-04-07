"""契约 5: 会话控制 API (§8.5).

POST   /v1/sessions            — create session (three-way handshake)
GET    /v1/sessions/{id}       — get session status
POST   /v1/sessions/{id}/message — send message to session
DELETE /v1/sessions/{id}       — terminate session
"""

from __future__ import annotations

import hashlib
import json

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel

from eaasp_governance.merger import merge_by_scope
from eaasp_governance.session_state import GovernanceSession, SessionStatus

router = APIRouter(prefix="/v1/sessions", tags=["sessions"])


class CreateSessionRequest(BaseModel):
    user_id: str
    user_role: str = "user"
    org_unit: str = ""
    skill_id: str = ""
    runtime_preference: str | None = None


class SendMessageRequest(BaseModel):
    content: str
    message_type: str = "text"


@router.post("")
async def create_session(req: CreateSessionRequest, request: Request):
    """Three-way handshake: L3 → L2 (skill) → compile policies → L1 (initialize).

    Returns session_id, runtime_id, governance_summary.
    """
    l2_client = request.app.state.l2_client
    l1_pool = request.app.state.runtime_pool
    policy_store = request.app.state.policy_store
    sessions = request.app.state.sessions

    # 1. Fetch skill from L2
    skill = await l2_client.get_skill_content(req.skill_id)
    if not skill:
        raise HTTPException(status_code=404, detail=f"Skill not found: {req.skill_id}")

    # 2. Compile & merge policies (policy_store values are version lists)
    managed_hooks_list = []
    for versions in policy_store.values():
        if versions:
            current = versions[-1]  # latest version
            managed_hooks_list.append(current.get("compiled_hooks_json", "{}"))

    # Merge all managed-scope hooks
    managed_merged = merge_by_scope(
        managed=managed_hooks_list[0] if managed_hooks_list else None,
        skill=managed_hooks_list[1] if len(managed_hooks_list) > 1 else None,
    )
    merged_data = json.loads(managed_merged)
    hooks_count = len(merged_data.get("rules", []))

    # 3. Select runtime
    runtime = l1_pool.select(preferred=req.runtime_preference)
    if not runtime:
        raise HTTPException(status_code=503, detail="No healthy runtime available")

    # 4. Initialize L1 session
    l1_client = request.app.state.l1_clients.get(runtime.id)
    if not l1_client:
        from eaasp_governance.clients.l1_runtime import L1RuntimeClient

        l1_client = L1RuntimeClient(endpoint=runtime.endpoint)
        request.app.state.l1_clients[runtime.id] = l1_client

    session_id = await l1_client.initialize(
        user_id=req.user_id,
        org_unit=req.org_unit,
        managed_hooks_json=managed_merged,
    )

    # 5. Load skill into L1
    await l1_client.load_skill(
        session_id=session_id,
        skill_id=skill.skill_id,
        frontmatter_yaml=skill.frontmatter_yaml,
        prose=skill.prose,
    )

    # 6. Create governance session record
    digest = hashlib.sha256(managed_merged.encode()).hexdigest()[:16]
    gov_session = GovernanceSession(
        session_id=session_id,
        user_id=req.user_id,
        org_unit=req.org_unit,
        skill_id=req.skill_id,
        runtime_id=runtime.id,
        runtime_endpoint=runtime.endpoint,
        managed_hooks_digest=digest,
        hooks_count=hooks_count,
    )
    gov_session.transition(SessionStatus.ACTIVE)
    sessions[session_id] = gov_session

    return {
        "session_id": session_id,
        "runtime_id": runtime.id,
        "runtime_endpoint": runtime.endpoint,
        "governance_summary": {
            "hooks_count": hooks_count,
            "scope_chain": ["managed", "skill"],
            "managed_hooks_digest": digest,
        },
    }


@router.get("/{session_id}")
async def get_session(session_id: str, request: Request):
    """Get session status."""
    sessions = request.app.state.sessions
    session = sessions.get(session_id)
    if not session:
        raise HTTPException(status_code=404, detail=f"Session not found: {session_id}")
    return session.to_dict()


@router.post("/{session_id}/message")
async def send_message(session_id: str, req: SendMessageRequest, request: Request):
    """Forward message to L1 runtime."""
    sessions = request.app.state.sessions
    session = sessions.get(session_id)
    if not session:
        raise HTTPException(status_code=404, detail=f"Session not found: {session_id}")

    if session.status != SessionStatus.ACTIVE:
        raise HTTPException(status_code=400, detail=f"Session not active: {session.status.value}")

    l1_client = request.app.state.l1_clients.get(session.runtime_id)
    if not l1_client:
        raise HTTPException(status_code=503, detail="Runtime client not available")

    chunks = await l1_client.send(session_id, req.content)
    return {"chunks": chunks}


@router.delete("/{session_id}")
async def terminate_session(session_id: str, request: Request):
    """Terminate a session and clean up."""
    sessions = request.app.state.sessions
    session = sessions.get(session_id)
    if not session:
        raise HTTPException(status_code=404, detail=f"Session not found: {session_id}")

    l1_client = request.app.state.l1_clients.get(session.runtime_id)
    if l1_client:
        await l1_client.terminate(session_id)

    session.transition(SessionStatus.TERMINATING)
    session.transition(SessionStatus.TERMINATED)

    return {
        "session_id": session_id,
        "status": "terminated",
    }
