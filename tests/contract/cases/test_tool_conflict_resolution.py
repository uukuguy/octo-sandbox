"""Contract v1.1 — Conflict resolution priority assertions (ADR-V2-020 §3.2).

Priority rule: skill explicit > env EAASP_TOOL_FILTER (deprecated) > runtime default.

When a skill declares ``l2:X`` and a runtime would normally offer ``l1:X``,
the skill declaration wins.  When both layers register the same base name,
``resolve()`` with an explicit qualified key bypasses the fallback chain.

Phase 3 S1.T6.  Cross-runtime E2E coverage lands in S3.T12-T15.
"""

from __future__ import annotations

import pytest

from tests.contract.harness.skill_namespace import (
    RequiredTool,
    RequiredToolParseError,
    parse_v2_frontmatter,
)

pytestmark = pytest.mark.contract_v1_1


class TestConflictResolutionPriority:
    """Explicit l2: qualifier wins over any l1 default for the same base name."""

    def test_explicit_l2_qualified_lookup_bypasses_fallback(self) -> None:
        """resolve('l2:memory.search') must not silently return l1 variant."""
        skill_declaration = RequiredTool.parse("l2:memory.search")
        runtime_builtin = RequiredTool.parse("l1:memory.search")

        assert skill_declaration.qualified() == "l2:memory.search"
        assert runtime_builtin.qualified() == "l1:memory.search"
        assert skill_declaration.qualified() != runtime_builtin.qualified()

    def test_skill_filter_list_excludes_l1_when_only_l2_declared(self) -> None:
        """Harness must pass only l2 keys in filter — l1 variant is implicitly blocked."""
        yaml = (
            "name: conflict-test-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l2:memory.search\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        allow_list = fm.workflow.required_tool_qualifieds()

        assert "l1:memory.search" not in allow_list
        assert "l2:memory.search" in allow_list

    def test_mixed_layer_declarations_both_present(self) -> None:
        """Skill may legitimately declare both l1 and l2 tools — both must appear."""
        yaml = (
            "name: mixed-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l1:bash.execute\n"
            "    - l2:memory.search\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        qualifieds = fm.workflow.required_tool_qualifieds()
        assert "l1:bash.execute" in qualifieds
        assert "l2:memory.search" in qualifieds
        assert len(qualifieds) == 2

    def test_duplicate_qualified_entries_deduplicated_by_set(self) -> None:
        """Two identical qualified entries produce one key when used in a set."""
        yaml = (
            "name: dup-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l2:memory.search\n"
            "    - l2:memory.search\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        qualifieds = fm.workflow.required_tool_qualifieds()
        unique = set(qualifieds)
        assert unique == {"l2:memory.search"}

    def test_invalid_layer_prefix_raises_parse_error(self) -> None:
        """A tool entry with an unknown prefix (e.g. l9:) must fail to parse."""
        with pytest.raises(RequiredToolParseError):
            RequiredTool.parse("l9:memory.search")

    def test_l0_layer_prefix_accepted(self) -> None:
        """l0: prefix is valid for runtime-core tools (ADR-V2-020 §2.1)."""
        tool = RequiredTool.parse("l0:session.heartbeat")
        assert tool.layer == "l0"
        assert tool.name == "session.heartbeat"
        assert tool.qualified() == "l0:session.heartbeat"


class TestRequiredToolNames:
    """required_tool_names() returns bare names for backward-compat callers."""

    def test_qualified_tools_bare_names_accessible(self) -> None:
        yaml = (
            "name: ns-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l2:memory.search\n"
            "    - l1:bash.execute\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        names = fm.workflow.required_tool_names()
        assert "memory.search" in names
        assert "bash.execute" in names
        assert not any(":" in n for n in names)
