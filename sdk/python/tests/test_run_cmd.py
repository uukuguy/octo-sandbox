"""Tests for eaasp run command and PlatformClient — 8 tests."""

from __future__ import annotations

from pathlib import Path

import pytest
from click.testing import CliRunner

from eaasp.cli.run_cmd import run_cmd
from eaasp.client.platform_client import PlatformClient

HR_SKILL_DIR = Path(__file__).resolve().parents[2] / "examples" / "hr-onboarding"


# ── Test 1: PlatformClient URL construction ─────────────────

def test_platform_client_url_trailing_slash():
    """PlatformClient strips trailing slash from base_url."""
    client = PlatformClient(base_url="http://localhost:8084/")
    assert client.base_url == "http://localhost:8084"


# ── Test 2: PlatformClient URL no trailing slash ────────────

def test_platform_client_url_clean():
    """PlatformClient keeps clean URL unchanged."""
    client = PlatformClient(base_url="http://localhost:8084")
    assert client.base_url == "http://localhost:8084"


# ── Test 3: PlatformClient is importable ────────────────────

def test_platform_client_methods():
    """PlatformClient has all required methods."""
    client = PlatformClient(base_url="http://mock:8084")
    assert hasattr(client, "create_conversation")
    assert hasattr(client, "send_message")
    assert hasattr(client, "get_conversation")
    assert hasattr(client, "terminate")
    assert callable(client.create_conversation)
    assert callable(client.send_message)


# ── Test 4: run_cmd missing SKILL.md ────────────────────────

def test_run_missing_skill_md(tmp_path):
    """run_cmd fails with clear error when SKILL.md is missing."""
    runner = CliRunner()
    result = runner.invoke(run_cmd, [str(tmp_path), "--platform", "http://localhost:8084"])
    assert result.exit_code != 0
    assert "SKILL.md not found" in result.output


# ── Test 5: run_cmd without --platform ──────────────────────

def test_run_no_platform():
    """run_cmd without --platform shows usage hint."""
    if not HR_SKILL_DIR.exists():
        pytest.skip("HR skill example not found")
    runner = CliRunner()
    result = runner.invoke(run_cmd, [str(HR_SKILL_DIR)])
    assert result.exit_code != 0
    assert "Direct mode" in result.output or "not yet implemented" in result.output


# ── Test 6: run_cmd shows skill name ────────────────────────

def test_run_shows_skill_name():
    """run_cmd parses and displays skill name before connecting."""
    if not HR_SKILL_DIR.exists():
        pytest.skip("HR skill example not found")
    runner = CliRunner()
    # Without platform, it will fail but should still show skill name
    result = runner.invoke(run_cmd, [str(HR_SKILL_DIR)])
    assert "hr-onboarding" in result.output


# ── Test 7: run_cmd has --mock-llm flag ─────────────────────

def test_run_cmd_flags():
    """run_cmd supports --mock-llm and --live-llm flags."""
    if not HR_SKILL_DIR.exists():
        pytest.skip("HR skill example not found")
    runner = CliRunner()
    # --mock-llm without --platform should still fail gracefully
    result = runner.invoke(run_cmd, [str(HR_SKILL_DIR), "--mock-llm"])
    assert result.exit_code != 0
    assert "hr-onboarding" in result.output


# ── Test 8: CLI registration ───────────────────────────────

def test_run_cmd_registered():
    """run command is registered in the CLI group."""
    from eaasp.cli.__main__ import main

    commands = main.list_commands(ctx=None)
    assert "run" in commands
