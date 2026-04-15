"""S3.T5 — ScopedCommandExecutor integration tests (ADR-V2-006).

Exercises the subprocess-based executor end-to-end against the real
threshold-calibration hook script and synthetic temp-file scripts. These
tests do NOT touch the gRPC server; they pin down the contract surface of
``ScopedCommandExecutor.execute`` so the wiring in ``service.py`` can rely
on its behaviors.

Scope per task T5.B: envelope serialization, exit-code mapping, timeout
fail-open, variable substitution smoke.
"""

from __future__ import annotations

import json
import logging
import stat
from pathlib import Path

import pytest

from claude_code_runtime.hook_substitution import HookVars, substitute_scoped_hooks
from claude_code_runtime.scoped_command_executor import (
    ScopedCommandExecutor,
    ScopedHookDecision,
)


REPO_ROOT = Path(__file__).resolve().parents[3]
BLOCK_WRITE_SCADA_HOOK = (
    REPO_ROOT / "examples" / "skills" / "threshold-calibration" / "hooks"
    / "block_write_scada.sh"
)


def _pre_tool_use_envelope(tool_name: str, tool_args: dict | None = None) -> dict:
    """Construct an ADR-V2-006 §2.1 PreToolUse envelope."""
    return {
        "event": "PreToolUse",
        "session_id": "sess-integration",
        "skill_id": "threshold-calibration",
        "tool_name": tool_name,
        "tool_args": tool_args or {},
        "created_at": "2026-04-15T12:00:00Z",
    }


@pytest.mark.asyncio
async def test_block_write_scada_denies_scada_write() -> None:
    """Real hook script rejects scada_write* (exit 2 → deny)."""
    assert BLOCK_WRITE_SCADA_HOOK.exists(), (
        f"Fixture hook missing at {BLOCK_WRITE_SCADA_HOOK}; "
        "threshold-calibration skill is the canonical scoped-hook fixture"
    )
    executor = ScopedCommandExecutor(timeout_secs=5.0)
    envelope = _pre_tool_use_envelope(
        "scada_write", {"device_id": "xfmr-042", "value": 75.0}
    )
    decision = await executor.execute(
        f"bash {BLOCK_WRITE_SCADA_HOOK}",
        envelope,
        {"GRID_EVENT": "PreToolUse", "GRID_TOOL_NAME": "scada_write"},
    )
    assert isinstance(decision, ScopedHookDecision)
    assert decision.action == "deny"
    # Stderr is empty on this hook; reason falls back to default deny string.
    # The contract only requires action=="deny" and a non-empty reason.
    assert decision.reason


@pytest.mark.asyncio
async def test_block_write_scada_allows_non_scada_tool() -> None:
    """Real hook script passes through unrelated tool names (exit 0 → allow)."""
    executor = ScopedCommandExecutor(timeout_secs=5.0)
    envelope = _pre_tool_use_envelope("bash", {"command": "ls"})
    decision = await executor.execute(
        f"bash {BLOCK_WRITE_SCADA_HOOK}",
        envelope,
        {"GRID_EVENT": "PreToolUse", "GRID_TOOL_NAME": "bash"},
    )
    assert decision.action == "allow"


@pytest.mark.asyncio
async def test_timeout_fails_open(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """sleep 10 with a 1s timeout → fail-open allow + WARN log."""
    executor = ScopedCommandExecutor(timeout_secs=1.0)
    with caplog.at_level(logging.WARNING, logger="claude_code_runtime.scoped_command_executor"):
        decision = await executor.execute(
            "sleep 10",
            _pre_tool_use_envelope("scada_write"),
        )
    assert decision.action == "allow"
    assert decision.reason == "timeout"
    assert any("timeout" in r.getMessage().lower() for r in caplog.records), (
        "expected at least one WARN log mentioning timeout"
    )


@pytest.mark.asyncio
async def test_non_zero_non_two_exit_fails_open(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """exit 127 (command not found) → fail-open allow + WARN with exit_code."""
    executor = ScopedCommandExecutor(timeout_secs=5.0)
    with caplog.at_level(logging.WARNING, logger="claude_code_runtime.scoped_command_executor"):
        # "exit 127" via explicit bash invocation avoids shell-lookup paths
        # that may return other codes on different systems.
        decision = await executor.execute(
            "exit 127",
            _pre_tool_use_envelope("bash"),
        )
    assert decision.action == "allow"
    # At least one log record must reference the non-zero exit. Accept either
    # the literal number or the "exit_code" keyword from the executor.
    joined = " ".join(r.getMessage() for r in caplog.records)
    assert "exit_code" in joined or "127" in joined, (
        "expected WARN log mentioning exit_code/127, got: {!r}".format(joined)
    )


@pytest.mark.asyncio
async def test_envelope_reaches_stdin(tmp_path: Path) -> None:
    """Hook receives the JSON envelope verbatim on stdin."""
    dump_path = tmp_path / "envelope-dump.json"
    script_path = tmp_path / "dump_stdin.sh"
    # `cat - > ...` captures stdin verbatim; set -eu for defensive shell.
    script_path.write_text(
        "#!/usr/bin/env bash\n"
        "set -eu\n"
        f'cat - > "{dump_path}"\n'
        "exit 0\n"
    )
    script_path.chmod(
        script_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH
    )

    executor = ScopedCommandExecutor(timeout_secs=5.0)
    envelope = _pre_tool_use_envelope(
        "scada_write", {"device_id": "xfmr-042", "value": 75.0}
    )
    decision = await executor.execute(
        f"bash {script_path}",
        envelope,
        {
            "GRID_SESSION_ID": "sess-integration",
            "GRID_TOOL_NAME": "scada_write",
            "GRID_SKILL_ID": "threshold-calibration",
            "GRID_EVENT": "PreToolUse",
        },
    )
    assert decision.action == "allow"
    assert dump_path.exists(), "hook did not write stdin dump"
    parsed = json.loads(dump_path.read_text())
    assert parsed["event"] == "PreToolUse"
    assert parsed["session_id"] == "sess-integration"
    assert parsed["tool_name"] == "scada_write"
    assert parsed["tool_args"] == {"device_id": "xfmr-042", "value": 75.0}
    # created_at is a Zulu ISO-8601 string per ADR §2.4.
    assert parsed["created_at"].endswith("Z")


@pytest.mark.asyncio
async def test_substitution_expands_skill_dir() -> None:
    """substitute_scoped_hooks resolves ${SKILL_DIR} before the executor sees it.

    This smoke-tests the integration boundary between hook_substitution and
    ScopedCommandExecutor: by the time a command string reaches execute(),
    every ${...} placeholder MUST be gone.
    """
    raw_hooks = [
        {
            "name": "test_hook",
            "type": "command",
            "command": "${SKILL_DIR}/hooks/x.sh --session ${SESSION_DIR}",
        }
    ]
    vars_ = HookVars(
        skill_dir="/abs/skills/demo",
        session_dir="/var/ws/sess-1",
        runtime_dir="/opt/runtime",
    )
    resolved = substitute_scoped_hooks(raw_hooks, vars_)
    assert len(resolved) == 1
    cmd = resolved[0]["command"]
    assert "${" not in cmd, f"substitution leaked a placeholder: {cmd!r}"
    assert cmd == "/abs/skills/demo/hooks/x.sh --session /var/ws/sess-1"
