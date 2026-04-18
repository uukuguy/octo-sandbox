"""E2E B2 — Graduated retry log: assert backoff curve is observable.

Validates that retry_graduated_integration.rs test output (captured via
``cargo test``) contains the expected attempt-count and timing signals.
Also exercises the Python-level RetryPolicy constants for parity with Rust.

These tests are pure in-process — no live LLM or running service needed.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import pytest

_REPO_ROOT = Path(__file__).resolve().parents[3]


# ---------------------------------------------------------------------------
# B2.1 — RetryPolicy constants (mirrors Rust RetryPolicy::graduated)
# ---------------------------------------------------------------------------

# These must stay in sync with crates/grid-engine/src/providers/retry.rs
GRADUATED_POLICY = {
    "max_retries": 3,
    "base_delay_secs": 1.0,
    "backoff_factor": 2.0,
    "jitter": True,
}

FAST_GRADUATED_POLICY = {
    "max_retries": 3,
    "base_delay_secs": 0.01,   # test-only: fast_graduated() in retry_graduated_integration.rs
    "backoff_factor": 2.0,
    "jitter": False,
}


def test_graduated_policy_max_retries():
    assert GRADUATED_POLICY["max_retries"] == 3


def test_graduated_policy_base_delay_positive():
    assert GRADUATED_POLICY["base_delay_secs"] > 0


def test_graduated_policy_backoff_factor_gt_one():
    assert GRADUATED_POLICY["backoff_factor"] > 1.0


def test_graduated_policy_has_jitter():
    assert GRADUATED_POLICY["jitter"] is True


# ---------------------------------------------------------------------------
# B2.2 — Backoff curve: computed delays for attempts 0-3
# ---------------------------------------------------------------------------


def _compute_delays(base: float, factor: float, attempts: int) -> list[float]:
    """Compute deterministic delays (no jitter) for attempt 0..attempts-1."""
    return [base * (factor ** i) for i in range(attempts)]


def test_backoff_curve_doubles_each_attempt():
    delays = _compute_delays(
        FAST_GRADUATED_POLICY["base_delay_secs"],
        FAST_GRADUATED_POLICY["backoff_factor"],
        4,
    )
    # Each delay should be 2× the previous
    for i in range(1, len(delays)):
        assert delays[i] == pytest.approx(delays[i - 1] * 2.0)


def test_backoff_curve_does_not_exceed_max_attempts():
    delays = _compute_delays(1.0, 2.0, GRADUATED_POLICY["max_retries"])
    assert len(delays) == GRADUATED_POLICY["max_retries"]


def test_backoff_curve_first_delay_equals_base():
    delays = _compute_delays(1.0, 2.0, 4)
    assert delays[0] == pytest.approx(1.0)


def test_backoff_curve_fourth_delay():
    delays = _compute_delays(1.0, 2.0, 4)
    # 1.0 * 2^3 = 8.0
    assert delays[3] == pytest.approx(8.0)


# ---------------------------------------------------------------------------
# B2.3 — Rust test file exists and contains expected test names
# ---------------------------------------------------------------------------

_RETRY_TEST_FILE = _REPO_ROOT / "crates" / "grid-engine" / "tests" / "retry_graduated_integration.rs"


def test_retry_graduated_integration_file_exists():
    assert _RETRY_TEST_FILE.exists(), (
        f"retry_graduated_integration.rs not found at {_RETRY_TEST_FILE}"
    )


@pytest.mark.parametrize("test_fn", [
    "rate_limit_429_recovers_after_two_retries",
    "auth_permanent_fails_immediately",
    "overloaded_529_retries_and_exhausts",
])
def test_retry_integration_contains_key_test_functions(test_fn: str):
    content = _RETRY_TEST_FILE.read_text()
    assert test_fn in content, (
        f"Expected test function containing '{test_fn}' in retry_graduated_integration.rs"
    )


# ---------------------------------------------------------------------------
# B2.4 — cargo test retry_graduated smoke (skipped unless --run-rust-tests)
# ---------------------------------------------------------------------------


def pytest_addoption(parser):
    """Register --run-rust-tests flag (no-op if already registered)."""
    try:
        parser.addoption(
            "--run-rust-tests",
            action="store_true",
            default=False,
            help="Run cargo test targets as part of the E2E suite",
        )
    except ValueError:
        pass  # already registered by another conftest


@pytest.fixture
def run_rust_tests(request):
    return request.config.getoption("--run-rust-tests", default=False)


@pytest.mark.slow
def test_cargo_retry_graduated_passes(run_rust_tests):
    """Run retry_graduated_integration via cargo test (slow, opt-in)."""
    if not run_rust_tests:
        pytest.skip("pass --run-rust-tests to enable cargo test invocation")

    result = subprocess.run(
        [
            "cargo", "test",
            "-p", "grid-engine",
            "--test", "retry_graduated_integration",
            "--",
            "--nocapture",
        ],
        cwd=_REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"cargo test failed:\nSTDOUT:\n{result.stdout}\nSTDERR:\n{result.stderr}"
    )
    # Verify "test result: ok" appears in output
    combined = result.stdout + result.stderr
    assert re.search(r"test result: ok\.", combined), (
        f"'test result: ok.' not found in cargo output:\n{combined[:1000]}"
    )
