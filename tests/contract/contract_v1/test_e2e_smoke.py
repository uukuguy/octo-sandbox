"""Contract-v1 end-to-end smoke.

Five minimal round-trips that every L1 runtime MUST support. These are
the baseline health checks — if any of these fail, the runtime is not
conformant regardless of deeper category results.
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.contract_v1


def test_initialize_send_events_close_round_trip(runtime_config):
    """Full Initialize -> Send -> Events -> STOP -> Close round-trip."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_double_initialize_same_session_is_noop_or_error(runtime_config):
    """Re-initialising an existing session MUST NOT corrupt state."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_send_without_initialize_errors_cleanly(runtime_config):
    """SendRequest on an unknown session_id MUST produce a typed error."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_close_idempotent_on_already_closed_session(runtime_config):
    """CloseRequest on a closed session MUST NOT panic; MUST return Empty."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")


def test_health_stays_serving_across_turns(runtime_config):
    """Health MUST remain SERVING for the full multi-turn session lifespan."""
    pytest.xfail("awaiting S0.T4/T5 runtime_config wiring")
