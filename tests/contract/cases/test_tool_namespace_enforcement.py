"""Contract v1.1 — Namespace enforcement assertions (ADR-V2-020 §3).

A skill that declares ``workflow.required_tools`` with ``l2:`` prefix MUST
route to the MCP-provided layer and NOT fall through to an L1 built-in with
the same base name.  These tests exercise the Python mirror of the Rust
skill_parser contract in isolation — no live runtime needed.

Phase 3 S1.T6.  Cross-runtime E2E coverage lands in S3.T12-T15.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from tests.contract.harness.skill_namespace import RequiredTool, parse_v2_frontmatter

pytestmark = pytest.mark.contract_v1_1


# ---------------------------------------------------------------------------
# Skill-registry parser: namespace syntax
# ---------------------------------------------------------------------------

class TestRequiredToolParsing:
    """parse_v2_frontmatter correctly decodes namespace-qualified tool entries."""

    def test_l2_qualified_name_parsed(self) -> None:
        yaml = (
            "name: test-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l2:memory.search\n"
            "    - l2:memory.write_file\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        tools = fm.workflow.required_tools
        assert len(tools) == 2
        assert tools[0].layer == "l2"
        assert tools[0].name == "memory.search"
        assert tools[1].layer == "l2"
        assert tools[1].name == "memory.write_file"

    def test_l1_qualified_name_parsed(self) -> None:
        yaml = (
            "name: test-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l1:bash.execute\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        tools = fm.workflow.required_tools
        assert len(tools) == 1
        assert tools[0].layer == "l1"
        assert tools[0].name == "bash.execute"

    def test_qualified_returns_correct_qualified_string(self) -> None:
        yaml = (
            "name: test-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l2:memory.search\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        qualifieds = fm.workflow.required_tool_qualifieds()
        assert qualifieds == ["l2:memory.search"]

    def test_skill_extraction_all_tools_are_l2(self) -> None:
        """skill-extraction canonical skill: all 4 tools carry l2 prefix (ADR-V2-020 §4)."""
        _REPO = Path(__file__).resolve().parent.parent.parent.parent
        skill_path = _REPO / "examples" / "skills" / "skill-extraction" / "SKILL.md"
        content = skill_path.read_text()
        rest = content[4:]  # strip leading ---\n
        end = rest.find("\n---\n")
        fm_yaml = rest[:end + 1]

        fm = parse_v2_frontmatter(fm_yaml)
        assert fm.workflow is not None
        assert all(t.layer == "l2" for t in fm.workflow.required_tools), (
            "All skill-extraction required_tools must carry l2 prefix per ADR-V2-020"
        )


# ---------------------------------------------------------------------------
# Namespace enforcement: l2-declared tool must NOT resolve as l1
# ---------------------------------------------------------------------------

class TestNamespaceRoutingIsolation:
    """l2:X and l1:X are distinct entries — declaring l2 excludes l1 resolution."""

    def test_l2_qualifier_does_not_match_l1_key(self) -> None:
        """qualified('l2:memory.search') != qualified('l1:memory.search')."""
        l2_tool = RequiredTool.parse("l2:memory.search")
        l1_tool = RequiredTool.parse("l1:memory.search")

        assert l2_tool.qualified() != l1_tool.qualified()
        assert l2_tool.layer == "l2"
        assert l1_tool.layer == "l1"
        assert l2_tool.name == l1_tool.name  # same base name, different layer

    def test_l2_and_l1_same_name_are_distinct_keys(self) -> None:
        """Two tools with same base name but different layers produce different keys."""
        tools = [
            RequiredTool.parse("l1:memory.search"),
            RequiredTool.parse("l2:memory.search"),
        ]
        keys = {t.qualified() for t in tools}
        assert len(keys) == 2, "l1 and l2 variants must be distinct registry keys"

    def test_skill_required_tools_filter_preserves_layer(self) -> None:
        """required_tool_qualifieds() returns exact qualified strings for filtering."""
        yaml = (
            "name: mcp-only-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - l2:memory.search\n"
            "    - l2:memory.read\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        qualifieds = fm.workflow.required_tool_qualifieds()
        # The filter list passed to harness must contain exactly these keys,
        # which means l1:memory.search is NOT in the allow-list.
        assert "l2:memory.search" in qualifieds
        assert "l2:memory.read" in qualifieds
        assert "l1:memory.search" not in qualifieds
        assert "l1:memory.read" not in qualifieds
