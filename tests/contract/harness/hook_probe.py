"""Hook-envelope probe harness — drives a real runtime subprocess through
an Initialize → LoadSkill → Send round-trip so the shipped scoped-hook
bash scripts write the stdin envelope + GRID_* env vars into a known
directory that the test can then read back.

The probe-skill lives under ``tests/contract/fixtures/probe-skill/`` and
is discovered by the runtime via ``EAASP_SKILL_CACHE_DIR`` (set on the
runtime subprocess before spawn in ``conftest.py``). The skill ships three
hooks (PreToolUse / PostToolUse / Stop) that each dump ``stdin`` to
``${GRID_CONTRACT_PROBE_OUT}/<event>.envelope.json`` and the relevant
GRID_* env vars to ``<event>.env.json``.

## Why this shape

The original blueprint sketched a client-driven probe using the gRPC
``OnToolCall`` / ``OnToolResult`` / ``OnStop`` RPCs. Those methods exist
in the contract and would route through ``GridHarness::on_tool_call``
etc., but they do NOT execute scoped-frontmatter hooks — the scoped-hook
path fires only during the ``Send`` agent loop when the engine dispatches
an actual tool call. To exercise the scoped-hook code path we need a
real Send turn with a real tool_use response from the (mocked) LLM.

So the probe:
 1. Initialize(SessionPayload{skill_instructions=probe-skill, empty content,
    non-empty frontmatter_hooks → runtime adopts EAASP_SKILL_CACHE_DIR path).
 2. LoadSkill(SkillInstructions{...}) — belt-and-braces; the harness also
    honors skill_instructions on Initialize when present, but calling
    LoadSkill explicitly matches the contract's documented flow and also
    covers the case where the runtime gates hook registration on LoadSkill.
 3. Send(UserMessage{"probe"}) → mock LLM returns tool_use for file_write,
    engine dispatches file_write, PreToolUse + PostToolUse fire inline,
    second LLM round returns plain text + finish=stop, Stop hook fires.
 4. Read back the three dump files.
 5. Terminate().
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class HookCapture:
    """Parsed envelope + env pair produced by one probe-skill hook run."""

    envelope: dict[str, Any]
    env: dict[str, str]


class HookProbe:
    """Drives a live runtime through a scripted turn and collects hook dumps.

    Usage::

        probe = HookProbe(stub=grpc_stub, probe_out_dir=tmp_path,
                          runtime_pb2=runtime_pb2, common_pb2=common_pb2)
        probe.setup()
        try:
            captures = probe.run_turn()  # dict[event_name] -> HookCapture
        finally:
            probe.teardown()

    The harness owns one session at a time; create a fresh instance for
    each test that needs an isolated session.
    """

    def __init__(
        self,
        *,
        stub: Any,
        probe_out_dir: Path,
        runtime_pb2: Any,
        common_pb2: Any,
    ) -> None:
        self._stub = stub
        self._probe_out_dir = Path(probe_out_dir)
        self._runtime_pb2 = runtime_pb2
        self._common_pb2 = common_pb2
        self._session_id: str | None = None

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def setup(self) -> None:
        """Initialize a session and load the probe-skill.

        Pre-condition: ``EAASP_SKILL_CACHE_DIR`` is set on the runtime
        subprocess to the parent of the probe-skill directory.
        """
        self._probe_out_dir.mkdir(parents=True, exist_ok=True)

        # grid-runtime treats `ScopedHook.action` as the shell command
        # string — see crates/grid-runtime/src/harness.rs:231
        # `substitute_hook_vars(&hook.action, ...)`. The canonical ADR
        # command body uses `${SKILL_DIR}/hooks/<name>.sh`.
        scoped_hooks = [
            self._common_pb2.ScopedHook(
                hook_id="probe_pre_tool_use",
                hook_type="PreToolUse",
                condition="",
                action="${SKILL_DIR}/hooks/pre_tool_use.sh",
                precedence=0,
            ),
            self._common_pb2.ScopedHook(
                hook_id="probe_post_tool_use",
                hook_type="PostToolUse",
                condition="",
                action="${SKILL_DIR}/hooks/post_tool_use.sh",
                precedence=0,
            ),
            self._common_pb2.ScopedHook(
                hook_id="probe_stop",
                hook_type="Stop",
                condition="",
                action="${SKILL_DIR}/hooks/stop.sh",
                precedence=0,
            ),
        ]

        skill_instructions = self._common_pb2.SkillInstructions(
            skill_id="probe-skill",
            name="probe-skill",
            # Leave content empty so the runtime falls through to
            # EAASP_SKILL_CACHE_DIR/{skill_id} for SKILL_DIR resolution
            # (see crates/grid-runtime/src/harness.rs:346-376). The
            # physical hook files live on disk in the fixtures dir and
            # are invoked verbatim by the materialized commands.
            content="",
            frontmatter_hooks=scoped_hooks,
            required_tools=["file_write"],
        )

        payload = self._common_pb2.SessionPayload(
            session_id="contract-probe-session",
            user_id="contract-probe-user",
            runtime_id="grid-contract-test",
            skill_instructions=skill_instructions,
            allow_trim_p5=True,
        )

        init_resp = self._stub.Initialize(
            self._runtime_pb2.InitializeRequest(payload=payload)
        )
        self._session_id = init_resp.session_id

        # Belt-and-braces LoadSkill — harness.initialize() already adopts
        # payload.skill_instructions, but the spec's documented hook flow
        # goes through LoadSkill, so we exercise it too.
        self._stub.LoadSkill(
            self._runtime_pb2.LoadSkillRequest(
                session_id=self._session_id,
                skill=skill_instructions,
            )
        )

    def teardown(self) -> None:
        """Terminate the probe session (Empty request — last-initialized)."""
        try:
            self._stub.Terminate(self._common_pb2.Empty())
        except Exception:
            # Teardown must never raise; the test already ran.
            pass
        self._session_id = None

    # ------------------------------------------------------------------
    # Probe turn
    # ------------------------------------------------------------------

    def run_turn(self, prompt: str = "write test.txt") -> dict[str, HookCapture]:
        """Drive one Send turn and return captures by event scope.

        Returns a dict with keys drawn from ``{"PreToolUse",
        "PostToolUse", "Stop"}`` — only captures whose dump files were
        written are included. Missing entries indicate the runtime did
        not fire that hook during the turn (which is itself a contract
        violation the caller may assert on).
        """
        assert self._session_id, "call setup() first"

        message = self._runtime_pb2.UserMessage(content=prompt, message_type="text")
        send_stream = self._stub.Send(
            self._runtime_pb2.SendRequest(
                session_id=self._session_id, message=message
            )
        )

        # Drain the stream so the runtime completes the full turn
        # (including the Stop hook at natural-termination boundary).
        for _chunk in send_stream:
            pass

        # Brief settle to let hooks that fire on the tail end of the
        # agent loop flush their writes. Hooks synchronously wait on
        # stdout before returning to the engine, but filesystem sync is
        # best-effort; a short sleep eliminates test flake without
        # masking real-timing bugs.
        time.sleep(0.1)

        return self._read_dumps()

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _read_dumps(self) -> dict[str, HookCapture]:
        captures: dict[str, HookCapture] = {}
        for slug, event_name in (
            ("pre_tool_use", "PreToolUse"),
            ("post_tool_use", "PostToolUse"),
            ("stop", "Stop"),
        ):
            env_path = self._probe_out_dir / f"{slug}.env.json"
            envelope_path = self._probe_out_dir / f"{slug}.envelope.json"
            if not env_path.exists() or not envelope_path.exists():
                continue
            try:
                env_raw = env_path.read_text(encoding="utf-8")
                envelope_raw = envelope_path.read_text(encoding="utf-8")
                env = json.loads(env_raw) if env_raw.strip() else {}
                envelope = (
                    json.loads(envelope_raw) if envelope_raw.strip() else {}
                )
            except json.JSONDecodeError:
                # Partial write or corrupted dump — skip; the caller
                # will notice a missing capture and raise its own
                # assertion with better context.
                continue
            captures[event_name] = HookCapture(envelope=envelope, env=env)
        return captures
