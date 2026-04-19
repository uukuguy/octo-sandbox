"""ADR-V2-021 — chunk_type wire-value whitelist tests for `_render_chunk`
and the `session run` command.

Goals:
  1. Cover all 7 whitelist values in `_render_chunk` (D5).
  2. Assert unknown / empty chunk_type → stderr warning (D5, D2, D3).
  3. Assert `run` end-to-end over SSE with text_delta + done produces
     correct stdout AND correct `X chars total` > 0 (D4).
"""

from __future__ import annotations

import io
import json

import httpx
from rich.console import Console
from typer.testing import CliRunner

from eaasp_cli_v2 import main as cli_main
from eaasp_cli_v2.cmd_session import _ALLOWED_CHUNK_TYPES, _render_chunk


# ── Unit tests: _render_chunk per wire value ────────────────────────────────


def _make_consoles() -> tuple[Console, io.StringIO, Console, io.StringIO]:
    """Fresh Rich consoles wired to in-memory buffers (ANSI disabled)."""
    stdout_buf = io.StringIO()
    stderr_buf = io.StringIO()
    console = Console(file=stdout_buf, force_terminal=False, width=120)
    err_console = Console(file=stderr_buf, force_terminal=False, width=120)
    return console, stdout_buf, err_console, stderr_buf


def test_whitelist_has_exactly_seven_values() -> None:
    """Lock ADR-V2-021 canonical set so drift is caught at test time."""
    assert _ALLOWED_CHUNK_TYPES == frozenset({
        "text_delta",
        "thinking",
        "tool_start",
        "tool_result",
        "done",
        "error",
        "workflow_continuation",
    })


def test_render_text_delta_writes_to_stdout(capsys) -> None:
    console, _sbuf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "text_delta", "content": "hello"},
        console,
        err_console,
    )
    captured = capsys.readouterr()
    assert captured.out == "hello"
    assert "ADR-V2-021" not in captured.err


def test_render_text_delta_drops_whitespace_only_chunks(capsys) -> None:
    """Tokenizer-emitted standalone \\n tokens are dropped (pre-existing UX)."""
    console, _sbuf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "text_delta", "content": "\n"},
        console,
        err_console,
    )
    captured = capsys.readouterr()
    assert captured.out == ""


def test_render_thinking_gated_by_show_thinking(capsys) -> None:
    console, _sbuf, err_console, _ebuf = _make_consoles()
    # show_thinking=False → silent.
    _render_chunk(
        {"chunk_type": "thinking", "content": "pondering"},
        console,
        err_console,
        show_thinking=False,
    )
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "pondering" not in captured.err

    # show_thinking=True → stderr line.
    _render_chunk(
        {"chunk_type": "thinking", "content": "pondering"},
        console,
        err_console,
        show_thinking=True,
    )
    captured = capsys.readouterr()
    assert "[thinking] pondering" in captured.err


def test_render_tool_start_goes_to_console(capsys) -> None:
    console, stdout_buf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "tool_start", "tool_name": "memory_search"},
        console,
        err_console,
    )
    # Rich console was wired to stdout_buf, so capsys won't see it — check buf.
    # Rich treats `[tool_call: ...]` as markup and may re-render; assert the
    # tool_name survives either way.
    rendered = stdout_buf.getvalue()
    assert "memory_search" in rendered


def test_render_tool_result_goes_to_console(capsys) -> None:
    console, stdout_buf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "tool_result", "tool_name": "memory_read"},
        console,
        err_console,
    )
    rendered = stdout_buf.getvalue()
    assert "memory_read" in rendered


def test_render_done_emits_newline(capsys) -> None:
    console, _sbuf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "done", "content": ""},
        console,
        err_console,
    )
    captured = capsys.readouterr()
    assert captured.out == "\n"


def test_render_error_goes_to_stderr_console(capsys) -> None:
    console, _sbuf, err_console, stderr_buf = _make_consoles()
    _render_chunk(
        {"chunk_type": "error", "content": "boom"},
        console,
        err_console,
    )
    rendered = stderr_buf.getvalue()
    # Rich may consume the `[error]` bracket prefix as a markup tag; what
    # matters is that (a) the error content lands on stderr, and (b) nothing
    # went to stdout.
    assert "boom" in rendered


def test_render_workflow_continuation_goes_to_stderr_console(capsys) -> None:
    """workflow_continuation is surfaced on stderr so stdout stays pipe-clean."""
    console, stdout_buf, err_console, stderr_buf = _make_consoles()
    _render_chunk(
        {"chunk_type": "workflow_continuation", "content": "next turn"},
        console,
        err_console,
    )
    assert "next turn" in stderr_buf.getvalue()
    # Stdout must stay clean for piped consumers.
    assert stdout_buf.getvalue() == ""


def test_render_unknown_chunk_type_warns_on_stderr(capsys) -> None:
    """Contract violation: a runtime emitting an off-whitelist value must be
    loud on stderr, not silently dropped."""
    console, _sbuf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "foo_new", "content": "ignored"},
        console,
        err_console,
    )
    captured = capsys.readouterr()
    assert "[ADR-V2-021 violation]" in captured.err
    assert "unknown chunk_type" in captured.err
    assert "'foo_new'" in captured.err
    # Content must NOT leak to stdout.
    assert "ignored" not in captured.out


def test_render_empty_chunk_type_warns_distinctly(capsys) -> None:
    """UNSPECIFIED → empty string at wire boundary gets its own warning message."""
    console, _sbuf, err_console, _ebuf = _make_consoles()
    _render_chunk(
        {"chunk_type": "", "content": "noise"},
        console,
        err_console,
    )
    captured = capsys.readouterr()
    assert "[ADR-V2-021 violation]" in captured.err
    assert "empty chunk_type" in captured.err
    assert "UNSPECIFIED" in captured.err
    assert "unknown chunk_type" not in captured.err  # distinct from foo_new path


def test_render_missing_chunk_type_key_treated_as_empty(capsys) -> None:
    """Defensive: if the runtime omits chunk_type entirely, treat as empty."""
    console, _sbuf, err_console, _ebuf = _make_consoles()
    _render_chunk({"content": "oops"}, console, err_console)
    captured = capsys.readouterr()
    assert "empty chunk_type" in captured.err


# ── End-to-end: `session run` command with mocked L4 SSE ────────────────────


def test_run_command_emits_response_text_summary(
    runner: CliRunner, install_mock
) -> None:
    """`eaasp session run` with 5 text_delta chunks + done must:
    1. Write all text to stdout.
    2. Render `── 1 events, N chars total` where N == len(response_text) > 0.

    Locks Task #2's fix (L4 now accumulates response_text from chunks).
    """
    create_body_holder: dict = {}

    sse_body_parts = [
        ("chunk", {"chunk_type": "text_delta", "content": "Hello "}),
        ("chunk", {"chunk_type": "text_delta", "content": "from "}),
        ("chunk", {"chunk_type": "text_delta", "content": "the "}),
        ("chunk", {"chunk_type": "text_delta", "content": "runtime"}),
        ("chunk", {"chunk_type": "text_delta", "content": "!"}),
        ("chunk", {"chunk_type": "done", "content": ""}),
        ("done", {
            "session_id": "sess_run1",
            "response_text": "Hello from the runtime!",
            "events": [{"seq": 1}],
        }),
    ]
    sse_lines: list[str] = []
    for event, data in sse_body_parts:
        sse_lines.append(f"event: {event}")
        sse_lines.append(f"data: {json.dumps(data)}")
        sse_lines.append("")
    sse_body = "\n".join(sse_lines) + "\n"

    def handler(req: httpx.Request) -> httpx.Response:
        if req.url.path == "/v1/sessions/create":
            create_body_holder["body"] = json.loads(req.content.decode("utf-8"))
            return httpx.Response(
                200,
                content=json.dumps({
                    "session_id": "sess_run1",
                    "status": "created",
                    "created_at": 1712920000,
                }).encode("utf-8"),
                headers={"content-type": "application/json"},
            )
        if req.url.path == "/v1/sessions/sess_run1/message/stream":
            return httpx.Response(
                200,
                content=sse_body.encode("utf-8"),
                headers={"content-type": "text/event-stream"},
            )
        return httpx.Response(
            404,
            content=json.dumps({"detail": "not routed"}).encode("utf-8"),
            headers={"content-type": "application/json"},
        )

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "run", "hi there", "--skill", "skill.greet", "--runtime", "grid-runtime"],
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    # The text chunks should show up on stdout.
    assert "Hello from the runtime!" in result.output
    # Summary line from the final `event: done` payload.
    # len("Hello from the runtime!") == 23 → the summary must report > 0.
    assert "1 events" in result.output
    assert "23 chars total" in result.output
    # Sanity: create payload went through.
    assert create_body_holder["body"]["skill_id"] == "skill.greet"


def test_run_command_unknown_chunk_type_warns_but_does_not_crash(
    runner: CliRunner, install_mock
) -> None:
    """Runtime drift safety net: a surprise chunk_type must warn on stderr and
    continue draining the stream, not break the session."""
    sse_body_parts = [
        ("chunk", {"chunk_type": "text_delta", "content": "pre "}),
        ("chunk", {"chunk_type": "tool_call_start", "tool_name": "legacy"}),  # drift!
        ("chunk", {"chunk_type": "text_delta", "content": "post"}),
        ("done", {
            "session_id": "sess_run2",
            "response_text": "pre post",
            "events": [],
        }),
    ]
    sse_lines: list[str] = []
    for event, data in sse_body_parts:
        sse_lines.append(f"event: {event}")
        sse_lines.append(f"data: {json.dumps(data)}")
        sse_lines.append("")
    sse_body = "\n".join(sse_lines) + "\n"

    def handler(req: httpx.Request) -> httpx.Response:
        if req.url.path == "/v1/sessions/create":
            return httpx.Response(
                200,
                content=json.dumps({
                    "session_id": "sess_run2",
                    "status": "created",
                    "created_at": 1712920001,
                }).encode("utf-8"),
                headers={"content-type": "application/json"},
            )
        return httpx.Response(
            200,
            content=sse_body.encode("utf-8"),
            headers={"content-type": "text/event-stream"},
        )

    install_mock(handler)
    result = runner.invoke(
        cli_main.app,
        ["session", "run", "hi", "--skill", "skill.x", "--runtime", "grid-runtime"],
    )
    assert result.exit_code == 0, result.stdout + result.stderr
    # Both real text chunks arrive.
    assert "pre " in result.output
    assert "post" in result.output
    # Drift warning surfaced on stderr (CliRunner mixes streams into .output
    # unless mix_stderr=False; we check via the combined output plus the
    # explicit stderr attribute when available).
    combined = result.output + (result.stderr if hasattr(result, "stderr") else "")
    assert "ADR-V2-021 violation" in combined
    assert "tool_call_start" in combined
