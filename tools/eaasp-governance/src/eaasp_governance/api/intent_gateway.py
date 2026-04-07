"""契约 2: 意图网关 API (§8.2).

POST /v1/intents — resolve user text to skill_id

Enhanced (BH-D5): configurable multi-keyword matching with weight scoring.
"""

from __future__ import annotations

import logging
import uuid
from dataclasses import dataclass
from pathlib import Path

import yaml
from fastapi import APIRouter, Request
from pydantic import BaseModel

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/v1/intents", tags=["intents"])


@dataclass
class IntentRule:
    """A single intent mapping rule."""

    keywords: list[str]
    skill_id: str
    confidence: float = 0.9
    weight: float = 1.0


class IntentResolver:
    """Configurable intent resolver with multi-keyword scoring (BH-D5).

    Scoring: for each keyword match, adds weight to the skill's score.
    Final confidence = base_confidence * (matches / total_keywords).
    """

    def __init__(self) -> None:
        self._rules: list[IntentRule] = []

    def load_config(self, config_path: str | Path) -> int:
        """Load intent mappings from YAML config. Returns rules loaded."""
        path = Path(config_path)
        if not path.exists():
            logger.warning("Intent config not found: %s", path)
            return 0

        data = yaml.safe_load(path.read_text())
        for entry in data.get("intents", []):
            self._rules.append(IntentRule(
                keywords=entry["keywords"],
                skill_id=entry["skill_id"],
                confidence=entry.get("confidence", 0.9),
                weight=entry.get("weight", 1.0),
            ))

        logger.info("Loaded %d intent rules", len(self._rules))
        return len(self._rules)

    def add_rule(self, rule: IntentRule) -> None:
        self._rules.append(rule)

    def resolve(self, text: str) -> tuple[str | None, float]:
        """Resolve text to (skill_id, confidence).

        Returns (None, 0.0) if no match.
        When multiple skills match, returns the one with highest score.
        """
        text_lower = text.lower()
        scores: dict[str, tuple[float, str]] = {}  # skill_id → (score, skill_id)

        for rule in self._rules:
            matches = sum(1 for kw in rule.keywords if kw.lower() in text_lower)
            if matches > 0:
                score = rule.confidence * (matches / len(rule.keywords)) * rule.weight
                if rule.skill_id not in scores or score > scores[rule.skill_id][0]:
                    scores[rule.skill_id] = (score, rule.skill_id)

        if not scores:
            return None, 0.0

        best_skill = max(scores.values(), key=lambda x: x[0])
        return best_skill[1], round(best_skill[0], 3)


# Global resolver (initialized in main.py)
_resolver = IntentResolver()

# Default fallback rules (used if no YAML config loaded)
_DEFAULT_RULES = [
    IntentRule(keywords=["入职", "onboarding", "新员工", "hire"], skill_id="hr-onboarding"),
    IntentRule(keywords=["离职", "offboarding", "resign", "退出"], skill_id="hr-offboarding"),
]


def get_resolver(app_state=None) -> IntentResolver:
    """Get or create the intent resolver."""
    if app_state and hasattr(app_state, "intent_resolver"):
        return app_state.intent_resolver
    return _resolver


def init_resolver(config_path: str | Path | None = None) -> IntentResolver:
    """Initialize a resolver with config or defaults."""
    resolver = IntentResolver()
    loaded = 0
    if config_path:
        loaded = resolver.load_config(config_path)

    if loaded == 0:
        for rule in _DEFAULT_RULES:
            resolver.add_rule(rule)

    return resolver


class IntentRequest(BaseModel):
    text: str
    user_id: str
    org_unit: str = ""


@router.post("")
async def resolve_intent(req: IntentRequest, request: Request):
    """Resolve user text to a skill_id via configurable multi-keyword matching."""
    resolver = get_resolver(request.app.state)

    skill_id, confidence = resolver.resolve(req.text)

    return {
        "intent_id": f"int-{uuid.uuid4().hex[:8]}",
        "skill_id": skill_id,
        "confidence": confidence,
        "skill_name": skill_id,
    }
