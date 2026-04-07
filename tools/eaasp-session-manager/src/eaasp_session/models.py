"""L4 Pydantic models for conversations and sessions."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field


class CreateConversationRequest(BaseModel):
    user_id: str
    org_unit: str = ""
    input: str | None = None
    skill_id: str | None = None


class SendMessageRequest(BaseModel):
    content: str


class ConversationResponse(BaseModel):
    conversation_id: str
    session_id: str
    skill_name: str = ""
    runtime: str = ""
    status: str = "creating"


class SessionInfo(BaseModel):
    id: str
    user_id: str = ""
    skill_id: str = ""
    runtime_id: str = ""
    status: str = "creating"
    duration_ms: int = 0


class TelemetrySummary(BaseModel):
    tools_called: int = 0
    hooks_fired: int = 0
    tokens_used: int = 0
    duration_ms: int = 0
