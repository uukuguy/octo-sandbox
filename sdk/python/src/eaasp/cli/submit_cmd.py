"""eaasp submit — submit a Skill to the L2 Skill Registry."""

from __future__ import annotations

from pathlib import Path

import click

from eaasp.authoring.skill_parser import SkillParser
from eaasp.authoring.skill_validator import SkillValidator


@click.command("submit")
@click.argument("path", type=click.Path(exists=True))
@click.option(
    "--registry",
    required=True,
    help="L2 Skill Registry URL (e.g. http://localhost:8081).",
)
def submit_cmd(path: str, registry: str) -> None:
    """Submit a Skill to the L2 Skill Registry.

    PATH can be a SKILL.md file or a directory containing one.
    The Skill is validated before submission.
    """
    skill_path = _resolve_skill_path(Path(path))
    skill = SkillParser.parse_file(skill_path)

    # Validate first
    result = SkillValidator.validate(skill)
    if not result.valid:
        click.echo("Validation failed — fix errors before submitting:", err=True)
        for e in result.errors:
            click.echo(f"  [{e.rule}] {e.message}", err=True)
        raise SystemExit(1)

    # Submit
    from eaasp.client.skill_registry import SkillRegistryClient

    client = SkillRegistryClient(base_url=registry)
    try:
        resp = client.submit_draft(skill)
        click.echo(f"Submitted: {resp.get('id', 'unknown')}")
        click.echo(f"Status: {resp.get('status', 'draft')}")
    except Exception as exc:
        click.echo(f"Submit error: {exc}", err=True)
        raise SystemExit(1) from exc


def _resolve_skill_path(p: Path) -> Path:
    if p.is_dir():
        candidate = p / "SKILL.md"
        if candidate.exists():
            return candidate
        raise click.BadParameter(f"No SKILL.md found in {p}")
    return p
