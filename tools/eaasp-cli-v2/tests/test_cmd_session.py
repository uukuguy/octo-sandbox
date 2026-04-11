"""CLI tests for `eaasp session`."""

from __future__ import annotations

import json

import httpx
from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main

from tests.conftest import json_response


def test_create_happy(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["method"] = req.method
        captured["url"] = str(req.url)
        captured["body"] = json.loads(req.content.decode("utf-8"))
        assert req.url.path == "/v1/sessions/create"
        return json_response(
            200,
            {
                "session_id": "sess_abc123",
                "status": "created",
                "created_at": 1712920000,
            },
        )

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "create", "--skill", "skill.test", "--runtime", "grid-runtime"],
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    assert "sess_abc123" in result.stdout
    assert captured["body"]["skill_id"] == "skill.test"
    assert captured["body"]["runtime_pref"] == "grid-runtime"
    assert captured["body"]["intent_text"] == "run skill skill.test"


def test_create_service_unavailable(runner: CliRunner, install_mock) -> None:
    def handler(_: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError("refused")

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "create", "--skill", "x", "--runtime", "y"],
    )
    assert result.exit_code == 3
    assert "service unavailable" in result.stderr


def test_show(runner: CliRunner, install_mock) -> None:
    def handler(req: httpx.Request) -> httpx.Response:
        path = req.url.path
        if path == "/v1/sessions/sess_xyz":
            return json_response(
                200,
                {
                    "session_id": "sess_xyz",
                    "status": "created",
                    "created_at": 1712920000,
                },
            )
        if path == "/v1/sessions/sess_xyz/events":
            return json_response(
                200,
                {
                    "session_id": "sess_xyz",
                    "events": [
                        {
                            "seq": 1,
                            "event_type": "SESSION_CREATED",
                            "created_at": 1712920000,
                            "payload": {
                                "anchors": ["anchor_1", {"anchor_id": "anchor_2"}]
                            },
                        }
                    ],
                },
            )
        return json_response(404, {"detail": "not routed"})

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["session", "show", "sess_xyz"])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert "sess_xyz" in result.stdout
    assert "SESSION_CREATED" in result.stdout
    assert "anchor_1" in result.stdout
    assert "anchor_2" in result.stdout


def test_send(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["body"] = json.loads(req.content.decode("utf-8"))
        assert req.url.path == "/v1/sessions/sess_xyz/message"
        return json_response(200, {"session_id": "sess_xyz", "seq": 3})

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "send", "sess_xyz", "hello"],
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["body"] == {"content": "hello"}
    assert "sess_xyz" in result.stdout
