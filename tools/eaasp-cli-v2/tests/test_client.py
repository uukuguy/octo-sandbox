"""Unit tests for ServiceClient error projection."""

from __future__ import annotations

import httpx
import pytest

from eaasp_cli_v2.client import CliError, ServiceClient

from tests.conftest import json_response


async def test_call_happy() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "GET"
        return json_response(200, {"ok": True})

    transport = httpx.MockTransport(handler)
    async with httpx.AsyncClient(transport=transport) as http_client:
        client = ServiceClient.from_httpx(http_client)
        result = await client.call("GET", "http://example.test/ping")
        assert result == {"ok": True}


async def test_call_404_raises_client_error() -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        return json_response(404, {"detail": "nope"})

    transport = httpx.MockTransport(handler)
    async with httpx.AsyncClient(transport=transport) as http_client:
        client = ServiceClient.from_httpx(http_client)
        with pytest.raises(CliError) as excinfo:
            await client.call("GET", "http://example.test/missing")
        assert excinfo.value.exit_code == 2
        assert "404" in excinfo.value.message


async def test_call_connect_error_raises_service_unavailable() -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError("connection refused")

    transport = httpx.MockTransport(handler)
    async with httpx.AsyncClient(transport=transport) as http_client:
        client = ServiceClient.from_httpx(http_client)
        with pytest.raises(CliError) as excinfo:
            await client.call("GET", "http://example.test/dead")
        assert excinfo.value.exit_code == 3
        assert "service unavailable" in excinfo.value.message
