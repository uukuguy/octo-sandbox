"""`eaasp session` — Session lifecycle commands hitting L4 orchestration."""

from __future__ import annotations

import os
import sys
from typing import Any, Optional

import typer
from rich.console import Console

from . import main as _main
from .config import CliConfig
from .output import print_json, print_panel, print_table

app = typer.Typer(help="Session lifecycle commands")


# ── ADR-V2-021: canonical chunk_type wire values ─────────────────────────────
# L4 SSE serializes the ChunkType proto enum → lowercase snake_case wire string
# (see tools/eaasp-l4-orchestration/.../session_orchestrator.py, S2.T1 fix
# @ 5494af1). The CLI consumes only these seven values; anything else is a
# contract violation and must be surfaced loudly on stderr — never silently
# dropped, because silent drops let runtime drift (e.g. the historical
# "tool_call_start" → "tool_start" rename) ship undetected.
_ALLOWED_CHUNK_TYPES: frozenset[str] = frozenset({
    "text_delta",
    "thinking",
    "tool_start",
    "tool_result",
    "done",
    "error",
    "workflow_continuation",
})


def _render_chunk(
    data: dict[str, Any],
    console: Console,
    err_console: Console,
    *,
    show_thinking: bool = False,
    debug_chunks: bool = False,
) -> None:
    """Render a single `event: chunk` payload to the proper sink.

    Contract (ADR-V2-021):
      - `text_delta`            → stdout (whitespace-only chunks dropped so the
                                  tokenizer's standalone `\\n` tokens don't
                                  manifest as bare blank lines; real content
                                  containing embedded `\\n` is preserved).
      - `thinking`              → dim stderr line when `show_thinking=True`.
      - `tool_start`            → cyan console line `[tool_call: NAME]`.
      - `tool_result`           → green console line `[tool_result: NAME]`.
      - `done`                  → newline on stdout + flush.
      - `error`                 → red `[error] …` on stderr console.
      - `workflow_continuation` → dim stderr `[continuation] …` (optional UX;
                                  L4 surfaces these but one-shot `run` rarely
                                  needs them).
      - `""` (UNSPECIFIED)      → stderr warning; dropped.
      - any other string        → stderr warning (unknown wire value); dropped.

    Shared by `send --stream` (`_do_stream`) and `run` — closes D146
    (chunk-dispatch duplication inside cmd_session.py). The sibling
    duplication in `tools/eaasp-l4-orchestration/.../session_orchestrator.py`
    is a separate concern tracked under the same ledger item.
    """
    chunk_type = data.get("chunk_type", "")
    content = data.get("content", "")

    if chunk_type == "text_delta":
        if debug_chunks:
            sys.stderr.write(f"CHUNK:{content!r}\n")
        # Drop chunks that are purely whitespace/newlines (standalone newline
        # tokens from the tokenizer); preserve \n within chunks that also
        # contain real text (Markdown structure).
        if content.strip():
            sys.stdout.write(content)
            sys.stdout.flush()
        return

    if chunk_type == "thinking":
        if show_thinking:
            # stderr so it doesn't interleave with stdout text flow.
            sys.stderr.write(f"[thinking] {content}\n")
            sys.stderr.flush()
        return

    if chunk_type == "tool_start":
        tool = data.get("tool_name", "?")
        # Rich parses `[tool_call: …]` as a markup tag and drops it; escape
        # the inner literal brackets with `\[` so the user actually sees them.
        console.print(f"[cyan]\\[tool_call: {tool}][/cyan]")
        return

    if chunk_type == "tool_result":
        tool = data.get("tool_name", "?")
        console.print(f"[green]\\[tool_result: {tool}][/green]")
        return

    if chunk_type == "done":
        sys.stdout.write("\n")
        sys.stdout.flush()
        return

    if chunk_type == "error":
        err_console.print(f"[red]\\[error] {content}[/red]")
        return

    if chunk_type == "workflow_continuation":
        # Surface quietly on stderr; don't pollute stdout. Tests may rely on
        # stdout staying clean for pipe consumers.
        err_console.print(f"[dim]\\[continuation] {content}[/dim]")
        return

    # ── Contract violations (ADR-V2-021) ─────────────────────────────────
    if chunk_type == "":
        sys.stderr.write(
            "[ADR-V2-021 violation] empty chunk_type (UNSPECIFIED); "
            "dropping chunk\n"
        )
        sys.stderr.flush()
        return

    # Defensive: chunk_type is in _ALLOWED_CHUNK_TYPES but not handled above
    # would be a code bug; treat anything else as runtime drift.
    if chunk_type not in _ALLOWED_CHUNK_TYPES:
        sys.stderr.write(
            f"[ADR-V2-021 violation] unknown chunk_type={chunk_type!r} "
            f"(runtime drift); dropping chunk\n"
        )
        sys.stderr.flush()
        return


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
def list_cmd(
    limit: int = typer.Option(50, "--limit", "-n", help="Max rows to return"),
    status: Optional[str] = typer.Option(None, "--status", help="Filter by status"),
) -> None:
    """List all sessions (newest first)."""
    cfg = CliConfig.from_env()

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            params: dict[str, Any] = {"limit": limit}
            if status is not None:
                params["status"] = status
            return await client.call(
                "GET", f"{cfg.l4_url}/v1/sessions", params=params
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    rows = result.get("sessions", []) if isinstance(result, dict) else []
    print_table(
        "Sessions",
        rows,
        ["session_id", "status", "skill_id", "runtime_id", "created_at"],
    )


@app.command("close")
def close(session_id: str = typer.Argument(...)) -> None:
    """Close a session via L4 (terminates L1 + transitions status to closed)."""
    cfg = CliConfig.from_env()

    async def _do() -> Any:
        client = _main.make_client(cfg)
        try:
            return await client.call(
                "POST", f"{cfg.l4_url}/v1/sessions/{session_id}/close"
            )
        finally:
            await client.aclose()

    result = _main.run_async(_do())
    row = result if isinstance(result, dict) else {"value": result}
    print_table(
        "Session closed",
        [row],
        ["session_id", "status"],
    )


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
    stream: bool = typer.Option(True, "--stream/--no-stream", help="Stream output via SSE"),
    show_thinking: bool = typer.Option(False, "--thinking", help="Show extended thinking chunks"),
) -> None:
    """Append a user message to a session (streaming by default)."""
    cfg = CliConfig.from_env()

    if not stream:
        # Legacy non-streaming path.
        async def _do_sync() -> Any:
            client = _main.make_client(cfg)
            try:
                return await client.call(
                    "POST",
                    f"{cfg.l4_url}/v1/sessions/{session_id}/message",
                    json={"content": message},
                )
            finally:
                await client.aclose()

        result = _main.run_async(_do_sync())
        print_json(result)
        return

    # ── SSE streaming path (default) ──────────────────────────────────────
    console = Console()
    err_console = Console(stderr=True)

    async def _do_stream() -> None:
        client = _main.make_client(cfg)
        debug_chunks = bool(os.environ.get("EAASP_DEBUG_CHUNKS"))
        try:
            async for msg in client.stream_sse(
                f"{cfg.l4_url}/v1/sessions/{session_id}/message/stream",
                json_body={"content": message},
            ):
                event = msg.get("event", "chunk")
                data = msg.get("data", {})

                if event == "chunk":
                    _render_chunk(
                        data,
                        console,
                        err_console,
                        show_thinking=show_thinking,
                        debug_chunks=debug_chunks,
                    )

                elif event == "done":
                    # Final summary — print a newline + summary info.
                    sys.stdout.write("\n")
                    resp_text = data.get("response_text", "")
                    n_events = len(data.get("events", []))
                    console.print(
                        f"[dim]── {n_events} events, "
                        f"{len(resp_text)} chars total[/dim]"
                    )

                elif event == "error":
                    err_console.print(
                        f"[bold red]runtime error:[/bold red] {data.get('error', '?')}",
                    )
        finally:
            await client.aclose()

    _main.run_async(_do_stream())


@app.command("run")
def run(
    message: str = typer.Argument(...),
    skill: str = typer.Option(..., "--skill", "-s"),
    runtime: str = typer.Option("grid-runtime", "--runtime", "-r"),
    no_stream: bool = typer.Option(False, "--no-stream"),
) -> None:
    """Create a session and immediately send a message (create + send in one step)."""
    cfg = CliConfig.from_env()
    console = Console()

    async def _do() -> None:
        client = _main.make_client(cfg)
        try:
            # Step 1: create session
            create_body: dict[str, Any] = {
                "intent_text": message,
                "skill_id": skill,
                "runtime_pref": runtime,
            }
            result = await client.call(
                "POST", f"{cfg.l4_url}/v1/sessions/create", json=create_body
            )
            session_id = result["session_id"] if isinstance(result, dict) else str(result)
            console.print(f"[dim]session created: {session_id}[/dim]")

            # Step 2: send message
            if no_stream:
                resp = await client.call(
                    "POST",
                    f"{cfg.l4_url}/v1/sessions/{session_id}/message",
                    json={"content": message},
                )
                print_json(resp)
            else:
                err_console = Console(stderr=True)
                debug_chunks = bool(os.environ.get("EAASP_DEBUG_CHUNKS"))
                async for msg in client.stream_sse(
                    f"{cfg.l4_url}/v1/sessions/{session_id}/message/stream",
                    json_body={"content": message},
                ):
                    event = msg.get("event", "chunk")
                    data = msg.get("data", {})

                    if event == "chunk":
                        # One-shot `run` shows thinking chunks inline on stderr
                        # — simpler UX than `send` (which gates on --thinking).
                        _render_chunk(
                            data,
                            console,
                            err_console,
                            show_thinking=True,
                            debug_chunks=debug_chunks,
                        )
                    elif event == "done":
                        sys.stdout.write("\n")
                        resp_text = data.get("response_text", "")
                        n_events = len(data.get("events", []))
                        console.print(
                            f"[dim]── {n_events} events, "
                            f"{len(resp_text)} chars total[/dim]"
                        )
                    elif event == "error":
                        err_console.print(
                            f"[bold red]runtime error:[/bold red] "
                            f"{data.get('error', '?')}",
                        )
        finally:
            await client.aclose()

    _main.run_async(_do())


# ── Phase 1: session events command ──────────────────────────────────────────

# Color mapping for event types.
_EVENT_COLORS: dict[str, str] = {
    "SESSION_START": "green",
    "PRE_TOOL_USE": "cyan",
    "POST_TOOL_USE": "blue",
    "POST_TOOL_USE_FAILURE": "red",
    "STOP": "yellow",
    "POST_SESSION_END": "magenta",
    "USER_PROMPT_SUBMIT": "white",
    "SESSION_CREATED": "green",
    "RUNTIME_INITIALIZED": "green",
    "SESSION_MCP_CONNECTED": "green",
    "USER_MESSAGE": "white",
    "RESPONSE_CHUNK": "dim",
    "SESSION_CLOSED": "magenta",
}


async def _fetch_events(
    cfg: CliConfig, session_id: str, limit: int = 500
) -> dict[str, Any]:
    """Fetch events from L4 API."""
    client = _main.make_client(cfg)
    try:
        return await client.call(
            "GET",
            f"{cfg.l4_url}/v1/sessions/{session_id}/events",
            params={"from": 1, "limit": limit},
        )
    finally:
        await client.aclose()


def _format_event_line(event: dict[str, Any], fmt: str) -> str:
    """Format a single event for display."""
    import json as _json
    if fmt == "json":
        return _json.dumps(event, ensure_ascii=False, default=str)

    import datetime
    ts = event.get("created_at", 0)
    dt = datetime.datetime.fromtimestamp(ts).strftime("%H:%M:%S") if ts else "??:??:??"
    etype = event.get("event_type", "")
    color = _EVENT_COLORS.get(etype, "white")
    payload = event.get("payload", {})

    # Summarize payload.
    parts: list[str] = []
    if "tool_name" in payload:
        parts.append(f"tool={payload['tool_name']}")
    if "runtime_id" in payload:
        parts.append(f"runtime={payload['runtime_id']}")
    if "reason" in payload:
        parts.append(f"reason={payload['reason']}")
    if "content" in payload and isinstance(payload["content"], str):
        c = payload["content"]
        if len(c) > 40:
            c = c[:40] + "..."
        parts.append(f'"{c}"')
    summary = "  ".join(parts)

    cluster = event.get("cluster_id", "")
    cluster_tag = f"  cluster={cluster}" if cluster else ""
    return f"[{dt}] [{color}]{etype.ljust(24)}[/{color}] {summary}{cluster_tag}"


@app.command("events")
def events_cmd(
    session_id: str = typer.Argument(...),
    format_: str = typer.Option("table", "--format", "-f", help="Output: table|json"),
    limit: int = typer.Option(500, "--limit", "-n"),
    follow: bool = typer.Option(
        False, "--follow", "-F", help="Tail-follow events via SSE (Ctrl+C to stop)"
    ),
    from_seq: int = typer.Option(
        1, "--from", help="Start from this seq (default 1; useful with --follow)"
    ),
) -> None:
    """List events for a session (optionally tail-follow via SSE)."""
    cfg = CliConfig.from_env()

    # ── Follow mode: SSE tail ────────────────────────────────────────────
    if follow:
        console = Console()
        err_console = Console(stderr=True)

        async def _do_follow() -> None:
            client = _main.make_client(cfg)
            try:
                url = f"{cfg.l4_url}/v1/sessions/{session_id}/events/stream"
                async for msg in client.stream_sse(
                    url, method="GET", params={"from": from_seq}
                ):
                    event = msg.get("event", "event")
                    data = msg.get("data", {})
                    if event == "event" and isinstance(data, dict):
                        if format_ == "json":
                            import json as _json
                            typer.echo(
                                _json.dumps(data, ensure_ascii=False, default=str)
                            )
                        else:
                            console.print(_format_event_line(data, format_))
                    elif event == "error":
                        err_console.print(
                            f"[bold red]stream error:[/bold red] "
                            f"{data.get('code', '?')} — {data.get('session_id', '?')}"
                        )
                        return
            finally:
                await client.aclose()

        _main.run_async(_do_follow())
        return

    # ── One-shot mode (original behavior) ────────────────────────────────
    result = _main.run_async(_fetch_events(cfg, session_id, limit=limit))

    if format_ == "json":
        import json as _json
        typer.echo(_json.dumps(result, ensure_ascii=False, indent=2, default=str))
        return

    console = Console()
    evt_rows: list[dict[str, Any]] = []
    if isinstance(result, dict):
        evt_rows = result.get("events", []) or []

    if not evt_rows:
        console.print("[dim]No events found.[/dim]")
        return

    for e in evt_rows:
        if isinstance(e, dict):
            console.print(_format_event_line(e, format_))
