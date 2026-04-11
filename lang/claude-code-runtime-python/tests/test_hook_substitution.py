from __future__ import annotations

import pytest

from claude_code_runtime.hook_substitution import (
    HookVars,
    MalformedVariableError,
    UnboundVariableError,
    UnknownVariableError,
    substitute_hook_vars,
    substitute_scoped_hooks,
)


def _full_vars() -> HookVars:
    return HookVars(
        skill_dir="/skills/threshold-calibration",
        session_dir="/var/session/abc",
        runtime_dir="/opt/claude-code-runtime",
    )


def test_substitutes_skill_dir() -> None:
    out = substitute_hook_vars(
        "${SKILL_DIR}/hooks/block_write_scada.sh", _full_vars()
    )
    assert out == "/skills/threshold-calibration/hooks/block_write_scada.sh"


def test_substitutes_multiple_vars_in_one_string() -> None:
    out = substitute_hook_vars(
        "${RUNTIME_DIR}/bin/runner ${SKILL_DIR}/entry --session ${SESSION_DIR}",
        _full_vars(),
    )
    assert out == (
        "/opt/claude-code-runtime/bin/runner "
        "/skills/threshold-calibration/entry --session /var/session/abc"
    )


def test_leaves_non_variable_text_untouched() -> None:
    assert substitute_hook_vars("/usr/bin/env bash", _full_vars()) == "/usr/bin/env bash"


def test_double_dollar_escapes_to_literal_dollar() -> None:
    out = substitute_hook_vars("echo $$HOME $${SKILL_DIR}", _full_vars())
    assert out == "echo $HOME ${SKILL_DIR}"


def test_unknown_variable_errors() -> None:
    with pytest.raises(UnknownVariableError) as exc_info:
        substitute_hook_vars("${WHO_KNOWS}/x", _full_vars())
    assert exc_info.value.name == "WHO_KNOWS"


def test_unbound_known_variable_errors() -> None:
    with pytest.raises(UnboundVariableError) as exc_info:
        substitute_hook_vars("${SKILL_DIR}/x", HookVars())
    assert exc_info.value.name == "SKILL_DIR"


def test_malformed_reference_errors() -> None:
    with pytest.raises(MalformedVariableError) as exc_info:
        substitute_hook_vars("${SKILL_DIR/x", _full_vars())
    assert exc_info.value.index == 0


def test_substitute_scoped_hooks_resolves_all_three_scopes() -> None:
    hooks = [
        {
            "name": "block_write_scada",
            "type": "command",
            "command": "${SKILL_DIR}/hooks/block_write_scada.sh",
        },
        {
            "name": "require_evidence",
            "type": "prompt",
            "prompt": "Check outputs under ${SKILL_DIR}",
        },
        {
            "name": "require_anchor",
            "type": "command",
            "command": "${SKILL_DIR}/hooks/check_output_anchor.sh",
        },
    ]
    resolved = substitute_scoped_hooks(hooks, _full_vars())
    assert resolved[0]["command"].endswith("/hooks/block_write_scada.sh")
    assert resolved[0]["command"].startswith("/skills/threshold-calibration/")
    assert resolved[1]["prompt"] == "Check outputs under /skills/threshold-calibration"
    assert resolved[2]["command"].endswith("/hooks/check_output_anchor.sh")


def test_substitute_scoped_hooks_propagates_errors() -> None:
    hooks = [
        {"name": "pre", "type": "command", "command": "${NOPE}"},
    ]
    with pytest.raises(UnknownVariableError):
        substitute_scoped_hooks(hooks, _full_vars())


def test_substitute_scoped_hooks_passes_through_unknown_body_shapes() -> None:
    # A hook without command/prompt (future scope type) is returned unchanged.
    hooks = [{"name": "other", "type": "future", "reserved": 1}]
    resolved = substitute_scoped_hooks(hooks, _full_vars())
    assert resolved == hooks
    assert resolved[0] is not hooks[0]  # still a fresh dict copy
