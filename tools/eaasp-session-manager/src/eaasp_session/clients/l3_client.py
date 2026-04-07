"""L3 Governance HTTP client for L4.

L4 communicates with L1 exclusively through L3 (KD-BH6).
"""

from __future__ import annotations

import logging

import httpx

logger = logging.getLogger(__name__)


class L3GovernanceClient:
    """HTTP client for L3 Governance Service (:8083)."""

    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")

    async def create_session(
        self,
        user_id: str,
        user_role: str,
        org_unit: str,
        skill_id: str,
        runtime_preference: str | None = None,
    ) -> dict:
        """POST /v1/sessions — create a governed session via L3."""
        body = {
            "user_id": user_id,
            "user_role": user_role,
            "org_unit": org_unit,
            "skill_id": skill_id,
        }
        if runtime_preference:
            body["runtime_preference"] = runtime_preference

        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{self.base_url}/v1/sessions",
                json=body,
                timeout=30.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def send_message(self, session_id: str, content: str) -> dict:
        """POST /v1/sessions/{id}/message — forward message via L3."""
        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{self.base_url}/v1/sessions/{session_id}/message",
                json={"content": content},
                timeout=60.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def get_session(self, session_id: str) -> dict:
        """GET /v1/sessions/{id}."""
        async with httpx.AsyncClient() as client:
            resp = await client.get(
                f"{self.base_url}/v1/sessions/{session_id}",
                timeout=10.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def terminate_session(self, session_id: str) -> dict:
        """DELETE /v1/sessions/{id}."""
        async with httpx.AsyncClient() as client:
            resp = await client.delete(
                f"{self.base_url}/v1/sessions/{session_id}",
                timeout=10.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def resolve_intent(self, text: str, user_id: str, org_unit: str) -> dict:
        """POST /v1/intents — resolve user text to skill."""
        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{self.base_url}/v1/intents",
                json={"text": text, "user_id": user_id, "org_unit": org_unit},
                timeout=10.0,
            )
            resp.raise_for_status()
            return resp.json()
