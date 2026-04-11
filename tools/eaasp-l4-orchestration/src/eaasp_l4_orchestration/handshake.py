"""HTTP clients for the three-way handshake (L2 memory + L3 governance).

Each client wraps a shared ``httpx.AsyncClient`` so connection pooling and
timeouts are owned by the FastAPI lifespan. Upstream errors are normalized
into a single ``UpstreamError`` exception so the api.py layer can map them
to HTTP status codes without caring about httpx internals.

Error taxonomy:

- ``unavailable`` — transport level (connect refused, DNS, timeout)
- ``error``       — upstream returned 5xx
- ``no_policy``   — L3 returned 404 from ``validate_session`` (Contract 5 guard)
"""

from __future__ import annotations

import os
from typing import Any

import httpx

L2_URL_DEFAULT = os.environ.get("EAASP_L2_URL", "http://127.0.0.1:8085")
L3_URL_DEFAULT = os.environ.get("EAASP_L3_URL", "http://127.0.0.1:8083")


class UpstreamError(Exception):
    """Normalized error raised by L2/L3 clients."""

    def __init__(self, service: str, kind: str, detail: str = "") -> None:
        self.service = service  # "l2" | "l3"
        self.kind = kind  # "unavailable" | "error" | "no_policy"
        self.detail = detail
        super().__init__(f"{service}:{kind} {detail}".strip())


class L2Client:
    """Thin wrapper around L2 Memory Engine REST surface."""

    def __init__(
        self,
        client: httpx.AsyncClient,
        base_url: str = L2_URL_DEFAULT,
    ) -> None:
        self._client = client
        self._base = base_url.rstrip("/")

    async def search_memory(
        self,
        *,
        query: str,
        top_k: int = 10,
        scope: str | None = None,
        category: str | None = None,
    ) -> list[dict[str, Any]]:
        """Call ``POST /api/v1/memory/search`` and return the ``hits`` list.

        Does NOT enforce top_k clamping here — L2 enforces its own bounds
        (M3 in S3.T2); we pass through the caller's value.
        """
        body: dict[str, Any] = {"query": query, "top_k": top_k}
        if scope is not None:
            body["scope"] = scope
        if category is not None:
            body["category"] = category

        try:
            resp = await self._client.post(
                f"{self._base}/api/v1/memory/search", json=body
            )
        except (httpx.ConnectError, httpx.TimeoutException) as exc:
            raise UpstreamError("l2", "unavailable", str(exc)) from exc
        except httpx.HTTPError as exc:
            # Catch-all for any other transport-level issue.
            raise UpstreamError("l2", "unavailable", str(exc)) from exc

        if resp.status_code >= 500:
            raise UpstreamError("l2", "error", resp.text)
        if resp.status_code >= 400:
            # L2 should rarely 4xx us; surface as error for visibility.
            raise UpstreamError("l2", "error", resp.text)

        data = resp.json()
        hits = data.get("hits", [])
        if not isinstance(hits, list):
            return []
        return hits


class L3Client:
    """Thin wrapper around L3 Governance REST surface."""

    def __init__(
        self,
        client: httpx.AsyncClient,
        base_url: str = L3_URL_DEFAULT,
    ) -> None:
        self._client = client
        self._base = base_url.rstrip("/")

    async def validate_session(
        self,
        *,
        session_id: str,
        skill_id: str | None,
        runtime_tier: str | None,
        agent_id: str | None = None,
    ) -> dict[str, Any]:
        """Call ``POST /v1/sessions/{session_id}/validate``.

        Returns the raw L3 response dict:
        ``{session_id, hooks_to_attach, managed_settings_version,
          validated_at, runtime_tier}``.
        """
        body: dict[str, Any] = {
            "skill_id": skill_id,
            "runtime_tier": runtime_tier,
            "agent_id": agent_id,
        }

        try:
            resp = await self._client.post(
                f"{self._base}/v1/sessions/{session_id}/validate", json=body
            )
        except (httpx.ConnectError, httpx.TimeoutException) as exc:
            raise UpstreamError("l3", "unavailable", str(exc)) from exc
        except httpx.HTTPError as exc:
            raise UpstreamError("l3", "unavailable", str(exc)) from exc

        if resp.status_code == 404:
            # L3 emits 404 {code:"no_policy"} when no managed-settings version
            # exists yet. Preserve the distinction so api.py can map to 424.
            raise UpstreamError("l3", "no_policy", resp.text)
        if resp.status_code >= 500:
            raise UpstreamError("l3", "error", resp.text)
        if resp.status_code >= 400:
            raise UpstreamError("l3", "error", resp.text)

        data = resp.json()
        if not isinstance(data, dict):
            raise UpstreamError("l3", "error", "unexpected response shape")
        return data
