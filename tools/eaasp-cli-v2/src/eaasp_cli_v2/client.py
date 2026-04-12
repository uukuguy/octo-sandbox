"""Async HTTP client wrapper with uniform error projection.

Exit code taxonomy used by ``CliError``:
    2 = client error (4xx)
    3 = service unavailable / connect / timeout
    4 = server error (5xx)
"""

from __future__ import annotations

import json
from collections.abc import AsyncIterator
from typing import Any

import httpx

from .config import CliConfig


class CliError(Exception):
    """Raised by ``ServiceClient.call`` on any non-2xx or transport failure."""

    def __init__(
        self,
        exit_code: int,
        message: str,
        detail: Any | None = None,
    ) -> None:
        self.exit_code = exit_code
        self.message = message
        self.detail = detail
        super().__init__(message)


class ServiceClient:
    """Thin async HTTP wrapper around a single ``httpx.AsyncClient``."""

    def __init__(self, client: httpx.AsyncClient) -> None:
        self._client = client

    # ─── Constructors ──────────────────────────────────────────────────────
    @classmethod
    def from_config(cls, cfg: CliConfig) -> "ServiceClient":
        # trust_env=False — see MEMORY.md "Ollama 本地模型已知问题": macOS system
        # proxies (Clash etc.) route 127.0.0.1 through HTTP proxy and the cli-v2
        # commands surface this as confusing "502 server error" lines.
        return cls(httpx.AsyncClient(timeout=cfg.timeout, trust_env=False))

    @classmethod
    def from_httpx(cls, client: httpx.AsyncClient) -> "ServiceClient":
        """Wrap an externally-owned httpx client — used by tests that install a MockTransport."""
        return cls(client)

    # ─── Lifecycle ─────────────────────────────────────────────────────────
    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "ServiceClient":
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.aclose()

    # ─── Core call ─────────────────────────────────────────────────────────
    async def call(
        self,
        method: str,
        url: str,
        *,
        json: Any = None,
        params: dict[str, Any] | None = None,
    ) -> Any:
        """Execute an HTTP request and project failures into ``CliError``."""
        try:
            response = await self._client.request(method, url, json=json, params=params)
        except (httpx.ConnectError, httpx.TimeoutException) as exc:
            raise CliError(
                3,
                f"service unavailable: {url}",
                {"error": str(exc) or exc.__class__.__name__},
            ) from exc

        if 200 <= response.status_code < 300:
            if not response.content:
                return {}
            try:
                return response.json()
            except ValueError:
                return {"raw": response.text}

        body = _safe_json(response)
        if 400 <= response.status_code < 500:
            raise CliError(
                2,
                f"{response.status_code} client error: {url}",
                body,
            )
        raise CliError(
            4,
            f"{response.status_code} server error: {url}",
            body,
        )

    # ─── SSE streaming call ───────────────────────────────────────────────
    async def stream_sse(
        self,
        url: str,
        *,
        json_body: Any = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """POST to an SSE endpoint and yield parsed ``{event, data}`` dicts.

        Each SSE message is expected as ``event: <type>\\ndata: <json>\\n\\n``.
        """
        try:
            async with self._client.stream(
                "POST", url, json=json_body,
                timeout=httpx.Timeout(connect=10.0, read=300.0, write=10.0, pool=10.0),
            ) as response:
                if response.status_code >= 400:
                    body = await response.aread()
                    try:
                        detail = json.loads(body)
                    except ValueError:
                        detail = {"text": body.decode("utf-8", errors="replace")}
                    exit_code = 2 if response.status_code < 500 else 4
                    raise CliError(
                        exit_code,
                        f"{response.status_code} error: {url}",
                        detail,
                    )

                current_event = "chunk"
                async for line in response.aiter_lines():
                    line = line.rstrip("\r\n")
                    if line.startswith("event: "):
                        current_event = line[7:]
                    elif line.startswith("data: "):
                        raw = line[6:]
                        try:
                            data = json.loads(raw)
                        except ValueError:
                            data = {"raw": raw}
                        yield {"event": current_event, "data": data}
                        current_event = "chunk"
                    # blank lines (SSE separator) are skipped
        except (httpx.ConnectError, httpx.TimeoutException) as exc:
            raise CliError(
                3,
                f"service unavailable: {url}",
                {"error": str(exc) or exc.__class__.__name__},
            ) from exc


def _safe_json(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError:
        return {"text": response.text}
