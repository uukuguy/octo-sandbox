"""Tests for eaasp.cli — CLI commands (init/validate/test/submit)."""

import json
import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from click.testing import CliRunner

from eaasp.cli.__main__ import main


# ── Fixtures ──


@pytest.fixture
def runner():
    return CliRunner()


@pytest.fixture
def example_skill_dir():
    """Path to the hr-onboarding example skill."""
    # Navigate from tests/ up to sdk/python/, then to sdk/examples/
    return Path(__file__).resolve().parent.parent.parent / "examples" / "hr-onboarding"


# ── Basic CLI tests ──


class TestCLIHelp:
    def test_main_help(self, runner):
        result = runner.invoke(main, ["--help"])
        assert result.exit_code == 0
        assert "EAASP Enterprise SDK" in result.output

    def test_main_version(self, runner):
        result = runner.invoke(main, ["--version"])
        assert result.exit_code == 0
        assert "0.1.0" in result.output

    def test_init_help(self, runner):
        result = runner.invoke(main, ["init", "--help"])
        assert result.exit_code == 0
        assert "NAME" in result.output

    def test_validate_help(self, runner):
        result = runner.invoke(main, ["validate", "--help"])
        assert result.exit_code == 0
        assert "PATH" in result.output

    def test_test_help(self, runner):
        result = runner.invoke(main, ["test", "--help"])
        assert result.exit_code == 0
        assert "--sandbox" in result.output

    def test_submit_help(self, runner):
        result = runner.invoke(main, ["submit", "--help"])
        assert result.exit_code == 0
        assert "--registry" in result.output


# ── Init command ──


class TestInitCmd:
    def test_init_creates_skill_dir(self, runner):
        with tempfile.TemporaryDirectory() as tmpdir:
            result = runner.invoke(main, ["init", "my-skill", "--output-dir", tmpdir])
            assert result.exit_code == 0
            assert "Created Skill project" in result.output

            skill_dir = Path(tmpdir) / "my-skill"
            assert skill_dir.exists()
            assert (skill_dir / "SKILL.md").exists()
            assert (skill_dir / "hooks").is_dir()
            assert (skill_dir / "tests").is_dir()

    def test_init_workflow_type(self, runner):
        with tempfile.TemporaryDirectory() as tmpdir:
            result = runner.invoke(main, ["init", "test-wf", "--type", "workflow", "--output-dir", tmpdir])
            assert result.exit_code == 0
            skill_md = (Path(tmpdir) / "test-wf" / "SKILL.md").read_text()
            assert "workflow" in skill_md

    def test_init_production_type(self, runner):
        with tempfile.TemporaryDirectory() as tmpdir:
            result = runner.invoke(main, ["init", "test-prod", "--type", "production", "--output-dir", tmpdir])
            assert result.exit_code == 0
            skill_md = (Path(tmpdir) / "test-prod" / "SKILL.md").read_text()
            assert "production" in skill_md


# ── Validate command ──


class TestValidateCmd:
    def test_validate_example_skill(self, runner, example_skill_dir):
        if not example_skill_dir.exists():
            pytest.skip("Example skill not found")
        result = runner.invoke(main, ["validate", str(example_skill_dir)])
        assert result.exit_code == 0
        assert "Validation passed" in result.output

    def test_validate_broken_skill(self, runner):
        with tempfile.TemporaryDirectory() as tmpdir:
            broken = Path(tmpdir) / "SKILL.md"
            # Use space-only fields so Pydantic accepts them but our validator catches empties
            broken.write_text(
                "---\nname: ' '\ndescription: ' '\nauthor: ' '\nskill_type: workflow\nscope: team\nhooks: []\n---\n\nshort",
                encoding="utf-8",
            )
            result = runner.invoke(main, ["validate", str(broken)])
            assert result.exit_code == 1
            assert "required_fields" in result.output or "prose_length" in result.output

    def test_validate_dir_with_skill_md(self, runner):
        with tempfile.TemporaryDirectory() as tmpdir:
            skill_md = Path(tmpdir) / "SKILL.md"
            skill_md.write_text(
                "---\nname: test\nversion: '1.0.0'\ndescription: A valid test skill\nauthor: tester\n"
                "tags: [test]\nskill_type: workflow\nscope: team\nhooks:\n"
                "  - event: Stop\n    handler_type: prompt\n    config:\n      prompt: check\n"
                "---\n\n"
                "You are a test assistant that helps validate the CLI works correctly in all cases.\n",
                encoding="utf-8",
            )
            result = runner.invoke(main, ["validate", tmpdir])
            assert result.exit_code == 0


# ── Submit command ──


class TestSubmitCmd:
    def test_submit_with_mock_httpx(self, runner, example_skill_dir):
        if not example_skill_dir.exists():
            pytest.skip("Example skill not found")

        mock_response = MagicMock()
        mock_response.json.return_value = {"id": "hr-onboarding", "status": "draft"}
        mock_response.raise_for_status = MagicMock()

        with patch("httpx.post", return_value=mock_response) as mock_post:
            result = runner.invoke(
                main, ["submit", str(example_skill_dir), "--registry", "http://localhost:8081"]
            )
            assert result.exit_code == 0
            assert "Submitted: hr-onboarding" in result.output
            assert "Status: draft" in result.output

            # Verify POST was called with correct URL
            mock_post.assert_called_once()
            call_args = mock_post.call_args
            assert "/api/v1/skills/draft" in call_args[0][0] or "/api/v1/skills/draft" in str(call_args)

            # Verify body content
            body = call_args[1]["json"] if "json" in call_args[1] else call_args.kwargs["json"]
            assert body["name"] == "hr-onboarding"
            assert body["author"] == "hr-team"
            assert "prose" in body

    def test_submit_validation_failure(self, runner):
        with tempfile.TemporaryDirectory() as tmpdir:
            broken = Path(tmpdir) / "SKILL.md"
            broken.write_text(
                "---\nname: ' '\nversion: '1.0.0'\ndescription: ' '\nauthor: ' '\nskill_type: workflow\nscope: team\nhooks: []\n---\n\nshort",
                encoding="utf-8",
            )
            result = runner.invoke(
                main, ["submit", str(broken), "--registry", "http://localhost:8081"]
            )
            assert result.exit_code == 1
            assert "Validation failed" in result.output


# ── Example Skill integration ──


class TestExampleSkill:
    def test_hr_onboarding_parses(self, example_skill_dir):
        if not example_skill_dir.exists():
            pytest.skip("Example skill not found")
        from eaasp.authoring.skill_parser import SkillParser

        skill = SkillParser.parse_file(example_skill_dir / "SKILL.md")
        assert skill.frontmatter.name == "hr-onboarding"
        assert skill.frontmatter.skill_type == "workflow"
        assert len(skill.frontmatter.hooks) == 2
        assert skill.frontmatter.hooks[0].event == "PreToolUse"
        assert skill.frontmatter.hooks[1].event == "Stop"

    def test_hr_onboarding_validates(self, example_skill_dir):
        if not example_skill_dir.exists():
            pytest.skip("Example skill not found")
        from eaasp.authoring.skill_parser import SkillParser
        from eaasp.authoring.skill_validator import SkillValidator

        skill = SkillParser.parse_file(example_skill_dir / "SKILL.md")
        result = SkillValidator.validate(skill)
        assert result.valid

    def test_hr_onboarding_test_cases(self, example_skill_dir):
        if not example_skill_dir.exists():
            pytest.skip("Example skill not found")
        cases_file = example_skill_dir / "tests" / "test_cases.jsonl"
        assert cases_file.exists()
        cases = [json.loads(line) for line in cases_file.read_text().strip().splitlines()]
        assert len(cases) == 3
        assert all("input" in c for c in cases)

    def test_check_pii_hook(self, example_skill_dir):
        if not example_skill_dir.exists():
            pytest.skip("Example skill not found")
        # Import and test the PII check function directly
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "check_pii", example_skill_dir / "hooks" / "check_pii.py"
        )
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)

        # Should detect SSN
        assert len(mod.check_pii("SSN: 123-45-6789")) > 0
        # Should detect Chinese phone
        assert len(mod.check_pii("phone: 13812345678")) > 0
        # Clean text
        assert len(mod.check_pii("Hello World")) == 0
