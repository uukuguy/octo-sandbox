"""Subprocess-based scoped-hook executor (S3.T5).

Implements ADR-V2-006 envelope + exit-code contract. Called per-hook at
PreToolUse / PostToolUse / Stop boundaries by RuntimeServiceImpl. Fail-open:
non-2 exit codes log a warning and return allow.

See `docs/design/EAASP/adrs/ADR-V2-006-hook-envelope-contract.md` for the
authoritative contract. Key invariants:

- stdin: exactly one JSON object (envelope) per §2 schema
- env vars: GRID_SESSION_ID / GRID_TOOL_NAME / GRID_SKILL_ID / GRID_EVENT (§3)
- exit 0: allow (stdout MAY contain decision JSON)
- exit 2: deny (stderr becomes reason)
- other exit / timeout / spawn error: fail-open (§7)
- timeout: default 5 seconds (§6) — SIGKILL on expiry, reap, then allow
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class ScopedHookDecision:
    """Decision returned by a scoped hook invocation.

    Attributes:
        action: ``"allow"`` or ``"deny"`` — the only two MVP actions. ADR §4
            maps ``{"decision":"ask"}`` to deny at MVP (no interactive branch).
        reason: Free-form UTF-8 explanation. Empty string when not provided.
        system_message: Optional message to inject as a system turn when a
            Stop hook denies (runtime-side feature; unused by Pre/Post).
    """

    action: str
    reason: str = ""
    system_message: str = ""


@dataclass
class ScopedHookBundle:
    """Partitioned view of substituted scoped hooks for one session.

    The bundle preserves the order in which hooks appeared in the skill
    frontmatter so ``matching`` can sort by ``precedence`` (lowest wins).
    Each hook dict is shaped like::

        {
            "hook_id": "block-scada-write",
            "hook_type": "PreToolUse",
            "condition": "scada_write*",
            "action": "bash /abs/path/hook.sh",  # post-substitution
            "precedence": 0,
        }
    """

    pre: list[dict] = field(default_factory=list)
    post: list[dict] = field(default_factory=list)
    stop: list[dict] = field(default_factory=list)

    @classmethod
    def from_hooks(cls, substituted_hooks: list[dict]) -> "ScopedHookBundle":
        """Partition hooks by scope.

        Recognises both Pascal-case names (``PreToolUse``, ``PostToolUse``,
        ``Stop``) and snake_case variants (``pre_tool_call`` /
        ``post_tool_result`` / ``stop``) to stay source-compatible with the
        HookExecutor rule mapping used by the rest of service.py.
        """
        bundle = cls()
        for hook in substituted_hooks:
            # Prefer the explicit condition scope when it names a scope
            # (e.g. condition="Stop" in frontmatter); fall back to hook_type.
            scope = (hook.get("condition") or "").strip()
            hook_type = (hook.get("hook_type") or "").strip()

            if scope in ("PreToolUse", "pre_tool_call"):
                bundle.pre.append(hook)
            elif scope in ("PostToolUse", "post_tool_result"):
                bundle.post.append(hook)
            elif scope in ("Stop", "stop"):
                bundle.stop.append(hook)
            elif hook_type in ("PreToolUse", "pre_tool_call"):
                bundle.pre.append(hook)
            elif hook_type in ("PostToolUse", "post_tool_result"):
                bundle.post.append(hook)
            elif hook_type in ("Stop", "stop"):
                bundle.stop.append(hook)
            else:
                logger.warning(
                    "ScopedHookBundle: skipping hook %s with unknown scope "
                    "(condition=%r hook_type=%r)",
                    hook.get("hook_id", "?"),
                    scope,
                    hook_type,
                )
        return bundle

    def matching(self, point: str, tool_name: str = "") -> list[dict]:
        """Return hooks for ``point`` sorted by precedence ascending.

        ``point`` is one of ``"PreToolUse"`` / ``"PostToolUse"`` / ``"Stop"``.

        For Pre/Post, the hook's ``condition`` is treated as a tool-name
        pattern (empty / ``*`` / ``prefix*`` / exact). Stop hooks ignore
        ``tool_name`` and are always included.
        """
        if point == "PreToolUse":
            bucket = self.pre
        elif point == "PostToolUse":
            bucket = self.post
        elif point == "Stop":
            bucket = self.stop
        else:
            return []

        if point in ("PreToolUse", "PostToolUse"):
            selected = [h for h in bucket if matches_tool(tool_name, h.get("condition", ""))]
        else:
            selected = list(bucket)

        # Lowest precedence runs first. Stable sort keeps frontmatter order
        # for equal precedence values.
        selected.sort(key=lambda h: int(h.get("precedence") or 0))
        return selected


def matches_tool(tool_name: str, condition: str) -> bool:
    """Match a tool name against a scoped-hook condition.

    - Empty string or ``"*"``: matches every tool.
    - Trailing ``*``: prefix match (``scada_write*`` matches
      ``scada_write_temperature``). Mirrors the Rust ``ScopedHookHandler``
      prefix semantics.
    - Any other string: exact match.
    """
    if not condition or condition == "*":
        return True
    if condition.endswith("*"):
        return tool_name.startswith(condition[:-1])
    return tool_name == condition


class ScopedCommandExecutor:
    """Async subprocess-based scoped-hook executor (ADR-V2-006)."""

    def __init__(self, timeout_secs: float = 5.0) -> None:
        self.timeout_secs = timeout_secs

    async def execute(
        self,
        command: str,
        envelope: dict,
        env_extras: dict | None = None,
    ) -> ScopedHookDecision:
        """Run ``command`` with the envelope on stdin and return the decision.

        Any failure path (timeout, spawn error, bad stdout, non-2 non-zero
        exit) returns ``allow`` per ADR §7 fail-open invariant.
        """
        stdin_bytes = json.dumps(envelope, default=str).encode("utf-8")

        env = dict(os.environ)
        if env_extras:
            # env must be dict[str, str] — coerce non-string values for safety.
            env.update({k: str(v) for k, v in env_extras.items() if v is not None})

        hook_id = envelope.get("hook_id", "")  # optional debug aid

        try:
            proc = await asyncio.create_subprocess_shell(
                command,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=env,
            )
        except Exception as exc:  # noqa: BLE001 — fail-open per ADR §7
            logger.warning(
                "Scoped hook spawn_error hook_id=%s error_kind=spawn_error exc=%s "
                "(failing open)",
                hook_id,
                exc,
            )
            return ScopedHookDecision(action="allow", reason=f"spawn_error: {exc}")

        try:
            stdout_data, stderr_data = await asyncio.wait_for(
                proc.communicate(input=stdin_bytes),
                timeout=self.timeout_secs,
            )
        except asyncio.TimeoutError:
            # SIGKILL + reap to avoid zombies; ADR §6.
            try:
                proc.kill()
            except ProcessLookupError:
                pass
            try:
                await proc.wait()
            except Exception:  # noqa: BLE001
                pass
            logger.warning(
                "Scoped hook timeout hook_id=%s error_kind=timeout timeout_secs=%s "
                "(failing open)",
                hook_id,
                self.timeout_secs,
            )
            return ScopedHookDecision(action="allow", reason="timeout")
        except Exception as exc:  # noqa: BLE001 — fail-open
            logger.warning(
                "Scoped hook io_error hook_id=%s error_kind=io_error exc=%s "
                "(failing open)",
                hook_id,
                exc,
            )
            return ScopedHookDecision(action="allow", reason=f"io_error: {exc}")

        rc = proc.returncode

        if rc == 2:
            # Deny path — stderr text becomes the reason (ADR §4).
            try:
                reason = stderr_data.decode("utf-8", errors="replace").strip()
            except Exception:  # noqa: BLE001
                reason = ""
            return ScopedHookDecision(
                action="deny",
                reason=reason or "denied by hook",
            )

        if rc == 0:
            # Allow path — stdout MAY contain a decision JSON. Empty / bad
            # JSON falls through to allow.
            try:
                text = stdout_data.decode("utf-8", errors="replace").strip()
            except Exception:  # noqa: BLE001
                text = ""
            if not text:
                return ScopedHookDecision(action="allow")
            try:
                parsed = json.loads(text)
            except (json.JSONDecodeError, ValueError) as exc:
                logger.warning(
                    "Scoped hook stdout decode_error hook_id=%s error_kind=decode_error "
                    "exc=%s (failing open)",
                    hook_id,
                    exc,
                )
                return ScopedHookDecision(action="allow")

            if not isinstance(parsed, dict):
                logger.warning(
                    "Scoped hook stdout not an object hook_id=%s (failing open)",
                    hook_id,
                )
                return ScopedHookDecision(action="allow")

            # ADR §4: accept camelCase or the documented lowercase keys.
            decision_raw = parsed.get("decision")
            reason = parsed.get("reason", "") or ""
            system_message = parsed.get("systemMessage") or parsed.get("system_message") or ""

            if isinstance(decision_raw, str):
                decision = decision_raw.lower()
            else:
                decision = "allow"

            if decision == "deny":
                return ScopedHookDecision(
                    action="deny",
                    reason=reason or "denied by hook",
                    system_message=str(system_message or ""),
                )
            if decision == "ask":
                # Orchestrator mapping: ask → deny at MVP.
                return ScopedHookDecision(
                    action="deny",
                    reason=reason or "hook requested confirmation (mapped to deny at MVP)",
                    system_message=str(system_message or ""),
                )
            # Treat "allow" and any unknown value as allow (§7 fail-open).
            return ScopedHookDecision(
                action="allow",
                reason=str(reason or ""),
                system_message=str(system_message or ""),
            )

        # Any other exit code: fail-open (ADR §4 + §7).
        try:
            stderr_snip = stderr_data.decode("utf-8", errors="replace")[:256]
        except Exception:  # noqa: BLE001
            stderr_snip = ""
        logger.warning(
            "Scoped hook exit_nonzero_nonblock hook_id=%s exit_code=%s "
            "error_kind=exit_nonzero_nonblock stderr_snip=%r (failing open)",
            hook_id,
            rc,
            stderr_snip,
        )
        return ScopedHookDecision(action="allow")


__all__ = [
    "ScopedCommandExecutor",
    "ScopedHookBundle",
    "ScopedHookDecision",
    "matches_tool",
]
