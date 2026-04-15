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


def test_send_no_stream(runner: CliRunner, install_mock) -> None:
    """--no-stream should use the legacy non-streaming path."""
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["body"] = json.loads(req.content.decode("utf-8"))
        assert req.url.path == "/v1/sessions/sess_xyz/message"
        return json_response(200, {"session_id": "sess_xyz", "seq": 3})

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "send", "--no-stream", "sess_xyz", "hello"],
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["body"] == {"content": "hello"}
    assert "sess_xyz" in result.stdout


def test_send_stream_default(runner: CliRunner, install_mock) -> None:
    """Default send (--stream) should hit /message/stream and display chunks."""
    sse_body = (
        "event: chunk\n"
        'data: {"chunk_type": "text_delta", "content": "Hello "}\n\n'
        "event: chunk\n"
        'data: {"chunk_type": "text_delta", "content": "world"}\n\n'
        "event: chunk\n"
        'data: {"chunk_type": "done", "content": ""}\n\n'
        "event: done\n"
        'data: {"session_id": "sess_xyz", "response_text": "Hello world", "events": [{"seq": 1}]}\n\n'
    )

    def handler(req: httpx.Request) -> httpx.Response:
        assert req.url.path == "/v1/sessions/sess_xyz/message/stream"
        return httpx.Response(
            200,
            content=sse_body.encode("utf-8"),
            headers={"content-type": "text/event-stream"},
        )

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "send", "sess_xyz", "hello stream"],
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    # Should contain the streamed text.
    assert "Hello " in result.output
    assert "world" in result.output


# ── close command (D89 — S4.T1) ──────────────────────────────────────────


def test_close_happy(runner: CliRunner, install_mock) -> None:
    """`session close` POSTs to /v1/sessions/{id}/close and renders status."""
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["method"] = req.method
        captured["url_path"] = req.url.path
        return json_response(
            200,
            {"session_id": "sess_close1", "status": "closed"},
        )

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["session", "close", "sess_close1"])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["method"] == "POST"
    assert captured["url_path"] == "/v1/sessions/sess_close1/close"
    assert "sess_close1" in result.stdout
    assert "closed" in result.stdout


def test_close_not_found(runner: CliRunner, install_mock) -> None:
    """404 from L4 → exit code 2 (4xx client error per ServiceClient taxonomy)."""

    def handler(_: httpx.Request) -> httpx.Response:
        return json_response(
            404,
            {"detail": {"code": "session_not_found", "session_id": "sess_missing"}},
        )

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["session", "close", "sess_missing"])
    assert result.exit_code == 2
    assert "404 client error" in result.stderr


def test_close_invalid_state(runner: CliRunner, install_mock) -> None:
    """409 (already-closed / invalid transition) → exit code 2 with detail surfaced."""

    def handler(_: httpx.Request) -> httpx.Response:
        return json_response(
            409,
            {
                "detail": {
                    "code": "invalid_state_transition",
                    "session_id": "sess_done",
                    "current": "closed",
                    "target": "closed",
                }
            },
        )

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["session", "close", "sess_done"])
    assert result.exit_code == 2
    assert "409 client error" in result.stderr
