"""L2 Skill Registry lightweight client.

Only implements ``submit_draft`` — the minimal API needed by ``eaasp submit``.
"""

from __future__ import annotations

import yaml

from eaasp.models.skill import Skill


class SkillRegistryClient:
    """Thin HTTP client for the L2 Skill Registry REST API."""

    def __init__(self, base_url: str) -> None:
        self.base_url = base_url.rstrip("/")

    def submit_draft(self, skill: Skill) -> dict:
        """POST a Skill as a draft to ``/api/v1/skills/draft``.

        Returns the JSON response body as a dict.
        """
        import httpx

        fm = skill.frontmatter
        body = {
            "id": fm.name,
            "name": fm.name,
            "description": fm.description,
            "version": fm.version,
            "author": fm.author,
            "tags": fm.tags,
            "frontmatter_yaml": yaml.dump(
                fm.model_dump(exclude_none=True),
                default_flow_style=False,
                allow_unicode=True,
                sort_keys=False,
            ),
            "prose": skill.prose,
        }
        resp = httpx.post(
            f"{self.base_url}/api/v1/skills/draft",
            json=body,
            timeout=30.0,
        )
        resp.raise_for_status()
        return resp.json()
