"""`eaasp skill` — Skill registry (Rust axum service) commands.

Important: unlike L2, the skill registry's ``/tools/*/invoke`` endpoints take the
request body directly (no ``{"args": {...}}`` wrapper). See `routes.rs` in
`tools/eaasp-skill-registry/src/`.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any, Optional

import typer

from . import main as _main
from .config import CliConfig
from .output import print_json, print_table

app = typer.Typer(help="Skill registry commands")

_FRONTMATTER_RE = re.compile(
    r"\A---\s*\n(?P<yaml>.*?)\n---\s*\n?(?P<prose>.*)\Z",
    re.DOTALL,
)


@app.command("list")
def list_cmd(
    query: str = typer.Option("", "--query", "-q"),
    scope: Optional[str] = typer.Option(None, "--scope"),
) -> None:
    """Search skills via the MCP-style tool facade."""
    cfg = CliConfig.from_env()
    body: dict[str, Any] = {}
    if query:
        body["q"] = query
    if scope:
        body["scope"] = scope

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST",
                f"{cfg.skill_url}/tools/skill_search/invoke",
                json=body,
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    rows: list[Any] = result if isinstance(result, list) else (
        result.get("results", []) if isinstance(result, dict) else []
    )
    print_table(
        "Skills",
        rows,
        ["id", "name", "version", "status"],
    )


@app.command("submit")
def submit(
    path: Path = typer.Argument(..., exists=True, readable=True),
) -> None:
    """Submit a skill from a directory (containing SKILL.md) or a SKILL.md file.

    Examples:

        eaasp skill submit examples/skills/threshold-calibration/
        eaasp skill submit examples/skills/threshold-calibration/SKILL.md
    """
    cfg = CliConfig.from_env()

    # Accept both directory and file path.
    if path.is_dir():
        skill_dir = path.resolve()
        skill_file = skill_dir / "SKILL.md"
        if not skill_file.exists():
            typer.echo(f"ERROR: {skill_dir}/SKILL.md not found.", err=True)
            raise typer.Exit(1)
    else:
        skill_file = path.resolve()
        skill_dir = skill_file.parent

    content = skill_file.read_text(encoding="utf-8")
    frontmatter_yaml, prose = _split_frontmatter(content)
    meta = _parse_simple_yaml(frontmatter_yaml)

    body: dict[str, Any] = {
        "id": str(meta.get("id") or meta.get("name") or skill_dir.name),
        "name": str(meta.get("name") or skill_dir.name),
        "description": str(meta.get("description") or ""),
        "version": str(meta.get("version") or "0.1.0"),
        "frontmatter_yaml": frontmatter_yaml,
        "prose": prose,
        "source_dir": str(skill_dir),
    }
    if "author" in meta:
        body["author"] = str(meta["author"])

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST",
                f"{cfg.skill_url}/tools/skill_submit_draft/invoke",
                json=body,
            )
        finally:
            await client.aclose()

    print_json(_main.run_async(_do()))


@app.command("promote")
def promote(
    skill_id: str = typer.Argument(..., metavar="SKILL_ID"),
    version: str = typer.Argument(..., metavar="VERSION"),
    target_status: str = typer.Argument(
        ..., metavar="STATUS", help="draft | tested | reviewed | production"
    ),
) -> None:
    """Promote a skill version to a new lifecycle status."""
    cfg = CliConfig.from_env()
    body = {"id": skill_id, "version": version, "target_status": target_status}

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST",
                f"{cfg.skill_url}/tools/skill_promote/invoke",
                json=body,
            )
        finally:
            await client.aclose()

    print_json(_main.run_async(_do()))


@app.command("run")
def run(skill_id: str = typer.Argument(...)) -> None:
    """Sugar: ``session create --skill <id>`` + ``session send <sid> 'run skill'``."""
    cfg = CliConfig.from_env()

    async def _do() -> tuple[Any, Any]:
        client = _main.make_client(cfg)
        try:
            created = await client.call(
                "POST",
                f"{cfg.l4_url}/v1/sessions/create",
                json={
                    "intent_text": f"run skill {skill_id}",
                    "skill_id": skill_id,
                    "runtime_pref": "grid-runtime",
                },
            )
            sid = created["session_id"] if isinstance(created, dict) else ""
            sent = await client.call(
                "POST",
                f"{cfg.l4_url}/v1/sessions/{sid}/message",
                json={"content": "run skill"},
            )
            return created, sent
        finally:
            await client.aclose()

    created, sent = _main.run_async(_do())
    row = created if isinstance(created, dict) else {"value": created}
    print_table("Session created", [row], ["session_id", "status", "created_at"])
    print_json(sent)


# ─── Helpers ───────────────────────────────────────────────────────────────


def _split_frontmatter(text: str) -> tuple[str, str]:
    """Split a SKILL.md into ``(frontmatter_yaml, prose)``.

    If no frontmatter block is detected, the entire file is treated as prose
    and the yaml block is empty.
    """
    match = _FRONTMATTER_RE.match(text)
    if match is None:
        return "", text
    return match.group("yaml"), match.group("prose")


def _parse_simple_yaml(yaml_text: str) -> dict[str, str]:
    """Parse ``key: value`` lines — deliberately minimal, no external yaml dep."""
    out: dict[str, str] = {}
    for raw_line in yaml_text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            continue
        key, _, value = line.partition(":")
        out[key.strip()] = value.strip().strip('"').strip("'")
    return out
