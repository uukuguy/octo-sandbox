"""W2.T2 OpenAI-compatible provider.

Strict OAI subset — NOT OpenRouter-specific.

Per design §3.4 of docs/plans/2026-04-16-v2-phase2_5-plan.md, this provider
sends ONLY the fields defined by OpenAI's /v1/chat/completions contract.
Vendor-specific routing headers and payload fields are deliberately banned:

- NO ``HTTP-Referer`` header (OpenRouter attribution)
- NO ``X-Title`` header (OpenRouter attribution)
- NO ``provider`` payload field (OpenRouter routing)

Upstream endpoints that require such fields should either have them injected
upstream by a proxy or configured out-of-band. This keeps the runtime portable
across any OAI-compatible server (vLLM, llama.cpp, Together, Groq, etc.).

Implementation notes:
- ``httpx.AsyncClient`` is constructed with ``trust_env=False`` per the macOS
  Clash/proxy precedent recorded in MEMORY — ``HTTPS_PROXY`` env vars from
  user shells must not silently redirect traffic from a library runtime.
- ``aclose()`` must be awaited by the caller to release the connection pool.
- ``chat()`` raises ``httpx.HTTPStatusError`` on non-2xx responses (fail fast;
  no retries/circuit breakers at this layer — that is T3+ territory).
"""
from __future__ import annotations

from typing import Any

import httpx


class OpenAICompatProvider:
    """Thin async wrapper over ``POST /v1/chat/completions``.

    Args:
        base_url: Endpoint root. Accepts both ``https://api.openai.com`` and
            ``https://api.openai.com/v1`` forms — trailing ``/v1`` and slashes
            are normalized away. ``/v1/chat/completions`` is always appended.
        api_key: Bearer token sent via ``Authorization`` header.
        model: Model identifier included in every request body.
        timeout_s: Per-request timeout, passed to ``httpx.AsyncClient``.
    """

    def __init__(
        self,
        base_url: str,
        api_key: str,
        model: str,
        timeout_s: float = 60.0,
    ) -> None:
        # Normalize: strip trailing slash, then strip trailing /v1 so users
        # can pass either "https://host" or "https://host/v1" (OpenRouter's
        # OPENAI_BASE_URL ships with /v1 suffix).
        root = base_url.rstrip("/")
        if root.endswith("/v1"):
            root = root[: -len("/v1")]
        self.base_url = root
        self.api_key = api_key
        self.model = model
        self.timeout_s = timeout_s
        # trust_env=False — MEMORY.md: macOS proxy precedent
        self.client = httpx.AsyncClient(timeout=timeout_s, trust_env=False)

    async def chat(
        self,
        messages: list[dict[str, Any]],
        tools: list[dict[str, Any]] | None = None,
    ) -> dict[str, Any]:
        """Send a single non-streaming chat completion request.

        Args:
            messages: OAI-shaped message list (role/content/tool_calls/...).
            tools: Optional OAI function/tool schema list. If ``None`` or
                empty, the ``tools`` key is omitted from the payload.

        Returns:
            The decoded JSON body of the response.

        Raises:
            httpx.HTTPStatusError: on non-2xx HTTP responses.
        """
        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
        }
        # NO HTTP-Referer, NO X-Title, NO provider routing — strict OAI subset
        payload: dict[str, Any] = {"model": self.model, "messages": messages}
        if tools:
            payload["tools"] = tools
        resp = await self.client.post(
            f"{self.base_url}/v1/chat/completions",
            headers=headers,
            json=payload,
        )
        resp.raise_for_status()
        return resp.json()

    async def aclose(self) -> None:
        """Release the underlying connection pool. Idempotent-safe."""
        await self.client.aclose()
