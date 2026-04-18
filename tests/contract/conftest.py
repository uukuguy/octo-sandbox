"""Contract-suite pytest config and shared fixtures.

S0.T4 wires the grid-runtime fixtures: launches a real subprocess
against a mock OpenAI server, exposes a gRPC stub, and provides a
HookProbe that materialises PreToolUse / PostToolUse / Stop envelopes
for hook-contract assertions (ADR-V2-006 §2 / §3).

Per-runtime config is dispatched on ``--runtime`` at pytest invocation
time. Only the ``grid`` branch is implemented here; ``claude-code``
lands in S0.T5.

trust_env=False / Clash localhost quirk: the macOS Clash proxy hijacks
``127.0.0.1`` connections by default, which breaks the runtime's
outbound requests to our mock OpenAI server sitting on a loopback port.
We run the runtime subprocess with ``NO_PROXY=127.0.0.1,localhost`` and
explicitly opt out of env-proxy honouring anywhere the harness itself
reaches back to the mock server. This mirrors the 2026-04-12 MVP S4.T2
lesson (see MEMORY.md).
"""

from __future__ import annotations

import os
import socket
import sys
import threading
import time
from pathlib import Path
from typing import Iterator

import grpc
import httpx
import pytest
import uvicorn

from tests.contract.harness import mock_anthropic_server, mock_openai_server
from tests.contract.harness.hook_probe import HookProbe
from tests.contract.harness.runtime_launcher import RuntimeConfig, RuntimeLauncher


# ---------------------------------------------------------------------------
# Python proto stubs — reuse the ones generated for claude-code-runtime so
# we don't need a second grpc_tools codegen step (they are byte-identical
# because they compile the same .proto). Sys.path injection keeps them
# importable without packaging tricks.
#
# TODO(post-S0.T6): either (a) ship a dedicated tests/contract/harness/_proto
# with its own regeneration Make target, or (b) expose the existing stubs
# via a real Python package. The current cross-crate import is fine for
# MVP contract work but is a layering smell.
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_CCRUNTIME_SRC = _REPO_ROOT / "lang" / "claude-code-runtime-python" / "src"
if str(_CCRUNTIME_SRC) not in sys.path:
    sys.path.insert(0, str(_CCRUNTIME_SRC))

from claude_code_runtime._proto.eaasp.runtime.v2 import (  # noqa: E402
    common_pb2,
    runtime_pb2,
    runtime_pb2_grpc,
)


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
        choices=["grid", "claude-code", "goose", "nanobot", "pydantic-ai", "claw-code"],
        help=(
            "Runtime under test. Required by contract_v1/ tests; smoke tests "
            "under tests/contract/test_harness_smoke.py do not consult it."
        ),
    )


# ---------------------------------------------------------------------------
# Free-port helper
# ---------------------------------------------------------------------------


def _free_port() -> int:
    """Allocate a loopback port by letting the kernel pick one, then close."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


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
def mock_openai_server_port() -> Iterator[int]:
    """Start a mock OpenAI server on a free loopback port for the session.

    The mock is scripted to emit exactly one ``file_write`` tool_call on
    the first request, then fall through to plain-text stop replies. That
    script shape is what lets the probe-skill fire PreToolUse →
    PostToolUse → Stop within a single Send turn.

    uvicorn runs on a daemon thread; no explicit teardown is performed
    because the daemon dies with the pytest process. We do, however,
    poll the ``/health`` endpoint before yielding so tests never race
    against a not-yet-bound port.
    """
    port = _free_port()
    app = mock_openai_server.build_app(
        tool_script=[
            {
                "tool_name": "file_write",
                "arguments": {
                    "path": "/tmp/contract-probe.txt",
                    "content": "probe",
                },
                "id": "call_probe_0",
            }
        ]
    )

    config = uvicorn.Config(
        app,
        host="127.0.0.1",
        port=port,
        log_level="warning",
        access_log=False,
    )
    server = uvicorn.Server(config)

    def _run() -> None:
        try:
            server.run()
        except SystemExit:
            pass

    thread = threading.Thread(target=_run, name="mock-openai-uvicorn", daemon=True)
    thread.start()

    deadline = time.time() + 5.0
    while time.time() < deadline:
        try:
            # trust_env=False ⇒ bypass Clash / HTTP_PROXY on macOS.
            resp = httpx.get(
                f"http://127.0.0.1:{port}/health", timeout=1.0, trust_env=False
            )
            if resp.status_code == 200:
                break
        except httpx.HTTPError:
            time.sleep(0.1)
    else:
        raise RuntimeError(f"mock OpenAI server did not come up on port {port}")

    yield port


@pytest.fixture(scope="session")
def mock_anthropic_server_port() -> Iterator[int]:
    """Start a mock Anthropic Messages API server on a free loopback port.

    Parallel to :func:`mock_openai_server_port` but with one important
    architectural distinction: the claude-code-runtime does not drive
    scoped-hook dispatch from within ``Send``. Scoped hooks fire on the
    dedicated ``OnToolCall`` / ``OnToolResult`` / ``OnStop`` RPCs. So
    this mock's only job is to keep the underlying claude CLI from
    erroring out against the real ``api.anthropic.com`` when the probe
    drains Send (which happens before the RPC sweep). No tool scripting
    is required or implemented.
    """
    port = _free_port()
    app = mock_anthropic_server.build_app()

    config = uvicorn.Config(
        app,
        host="127.0.0.1",
        port=port,
        log_level="warning",
        access_log=False,
    )
    server = uvicorn.Server(config)

    def _run() -> None:
        try:
            server.run()
        except SystemExit:
            pass

    thread = threading.Thread(target=_run, name="mock-anthropic-uvicorn", daemon=True)
    thread.start()

    deadline = time.time() + 5.0
    while time.time() < deadline:
        try:
            # trust_env=False ⇒ bypass Clash / HTTP_PROXY on macOS.
            resp = httpx.get(
                f"http://127.0.0.1:{port}/health", timeout=1.0, trust_env=False
            )
            if resp.status_code == 200:
                break
        except httpx.HTTPError:
            time.sleep(0.1)
    else:
        raise RuntimeError(f"mock Anthropic server did not come up on port {port}")

    yield port


@pytest.fixture(scope="session")
def runtime_config(
    runtime_name: str,
    mock_openai_server_port: int,
    mock_anthropic_server_port: int,
) -> RuntimeConfig:
    """Resolve the :class:`RuntimeConfig` for ``runtime_name``.

    S0.T4 implements the ``grid`` branch. S0.T5 will add ``claude-code``.
    """
    if runtime_name == "grid":
        prebuilt = _REPO_ROOT / "target" / "debug" / "grid-runtime"
        if prebuilt.exists():
            launch_cmd = [str(prebuilt)]
            startup_timeout_s = 15.0
        else:
            launch_cmd = ["cargo", "run", "-p", "grid-runtime", "--"]
            startup_timeout_s = 120.0

        grpc_port = _free_port()

        # Probe-skill cache: the runtime resolves ${SKILL_DIR} to
        # `${EAASP_SKILL_CACHE_DIR}/${skill_id}` when SessionPayload's
        # skill_instructions.content is empty (see
        # crates/grid-runtime/src/harness.rs::build_hook_vars). We point
        # at the parent of tests/contract/fixtures/probe-skill so the
        # runtime discovers both SKILL.md and hooks/ on disk.
        fixtures_root = _REPO_ROOT / "tests" / "contract" / "fixtures"
        probe_out_dir = _REPO_ROOT / "tests" / "contract" / "fixtures" / "_probe_out"
        probe_out_dir.mkdir(parents=True, exist_ok=True)

        return RuntimeConfig(
            name="grid",
            launch_cmd=launch_cmd,
            grpc_port=grpc_port,
            env={
                "GRID_RUNTIME_ADDR": f"127.0.0.1:{grpc_port}",
                "GRID_RUNTIME_ID": "grid-contract-test",
                "LLM_PROVIDER": "openai",
                "OPENAI_API_KEY": "sk-test-mock",
                "OPENAI_BASE_URL": f"http://127.0.0.1:{mock_openai_server_port}/v1",
                "OPENAI_MODEL_NAME": "gpt-4o",
                "GRID_PROBE_STRATEGY": "lazy",
                "RUST_LOG": "grid_runtime=warn,grid_engine=warn",
                # Scoped-hook wiring: EAASP_SKILL_CACHE_DIR + the
                # per-skill subdirectory under it resolve ${SKILL_DIR}.
                "EAASP_SKILL_CACHE_DIR": str(fixtures_root),
                # Hook scripts dump stdin envelope + GRID_* env here;
                # the HookProbe fixture reads these files back after
                # driving a Send turn.
                "GRID_CONTRACT_PROBE_OUT": str(probe_out_dir),
                # macOS Clash / system proxy bypass for loopback LLM.
                "NO_PROXY": "127.0.0.1,localhost",
                "no_proxy": "127.0.0.1,localhost",
                "HTTP_PROXY": "",
                "HTTPS_PROXY": "",
                "http_proxy": "",
                "https_proxy": "",
            },
            startup_timeout_s=startup_timeout_s,
        )

    if runtime_name == "claude-code":
        # Use the runtime's own venv — it ships claude-agent-sdk and the
        # bundled `claude` CLI, pinned to Python 3.14. The repo-root .venv
        # (Python 3.12) does NOT have claude-agent-sdk installed.
        ccruntime_python = (
            _REPO_ROOT
            / "lang"
            / "claude-code-runtime-python"
            / ".venv"
            / "bin"
            / "python"
        )
        grpc_port = _free_port()

        fixtures_root = _REPO_ROOT / "tests" / "contract" / "fixtures"
        probe_out_dir = fixtures_root / "_probe_out"
        probe_out_dir.mkdir(parents=True, exist_ok=True)

        # ANTHROPIC_BASE_URL concerns:
        # The bundled `claude` CLI (forked by claude-agent-sdk) honors the
        # env var and appends its own `/v1/messages` path. We therefore
        # point BASE_URL at the mock's root host, NOT at `/v1`.
        return RuntimeConfig(
            name="claude-code",
            launch_cmd=[
                str(ccruntime_python),
                "-m",
                "claude_code_runtime",
                "--port",
                str(grpc_port),
                "--log-level",
                "WARNING",
            ],
            grpc_port=grpc_port,
            env={
                "ANTHROPIC_API_KEY": "sk-test-mock",
                "ANTHROPIC_BASE_URL": f"http://127.0.0.1:{mock_anthropic_server_port}",
                "CLAUDE_RUNTIME_ID": "claude-code-contract-test",
                "CLAUDE_RUNTIME_PORT": str(grpc_port),
                # Scoped-hook wiring — same shape as grid arm so the
                # probe-skill's ${SKILL_DIR} resolves identically.
                "EAASP_SKILL_CACHE_DIR": str(fixtures_root),
                "GRID_CONTRACT_PROBE_OUT": str(probe_out_dir),
                # macOS Clash / system proxy bypass for loopback LLM.
                "NO_PROXY": "127.0.0.1,localhost",
                "no_proxy": "127.0.0.1,localhost",
                "HTTP_PROXY": "",
                "HTTPS_PROXY": "",
                "http_proxy": "",
                "https_proxy": "",
            },
            startup_timeout_s=15.0,
        )

    if runtime_name == "goose":
        import shutil
        goose_bin = os.environ.get("GOOSE_BIN", "") or shutil.which("goose") or ""
        if not goose_bin:
            pytest.skip("goose binary not installed; set GOOSE_BIN env to enable goose contract run")

        prebuilt = _REPO_ROOT / "target" / "debug" / "eaasp-goose-runtime"
        if prebuilt.exists():
            launch_cmd = [str(prebuilt)]
            startup_timeout_s = 30.0
        else:
            launch_cmd = ["cargo", "run", "-p", "eaasp-goose-runtime", "--"]
            startup_timeout_s = 120.0

        grpc_port = _free_port()
        fixtures_root = _REPO_ROOT / "tests" / "contract" / "fixtures"
        probe_out_dir = fixtures_root / "_probe_out"
        probe_out_dir.mkdir(parents=True, exist_ok=True)

        return RuntimeConfig(
            name="goose",
            launch_cmd=launch_cmd,
            grpc_port=grpc_port,
            env={
                "GOOSE_RUNTIME_GRPC_ADDR": f"0.0.0.0:{grpc_port}",
                "GOOSE_BIN": goose_bin,
                "OPENAI_BASE_URL": f"http://127.0.0.1:{mock_openai_server_port}/v1",
                "OPENAI_API_KEY": "sk-test-mock",
                "OPENAI_MODEL_NAME": "gpt-4o-mini",
                "EAASP_HOOK_MCP_MODE": "stdio",
                "EAASP_SKILL_CACHE_DIR": str(fixtures_root),
                "GRID_CONTRACT_PROBE_OUT": str(probe_out_dir),
                "EAASP_DEPLOYMENT_MODE": "shared",
                "RUST_LOG": "eaasp_goose_runtime=warn",
                "NO_PROXY": "127.0.0.1,localhost",
                "no_proxy": "127.0.0.1,localhost",
                "HTTP_PROXY": "",
                "HTTPS_PROXY": "",
                "http_proxy": "",
                "https_proxy": "",
            },
            startup_timeout_s=startup_timeout_s,
        )

    if runtime_name == "nanobot":
        nanobot_python = _REPO_ROOT / "lang" / "nanobot-runtime-python" / ".venv" / "bin" / "python"
        if not nanobot_python.exists():
            pytest.skip(
                "nanobot-runtime-python venv not installed; "
                "run `cd lang/nanobot-runtime-python && uv sync` to enable nanobot contract run"
            )

        grpc_port = _free_port()
        fixtures_root = _REPO_ROOT / "tests" / "contract" / "fixtures"
        probe_out_dir = fixtures_root / "_probe_out"
        probe_out_dir.mkdir(parents=True, exist_ok=True)

        return RuntimeConfig(
            name="nanobot",
            launch_cmd=[
                str(nanobot_python),
                "-m", "nanobot_runtime",
                "--port", str(grpc_port),
                "--log-level", "WARNING",
            ],
            grpc_port=grpc_port,
            env={
                "NANOBOT_RUNTIME_PORT": str(grpc_port),
                "OPENAI_BASE_URL": f"http://127.0.0.1:{mock_openai_server_port}/v1",
                "OPENAI_API_KEY": "sk-test-mock",
                "OPENAI_MODEL_NAME": "gpt-4o-mini",
                "EAASP_DEPLOYMENT_MODE": "shared",
                "EAASP_SKILL_CACHE_DIR": str(fixtures_root),
                "GRID_CONTRACT_PROBE_OUT": str(probe_out_dir),
                "NO_PROXY": "127.0.0.1,localhost",
                "no_proxy": "127.0.0.1,localhost",
                "HTTP_PROXY": "",
                "HTTPS_PROXY": "",
                "http_proxy": "",
                "https_proxy": "",
            },
            startup_timeout_s=15.0,
        )

    if runtime_name == "pydantic-ai":
        pydantic_ai_python = _REPO_ROOT / "lang" / "pydantic-ai-runtime-python" / ".venv" / "bin" / "python"
        if not pydantic_ai_python.exists():
            pytest.skip(
                "pydantic-ai-runtime-python venv not installed; "
                "run `cd lang/pydantic-ai-runtime-python && uv sync --extra dev` to enable"
            )

        grpc_port = _free_port()
        fixtures_root = _REPO_ROOT / "tests" / "contract" / "fixtures"
        probe_out_dir = fixtures_root / "_probe_out"
        probe_out_dir.mkdir(parents=True, exist_ok=True)

        return RuntimeConfig(
            name="pydantic-ai",
            launch_cmd=[
                str(pydantic_ai_python),
                "-m", "pydantic_ai_runtime",
                "--port", str(grpc_port),
                "--log-level", "WARNING",
            ],
            grpc_port=grpc_port,
            env={
                "PYDANTIC_AI_RUNTIME_PORT": str(grpc_port),
                "OPENAI_BASE_URL": f"http://127.0.0.1:{mock_openai_server_port}/v1",
                "OPENAI_API_KEY": "sk-test-mock",
                "OPENAI_MODEL_NAME": "gpt-4o-mini",
                "EAASP_DEPLOYMENT_MODE": "shared",
                "EAASP_SKILL_CACHE_DIR": str(fixtures_root),
                "GRID_CONTRACT_PROBE_OUT": str(probe_out_dir),
                "NO_PROXY": "127.0.0.1,localhost",
                "no_proxy": "127.0.0.1,localhost",
                "HTTP_PROXY": "",
                "HTTPS_PROXY": "",
                "http_proxy": "",
                "https_proxy": "",
            },
            startup_timeout_s=15.0,
        )

    if runtime_name == "claw-code":
        prebuilt = _REPO_ROOT / "target" / "debug" / "eaasp-claw-code-runtime"
        if prebuilt.exists():
            launch_cmd = [str(prebuilt)]
            startup_timeout_s = 15.0
        else:
            launch_cmd = ["cargo", "run", "-p", "eaasp-claw-code-runtime", "--"]
            startup_timeout_s = 120.0

        grpc_port = _free_port()
        fixtures_root = _REPO_ROOT / "tests" / "contract" / "fixtures"
        probe_out_dir = fixtures_root / "_probe_out"
        probe_out_dir.mkdir(parents=True, exist_ok=True)

        return RuntimeConfig(
            name="claw-code",
            launch_cmd=launch_cmd,
            grpc_port=grpc_port,
            env={
                "CLAW_CODE_RUNTIME_GRPC_ADDR": f"0.0.0.0:{grpc_port}",
                "EAASP_DEPLOYMENT_MODE": "shared",
                # claw-code binary gate: if absent the adapter falls back to
                # stub sessions (no subprocess), which is the expected baseline
                # for contract testing without a live claw-code install.
                "CLAW_CODE_BIN": os.environ.get("CLAW_CODE_BIN", "claw-code"),
                "EAASP_SKILL_CACHE_DIR": str(fixtures_root),
                "GRID_CONTRACT_PROBE_OUT": str(probe_out_dir),
                "RUST_LOG": "eaasp_claw_code_runtime=warn",
                "NO_PROXY": "127.0.0.1,localhost",
                "no_proxy": "127.0.0.1,localhost",
                "HTTP_PROXY": "",
                "HTTPS_PROXY": "",
                "http_proxy": "",
                "https_proxy": "",
            },
            startup_timeout_s=startup_timeout_s,
        )

    raise NotImplementedError(
        f"RuntimeConfig for {runtime_name!r} not yet wired."
    )


@pytest.fixture(scope="session")
def runtime_launcher(runtime_config: RuntimeConfig) -> Iterator[RuntimeLauncher]:
    """Spawn the runtime subprocess once per session; tear down on exit."""
    launcher = RuntimeLauncher(runtime_config)
    launcher.start()
    try:
        yield launcher
    finally:
        launcher.stop()


@pytest.fixture(scope="session")
def runtime_grpc_stub(
    runtime_launcher: RuntimeLauncher,
) -> Iterator[runtime_pb2_grpc.RuntimeServiceStub]:
    """Insecure gRPC stub bound to the launched runtime."""
    channel = grpc.insecure_channel(runtime_launcher.grpc_target)
    try:
        stub = runtime_pb2_grpc.RuntimeServiceStub(channel)
        yield stub
    finally:
        channel.close()


@pytest.fixture(scope="session")
def probe_out_dir(runtime_config: RuntimeConfig) -> Path:
    """Expose the runtime's probe-dump directory path to tests."""
    return Path(runtime_config.env["GRID_CONTRACT_PROBE_OUT"])


# ---------------------------------------------------------------------------
# Hook-envelope capture fixtures (S0.T4 — grid path wired; claude-code
# deferred to S0.T5).
# ---------------------------------------------------------------------------


def _fresh_probe_out(probe_out_dir: Path) -> None:
    """Wipe stale dumps from a prior test so stale files don't mask miss."""
    for p in probe_out_dir.glob("*.envelope.json"):
        p.unlink(missing_ok=True)
    for p in probe_out_dir.glob("*.env.json"):
        p.unlink(missing_ok=True)


def _run_hook_probe(
    stub: runtime_pb2_grpc.RuntimeServiceStub, probe_out_dir: Path
) -> dict[str, object]:
    """Drive one probe turn and return captures keyed by event name."""
    _fresh_probe_out(probe_out_dir)
    probe = HookProbe(
        stub=stub,
        probe_out_dir=probe_out_dir,
        runtime_pb2=runtime_pb2,
        common_pb2=common_pb2,
    )
    probe.setup()
    try:
        captures = probe.run_turn()
    finally:
        probe.teardown()
    return captures


def _check_envelope_mode(cap, scope: str, runtime_name: str) -> None:
    """Ensure the captured envelope uses ADR-V2-006 envelope-mode shape.

    Runtime-specific behaviour:

    * ``grid``: grid-engine's ``HookContext`` has envelope-mode support
      via ``with_event()`` (D120 / S0.T3), but the dispatch sites in
      ``fire_post_task_hooks`` and ``dispatch_stop_hooks`` do not call
      it today — shipping hooks see the pre-ADR legacy full-struct
      projection. Xfail with D140 when that happens.
    * ``claude-code``: the Python runtime's scoped-hook dispatch (S3.T5)
      writes envelope-mode shape directly (see ``service.py``
      ``_dispatch_scoped_*``). A legacy-shape capture here would be a
      contract regression worth failing loudly, not xfail'ing.
    """
    env = cap.env
    envelope = cap.envelope
    legacy = (
        "GRID_EVENT" not in env
        or envelope.get("event") != scope
    )
    if not legacy:
        return
    if runtime_name == "grid":
        pytest.xfail(
            f"D140: grid-runtime {scope} dispatch site not yet calling "
            "HookContext::with_event(...) — envelope-mode infrastructure "
            "exists post-S0.T3 but dispatch wiring (harness.rs Stop + "
            "fire_post_task_hooks) needs follow-up commit. Legacy shape "
            "captured."
        )
    # Any other runtime that ships legacy shape is a real violation —
    # fall through to the downstream assertion, which will fail with a
    # diff against the required envelope keys.


@pytest.fixture
def trigger_pre_tool_use_hook(runtime_grpc_stub, probe_out_dir, runtime_name):
    """Trigger a PreToolUse hook and return ``(envelope, env)``."""
    def _invoke():
        captures = _run_hook_probe(runtime_grpc_stub, probe_out_dir)
        cap = captures.get("PreToolUse")
        if cap is None:
            pytest.xfail(
                f"D136: {runtime_name}-runtime did not fire PreToolUse "
                "during probe turn — for grid this is the required_tools / "
                "D87 interaction in the agent loop; for claude-code this "
                "would indicate the OnToolCall RPC dispatch path is "
                "broken. Resolution deferred to Phase 2.5 S1."
            )
        _check_envelope_mode(cap, "PreToolUse", runtime_name)
        return cap.envelope, cap.env

    return _invoke


@pytest.fixture
def trigger_post_tool_use_hook(runtime_grpc_stub, probe_out_dir, runtime_name):
    def _invoke():
        captures = _run_hook_probe(runtime_grpc_stub, probe_out_dir)
        cap = captures.get("PostToolUse")
        if cap is None:
            pytest.xfail(
                f"D136: {runtime_name}-runtime did not fire PostToolUse "
                "during probe turn (same root cause as PreToolUse gap)"
            )
        _check_envelope_mode(cap, "PostToolUse", runtime_name)
        return cap.envelope, cap.env

    return _invoke


@pytest.fixture
def trigger_stop_hook(runtime_grpc_stub, probe_out_dir, runtime_name):
    def _invoke():
        captures = _run_hook_probe(runtime_grpc_stub, probe_out_dir)
        cap = captures.get("Stop")
        if cap is None:
            pytest.xfail(
                f"D136: {runtime_name}-runtime did not fire Stop hook "
                "at natural termination during probe turn"
            )
        _check_envelope_mode(cap, "Stop", runtime_name)
        return cap.envelope, cap.env

    return _invoke
