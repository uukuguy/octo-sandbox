"""Pytest fixtures — CliRunner + httpx.MockTransport-backed ServiceClient factory."""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any

import httpx
import pytest
from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main
from eaasp_cli_v2.client import ServiceClient

Handler = Callable[[httpx.Request], httpx.Response]


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


@pytest.fixture
def install_mock(
    monkeypatch: pytest.MonkeyPatch,
) -> Callable[[Handler], httpx.AsyncClient]:
    """Install a MockTransport-backed httpx.AsyncClient into the CLI factory slot."""

    def _install(handler: Handler) -> httpx.AsyncClient:
        transport = httpx.MockTransport(handler)
        mock_client = httpx.AsyncClient(transport=transport)
        monkeypatch.setattr(
            cli_main,
            "_client_factory",
            lambda cfg: ServiceClient.from_httpx(mock_client),
        )
        return mock_client

    return _install


def json_response(status: int, body: Any) -> httpx.Response:
    return httpx.Response(
        status,
        content=json.dumps(body).encode("utf-8"),
        headers={"content-type": "application/json"},
    )
