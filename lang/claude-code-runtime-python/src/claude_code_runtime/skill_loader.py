"""Skill loader — parses SkillContent and extracts scoped hooks."""

from __future__ import annotations

import logging
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class LoadedSkill:
    """A loaded skill with parsed metadata."""

    skill_id: str
    name: str
    frontmatter: dict = field(default_factory=dict)
    prose: str = ""
    scoped_hooks: list[dict] = field(default_factory=list)

    @property
    def system_prompt_fragment(self) -> str:
        """Generate system prompt fragment from skill prose."""
        if not self.prose:
            return ""
        return f"## Skill: {self.name}\n\n{self.prose}"


class SkillLoader:
    """Loads and manages skills for a session."""

    def __init__(self):
        self._skills: dict[str, LoadedSkill] = {}

    def load(
        self,
        skill_id: str,
        name: str,
        frontmatter_yaml: str,
        prose: str,
    ) -> LoadedSkill:
        """Load a skill from SkillContent fields.

        Parses YAML frontmatter to extract scoped hooks and metadata.
        Full YAML parsing deferred to BE-D8; currently stores raw string.
        """
        frontmatter: dict = {}
        scoped_hooks: list[dict] = []

        # Minimal frontmatter parsing (key: value lines)
        if frontmatter_yaml and frontmatter_yaml.strip() not in ("---", ""):
            for line in frontmatter_yaml.strip().split("\n"):
                line = line.strip()
                if line in ("---", "") or ":" not in line:
                    continue
                key, _, value = line.partition(":")
                frontmatter[key.strip()] = value.strip()

        skill = LoadedSkill(
            skill_id=skill_id,
            name=name,
            frontmatter=frontmatter,
            prose=prose,
            scoped_hooks=scoped_hooks,
        )
        self._skills[skill_id] = skill
        logger.info("Skill loaded: %s (%s)", name, skill_id)
        return skill

    def get(self, skill_id: str) -> LoadedSkill | None:
        return self._skills.get(skill_id)

    def all_system_prompt_fragments(self) -> str:
        """Concatenate all skill prose for system prompt injection."""
        fragments = [
            s.system_prompt_fragment
            for s in self._skills.values()
            if s.system_prompt_fragment
        ]
        return "\n\n".join(fragments)

    @property
    def count(self) -> int:
        return len(self._skills)
