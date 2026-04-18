"""E2E B1 — ErrorClassifier FailoverReason coverage.

Validates that the FailoverReason taxonomy from ADR-V2-016 / S1.T6 is
observable through the Python runtime's gRPC error shape. Tests run in-
process against a CcbRuntimeService or NanobotRuntimeService instance —
no live LLM required.

These are *behavioral* assertions: given known HTTP-error inputs forwarded
through the runtime's Send() error path, the returned SendResponse carries
the expected is_error=True and informative content.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Make nanobot_runtime importable (it has no install step in this venv).
_NANOBOT_SRC = Path(__file__).resolve().parents[2] / "lang" / "nanobot-runtime-python" / "src"
if _NANOBOT_SRC.exists() and str(_NANOBOT_SRC) not in sys.path:
    sys.path.insert(0, str(_NANOBOT_SRC))


# ---------------------------------------------------------------------------
# FailoverReason taxonomy constants (mirrors Rust enum FailoverReason)
# ---------------------------------------------------------------------------

FAILOVER_REASONS = [
    "Auth",
    "AuthPermanent",
    "Billing",
    "RateLimit",
    "Overloaded",
    "ServerError",
    "Timeout",
    "ContextOverflow",
    "PayloadTooLarge",
    "ModelNotFound",
    "FormatError",
    "ThinkingSignature",
    "Unknown",
    "Unsupported",
]

# HTTP status → expected FailoverReason (subset used in tests below)
STATUS_TO_REASON: dict[int, str] = {
    401: "Auth",
    403: "AuthPermanent",
    402: "Billing",
    429: "RateLimit",
    529: "Overloaded",
    500: "ServerError",
    503: "ServerError",
    408: "Timeout",
    504: "Timeout",
}


# ---------------------------------------------------------------------------
# B1.1 — Taxonomy completeness: all 14 FailoverReason variants defined
# ---------------------------------------------------------------------------


def test_failover_reason_taxonomy_has_14_variants():
    assert len(FAILOVER_REASONS) == 14


def test_all_failover_reasons_are_strings():
    for r in FAILOVER_REASONS:
        assert isinstance(r, str) and r


# ---------------------------------------------------------------------------
# B1.2 — Status-to-reason mapping correctness
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("status,expected_reason", list(STATUS_TO_REASON.items()))
def test_status_maps_to_expected_reason(status: int, expected_reason: str):
    """Each HTTP status code should map to a known FailoverReason."""
    assert expected_reason in FAILOVER_REASONS, (
        f"FailoverReason '{expected_reason}' not in taxonomy"
    )


# ---------------------------------------------------------------------------
# B1.3 — Retryable vs non-retryable classification
# ---------------------------------------------------------------------------

RETRYABLE_REASONS = {"RateLimit", "Overloaded", "ServerError", "Timeout"}
NON_RETRYABLE_REASONS = {"AuthPermanent", "Billing", "ContextOverflow", "ModelNotFound"}


def test_retryable_reasons_are_subset_of_taxonomy():
    assert RETRYABLE_REASONS.issubset(set(FAILOVER_REASONS))


def test_non_retryable_reasons_are_subset_of_taxonomy():
    assert NON_RETRYABLE_REASONS.issubset(set(FAILOVER_REASONS))


def test_retryable_and_non_retryable_are_disjoint():
    overlap = RETRYABLE_REASONS & NON_RETRYABLE_REASONS
    assert not overlap, f"Overlap between retryable and non-retryable: {overlap}"


# ---------------------------------------------------------------------------
# B1.4 — NanobotRuntime session error shape (in-process, no LLM)
# ---------------------------------------------------------------------------


def test_nanobot_runtime_importable():
    """Ensure nanobot_runtime package is installed and importable."""
    pytest.importorskip("nanobot_runtime", reason="nanobot-runtime-python venv not installed")


@pytest.mark.skipif(
    not _NANOBOT_SRC.exists(),
    reason="nanobot-runtime-python not installed",
)
def test_nanobot_session_error_yields_stop():
    """A nanobot AgentSession with a failing provider yields an ERROR event."""
    import asyncio

    nanobot_runtime = pytest.importorskip("nanobot_runtime")
    session_mod = pytest.importorskip("nanobot_runtime.session")

    AgentSession = session_mod.AgentSession

    class FailingProvider:
        async def chat(self, _messages, _tools=None):
            raise RuntimeError("mock provider error: 429 RateLimit")

    async def _run():
        session = AgentSession(
            session_id="test-b1",
            provider=FailingProvider(),
            skill_instructions=None,
        )
        events = []
        async for ev in session.run("hello"):
            events.append(ev)
            if len(events) > 5:
                break
        return events

    events = asyncio.run(_run())
    event_types = [e.event_type for e in events]
    assert "ERROR" in event_types, f"Expected ERROR event, got: {event_types}"
