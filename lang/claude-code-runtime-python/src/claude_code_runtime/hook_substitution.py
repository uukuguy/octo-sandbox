"""Hook-body variable substitution for v2 scoped hooks.

Mirrors the Rust helper at
`tools/eaasp-skill-registry/src/skill_parser.rs::substitute_hook_vars`.

A v2 skill hook body looks like:

    ${SKILL_DIR}/hooks/block_write_scada.sh

where `${SKILL_DIR}` is a runtime-provided directory. This module expands
those references before the runtime execs the hook. Unknown variables fail
fast so a runtime never exec's a literal `${FOO}` as a path.

Supported variables: `SKILL_DIR`, `SESSION_DIR`, `RUNTIME_DIR`.
Escape sequence: `$$` collapses to a literal `$`.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

_KNOWN_VARS: frozenset[str] = frozenset({"SKILL_DIR", "SESSION_DIR", "RUNTIME_DIR"})


@dataclass(frozen=True)
class HookVars:
    """Directory variables a runtime can expose to hook bodies."""

    skill_dir: str | None = None
    session_dir: str | None = None
    runtime_dir: str | None = None

    def _lookup(self, name: str) -> str | None:
        if name == "SKILL_DIR":
            return self.skill_dir
        if name == "SESSION_DIR":
            return self.session_dir
        if name == "RUNTIME_DIR":
            return self.runtime_dir
        return None


class HookSubstitutionError(Exception):
    """Raised when hook-var substitution cannot complete safely."""


class UnknownVariableError(HookSubstitutionError):
    """Variable name is not in the allow-list."""

    def __init__(self, name: str) -> None:
        super().__init__(
            f"unknown variable `${{{name}}}` — allowed: SKILL_DIR, SESSION_DIR, RUNTIME_DIR"
        )
        self.name = name


class UnboundVariableError(HookSubstitutionError):
    """Variable is recognized but the runtime did not provide a value."""

    def __init__(self, name: str) -> None:
        super().__init__(
            f"unbound variable `${{{name}}}` — runtime did not provide a value"
        )
        self.name = name


class MalformedVariableError(HookSubstitutionError):
    """The input contains an unterminated `${...` reference."""

    def __init__(self, index: int) -> None:
        super().__init__(
            f"malformed variable reference near index {index} (unterminated `${{`)"
        )
        self.index = index


def substitute_hook_vars(text: str, vars: HookVars) -> str:
    """Expand `${VAR}` references in `text` using `vars`.

    Raises `UnknownVariableError`, `UnboundVariableError`, or
    `MalformedVariableError` instead of silently leaving the token in place.
    """
    out: list[str] = []
    n = len(text)
    i = 0
    while i < n:
        c = text[i]
        if c == "$" and i + 1 < n and text[i + 1] == "$":
            out.append("$")
            i += 2
            continue
        if c == "$" and i + 1 < n and text[i + 1] == "{":
            end = text.find("}", i + 2)
            if end == -1:
                raise MalformedVariableError(i)
            name = text[i + 2 : end]
            if name not in _KNOWN_VARS:
                raise UnknownVariableError(name)
            value = vars._lookup(name)
            if value is None:
                raise UnboundVariableError(name)
            out.append(value)
            i = end + 1
            continue
        out.append(c)
        i += 1
    return "".join(out)


def substitute_scoped_hooks(
    hooks: list[dict[str, Any]], vars: HookVars
) -> list[dict[str, Any]]:
    """Return a new hook list with every `command` / `prompt` body substituted.

    Each hook dict is shaped like the parsed v2 frontmatter:

        {"name": "...", "type": "command", "command": "${SKILL_DIR}/..."}
        {"name": "...", "type": "prompt", "prompt": "check ${SKILL_DIR}/..."}

    Hooks without a body to substitute are passed through unchanged.
    """
    resolved: list[dict[str, Any]] = []
    for hook in hooks:
        new_hook = dict(hook)
        if "command" in new_hook and isinstance(new_hook["command"], str):
            new_hook["command"] = substitute_hook_vars(new_hook["command"], vars)
        if "prompt" in new_hook and isinstance(new_hook["prompt"], str):
            new_hook["prompt"] = substitute_hook_vars(new_hook["prompt"], vars)
        resolved.append(new_hook)
    return resolved


__all__ = [
    "HookSubstitutionError",
    "HookVars",
    "MalformedVariableError",
    "UnboundVariableError",
    "UnknownVariableError",
    "substitute_hook_vars",
    "substitute_scoped_hooks",
]
