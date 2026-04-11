"""Unit tests for CliConfig defaults and env resolution."""

from __future__ import annotations

import pytest

from eaasp_cli_v2.config import CliConfig


def test_defaults() -> None:
    cfg = CliConfig()
    assert cfg.skill_url == "http://127.0.0.1:18081"
    assert cfg.l3_url == "http://127.0.0.1:18083"
    assert cfg.l4_url == "http://127.0.0.1:18084"
    assert cfg.l2_url == "http://127.0.0.1:18085"
    assert cfg.timeout == 10.0


def test_from_env_overrides(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("EAASP_SKILL_URL", "http://skill.example:9001")
    monkeypatch.setenv("EAASP_L3_URL", "http://l3.example:9003")
    monkeypatch.setenv("EAASP_L4_URL", "http://l4.example:9004")
    monkeypatch.setenv("EAASP_L2_URL", "http://l2.example:9005")
    monkeypatch.setenv("EAASP_CLI_TIMEOUT", "2.5")

    cfg = CliConfig.from_env()
    assert cfg.skill_url == "http://skill.example:9001"
    assert cfg.l3_url == "http://l3.example:9003"
    assert cfg.l4_url == "http://l4.example:9004"
    assert cfg.l2_url == "http://l2.example:9005"
    assert cfg.timeout == 2.5
