"""Deferred D41 — `session list` should show a notice and exit 0."""

from __future__ import annotations

from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main


def test_session_list_d41(runner: CliRunner) -> None:
    result = runner.invoke(cli_main.app, ["session", "list"])
    assert result.exit_code == 0, result.stdout + result.stderr
    assert "D41" in result.stdout
    assert "not available in MVP" in result.stdout
