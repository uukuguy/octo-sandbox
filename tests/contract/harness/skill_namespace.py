"""Python mirror of the Rust skill_parser namespace types (ADR-V2-020).

This module reimplements the ``RequiredTool`` parsing contract in Python so
that contract-v1.1 tests can verify namespace semantics without requiring
Rust FFI.  The logic must stay byte-identical to
``tools/eaasp-skill-registry/src/skill_parser.rs`` — any divergence is a
contract violation and must be caught by the Rust unit tests in that crate.

Valid layer prefixes: ``l0``, ``l1``, ``l2``.
Format: ``{layer}:{name}``  or  bare ``{name}`` (no colon, layer = None).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import yaml

VALID_LAYERS = frozenset({"l0", "l1", "l2"})


class RequiredToolParseError(ValueError):
    """Raised when a tool entry has an unrecognised layer prefix."""


@dataclass
class RequiredTool:
    name: str
    layer: str | None = None

    @classmethod
    def parse(cls, s: str) -> "RequiredTool":
        if ":" in s:
            layer_str, name = s.split(":", 1)
            if layer_str not in VALID_LAYERS:
                raise RequiredToolParseError(
                    f"Invalid layer prefix {layer_str!r} in {s!r}; "
                    f"expected one of {sorted(VALID_LAYERS)}"
                )
            return cls(name=name, layer=layer_str)
        return cls(name=s, layer=None)

    def qualified(self) -> str:
        if self.layer is not None:
            return f"{self.layer}:{self.name}"
        return self.name


@dataclass
class WorkflowMetadata:
    required_tools: list[RequiredTool] = field(default_factory=list)

    def required_tool_names(self) -> list[str]:
        return [t.name for t in self.required_tools]

    def required_tool_qualifieds(self) -> list[str]:
        return [t.qualified() for t in self.required_tools]


@dataclass
class V2Frontmatter:
    name: str | None = None
    version: str | None = None
    author: str | None = None
    access_scope: str | None = None
    workflow: WorkflowMetadata | None = None
    dependencies: list[str] = field(default_factory=list)


def parse_v2_frontmatter(yaml_text: str) -> V2Frontmatter:
    """Parse a SKILL.md frontmatter YAML string into V2Frontmatter.

    Mirrors the Rust ``parse_v2_frontmatter`` contract:
    - Empty / whitespace-only input raises ``ValueError``.
    - Missing optional fields default to None / empty lists.
    - ``workflow.required_tools`` entries are parsed via ``RequiredTool.parse``.
    """
    if not yaml_text.strip():
        raise ValueError("Empty frontmatter")

    data: dict[str, Any] = yaml.safe_load(yaml_text) or {}

    workflow: WorkflowMetadata | None = None
    wf_raw = data.get("workflow")
    if wf_raw is not None:
        rt_raw: list[str] = wf_raw.get("required_tools") or []
        workflow = WorkflowMetadata(
            required_tools=[RequiredTool.parse(e) for e in rt_raw]
        )

    return V2Frontmatter(
        name=data.get("name"),
        version=data.get("version"),
        author=data.get("author"),
        access_scope=data.get("access_scope"),
        workflow=workflow,
        dependencies=data.get("dependencies") or [],
    )
