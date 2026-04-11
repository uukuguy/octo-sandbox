"""`eaasp session` — Session lifecycle commands hitting L4 orchestration."""

from __future__ import annotations

from typing import Any, Optional

import typer

from . import main as _main
from .config import CliConfig
from .output import print_json, print_panel, print_table

app = typer.Typer(help="Session lifecycle commands")


@app.command("create")
def create(
    skill: str = typer.Option(..., "--skill", help="Skill ID to run"),
    runtime: str = typer.Option(..., "--runtime", help="Runtime preference"),
    user_id: Optional[str] = typer.Option(None, "--user-id"),
    intent_text: Optional[str] = typer.Option(None, "--intent-text"),
) -> None:
    """Create a new session via the L4 three-way handshake."""
    cfg = CliConfig.from_env()
    body: dict[str, Any] = {
        "intent_text": intent_text or f"run skill {skill}",
        "skill_id": skill,
        "runtime_pref": runtime,
    }
    if user_id:
        body["user_id"] = user_id

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST", f"{cfg.l4_url}/v1/sessions/create", json=body
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    row = result if isinstance(result, dict) else {"value": result}
    print_table(
        "Session created",
        [row],
        ["session_id", "status", "created_at"],
    )


@app.command("list")
def list_cmd() -> None:
    """(Deferred D41) — L4 has no ``GET /v1/sessions`` in MVP Phase 0."""
    print_panel(
        "[yellow]session list[/yellow] is not available in MVP Phase 0.\n"
        "L4 has no [cyan]GET /v1/sessions[/cyan] endpoint "
        "(tracked as [bold]Deferred D41[/bold]).\n"
        "Use [cyan]eaasp session show <id>[/cyan] with a known session_id instead.",
        title="D41",
        style="yellow",
    )
    raise typer.Exit(0)


@app.command("show")
def show(session_id: str = typer.Argument(...)) -> None:
    """Fetch a session + its event stream and render the evidence pack."""
    cfg = CliConfig.from_env()

    async def _do() -> tuple[Any, Any]:
        client = _main.make_client(cfg)
        try:
            meta = await client.call("GET", f"{cfg.l4_url}/v1/sessions/{session_id}")
            events = await client.call(
                "GET",
                f"{cfg.l4_url}/v1/sessions/{session_id}/events",
                params={"from": 1, "to": 100, "limit": 100},
            )
            return meta, events
        finally:
            await client.aclose()

    meta, events = _main.run_async(_do())

    meta_row = meta if isinstance(meta, dict) else {"value": meta}
    print_table("Session", [meta_row], ["session_id", "status", "created_at"])

    evt_rows: list[Any] = []
    if isinstance(events, dict):
        evt_rows = events.get("events", []) or []
    elif isinstance(events, list):
        evt_rows = events
    print_table(
        "Events",
        evt_rows,
        ["seq", "event_type", "created_at"],
    )

    # Evidence pack — pull anchor ids out of event payloads.
    anchors: list[str] = []
    for e in evt_rows:
        if not isinstance(e, dict):
            continue
        payload = e.get("payload") if isinstance(e.get("payload"), dict) else {}
        for anchor in (payload or {}).get("anchors", []) or []:
            if isinstance(anchor, str):
                anchors.append(anchor)
            elif isinstance(anchor, dict) and "anchor_id" in anchor:
                anchors.append(str(anchor["anchor_id"]))

    print_panel(
        f"Evidence pack: {len(anchors)} anchor(s)\n" + "\n".join(anchors or ["(none)"]),
        title="Evidence",
        style="cyan",
    )
    print_panel(
        "action/approval cards: [TODO Phase 1]",
        title="Other cards",
        style="dim",
    )


@app.command("send")
def send(
    session_id: str = typer.Argument(...),
    message: str = typer.Argument(...),
) -> None:
    """Append a user message to a session."""
    cfg = CliConfig.from_env()

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST",
                f"{cfg.l4_url}/v1/sessions/{session_id}/message",
                json={"content": message},
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    print_json(result)
