"""eaasp run — execute a Skill via platform (L4→L3→L1) or direct (L1-only).

Usage:
  eaasp run ./path/to/skill/ --platform http://localhost:8084 --mock-llm
  eaasp run ./path/to/skill/ --platform http://localhost:8084 --live-llm --input "..."
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path

import click

from eaasp.authoring.skill_parser import SkillParser


@click.command("run")
@click.argument("skill_path", type=click.Path(exists=True))
@click.option("--platform", default=None, help="L4 platform URL (e.g. http://localhost:8084)")
@click.option("--mock-llm", is_flag=True, help="Use mock LLM (no API key needed)")
@click.option("--live-llm", is_flag=True, help="Use real LLM (needs API key)")
@click.option("--input", "user_input", default=None, help="Initial user message")
@click.option("--user-id", default="sdk-user", help="User ID")
@click.option("--org-unit", default="default", help="Organization unit")
def run_cmd(
    skill_path: str,
    platform: str | None,
    mock_llm: bool,
    live_llm: bool,
    user_input: str | None,
    user_id: str,
    org_unit: str,
) -> None:
    """Run a Skill end-to-end via platform or direct mode."""
    skill_dir = Path(skill_path)
    skill_md = skill_dir / "SKILL.md"

    if not skill_md.exists():
        click.echo(f"Error: SKILL.md not found in {skill_dir}", err=True)
        sys.exit(1)

    # Parse skill
    parser = SkillParser()
    skill = parser.parse(skill_md.read_text())
    click.echo(f"Skill: {skill.frontmatter.name} v{skill.frontmatter.version}")

    if platform:
        asyncio.run(_run_platform(
            platform_url=platform,
            skill_id=skill.frontmatter.name,
            user_id=user_id,
            org_unit=org_unit,
            user_input=user_input or f"执行 {skill.frontmatter.name}",
            mock_llm=mock_llm,
        ))
    else:
        click.echo("Direct mode (no platform) — not yet implemented")
        click.echo("Use --platform to run via L4→L3→L1 pipeline")
        sys.exit(1)


async def _run_platform(
    platform_url: str,
    skill_id: str,
    user_id: str,
    org_unit: str,
    user_input: str,
    mock_llm: bool,
) -> None:
    """Run via platform pipeline: L4 → L3 → L1."""
    from eaasp.client.platform_client import PlatformClient

    client = PlatformClient(base_url=platform_url)

    # 1. Create conversation
    click.echo(f"\n1. Creating conversation (skill={skill_id})...")
    conv = await client.create_conversation(
        user_id=user_id,
        org_unit=org_unit,
        skill_id=skill_id,
    )
    conv_id = conv["conversation_id"]
    click.echo(f"   Conversation: {conv_id}")
    click.echo(f"   Session: {conv['session_id']}")
    click.echo(f"   Runtime: {conv['runtime']}")

    # 2. Send initial message
    click.echo(f"\n2. Sending message: {user_input[:50]}...")
    result = await client.send_message(conv_id, user_input)

    chunks = result.get("chunks", [])
    for chunk in chunks:
        if chunk["chunk_type"] == "text_delta":
            click.echo(f"   Agent: {chunk['content']}")
        elif chunk["chunk_type"] == "done":
            click.echo("   [Done]")

    # 3. Get status
    click.echo("\n3. Checking status...")
    status = await client.get_conversation(conv_id)
    click.echo(f"   Status: {status['status']}")

    # 4. Terminate
    click.echo("\n4. Terminating conversation...")
    term = await client.terminate(conv_id)
    click.echo(f"   Final status: {term['status']}")

    click.echo("\nE2E run complete.")
