"""Contract-suite pytest config and shared fixtures.

Fixtures in this file intentionally stay runtime-agnostic. Per-runtime
config (``launch_cmd``, ``grpc_port``, env vars) is injected by
``--runtime`` at invocation time. Tasks S0.T4/T5 will backfill the
actual :class:`RuntimeConfig` objects; for now the ``runtime_config``
fixture raises ``NotImplementedError`` so that T2 RED tests don't
silently pass against a non-existent runtime.
"""

from __future__ import annotations

import pytest

from tests.contract.harness.runtime_launcher import RuntimeConfig


# ---------------------------------------------------------------------------
# Marker registration
# ---------------------------------------------------------------------------


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers",
        "contract_v1: Part of the frozen contract-v1 cross-runtime suite "
        "(see ADR-V2-017 §2 / plan §S0.T2).",
    )


# ---------------------------------------------------------------------------
# CLI options
# ---------------------------------------------------------------------------


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--runtime",
        action="store",
        default=None,
        choices=["grid", "claude-code", "goose", "nanobot"],
        help=(
            "Runtime under test. Required by contract_v1/ tests; smoke tests "
            "under tests/contract/test_harness_smoke.py do not consult it."
        ),
    )


# ---------------------------------------------------------------------------
# Session-scoped fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(scope="session")
def runtime_name(request: pytest.FixtureRequest) -> str:
    """Runtime identifier passed via ``--runtime``.

    Skips the test cleanly if ``--runtime`` was not supplied — this lets
    the T1 smoke test run without --runtime while keeping T2 contract_v1
    cases explicit about their runtime dependency.
    """
    value = request.config.getoption("--runtime")
    if value is None:
        pytest.skip("--runtime not supplied; contract_v1 requires --runtime=<name>")
    return value


@pytest.fixture(scope="session")
def runtime_config(runtime_name: str) -> RuntimeConfig:
    """Resolve the :class:`RuntimeConfig` for ``runtime_name``.

    NOTE: deliberately unimplemented in S0.T1 — concrete configs land in
    S0.T4 (grid) and S0.T5 (claude-code). Contract-v1 tests that reach
    this fixture will error out loudly rather than silently skip.
    """
    raise NotImplementedError(
        f"RuntimeConfig for {runtime_name!r} not yet wired; "
        "see plan §S0.T4 / §S0.T5."
    )


# ---------------------------------------------------------------------------
# Hook-envelope capture stubs (filled in by T4/T5)
# ---------------------------------------------------------------------------


@pytest.fixture
def trigger_pre_tool_use_hook():
    """Trigger a PreToolUse hook against the runtime and return its envelope.

    The concrete implementation of this fixture (real skill + bash hook
    that dumps stdin JSON and env vars to a temp file) is the contract
    of S0.T4 per plan. For S0.T2, tests depending on this fixture use
    :func:`pytest.xfail` so the RED state is deterministic.
    """
    pytest.xfail(
        "hook-envelope capture fixture awaits S0.T4 (grid) / S0.T5 (claude-code); "
        "see plan §S0.T2 RED policy."
    )


@pytest.fixture
def trigger_post_tool_use_hook():
    """PostToolUse analog of :func:`trigger_pre_tool_use_hook`."""
    pytest.xfail(
        "hook-envelope capture fixture awaits S0.T4 (grid) / S0.T5 (claude-code); "
        "see plan §S0.T2 RED policy."
    )


@pytest.fixture
def trigger_stop_hook():
    """Stop analog of :func:`trigger_pre_tool_use_hook`."""
    pytest.xfail(
        "hook-envelope capture fixture awaits S0.T4 (grid) / S0.T5 (claude-code); "
        "see plan §S0.T2 RED policy."
    )
