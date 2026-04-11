"""CLI tests for `eaasp policy`."""

from __future__ import annotations

import json
from pathlib import Path

import httpx
from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main

from tests.conftest import json_response


def test_deploy_happy(runner: CliRunner, install_mock, tmp_path: Path) -> None:
    cfg_path = tmp_path / "managed.json"
    payload = {
        "version": 1,
        "hooks": [
            {"hook_id": "h1", "mode": "enforce", "match": {"all": True}},
        ],
    }
    cfg_path.write_text(json.dumps(payload), encoding="utf-8")

    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["method"] = req.method
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(200, {"version": 1, "created_at": "2026-04-12"})

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["policy", "deploy", str(cfg_path)])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["method"] == "PUT"
    assert captured["path"] == "/v1/policies/managed-hooks"
    assert captured["body"] == payload


def test_deploy_422(runner: CliRunner, install_mock, tmp_path: Path) -> None:
    cfg_path = tmp_path / "bad.json"
    cfg_path.write_text(json.dumps({"hooks": "not a list"}), encoding="utf-8")

    def handler(_: httpx.Request) -> httpx.Response:
        return json_response(
            422,
            {"detail": [{"loc": ["hooks"], "msg": "must be a list", "type": "type_error"}]},
        )

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["policy", "deploy", str(cfg_path)])
    assert result.exit_code == 2
    assert "client error" in result.stderr


def test_mode(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["method"] = req.method
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(200, {"hook_id": "h1", "mode": "shadow"})

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["policy", "mode", "h1", "shadow"])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["method"] == "PUT"
    assert captured["path"] == "/v1/policies/h1/mode"
    assert captured["body"] == {"mode": "shadow"}
