# Phase BE W4-W6: claude-code-runtime Python 实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 创建第二个 EAASP L1 Runtime：用 Python 实现的 claude-code-runtime，底层封装 claude-agent-sdk，通过 gRPC 暴露 16 方法 RuntimeService 契约，最终用 eaasp-certifier 验证两个 runtime 的合规性。

**Architecture:** claude-code-runtime 是 T1 Harness（本地执行 hooks，不依赖 HookBridge）。底层通过 `claude-agent-sdk`（Python）驱动 Claude Code CLI 进程。gRPC server 使用 `grpcio` 实现 `RuntimeService`。

**Tech Stack:** Python 3.12+, uv, grpcio, grpcio-tools, claude-agent-sdk, protobuf, anyio

**设计决策（讨论确认）：**
1. 用 `claude-agent-sdk` Python SDK（封装 Claude Code CLI）
2. 工具链: `uv` + `grpcio`
3. 支持 `ANTHROPIC_BASE_URL` / `ANTHROPIC_MODEL_NAME` / `ANTHROPIC_API_KEY` 环境变量
4. 运行时验证测试交给人工（需要 API Key + 两个 gRPC server）
5. grid-runtime :50051, claude-code-runtime :50052，集成测试可并行运行

---

## 目录结构

```
lang/
└── claude-code-runtime-python/
    ├── pyproject.toml              ← W4 uv 项目配置
    ├── README.md                   ← W4 简要说明
    ├── proto/                      ← W4 symlink 或 copy 到根 proto/
    ├── src/
    │   └── claude_code_runtime/
    │       ├── __init__.py         ← W4 package init
    │       ├── __main__.py         ← W4 gRPC server 入口
    │       ├── config.py           ← W4 配置（env vars, ports）
    │       ├── sdk_wrapper.py      ← W4 claude-agent-sdk 封装
    │       ├── service.py          ← W4 gRPC RuntimeService 实现（16 方法桩）
    │       ├── session.py          ← W5 会话管理
    │       ├── hook_executor.py    ← W5 本地 hook 执行（T1 模式）
    │       ├── telemetry.py        ← W5 遥测采集
    │       ├── skill_loader.py     ← W5 Skill 加载
    │       ├── state_manager.py    ← W5 get_state / restore_state
    │       └── mapper.py           ← W5 SDK event ↔ ResponseChunk 映射
    └── tests/
        ├── test_config.py          ← W4
        ├── test_sdk_wrapper.py     ← W4
        ├── test_service.py         ← W4 gRPC service 单元测试
        ├── test_session.py         ← W5
        ├── test_hook_executor.py   ← W5
        ├── test_telemetry.py       ← W5
        └── test_integration.py     ← W6 集成测试（需要 API Key）
```

---

## W4: 项目骨架 + claude-agent-sdk 封装 + gRPC service 桩

### Task W4-T1: 项目骨架 + uv 配置

**Files:**
- Create: `lang/claude-code-runtime-python/pyproject.toml`
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/__init__.py`
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/config.py`

**Step 1: 创建 pyproject.toml**

```toml
[project]
name = "claude-code-runtime"
version = "0.1.0"
description = "EAASP L1 Runtime — Python T1 Harness wrapping claude-agent-sdk"
requires-python = ">=3.12"
dependencies = [
    "claude-agent-sdk>=0.1.0",
    "grpcio>=1.70.0",
    "grpcio-tools>=1.70.0",
    "protobuf>=5.0",
    "anyio>=4.0",
    "pydantic>=2.0",
    "python-dotenv>=1.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.24",
    "grpcio-testing>=1.70.0",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/claude_code_runtime"]

[tool.pytest.ini_options]
asyncio_mode = "auto"
testpaths = ["tests"]
```

**Step 2: 创建 config.py**

```python
"""Configuration for claude-code-runtime."""

import os
from dataclasses import dataclass, field
from pathlib import Path

from dotenv import load_dotenv


@dataclass
class RuntimeConfig:
    """Runtime configuration from environment variables."""

    grpc_port: int = 50052
    runtime_id: str = "claude-code-runtime"
    runtime_name: str = "Claude Code Runtime"
    tier: str = "harness"

    # Anthropic SDK config
    anthropic_api_key: str = ""
    anthropic_base_url: str = ""
    anthropic_model_name: str = "claude-sonnet-4-20250514"

    # Claude Agent SDK config
    max_turns: int = 10
    max_budget_usd: float | None = None
    permission_mode: str = "acceptEdits"

    @classmethod
    def from_env(cls, env_file: str | Path | None = None) -> "RuntimeConfig":
        """Load config from environment variables."""
        if env_file:
            load_dotenv(env_file)
        else:
            # Try project root .env
            root_env = Path(__file__).parent.parent.parent.parent.parent / ".env"
            if root_env.exists():
                load_dotenv(root_env)

        return cls(
            grpc_port=int(os.getenv("CLAUDE_RUNTIME_PORT", "50052")),
            runtime_id=os.getenv("CLAUDE_RUNTIME_ID", "claude-code-runtime"),
            runtime_name=os.getenv("CLAUDE_RUNTIME_NAME", "Claude Code Runtime"),
            anthropic_api_key=os.getenv("ANTHROPIC_API_KEY", ""),
            anthropic_base_url=os.getenv("ANTHROPIC_BASE_URL", ""),
            anthropic_model_name=os.getenv(
                "ANTHROPIC_MODEL_NAME", "claude-sonnet-4-20250514"
            ),
            max_turns=int(os.getenv("CLAUDE_MAX_TURNS", "10")),
            permission_mode=os.getenv("CLAUDE_PERMISSION_MODE", "acceptEdits"),
        )
```

**Step 3: 创建 __init__.py**

```python
"""claude-code-runtime — EAASP L1 Runtime wrapping claude-agent-sdk."""

__version__ = "0.1.0"
```

**Step 4: 验证 uv 初始化**

```bash
cd lang/claude-code-runtime-python
uv sync
```

---

### Task W4-T2: Proto 编译 + gRPC 生成

**Files:**
- Create: `lang/claude-code-runtime-python/build_proto.py`

**Step 1: 创建 proto 编译脚本**

```python
"""Build gRPC Python stubs from proto files."""

import subprocess
import sys
from pathlib import Path

PROTO_ROOT = Path(__file__).parent.parent.parent / "proto"
OUT_DIR = Path(__file__).parent / "src" / "claude_code_runtime" / "_proto"


def build():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "__init__.py").touch()

    proto_files = [
        "eaasp/common/v1/common.proto",
        "eaasp/runtime/v1/runtime.proto",
    ]

    for proto in proto_files:
        cmd = [
            sys.executable, "-m", "grpc_tools.protoc",
            f"--proto_path={PROTO_ROOT}",
            f"--python_out={OUT_DIR}",
            f"--grpc_python_out={OUT_DIR}",
            f"--pyi_out={OUT_DIR}",
            str(PROTO_ROOT / proto),
        ]
        print(f"Compiling {proto}...")
        subprocess.check_call(cmd)

    # Fix imports in generated files (grpcio generates absolute imports)
    _fix_imports(OUT_DIR)
    print("Proto build complete.")


def _fix_imports(out_dir: Path):
    """Fix generated import paths to use relative imports."""
    for py_file in out_dir.rglob("*.py"):
        content = py_file.read_text()
        # Fix: "from eaasp.common.v1 import common_pb2"
        # To:  "from claude_code_runtime._proto.eaasp.common.v1 import common_pb2"
        fixed = content.replace(
            "from eaasp.", "from claude_code_runtime._proto.eaasp."
        )
        if fixed != content:
            py_file.write_text(fixed)


if __name__ == "__main__":
    build()
```

**Step 2: 运行 proto 编译**

```bash
cd lang/claude-code-runtime-python
uv run python build_proto.py
```

Expected: `src/claude_code_runtime/_proto/eaasp/` 目录生成 `common_pb2.py`, `runtime_pb2.py`, `runtime_pb2_grpc.py`

---

### Task W4-T3: claude-agent-sdk 封装

**Files:**
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/sdk_wrapper.py`

**Step 1: 实现 SDK 封装**

封装 `claude-agent-sdk` 的 `query()` 函数，处理：
- 配置传递（model, base_url, api_key）
- 流式响应 → ResponseChunk 转换
- 错误处理

```python
"""SDK wrapper — encapsulates claude-agent-sdk interactions."""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import AsyncIterator

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    ResultMessage,
    TextBlock,
    ToolResultBlock,
    ToolUseBlock,
    query,
)

from .config import RuntimeConfig

logger = logging.getLogger(__name__)


@dataclass
class ChunkEvent:
    """Normalized response chunk from SDK."""

    chunk_type: str  # "text_delta" | "tool_start" | "tool_result" | "done" | "error"
    content: str = ""
    tool_name: str = ""
    tool_id: str = ""
    is_error: bool = False


class SdkWrapper:
    """Wraps claude-agent-sdk for use by the gRPC service."""

    def __init__(self, config: RuntimeConfig):
        self.config = config

    def _build_options(
        self,
        system_prompt: str | None = None,
        allowed_tools: list[str] | None = None,
    ) -> ClaudeAgentOptions:
        """Build ClaudeAgentOptions from config."""
        env: dict[str, str] = {}
        if self.config.anthropic_api_key:
            env["ANTHROPIC_API_KEY"] = self.config.anthropic_api_key
        if self.config.anthropic_base_url:
            env["ANTHROPIC_BASE_URL"] = self.config.anthropic_base_url

        opts = ClaudeAgentOptions(
            model=self.config.anthropic_model_name or None,
            max_turns=self.config.max_turns,
            permission_mode=self.config.permission_mode,
            env=env,
        )

        if system_prompt:
            opts.system_prompt = system_prompt
        if allowed_tools:
            opts.allowed_tools = allowed_tools

        return opts

    async def send_message(
        self,
        prompt: str,
        system_prompt: str | None = None,
        allowed_tools: list[str] | None = None,
    ) -> AsyncIterator[ChunkEvent]:
        """Send a message and yield response chunks."""
        options = self._build_options(system_prompt, allowed_tools)

        try:
            async for message in query(prompt=prompt, options=options):
                if isinstance(message, AssistantMessage):
                    for block in message.content:
                        if isinstance(block, TextBlock):
                            yield ChunkEvent(
                                chunk_type="text_delta",
                                content=block.text,
                            )
                        elif isinstance(block, ToolUseBlock):
                            yield ChunkEvent(
                                chunk_type="tool_start",
                                tool_name=block.name,
                                tool_id=block.id,
                                content=str(block.input),
                            )
                        elif isinstance(block, ToolResultBlock):
                            yield ChunkEvent(
                                chunk_type="tool_result",
                                tool_id=block.tool_use_id,
                                content=block.content if isinstance(block.content, str) else str(block.content),
                                is_error=block.is_error or False,
                            )
                elif isinstance(message, ResultMessage):
                    yield ChunkEvent(
                        chunk_type="done",
                        content="",
                    )
        except Exception as e:
            logger.error("SDK error: %s", e)
            yield ChunkEvent(
                chunk_type="error",
                content=str(e),
                is_error=True,
            )
```

---

### Task W4-T4: gRPC RuntimeService 实现（16 方法）

**Files:**
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/service.py`
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/__main__.py`

**Step 1: 实现 service.py — 16 方法 gRPC 服务**

所有 16 方法实现为可运行的桩（W4 不需要完整业务逻辑，Send 通过 SDK 真实调用）。

```python
"""gRPC RuntimeService implementation — 16-method EAASP L1 contract."""

from __future__ import annotations

import json
import logging
import time
import uuid
from typing import TYPE_CHECKING

import grpc

from ._proto.eaasp.common.v1 import common_pb2
from ._proto.eaasp.runtime.v1 import runtime_pb2, runtime_pb2_grpc
from .config import RuntimeConfig
from .sdk_wrapper import SdkWrapper

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


class RuntimeServiceImpl(runtime_pb2_grpc.RuntimeServiceServicer):
    """EAASP L1 RuntimeService — Python T1 Harness."""

    def __init__(self, config: RuntimeConfig):
        self.config = config
        self.sdk = SdkWrapper(config)
        self.sessions: dict[str, dict] = {}  # session_id -> session state
        self._start_time = time.time()

    # ── 1. Health ──

    async def Health(self, request, context):
        return runtime_pb2.HealthStatus(
            healthy=True,
            runtime_id=self.config.runtime_id,
            checks={"sdk": "ok", "uptime": f"{time.time() - self._start_time:.0f}s"},
        )

    # ── 2. GetCapabilities ──

    async def GetCapabilities(self, request, context):
        return runtime_pb2.CapabilityManifest(
            runtime_id=self.config.runtime_id,
            runtime_name=self.config.runtime_name,
            tier=self.config.tier,
            model=self.config.anthropic_model_name,
            context_window=200000,
            supported_tools=["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
            native_hooks=True,  # T1 Harness — hooks execute natively
            native_mcp=True,
            native_skills=True,
            requires_hook_bridge=False,
            cost=runtime_pb2.CostEstimate(
                input_cost_per_1k=0.003,
                output_cost_per_1k=0.015,
            ),
        )

    # ── 3. Initialize ──

    async def Initialize(self, request, context):
        payload = request.payload
        session_id = f"crt-{uuid.uuid4().hex[:12]}"

        self.sessions[session_id] = {
            "user_id": payload.user_id,
            "user_role": payload.user_role,
            "org_unit": payload.org_unit,
            "managed_hooks_json": payload.managed_hooks_json,
            "skills": [],
            "mcp_servers": [],
            "telemetry": [],
            "state": "active",
            "created_at": time.time(),
        }

        logger.info("Session initialized: %s (user=%s)", session_id, payload.user_id)
        return runtime_pb2.InitializeResponse(session_id=session_id)

    # ── 4. Send (streaming) ──

    async def Send(self, request, context):
        session_id = request.session_id
        if session_id not in self.sessions:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            context.set_details(f"Session {session_id} not found")
            return

        message = request.message
        logger.info("Send: session=%s content=%s", session_id, message.content[:50])

        # Record telemetry
        self.sessions[session_id]["telemetry"].append({
            "event_type": "send",
            "timestamp": time.time(),
        })

        async for chunk in self.sdk.send_message(prompt=message.content):
            yield runtime_pb2.ResponseChunk(
                chunk_type=chunk.chunk_type,
                content=chunk.content,
                tool_name=chunk.tool_name,
                tool_id=chunk.tool_id,
                is_error=chunk.is_error,
            )

    # ── 5. LoadSkill ──

    async def LoadSkill(self, request, context):
        session_id = request.session_id
        if session_id not in self.sessions:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            return runtime_pb2.LoadSkillResponse(success=False, error="session not found")

        skill = request.skill
        self.sessions[session_id]["skills"].append({
            "skill_id": skill.skill_id,
            "name": skill.name,
        })
        logger.info("Skill loaded: %s in session %s", skill.name, session_id)
        return runtime_pb2.LoadSkillResponse(success=True)

    # ── 6. OnToolCall ──

    async def OnToolCall(self, request, context):
        logger.info(
            "OnToolCall: session=%s tool=%s",
            request.session_id,
            request.tool_name,
        )
        # T1 Harness: hooks execute natively, always allow
        return common_pb2.HookDecision(decision="allow", reason="", modified_input="")

    # ── 7. OnToolResult ──

    async def OnToolResult(self, request, context):
        logger.info(
            "OnToolResult: session=%s tool=%s error=%s",
            request.session_id,
            request.tool_name,
            request.is_error,
        )
        return common_pb2.HookDecision(decision="allow", reason="", modified_input="")

    # ── 8. OnStop ──

    async def OnStop(self, request, context):
        logger.info("OnStop: session=%s", request.session_id)
        return common_pb2.StopDecision(decision="complete", feedback="")

    # ── 9. ConnectMcp ──

    async def ConnectMcp(self, request, context):
        session_id = request.session_id
        if session_id not in self.sessions:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            return runtime_pb2.ConnectMcpResponse(success=False)

        connected = []
        failed = []
        for server in request.servers:
            # In a real implementation, this would start MCP server processes
            self.sessions[session_id]["mcp_servers"].append(server.name)
            connected.append(server.name)
            logger.info("MCP connected: %s in session %s", server.name, session_id)

        return runtime_pb2.ConnectMcpResponse(
            success=len(failed) == 0,
            connected=connected,
            failed=failed,
        )

    # ── 10. DisconnectMcp ──

    async def DisconnectMcp(self, request, context):
        session_id = request.session_id
        if session_id in self.sessions:
            servers = self.sessions[session_id]["mcp_servers"]
            if request.server_name in servers:
                servers.remove(request.server_name)
        return runtime_pb2.DisconnectMcpResponse(success=True)

    # ── 11. EmitTelemetry ──

    async def EmitTelemetry(self, request, context):
        session_id = request.session_id
        events = []
        if session_id in self.sessions:
            for t in self.sessions[session_id].get("telemetry", []):
                events.append(common_pb2.TelemetryEvent(
                    session_id=session_id,
                    runtime_id=self.config.runtime_id,
                    event_type=t.get("event_type", "unknown"),
                    timestamp=str(t.get("timestamp", "")),
                    payload_json=json.dumps(t),
                ))
        return common_pb2.TelemetryBatch(events=events)

    # ── 12. GetState ──

    async def GetState(self, request, context):
        session_id = request.session_id
        if session_id not in self.sessions:
            context.set_code(grpc.StatusCode.NOT_FOUND)
            return runtime_pb2.SessionState()

        state_data = json.dumps(self.sessions[session_id]).encode()
        return runtime_pb2.SessionState(
            session_id=session_id,
            state_data=state_data,
            runtime_id=self.config.runtime_id,
            created_at=str(self.sessions[session_id].get("created_at", "")),
            state_format="python-json",
        )

    # ── 13. RestoreState ──

    async def RestoreState(self, request, context):
        try:
            state = json.loads(request.state_data)
            session_id = request.session_id or f"crt-restored-{uuid.uuid4().hex[:8]}"
            self.sessions[session_id] = state
            logger.info("State restored: session=%s", session_id)
            return runtime_pb2.InitializeResponse(session_id=session_id)
        except Exception as e:
            context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
            context.set_details(str(e))
            return runtime_pb2.InitializeResponse(session_id="")

    # ── 14. PauseSession ──

    async def PauseSession(self, request, context):
        session_id = request.session_id
        if session_id in self.sessions:
            self.sessions[session_id]["state"] = "paused"
            return runtime_pb2.PauseResponse(success=True)
        return runtime_pb2.PauseResponse(success=False)

    # ── 15. ResumeSession ──

    async def ResumeSession(self, request, context):
        session_id = request.session_id
        if session_id in self.sessions:
            self.sessions[session_id]["state"] = "active"
            return runtime_pb2.ResumeResponse(success=True, session_id=session_id)
        context.set_code(grpc.StatusCode.NOT_FOUND)
        return runtime_pb2.ResumeResponse(success=False, session_id="")

    # ── 16. Terminate ──

    async def Terminate(self, request, context):
        session_id = request.session_id
        telemetry_batch = None

        if session_id in self.sessions:
            # Collect final telemetry
            events = []
            for t in self.sessions[session_id].get("telemetry", []):
                events.append(common_pb2.TelemetryEvent(
                    session_id=session_id,
                    runtime_id=self.config.runtime_id,
                    event_type=t.get("event_type", ""),
                    timestamp=str(t.get("timestamp", "")),
                ))
            telemetry_batch = common_pb2.TelemetryBatch(events=events)

            del self.sessions[session_id]
            logger.info("Session terminated: %s", session_id)

        return runtime_pb2.TerminateResponse(
            success=True,
            final_telemetry=telemetry_batch,
        )
```

**Step 2: 实现 __main__.py — gRPC server 入口**

```python
"""claude-code-runtime gRPC server entry point."""

import argparse
import asyncio
import logging
import signal

import grpc
from grpc.aio import server as aio_server

from ._proto.eaasp.runtime.v1 import runtime_pb2_grpc
from .config import RuntimeConfig
from .service import RuntimeServiceImpl

logger = logging.getLogger(__name__)


async def serve(config: RuntimeConfig) -> None:
    """Start the gRPC server."""
    server = aio_server()
    service = RuntimeServiceImpl(config)
    runtime_pb2_grpc.add_RuntimeServiceServicer_to_server(service, server)

    addr = f"[::]:{config.grpc_port}"
    server.add_insecure_port(addr)

    await server.start()
    logger.info(
        "claude-code-runtime gRPC server started on %s (model=%s)",
        addr,
        config.anthropic_model_name,
    )

    # Graceful shutdown
    loop = asyncio.get_event_loop()
    stop_event = asyncio.Event()

    def _signal_handler():
        logger.info("Shutdown signal received")
        stop_event.set()

    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, _signal_handler)

    await stop_event.wait()
    await server.stop(grace=5)
    logger.info("Server stopped.")


def main():
    parser = argparse.ArgumentParser(description="claude-code-runtime gRPC server")
    parser.add_argument("--port", type=int, default=None, help="gRPC port (default: 50052)")
    parser.add_argument("--env-file", type=str, default=None, help="Path to .env file")
    parser.add_argument("--log-level", type=str, default="INFO", help="Log level")
    args = parser.parse_args()

    logging.basicConfig(
        level=getattr(logging, args.log_level.upper()),
        format="%(asctime)s %(name)s %(levelname)s %(message)s",
    )

    config = RuntimeConfig.from_env(env_file=args.env_file)
    if args.port:
        config.grpc_port = args.port

    asyncio.run(serve(config))


if __name__ == "__main__":
    main()
```

**Step 3: 编译 + 验证**

```bash
cd lang/claude-code-runtime-python
uv run python build_proto.py
uv run python -m claude_code_runtime --help
```

**Step 4: 单元测试**

创建 `tests/test_config.py` 和 `tests/test_service.py`，验证配置加载和 gRPC 服务桩。

**Step 5: Commit**

```bash
git add lang/claude-code-runtime-python/
git commit -m "feat(claude-code-runtime): W4 — Python T1 Harness skeleton + gRPC service"
```

---

## W5: Hook 执行 + 遥测 + Skill + 状态管理

### Task W5-T1: Session 管理器

**Files:**
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/session.py`

实现 `SessionManager` 类：
- `create_session(payload) -> session_id`
- `get_session(session_id) -> Session`
- `terminate_session(session_id)`
- `pause_session(session_id)` / `resume_session(session_id)`
- Session 对象持有会话状态、已加载 skills、MCP 连接

---

### Task W5-T2: Hook 执行器（T1 本地模式）

**Files:**
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/hook_executor.py`

实现 `HookExecutor` 类：
- `evaluate_pre_tool_call(session, tool_name, input) -> HookDecision`
- `evaluate_post_tool_result(session, tool_name, output) -> HookDecision`
- `evaluate_stop(session) -> StopDecision`
- 从 `managed_hooks_json`（SessionPayload 中的 L3 策略）加载规则
- Deny-always-wins 策略（与 Rust InProcessHookBridge 对齐）

---

### Task W5-T3: 遥测采集器

**Files:**
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/telemetry.py`

实现 `TelemetryCollector` 类：
- `record_event(event_type, payload)`
- `flush() -> list[TelemetryEvent]`
- 事件类型对齐 grid-runtime 的 `EaaspEventType`

---

### Task W5-T4: Skill 加载器 + 状态管理 + 事件映射

**Files:**
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/skill_loader.py`
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/state_manager.py`
- Create: `lang/claude-code-runtime-python/src/claude_code_runtime/mapper.py`

**skill_loader.py**: 解析 SkillContent（YAML frontmatter + prose），提取 scoped hooks。
**state_manager.py**: Session 状态序列化/反序列化（JSON format, `state_format="python-json"`）。
**mapper.py**: SDK `AssistantMessage`/`ToolUseBlock`/`ResultMessage` → gRPC `ResponseChunk` 映射。

---

### Task W5-T5: 集成 service.py + 测试

将 W5-T1~T4 的组件集成到 `service.py` 中，替换桩实现。运行单元测试。

**Step: Commit**

```bash
git add lang/claude-code-runtime-python/
git commit -m "feat(claude-code-runtime): W5 — hooks + telemetry + skill + state management"
```

---

## W6: 集成验证（certifier 验证两个 runtime）

### Task W6-T1: grid-runtime gRPC server 启动支持

**Files:**
- Modify: `crates/grid-runtime/src/main.rs`（如果需要，确保可以独立启动 gRPC server）

验证 `grid-runtime` 可以在 :50051 启动 gRPC server。

---

### Task W6-T2: 集成测试脚本

**Files:**
- Create: `lang/claude-code-runtime-python/tests/test_integration.py`
- Create: `scripts/verify-dual-runtime.sh`

**verify-dual-runtime.sh**: 启动两个 runtime，运行 certifier 验证两者。

```bash
#!/bin/bash
# Verify both runtimes pass EAASP contract verification.
# Requires: ANTHROPIC_API_KEY in environment.

set -euo pipefail

echo "=== Starting grid-runtime on :50051 ==="
cargo run -p grid-runtime -- --port 50051 &
GRID_PID=$!

echo "=== Starting claude-code-runtime on :50052 ==="
cd lang/claude-code-runtime-python
uv run python -m claude_code_runtime --port 50052 &
CLAUDE_PID=$!

sleep 3

echo "=== Verifying grid-runtime ==="
cargo run -p eaasp-certifier -- verify --endpoint http://localhost:50051

echo "=== Verifying claude-code-runtime ==="
cargo run -p eaasp-certifier -- verify --endpoint http://localhost:50052

kill $GRID_PID $CLAUDE_PID 2>/dev/null || true
echo "=== Both runtimes verified ==="
```

**Step: Commit**

```bash
git add lang/ scripts/
git commit -m "feat(claude-code-runtime): W6 — integration verification + dual-runtime script"
```

---

## Deferred Items (Phase BE W4-W6)

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| BE-D6 | claude-code-runtime Dockerfile 容器化 | 基本功能稳定后 | ✅ 已补 (bd2c967) |
| BE-D7 | MCP server 真实连接（当前是记录名称） | claude-agent-sdk MCP 支持 | ⏳ |
| BE-D8 | Skill frontmatter YAML hook 解析 + 注入到 SDK | Skill 规范稳定 | ⏳ |
| BE-D9 | 会话持久化（当前是内存） | L4 Session Store | ⏳ |
| BE-D10 | ANTHROPIC_BASE_URL 端到端验证 | 手动测试 | ⏳ |

---

## 验收标准

1. `uv run python -m claude_code_runtime --help` 正常输出
2. `uv run python -m claude_code_runtime --port 50052` 启动 gRPC server
3. `eaasp-certifier verify --endpoint http://localhost:50052` 16/16 方法通过
4. 单元测试: `uv run pytest` 全部通过（不需要 API Key）
5. 集成测试: `scripts/verify-dual-runtime.sh` 两个 runtime 都通过（需要 API Key）
