"""Smoke tests for the contract-suite harness (plan §S0.T1 step 7)."""

from __future__ import annotations


def test_runtime_launcher_importable():
    from tests.contract.harness import runtime_launcher

    assert hasattr(runtime_launcher, "RuntimeLauncher")
    assert hasattr(runtime_launcher, "RuntimeConfig")


def test_mock_openai_server_importable():
    from tests.contract.harness import mock_openai_server

    assert hasattr(mock_openai_server, "build_app")
    app = mock_openai_server.build_app()
    # Minimal structural check: FastAPI routes include /v1/chat/completions.
    paths = {route.path for route in app.routes}  # type: ignore[attr-defined]
    assert "/v1/chat/completions" in paths
    assert "/health" in paths


def test_assertions_helpers_importable():
    from tests.contract.harness import assertions

    assert assertions.EVENT_TYPES_V1 == frozenset(
        {
            "CHUNK",
            "TOOL_CALL",
            "TOOL_RESULT",
            "STOP",
            "ERROR",
            "HOOK_FIRED",
            "PRE_COMPACT",
        }
    )
    assert assertions.HOOK_EVENTS_V1 == frozenset(
        {"PreToolUse", "PostToolUse", "Stop"}
    )


def test_runtime_config_dataclass_shape():
    from tests.contract.harness.runtime_launcher import RuntimeConfig

    cfg = RuntimeConfig(
        name="grid",
        launch_cmd=["cargo", "run", "-p", "grid-runtime"],
        grpc_port=50061,
    )
    assert cfg.name == "grid"
    assert cfg.grpc_port == 50061
    assert cfg.env == {}
    assert cfg.startup_timeout_s == 30.0


def test_hook_envelope_assertion_rejects_missing_field():
    from tests.contract.harness.assertions import assert_hook_envelope_required_fields

    bad_envelope = {
        "event": "PreToolUse",
        "session_id": "s1",
        # missing: skill_id, tool_name, tool_args, created_at
    }
    try:
        assert_hook_envelope_required_fields(bad_envelope, "PreToolUse")
    except AssertionError as err:
        msg = str(err)
        assert "skill_id" in msg
        assert "tool_name" in msg
        return
    raise AssertionError("expected AssertionError for incomplete envelope")


def test_grid_env_vars_assertion_rejects_missing():
    from tests.contract.harness.assertions import assert_grid_env_vars_present

    try:
        assert_grid_env_vars_present(
            {"GRID_SESSION_ID": "s1", "GRID_EVENT": "Stop"}, "Stop"
        )
    except AssertionError as err:
        msg = str(err)
        assert "GRID_TOOL_NAME" in msg
        assert "GRID_SKILL_ID" in msg
        return
    raise AssertionError("expected AssertionError for missing GRID_* vars")
