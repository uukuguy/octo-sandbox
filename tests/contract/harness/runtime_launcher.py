"""Subprocess launcher for EAASP L1 runtimes under contract test.

Per plan §S0.T1 step 3, this module owns the lifecycle (spawn -> gRPC
readiness probe -> teardown) of a single runtime under test. It is
runtime-agnostic: callers construct a :class:`RuntimeConfig` with the
correct ``launch_cmd`` and environment for their target runtime (grid,
claude-code, goose, nanobot).

The launcher is used as a pytest fixture in ``conftest.py`` with
``scope="session"`` — one runtime process per test session.
"""

from __future__ import annotations

import os
import subprocess
import time
from dataclasses import dataclass, field
from typing import Optional

import grpc


@dataclass
class RuntimeConfig:
    """Describes how to launch a single L1 runtime for contract testing.

    Attributes:
        name: Stable runtime identifier (``"grid"`` | ``"claude-code"`` |
            ``"goose"`` | ``"nanobot"``). Used for logging + fixture routing.
        launch_cmd: Full argv passed to :class:`subprocess.Popen`. The first
            element MUST be a resolvable executable (binary path, ``cargo``,
            ``python``, etc.). No shell expansion is performed.
        grpc_port: Port the runtime binds its gRPC server to. The launcher
            probes ``localhost:<grpc_port>`` with :func:`grpc.channel_ready_future`
            until either readiness or ``startup_timeout_s`` elapses.
        env: Extra environment variables merged on top of ``os.environ``
            before the subprocess starts. Runtime-specific (e.g.
            ``LLM_PROVIDER``, ``OPENAI_API_KEY``).
        startup_timeout_s: Max seconds to wait for gRPC readiness before
            raising :class:`TimeoutError`. Defaults to 30s; grid-runtime
            callers that invoke ``cargo run`` may want 60s+.
    """

    name: str
    launch_cmd: list[str]
    grpc_port: int
    env: dict[str, str] = field(default_factory=dict)
    startup_timeout_s: float = 30.0


class RuntimeLauncher:
    """Spawns a runtime subprocess and waits for gRPC readiness.

    Usage::

        cfg = RuntimeConfig(name="grid", launch_cmd=[...], grpc_port=50061)
        launcher = RuntimeLauncher(cfg)
        launcher.start()
        try:
            ...  # drive contract tests against localhost:50061
        finally:
            launcher.stop()

    Not thread-safe; tests should hold the instance behind a session-scoped
    pytest fixture.
    """

    def __init__(self, cfg: RuntimeConfig) -> None:
        self.cfg = cfg
        self._proc: Optional[subprocess.Popen] = None

    @property
    def grpc_target(self) -> str:
        """gRPC target string (``localhost:<port>``) the runtime listens on."""
        return f"localhost:{self.cfg.grpc_port}"

    def start(self) -> None:
        """Spawn the subprocess and block until gRPC is ready.

        Raises:
            TimeoutError: If the runtime did not become ready within
                ``cfg.startup_timeout_s``. The subprocess is killed before
                the exception propagates.
            RuntimeError: If the subprocess exits before readiness.
        """
        merged_env = {**os.environ, **self.cfg.env}
        self._proc = subprocess.Popen(
            self.cfg.launch_cmd,
            env=merged_env,
        )

        deadline = time.time() + self.cfg.startup_timeout_s
        last_err: Optional[BaseException] = None
        while time.time() < deadline:
            # Early-exit bail-out: subprocess died before gRPC came up.
            if self._proc.poll() is not None:
                rc = self._proc.returncode
                self._proc = None
                raise RuntimeError(
                    f"runtime {self.cfg.name!r} exited with code {rc} "
                    f"before gRPC readiness"
                )
            try:
                with grpc.insecure_channel(self.grpc_target) as channel:
                    grpc.channel_ready_future(channel).result(timeout=1.0)
                return
            except grpc.FutureTimeoutError as err:
                last_err = err
                time.sleep(0.5)

        # Timed out — kill subprocess to avoid leaking a zombie.
        self.stop()
        raise TimeoutError(
            f"runtime {self.cfg.name!r} not ready on {self.grpc_target} "
            f"after {self.cfg.startup_timeout_s}s (last: {last_err!r})"
        )

    def stop(self) -> None:
        """Send SIGTERM, wait up to 10s, then SIGKILL if still alive."""
        if self._proc is None:
            return
        if self._proc.poll() is None:
            self._proc.terminate()
            try:
                self._proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self._proc.kill()
                self._proc.wait()
        self._proc = None

    def is_running(self) -> bool:
        """True iff the subprocess is spawned and has not exited."""
        return self._proc is not None and self._proc.poll() is None
