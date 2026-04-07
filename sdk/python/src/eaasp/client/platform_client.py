"""L4 Platform HTTP client for SDK `eaasp run` command.

Communicates with the L4 Session Manager (:8084) to:
  - Create conversations
  - Send messages
  - Retrieve conversation status
  - Terminate conversations
"""

from __future__ import annotations

import logging

import httpx

logger = logging.getLogger(__name__)


class PlatformClient:
    """HTTP client for L4 Session Manager (§6)."""

    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")

    async def create_conversation(
        self,
        user_id: str,
        org_unit: str = "",
        skill_id: str | None = None,
        input_text: str | None = None,
    ) -> dict:
        """POST /v1/conversations — start a new conversation."""
        body: dict = {"user_id": user_id, "org_unit": org_unit}
        if skill_id:
            body["skill_id"] = skill_id
        if input_text:
            body["input"] = input_text

        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{self.base_url}/v1/conversations",
                json=body,
                timeout=30.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def send_message(self, conversation_id: str, content: str) -> dict:
        """POST /v1/conversations/{id}/message."""
        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{self.base_url}/v1/conversations/{conversation_id}/message",
                json={"content": content},
                timeout=60.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def get_conversation(self, conversation_id: str) -> dict:
        """GET /v1/conversations/{id}."""
        async with httpx.AsyncClient() as client:
            resp = await client.get(
                f"{self.base_url}/v1/conversations/{conversation_id}",
                timeout=10.0,
            )
            resp.raise_for_status()
            return resp.json()

    async def terminate(self, conversation_id: str) -> dict:
        """DELETE /v1/conversations/{id}."""
        async with httpx.AsyncClient() as client:
            resp = await client.delete(
                f"{self.base_url}/v1/conversations/{conversation_id}",
                timeout=10.0,
            )
            resp.raise_for_status()
            return resp.json()
