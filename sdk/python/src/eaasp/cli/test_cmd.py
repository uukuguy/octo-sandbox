"""eaasp test — run a Skill in a sandbox or compare across runtimes."""

from __future__ import annotations

import asyncio
from pathlib import Path

import click

from eaasp.authoring.skill_parser import SkillParser
from eaasp.models.message import UserMessage
from eaasp.models.session import SessionConfig
from eaasp.sandbox.base import SandboxError


@click.command("test")
@click.argument("path", type=click.Path(exists=True))
@click.option(
    "--sandbox",
    default="local",
    help="Sandbox backend: 'local' (grid-cli) or 'grpc://host:port'.",
)
@click.option(
    "--compare",
    default=None,
    help="Comma-separated endpoints for multi-runtime comparison (e.g. grpc://a:50051,grpc://b:50052).",
)
@click.option("--input", "input_text", default=None, help="User message to send.")
def test_cmd(path: str, sandbox: str, compare: str | None, input_text: str | None) -> None:
    """Test a Skill in a sandbox environment.

    PATH can be a SKILL.md file or a directory containing one.
    """
    skill_path = _resolve_skill_path(Path(path))
    skill = SkillParser.parse_file(skill_path)

    if compare:
        endpoints = [e.strip() for e in compare.split(",") if e.strip()]
        asyncio.run(_run_compare(skill, endpoints, input_text))
    else:
        asyncio.run(_run_single(skill, sandbox, input_text))


async def _run_single(skill, sandbox_spec: str, input_text: str | None) -> None:
    adapter = _make_adapter(sandbox_spec)
    try:
        session_id = await adapter.initialize(skill, SessionConfig())
        click.echo(f"Session started: {session_id}")

        if input_text:
            msg = UserMessage(role="user", content=input_text)
            async for chunk in adapter.send(msg):
                if chunk.text:
                    click.echo(chunk.text, nl=False)
            click.echo()

        summary = await adapter.terminate()
        click.echo(f"Completed: {summary.total_turns} turns, {len(summary.tools_called)} tools")
    except SandboxError as exc:
        click.echo(f"Sandbox error: {exc}", err=True)
        raise SystemExit(1) from exc


async def _run_compare(skill, endpoints: list[str], input_text: str | None) -> None:
    from eaasp.sandbox.multi_runtime import MultiRuntimeSandbox

    multi = MultiRuntimeSandbox(endpoints=endpoints)
    msg = UserMessage(role="user", content=input_text or "Hello")
    try:
        result = await multi.compare(
            config=SessionConfig(),
            skill=skill,
            message=msg,
        )
        click.echo(f"Comparison across {len(endpoints)} runtimes:")
        click.echo(f"  All completed: {result.consistency.all_completed}")
        if result.consistency.tools_diff:
            click.echo(f"  Tool differences: {result.consistency.tools_diff}")
        if result.consistency.hooks_diff:
            click.echo(f"  Hook differences: {result.consistency.hooks_diff}")
        click.echo(f"  Output similarity: {result.consistency.output_similarity:.2f}")
    except SandboxError as exc:
        click.echo(f"Comparison error: {exc}", err=True)
        raise SystemExit(1) from exc


def _make_adapter(spec: str):
    """Create a sandbox adapter from a spec string."""
    if spec == "local":
        from eaasp.sandbox.grid_cli import GridCliSandbox
        return GridCliSandbox()
    if spec.startswith("grpc://"):
        from eaasp.sandbox.runtime import RuntimeSandbox
        return RuntimeSandbox(endpoint=spec)
    raise click.BadParameter(f"Unknown sandbox spec: {spec!r}. Use 'local' or 'grpc://host:port'.")


def _resolve_skill_path(p: Path) -> Path:
    if p.is_dir():
        candidate = p / "SKILL.md"
        if candidate.exists():
            return candidate
        raise click.BadParameter(f"No SKILL.md found in {p}")
    return p
