"""`eaasp policy` — L3 governance policy management."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import typer

from . import main as _main
from .config import CliConfig
from .output import print_json

app = typer.Typer(help="L3 policy management")


@app.command("deploy")
def deploy(
    config_path: Path = typer.Argument(
        ..., exists=True, dir_okay=False, readable=True,
        metavar="CONFIG_JSON",
    ),
) -> None:
    """Deploy a managed-settings JSON file (PUT /v1/policies/managed-hooks)."""
    cfg = CliConfig.from_env()
    body = json.loads(config_path.read_text(encoding="utf-8"))

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "PUT",
                f"{cfg.l3_url}/v1/policies/managed-hooks",
                json=body,
            )
        finally:
            await client.aclose()

    print_json(_main.run_async(_do()))


@app.command("mode")
def mode(
    hook_id: str = typer.Argument(...),
    mode: str = typer.Argument(..., help="enforce | shadow"),
) -> None:
    """Flip a hook between enforce and shadow mode."""
    cfg = CliConfig.from_env()

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "PUT",
                f"{cfg.l3_url}/v1/policies/{hook_id}/mode",
                json={"mode": mode},
            )
        finally:
            await client.aclose()

    print_json(_main.run_async(_do()))
