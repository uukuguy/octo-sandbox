"""eaasp validate — validate a SKILL.md file."""

from __future__ import annotations

from pathlib import Path

import click

from eaasp.authoring.skill_parser import SkillParser
from eaasp.authoring.skill_validator import SkillValidator


@click.command("validate")
@click.argument("path", type=click.Path(exists=True))
def validate_cmd(path: str) -> None:
    """Validate a Skill definition.

    PATH can be a SKILL.md file or a directory containing one.
    """
    skill_path = _resolve_skill_path(Path(path))
    try:
        skill = SkillParser.parse_file(skill_path)
    except Exception as exc:
        click.echo(f"Parse error: {exc}", err=True)
        raise SystemExit(1) from exc

    result = SkillValidator.validate(skill)

    if result.errors:
        click.echo(f"Errors ({len(result.errors)}):")
        for e in result.errors:
            click.echo(f"  [{e.rule}] {e.message}")

    if result.warnings:
        click.echo(f"Warnings ({len(result.warnings)}):")
        for w in result.warnings:
            click.echo(f"  [{w.rule}] {w.message}")

    if result.valid:
        click.echo("Validation passed.")
    else:
        click.echo("Validation failed.", err=True)
        raise SystemExit(1)


def _resolve_skill_path(p: Path) -> Path:
    """If *p* is a directory, look for SKILL.md inside it."""
    if p.is_dir():
        candidate = p / "SKILL.md"
        if candidate.exists():
            return candidate
        raise click.BadParameter(f"No SKILL.md found in {p}")
    return p
