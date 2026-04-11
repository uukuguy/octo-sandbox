"""CLI tests for `eaasp skill`."""

from __future__ import annotations

import json
from pathlib import Path

import httpx
from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main

from tests.conftest import json_response


def test_list(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8") or "{}")
        return json_response(
            200,
            [{"id": "skill.a", "name": "A", "version": "0.1.0", "status": "draft"}],
        )

    install_mock(handler)
    result = runner.invoke(
        cli_main.app, ["skill", "list", "--query", "auth", "--scope", "team"]
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["path"] == "/tools/skill_search/invoke"
    assert captured["body"]["q"] == "auth"
    assert captured["body"]["scope"] == "team"
    assert "skill.a" in result.stdout


def test_submit(runner: CliRunner, install_mock, tmp_path: Path) -> None:
    skill_path = tmp_path / "SKILL.md"
    skill_path.write_text(
        "---\n"
        "id: skill.hello\n"
        'name: "Hello Skill"\n'
        "description: says hi\n"
        "version: 0.1.0\n"
        "---\n"
        "This is the prose body.\n",
        encoding="utf-8",
    )

    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(
            201,
            {
                "id": "skill.hello",
                "version": "0.1.0",
                "status": "draft",
            },
        )

    install_mock(handler)
    result = runner.invoke(cli_main.app, ["skill", "submit", str(skill_path)])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["path"] == "/tools/skill_submit_draft/invoke"
    body = captured["body"]
    assert body["id"] == "skill.hello"
    assert body["name"] == "Hello Skill"
    assert body["description"] == "says hi"
    assert body["version"] == "0.1.0"
    assert "prose body" in body["prose"]
    assert "id: skill.hello" in body["frontmatter_yaml"]


def test_promote(runner: CliRunner, install_mock) -> None:
    captured: dict = {}

    def handler(req: httpx.Request) -> httpx.Response:
        captured["path"] = req.url.path
        captured["body"] = json.loads(req.content.decode("utf-8"))
        return json_response(200, {"promoted": True})

    install_mock(handler)
    result = runner.invoke(
        cli_main.app, ["skill", "promote", "skill.hello", "0.1.0", "tested"]
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    assert captured["path"] == "/tools/skill_promote/invoke"
    assert captured["body"] == {
        "id": "skill.hello",
        "version": "0.1.0",
        "target_status": "tested",
    }
