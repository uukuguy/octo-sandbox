"""Contract v1.1 — Pre-Phase 3 skill backward-compatibility assertions (ADR-V2-020 §5).

Skills written before Phase 3 (no namespace prefix) must still work via the
fallback chain: L2 → L1 → L0 → bare.  The parser must accept bare names and
produce RequiredTool entries with layer=None.

Phase 3 S1.T6.  Cross-runtime E2E coverage lands in S3.T12-T15.
"""

from __future__ import annotations

import pytest

from tests.contract.harness.skill_namespace import RequiredTool, parse_v2_frontmatter

pytestmark = pytest.mark.contract_v1_1


class TestBareNameBackwardCompat:
    """Unprefixed tool names parse correctly and carry layer=None."""

    def test_bare_name_parses_with_none_layer(self) -> None:
        tool = RequiredTool.parse("memory_search")
        assert tool.layer is None
        assert tool.name == "memory_search"

    def test_bare_name_qualified_equals_name(self) -> None:
        """qualified() on a bare-name tool returns just the name (no prefix)."""
        tool = RequiredTool.parse("bash_execute")
        assert tool.qualified() == "bash_execute"

    def test_pre_phase3_skill_yaml_parsed_without_error(self) -> None:
        """A v1.0 skill YAML with bare required_tools parses without exception."""
        yaml = (
            "name: legacy-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - memory_search\n"
            "    - memory_read\n"
            "    - memory_write_anchor\n"
            "    - memory_write_file\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        tools = fm.workflow.required_tools
        assert len(tools) == 4
        for t in tools:
            assert t.layer is None

    def test_pre_phase3_tool_names_accessible_as_bare(self) -> None:
        """required_tool_names() on bare-name tools returns names unchanged."""
        yaml = (
            "name: legacy-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - memory_search\n"
            "    - bash_execute\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        names = fm.workflow.required_tool_names()
        assert names == ["memory_search", "bash_execute"]

    def test_pre_phase3_qualifieds_returns_bare_strings(self) -> None:
        """required_tool_qualifieds() on bare-name tools returns names without prefix."""
        yaml = (
            "name: legacy-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - memory_search\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        qualifieds = fm.workflow.required_tool_qualifieds()
        assert qualifieds == ["memory_search"]
        assert not any(":" in q for q in qualifieds)

    def test_no_workflow_section_is_valid(self) -> None:
        """Skills without a workflow section have workflow=None (pre-Phase 3 default)."""
        yaml = "name: minimal-skill\nversion: 0.1.0\n"
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is None

    def test_empty_required_tools_is_valid(self) -> None:
        """An explicit empty required_tools list is valid — means no filter applied."""
        yaml = (
            "name: open-skill\n"
            "version: 0.1.0\n"
            "workflow:\n"
            "  required_tools: []\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        assert fm.workflow.required_tools == []


class TestMixedLegacyAndNamespaced:
    """A skill may mix bare and namespaced entries during the migration window."""

    def test_mixed_bare_and_namespaced_parses(self) -> None:
        yaml = (
            "name: migrating-skill\n"
            "version: 0.2.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - memory_search\n"
            "    - l2:memory.write_file\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        tools = fm.workflow.required_tools
        assert len(tools) == 2

        bare = tools[0]
        assert bare.layer is None
        assert bare.name == "memory_search"

        ns = tools[1]
        assert ns.layer == "l2"
        assert ns.name == "memory.write_file"

    def test_mixed_qualifieds_preserves_both_forms(self) -> None:
        yaml = (
            "name: migrating-skill\n"
            "version: 0.2.0\n"
            "workflow:\n"
            "  required_tools:\n"
            "    - old_tool_name\n"
            "    - l1:bash.execute\n"
        )
        fm = parse_v2_frontmatter(yaml)
        assert fm.workflow is not None
        qualifieds = fm.workflow.required_tool_qualifieds()
        assert "old_tool_name" in qualifieds
        assert "l1:bash.execute" in qualifieds
