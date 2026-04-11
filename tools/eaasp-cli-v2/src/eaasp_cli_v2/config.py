"""CLI endpoint configuration — env-backed defaults targeting localhost 1808x."""

from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class CliConfig:
    """Resolved endpoint + timeout bundle.

    Defaults match the ports chosen in S3.T4.5 (2026-04-12):
    18081 skill-registry, 18083 L3, 18084 L4, 18085 L2.
    """

    skill_url: str = "http://127.0.0.1:18081"
    l3_url: str = "http://127.0.0.1:18083"
    l4_url: str = "http://127.0.0.1:18084"
    l2_url: str = "http://127.0.0.1:18085"
    timeout: float = 10.0

    @classmethod
    def from_env(cls) -> "CliConfig":
        """Build a config from environment, falling back to class defaults."""
        defaults = cls()
        return cls(
            skill_url=os.environ.get("EAASP_SKILL_URL", defaults.skill_url),
            l3_url=os.environ.get("EAASP_L3_URL", defaults.l3_url),
            l4_url=os.environ.get("EAASP_L4_URL", defaults.l4_url),
            l2_url=os.environ.get("EAASP_L2_URL", defaults.l2_url),
            timeout=float(os.environ.get("EAASP_CLI_TIMEOUT", str(defaults.timeout))),
        )
