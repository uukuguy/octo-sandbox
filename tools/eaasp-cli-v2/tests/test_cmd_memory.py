"""CLI tests for `eaasp memory`."""

from __future__ import annotations

import json

import httpx
from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main

from tests.conftest import json_response


def test_search(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(
            200,
            {
                "hits": [
                    {
                        "memory_id": "mem_1",
                        "scope": "team",
                        "category": "decision",
                        "score": 0.91,
                    }
                ]
            },
        )

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["memory", "search", "authz patterns"])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["path"] == "/api/v1/memory/search"
    assert captured["body"]["query"] == "authz patterns"
    assert captured["body"]["top_k"] == 10
    assert "mem_1" in result.stdout


def test_read(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(200, {"memory_id": "m1", "version": 3, "content": "hi"})

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["memory", "read", "m1"])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["path"] == "/tools/memory_read/invoke"
    assert captured["body"] == {"args": {"memory_id": "m1"}}
    assert "m1" in result.stdout


def test_list(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(200, {"items": [{"memory_id": "m1", "scope": "team"}]})

    install_mock(handler)
    result = runner.invoke(
        cli_main.app, ["memory", "list", "--limit", "5", "--scope", "team"]
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["path"] == "/tools/memory_list/invoke"
    assert captured["body"] == {"args": {"limit": 5, "scope": "team"}}
    assert "m1" in result.stdout
