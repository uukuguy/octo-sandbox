"""eaasp init — create a Skill project skeleton."""

from __future__ import annotations

from pathlib import Path

import click

from eaasp.authoring.skill_scaffold import SkillScaffold


@click.command("init")
@click.argument("name")
@click.option(
    "--type",
    "skill_type",
    type=click.Choice(["workflow", "production", "domain", "meta"]),
    default="workflow",
    help="Skill template type.",
)
@click.option(
    "--output-dir",
    type=click.Path(file_okay=False, writable=True),
    default=".",
    help="Parent directory for the generated project.",
)
def init_cmd(name: str, skill_type: str, output_dir: str) -> None:
    """Create a new Skill project skeleton.

    NAME is the skill name (used as directory name).
    """
    out = Path(output_dir)
    skill_dir = SkillScaffold.create(name=name, skill_type=skill_type, output_dir=out)
    click.echo(f"Created Skill project: {skill_dir}")
    click.echo(f"  SKILL.md     — edit the frontmatter and prose")
    click.echo(f"  hooks/       — add hook handler scripts")
    click.echo(f"  tests/       — add test cases (JSONL)")
