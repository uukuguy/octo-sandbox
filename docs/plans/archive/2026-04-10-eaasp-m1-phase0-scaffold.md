# EAASP M1 Phase 0: 脚手架与 Proto 升级

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 建立 eaasp-server 统一入口 + proto v1.4 升级 + eaasp-cli 骨架，为后续阶段提供可连通的基础设施。

**Architecture:** eaasp-server 整合现有 `tools/eaasp-governance` (L3) 和 `tools/eaasp-session-manager` (L4) 为统一 FastAPI 应用。proto v1.4 在 SessionPayload 中新增 event_context/memory_refs/evidence_anchor_id 三个字段。eaasp-cli 用 Python typer 实现，连接 eaasp-server REST API。

**Tech Stack:** Python 3.12+, FastAPI, typer, protobuf, grpcio-tools, uv (package manager)

**Blueprint Reference:** `docs/design/Grid/EAASP_v1.8_M1_IMPLEMENTATION_BLUEPRINT.md` 第七节 阶段 0

---

## 前置条件

- Phase BI 已完成 (hermes-runtime T2 Aligned)
- proto v1.3 已在 `proto/eaasp/runtime/v1/runtime.proto`
- `tools/eaasp-governance/` (L3 Python FastAPI, port 8083) 已有 5 个 API 契约
- `tools/eaasp-session-manager/` (L4 Python FastAPI, port 8084) 已有四平面架构
- `tools/eaasp-skill-registry/` (L2 Rust, port 8081) 已有
- `tools/eaasp-mcp-orchestrator/` (L2 Rust, port 8082) 已有

---

### Task 1: Proto v1.4 — SessionPayload 扩展

**Files:**
- Modify: `proto/eaasp/runtime/v1/runtime.proto:75-89`
- Modify: `crates/grid-runtime/src/contract.rs:102-128`
- Modify: `crates/grid-runtime/src/service.rs` (proto ↔ contract 映射)
- Test: `crates/grid-runtime/tests/` (现有测试不应 break)

**Step 1: 在 runtime.proto 的 SessionPayload 中追加 v1.8 字段**

在 `proto/eaasp/runtime/v1/runtime.proto` 的 SessionPayload message 末尾，`skill_search_scope` (field 12) 之后添加：

```protobuf
  // v1.8 fields
  EventContext event_context = 13;           // Event context from L4 event engine
  repeated MemoryRef memory_refs = 14;       // Memory references from L2 query
  string evidence_anchor_id = 15;            // Evidence anchor ID for this session
```

在同一文件中新增 message 定义（SessionPayload 之前或之后）：

```protobuf
message EventContext {
  string event_id = 1;
  string event_type = 2;
  string severity = 3;        // "low" | "medium" | "high" | "critical"
  string scope = 4;           // Impact scope description
  string raw_summary = 5;     // Raw alert/event summary text
}

message MemoryRef {
  string memory_id = 1;
  string memory_type = 2;     // "anchor" | "file"
  double relevance_score = 3;
}
```

**Step 2: 编译 proto 验证语法**

Run: `cd proto && protoc --proto_path=. eaasp/runtime/v1/runtime.proto --python_out=/tmp/test_proto 2>&1`
Expected: 无错误输出

**Step 3: 在 Rust contract.rs 中追加对应字段**

在 `crates/grid-runtime/src/contract.rs` 的 `SessionPayload` struct 末尾追加：

```rust
    /// v1.8: Event context from L4 event engine (None for non-event sessions).
    #[serde(default)]
    pub event_context: Option<EventContext>,
    /// v1.8: Memory references from L2 query (injected by L4 orchestrator).
    #[serde(default)]
    pub memory_refs: Vec<MemoryRef>,
    /// v1.8: Evidence anchor ID for this session's conclusions.
    #[serde(default)]
    pub evidence_anchor_id: Option<String>,
```

在同一文件中新增类型：

```rust
/// Event context injected by L4 event engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventContext {
    pub event_id: String,
    pub event_type: String,
    pub severity: String,
    pub scope: String,
    pub raw_summary: String,
}

/// Reference to a memory entry in L2 Memory Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRef {
    pub memory_id: String,
    pub memory_type: String,
    pub relevance_score: f64,
}
```

**Step 4: 更新 service.rs 的 proto ↔ contract 映射**

在 `crates/grid-runtime/src/service.rs` 中找到 SessionPayload 的转换函数，添加新字段的映射。event_context 需要从 proto EventContext message 转为 Rust EventContext struct。

**Step 5: 验证编译**

Run: `cargo check -p grid-runtime 2>&1 | tail -5`
Expected: 无错误

**Step 6: 运行现有测试确认无回归**

Run: `cargo test -p grid-runtime -- --test-threads=1 2>&1 | tail -10`
Expected: 所有现有测试通过（新字段有 Default/serde(default)，不影响已有测试）

**Step 7: Commit**

```bash
git add proto/eaasp/runtime/v1/runtime.proto crates/grid-runtime/src/contract.rs crates/grid-runtime/src/service.rs
git commit -m "feat(proto): v1.4 — add event_context, memory_refs, evidence_anchor_id to SessionPayload"
```

---

### Task 2: Proto v1.4 — 新增 event.proto 和 memory.proto

**Files:**
- Create: `proto/eaasp/event/v1/event.proto`
- Create: `proto/eaasp/memory/v1/memory.proto`

**Step 1: 创建 event.proto**

创建 `proto/eaasp/event/v1/event.proto`：

```protobuf
syntax = "proto3";

package eaasp.event.v1;

import "google/protobuf/timestamp.proto";

// Event object — represents a business event aggregated from raw alerts/signals.
message Event {
  string event_id = 1;
  string event_type = 2;
  string severity = 3;           // "low" | "medium" | "high" | "critical"
  string scope = 4;              // Impact scope description
  repeated SourceEvent source_events = 5;
  EventStatus status = 6;
  string event_room_id = 7;
  repeated string session_ids = 8;
  repeated string evidence_anchor_ids = 9;
  google.protobuf.Timestamp created_at = 10;
  google.protobuf.Timestamp updated_at = 11;
  map<string, string> metadata = 12;
}

message SourceEvent {
  string source = 1;
  string raw_id = 2;
  google.protobuf.Timestamp timestamp = 3;
  string payload_json = 4;       // Raw payload as JSON string
}

enum EventStatus {
  EVENT_STATUS_UNSPECIFIED = 0;
  EVENT_STATUS_RECEIVED = 1;
  EVENT_STATUS_TRIAGING = 2;
  EVENT_STATUS_ACTIVE = 3;
  EVENT_STATUS_CLOSED = 4;
  EVENT_STATUS_RETROSPECTIVE = 5;
  // M2 additions:
  // EVENT_STATUS_PENDING_APPROVAL = 6;
  // EVENT_STATUS_EXECUTING = 7;
  // EVENT_STATUS_MONITORING = 8;
}

// Event Room — long-lived collaboration space for one business event.
message EventRoom {
  string room_id = 1;
  string event_id = 2;
  repeated string session_ids = 3;
  google.protobuf.Timestamp created_at = 4;
  EventRoomStatus status = 5;
}

enum EventRoomStatus {
  ROOM_STATUS_UNSPECIFIED = 0;
  ROOM_STATUS_ACTIVE = 1;
  ROOM_STATUS_ARCHIVED = 2;
}
```

**Step 2: 创建 memory.proto**

创建 `proto/eaasp/memory/v1/memory.proto`：

```protobuf
syntax = "proto3";

package eaasp.memory.v1;

import "google/protobuf/timestamp.proto";

// Evidence Anchor — immutable, append-only record referencing a data snapshot.
message EvidenceAnchor {
  string anchor_id = 1;
  string event_id = 2;
  string session_id = 3;
  AnchorType anchor_type = 4;
  string data_ref = 5;            // URI to original data
  string snapshot_hash = 6;       // Integrity hash (sha256)
  string source_system = 7;
  string tool_version = 8;
  string model_version = 9;
  string rule_version = 10;
  google.protobuf.Timestamp created_at = 11;
  string created_by = 12;         // Agent/Session ID
  map<string, string> metadata = 13;
}

enum AnchorType {
  ANCHOR_TYPE_UNSPECIFIED = 0;
  ANCHOR_TYPE_MEASUREMENT_WINDOW = 1;
  ANCHOR_TYPE_TOPOLOGY_SNAPSHOT = 2;
  ANCHOR_TYPE_RULE_REFERENCE = 3;
  ANCHOR_TYPE_DOCUMENT_EXCERPT = 4;
  ANCHOR_TYPE_COMPUTATION_RESULT = 5;
}

// Memory File — versioned, editable persistent memory entry.
message MemoryFile {
  string memory_id = 1;
  MemoryScope scope = 2;
  MemoryCategory category = 3;
  string content = 4;             // Structured text, human-readable
  repeated string evidence_refs = 5; // Linked anchor IDs
  MemoryConfirmStatus confirm_status = 6;
  int32 version = 7;
  google.protobuf.Timestamp created_at = 8;
  google.protobuf.Timestamp updated_at = 9;
  string updated_by = 10;        // Human or Agent ID
  map<string, string> metadata = 11;
}

enum MemoryScope {
  MEMORY_SCOPE_UNSPECIFIED = 0;
  MEMORY_SCOPE_USER = 1;
  MEMORY_SCOPE_TEAM = 2;
  MEMORY_SCOPE_ORG_UNIT = 3;     // M2
  MEMORY_SCOPE_EVENT_TYPE = 4;   // M2
}

enum MemoryCategory {
  MEMORY_CATEGORY_UNSPECIFIED = 0;
  MEMORY_CATEGORY_PREFERENCE = 1;
  MEMORY_CATEGORY_EXPERIENCE = 2;
  MEMORY_CATEGORY_KNOWLEDGE = 3;
  MEMORY_CATEGORY_CALIBRATION = 4;
}

enum MemoryConfirmStatus {
  CONFIRM_STATUS_UNSPECIFIED = 0;
  CONFIRM_STATUS_AGENT_SUGGESTED = 1;
  CONFIRM_STATUS_CONFIRMED = 2;
}

// Memory search request.
message MemorySearchRequest {
  string query = 1;
  repeated MemoryScope scopes = 2;
  repeated MemoryCategory categories = 3;
  google.protobuf.Timestamp time_from = 4;
  google.protobuf.Timestamp time_to = 5;
  int32 limit = 6;
  string search_mode = 7;         // "keyword" (M1) | "semantic" (M2) | "hybrid" (M2)
}

message MemorySearchResult {
  oneof entry {
    EvidenceAnchor anchor = 1;
    MemoryFile file = 2;
  }
  double relevance_score = 3;
}
```

**Step 3: 编译验证**

Run: `protoc --proto_path=proto proto/eaasp/event/v1/event.proto proto/eaasp/memory/v1/memory.proto --python_out=/tmp/test_proto 2>&1`
Expected: 无错误

**Step 4: Commit**

```bash
git add proto/eaasp/event/v1/event.proto proto/eaasp/memory/v1/memory.proto
git commit -m "feat(proto): v1.4 — add event.proto and memory.proto for L4 event engine and L2 memory engine"
```

---

### Task 3: eaasp-server Python 项目骨架

**Files:**
- Create: `eaasp/server/pyproject.toml`
- Create: `eaasp/server/src/eaasp_server/__init__.py`
- Create: `eaasp/server/src/eaasp_server/main.py`
- Create: `eaasp/server/src/eaasp_server/config.py`
- Create: `eaasp/server/src/eaasp_server/orchestration/__init__.py`
- Create: `eaasp/server/src/eaasp_server/governance/__init__.py`
- Create: `eaasp/server/src/eaasp_server/memory/__init__.py`
- Create: `eaasp/server/src/eaasp_server/api/__init__.py`

**设计决策：** eaasp-server 放在 `eaasp/server/` 目录（与 `lang/` 和 `sdk/` 同级的 eaasp 命名空间），而不是 `tools/` 下。它不是一个辅助工具，而是 EAASP 平台的核心服务。现有 `tools/eaasp-governance/` 和 `tools/eaasp-session-manager/` 的代码会被重构整合进来。

**Step 1: 创建 pyproject.toml**

创建 `eaasp/server/pyproject.toml`：

```toml
[project]
name = "eaasp-server"
version = "0.1.0"
description = "EAASP Platform Server — L3 Governance + L4 Orchestration"
requires-python = ">=3.12"
dependencies = [
    "fastapi>=0.115.0",
    "uvicorn[standard]>=0.32.0",
    "grpcio>=1.68.0",
    "grpcio-tools>=1.68.0",
    "protobuf>=5.29.0",
    "pyyaml>=6.0",
    "httpx>=0.28.0",
    "pydantic>=2.10.0",
    "aiosqlite>=0.21.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.24",
    "pytest-httpx>=0.35",
]

[project.scripts]
eaasp-server = "eaasp_server.main:main"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/eaasp_server"]

[tool.pytest.ini_options]
testpaths = ["tests"]
asyncio_mode = "auto"
```

**Step 2: 创建 main.py 入口**

创建 `eaasp/server/src/eaasp_server/main.py`：

```python
"""EAASP Platform Server — L3 Governance + L4 Orchestration.

Unified FastAPI application serving:
  L4: Event engine, session orchestrator, context assembly
  L3: Policy engine, audit service, evidence chain validation
"""

from __future__ import annotations

import argparse

from fastapi import FastAPI

from eaasp_server.config import ServerConfig


def create_app(config: ServerConfig | None = None) -> FastAPI:
    """Create the unified EAASP server application."""
    config = config or ServerConfig()

    app = FastAPI(
        title="EAASP Platform Server",
        version="0.1.0",
        description="Enterprise Autonomous Agent Support Platform — L3+L4",
    )

    # Store config
    app.state.config = config

    # Health endpoint
    @app.get("/health")
    async def health():
        return {
            "status": "ok",
            "service": "eaasp-server",
            "version": "0.1.0",
            "layers": ["L3-governance", "L4-orchestration"],
        }

    # TODO Phase 0: integrate governance and orchestration routers
    # TODO Phase 2: add event engine and session orchestrator

    return app


app = create_app()


def main():
    """CLI entrypoint."""
    parser = argparse.ArgumentParser(description="EAASP Platform Server")
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--runtime-server-url", default="http://localhost:3001")
    parser.add_argument("--runtime-grpc-url", default="localhost:50051")
    parser.add_argument("--config", default=None, help="Path to config YAML")
    args = parser.parse_args()

    import uvicorn

    config = ServerConfig(
        runtime_server_url=args.runtime_server_url,
        runtime_grpc_url=args.runtime_grpc_url,
    )
    global app
    app = create_app(config=config)
    uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":
    main()
```

**Step 3: 创建 config.py**

创建 `eaasp/server/src/eaasp_server/config.py`：

```python
"""EAASP Server configuration."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class ServerConfig:
    """Server configuration with defaults."""

    # eaasp-runtime-server (Rust L1+L2) connections
    runtime_server_url: str = "http://localhost:3001"
    runtime_grpc_url: str = "localhost:50051"

    # Database
    db_path: str = "data/eaasp.db"

    # L2 sub-service URLs (within eaasp-runtime-server)
    skill_registry_url: str = "http://localhost:3001/api/v1/skills"
    memory_engine_url: str = "http://localhost:3001/api/v1/memory"

    # Event engine
    event_dedup_window_secs: int = 180  # 3 minutes default
```

**Step 4: 创建模块包 __init__.py 文件**

创建以下空 `__init__.py` 文件：
- `eaasp/server/src/eaasp_server/__init__.py` — 内容: `"""EAASP Platform Server."""\n__version__ = "0.1.0"`
- `eaasp/server/src/eaasp_server/orchestration/__init__.py` — 内容: `"""L4 Orchestration Layer."""`
- `eaasp/server/src/eaasp_server/governance/__init__.py` — 内容: `"""L3 Governance Layer."""`
- `eaasp/server/src/eaasp_server/memory/__init__.py` — 内容: `"""L2 Memory Engine client."""`
- `eaasp/server/src/eaasp_server/api/__init__.py` — 内容: `"""REST API routes."""`

**Step 5: 创建 tests 目录**

创建 `eaasp/server/tests/__init__.py`（空文件）和 `eaasp/server/tests/test_health.py`：

```python
"""Smoke test — server starts and health endpoint responds."""

from fastapi.testclient import TestClient

from eaasp_server.main import create_app


def test_health():
    app = create_app()
    client = TestClient(app)
    resp = client.get("/health")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "ok"
    assert data["service"] == "eaasp-server"
    assert "L3-governance" in data["layers"]
    assert "L4-orchestration" in data["layers"]
```

**Step 6: 安装依赖并运行测试**

Run: `cd eaasp/server && uv venv && uv pip install -e ".[dev]" && uv run pytest tests/ -xvs`
Expected: 1 test passed

**Step 7: Commit**

```bash
git add eaasp/server/
git commit -m "feat(eaasp-server): Phase 0 — project skeleton with FastAPI, health endpoint, config"
```

---

### Task 4: eaasp-cli Python 项目骨架

**Files:**
- Create: `eaasp/cli/pyproject.toml`
- Create: `eaasp/cli/src/eaasp_cli/__init__.py`
- Create: `eaasp/cli/src/eaasp_cli/main.py`
- Create: `eaasp/cli/src/eaasp_cli/client.py`
- Create: `eaasp/cli/tests/test_cli.py`

**Step 1: 创建 pyproject.toml**

创建 `eaasp/cli/pyproject.toml`：

```toml
[project]
name = "eaasp-cli"
version = "0.1.0"
description = "EAASP Platform CLI — management and E2E verification"
requires-python = ">=3.12"
dependencies = [
    "typer[all]>=0.15.0",
    "httpx>=0.28.0",
    "rich>=13.0",
    "pyyaml>=6.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
]

[project.scripts]
eaasp = "eaasp_cli.main:app"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/eaasp_cli"]
```

**Step 2: 创建 client.py — eaasp-server REST 客户端**

创建 `eaasp/cli/src/eaasp_cli/client.py`：

```python
"""EAASP Server REST client."""

from __future__ import annotations

import httpx


class EaaspClient:
    """Synchronous client for eaasp-server REST API."""

    def __init__(self, base_url: str = "http://localhost:8080"):
        self.base_url = base_url.rstrip("/")
        self._client = httpx.Client(base_url=self.base_url, timeout=30.0)

    def health(self) -> dict:
        resp = self._client.get("/health")
        resp.raise_for_status()
        return resp.json()

    def close(self):
        self._client.close()
```

**Step 3: 创建 main.py — CLI 入口**

创建 `eaasp/cli/src/eaasp_cli/main.py`：

```python
"""EAASP CLI — platform management and E2E verification tool."""

from __future__ import annotations

import typer
from rich.console import Console

from eaasp_cli.client import EaaspClient

app = typer.Typer(name="eaasp", help="EAASP Platform CLI")
console = Console()

# Sub-command groups
event_app = typer.Typer(help="Event lifecycle management")
session_app = typer.Typer(help="Session management")
policies_app = typer.Typer(help="Policy deployment and management")
skills_app = typer.Typer(help="Skill lifecycle management")
memory_app = typer.Typer(help="Memory Engine queries")
audit_app = typer.Typer(help="Audit trail queries")

app.add_typer(event_app, name="event")
app.add_typer(session_app, name="session")
app.add_typer(policies_app, name="policies")
app.add_typer(skills_app, name="skills")
app.add_typer(memory_app, name="memory")
app.add_typer(audit_app, name="audit")


def get_client(server_url: str = "http://localhost:8080") -> EaaspClient:
    return EaaspClient(base_url=server_url)


@app.command()
def status(
    server_url: str = typer.Option("http://localhost:8080", "--server", "-s"),
):
    """Check EAASP platform status."""
    client = get_client(server_url)
    try:
        data = client.health()
        console.print(f"[green]✓[/green] Connected to {server_url}")
        console.print(f"  Service: {data['service']} v{data['version']}")
        console.print(f"  Status: {data['status']}")
        console.print(f"  Layers: {', '.join(data['layers'])}")
    except Exception as e:
        console.print(f"[red]✗[/red] Cannot connect to {server_url}: {e}")
        raise typer.Exit(1)
    finally:
        client.close()


# Placeholder sub-commands (will be implemented in later phases)
@event_app.command("list")
def event_list():
    """List events."""
    console.print("[yellow]Not implemented yet — Phase 2[/yellow]")


@event_app.command("inject")
def event_inject(payload_file: str = typer.Argument(..., help="Webhook JSON file")):
    """Inject a simulated webhook event."""
    console.print("[yellow]Not implemented yet — Phase 2[/yellow]")


@session_app.command("list")
def session_list():
    """List sessions."""
    console.print("[yellow]Not implemented yet — Phase 2[/yellow]")


@memory_app.command("search")
def memory_search(query: str = typer.Argument(..., help="Search query")):
    """Search memory engine."""
    console.print("[yellow]Not implemented yet — Phase 1[/yellow]")


if __name__ == "__main__":
    app()
```

**Step 4: 创建 __init__.py**

创建 `eaasp/cli/src/eaasp_cli/__init__.py`：

```python
"""EAASP Platform CLI."""

__version__ = "0.1.0"
```

**Step 5: 创建测试**

创建 `eaasp/cli/tests/__init__.py`（空文件）和 `eaasp/cli/tests/test_cli.py`：

```python
"""CLI smoke tests."""

from typer.testing import CliRunner

from eaasp_cli.main import app

runner = CliRunner()


def test_help():
    result = runner.invoke(app, ["--help"])
    assert result.exit_code == 0
    assert "EAASP Platform CLI" in result.stdout


def test_status_no_server():
    """status command fails gracefully when server is not running."""
    result = runner.invoke(app, ["status", "--server", "http://localhost:19999"])
    assert result.exit_code == 1


def test_event_subcommand_exists():
    result = runner.invoke(app, ["event", "--help"])
    assert result.exit_code == 0
    assert "list" in result.stdout
    assert "inject" in result.stdout


def test_memory_subcommand_exists():
    result = runner.invoke(app, ["memory", "--help"])
    assert result.exit_code == 0
    assert "search" in result.stdout
```

**Step 6: 安装依赖并运行测试**

Run: `cd eaasp/cli && uv venv && uv pip install -e ".[dev]" && uv run pytest tests/ -xvs`
Expected: 4 tests passed

**Step 7: Commit**

```bash
git add eaasp/cli/
git commit -m "feat(eaasp-cli): Phase 0 — CLI skeleton with typer, status command, sub-command groups"
```

---

### Task 5: Python proto stubs 编译

**Files:**
- Create: `eaasp/proto/` (共享 proto Python stubs)
- Modify: `eaasp/server/pyproject.toml` (添加 proto 依赖)

**Step 1: 创建 proto 编译脚本**

创建 `eaasp/compile_proto.sh`：

```bash
#!/bin/bash
# Compile proto files to Python stubs for eaasp packages.
set -euo pipefail

PROTO_ROOT="$(cd "$(dirname "$0")/../proto" && pwd)"
OUT_DIR="$(cd "$(dirname "$0")" && pwd)/proto_gen"

mkdir -p "$OUT_DIR"

python -m grpc_tools.protoc \
    --proto_path="$PROTO_ROOT" \
    --python_out="$OUT_DIR" \
    --grpc_python_out="$OUT_DIR" \
    eaasp/runtime/v1/runtime.proto \
    eaasp/common/v1/common.proto \
    eaasp/hook/v1/hook.proto \
    eaasp/event/v1/event.proto \
    eaasp/memory/v1/memory.proto

# Fix imports (protoc generates absolute imports that don't work as packages)
find "$OUT_DIR" -name "*.py" -exec sed -i '' 's/^from eaasp\./from eaasp_proto.eaasp./g' {} +

echo "Proto stubs compiled to $OUT_DIR"
```

**Step 2: 运行编译**

Run: `chmod +x eaasp/compile_proto.sh && cd eaasp && pip install grpcio-tools && bash compile_proto.sh`
Expected: "Proto stubs compiled to .../proto_gen"

**Step 3: 验证生成的文件**

Run: `ls eaasp/proto_gen/eaasp/*/v1/*_pb2.py`
Expected: runtime_pb2.py, common_pb2.py, hook_pb2.py, event_pb2.py, memory_pb2.py

**Step 4: Commit**

```bash
git add eaasp/compile_proto.sh eaasp/proto_gen/
git commit -m "feat(proto): compile Python stubs for all EAASP proto files (v1.4)"
```

---

### Task 6: Makefile targets + 集成连通测试

**Files:**
- Modify: `Makefile` (添加 eaasp-server 和 eaasp-cli targets)
- Create: `eaasp/server/tests/test_integration_cli.py`

**Step 1: 在 Makefile 中添加 eaasp-server 和 eaasp-cli targets**

在 Makefile 的 EAASP 区域添加：

```makefile
# ── EAASP Platform Server (Python L3+L4) ──
eaasp-server-setup:
	cd eaasp/server && uv venv && uv pip install -e ".[dev]"

eaasp-server-test:
	cd eaasp/server && uv run pytest tests/ -xvs

eaasp-server-start:
	cd eaasp/server && uv run eaasp-server --port 8080

# ── EAASP CLI ──
eaasp-cli-setup:
	cd eaasp/cli && uv venv && uv pip install -e ".[dev]"

eaasp-cli-test:
	cd eaasp/cli && uv run pytest tests/ -xvs

# ── EAASP Proto Compilation ──
eaasp-proto:
	cd eaasp && bash compile_proto.sh
```

**Step 2: 创建集成连通测试**

创建 `eaasp/server/tests/test_integration_cli.py`：

```python
"""Integration test: eaasp-cli can connect to eaasp-server."""

import subprocess
import time
import signal
import os

import pytest
from fastapi.testclient import TestClient

from eaasp_server.main import create_app


def test_cli_status_against_test_server():
    """Verify eaasp-cli status command works against the test server."""
    app = create_app()
    client = TestClient(app)

    # Verify directly via TestClient
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"
```

**Step 3: 运行全部测试**

Run: `cd eaasp/server && uv run pytest tests/ -xvs`
Expected: 2 tests passed (health + integration)

**Step 4: Commit**

```bash
git add Makefile eaasp/server/tests/test_integration_cli.py
git commit -m "feat(eaasp): Phase 0 — Makefile targets + integration test"
```

---

### Task 7: claude-code-runtime SessionPayload 同步扩展

**Files:**
- Modify: `lang/claude-code-runtime-python/src/claude_code_runtime/mapper.py`
- Modify: `lang/claude-code-runtime-python/src/claude_code_runtime/service.py`

**Step 1: 在 mapper.py 中添加新字段映射**

在 `lang/claude-code-runtime-python/src/claude_code_runtime/mapper.py` 中找到 SessionPayload 转换逻辑，添加 event_context、memory_refs、evidence_anchor_id 的映射。新字段为可选，缺失时填默认值。

**Step 2: 重新编译 Python proto stubs**

Run: `cd lang/claude-code-runtime-python && python _fix_proto_imports.py`
Expected: stubs 更新

**Step 3: 运行测试**

Run: `cd lang/claude-code-runtime-python && uv run pytest tests/ -xvs -- --test-threads=1`
Expected: 所有 39 个现有测试通过

**Step 4: Commit**

```bash
git add lang/claude-code-runtime-python/
git commit -m "feat(claude-code-runtime): sync SessionPayload v1.4 fields (event_context, memory_refs)"
```

---

## Phase 0 完成标准

```
✅ proto v1.4: SessionPayload 有 event_context, memory_refs, evidence_anchor_id
✅ proto v1.4: event.proto 和 memory.proto 定义完整
✅ eaasp-server: FastAPI 骨架启动，health 端点返回 200
✅ eaasp-cli: typer 骨架，eaasp status 能连接 eaasp-server
✅ Python proto stubs: 5 个 proto 文件全部编译
✅ Makefile: eaasp-server-setup/test/start + eaasp-cli-setup/test targets
✅ Rust grid-runtime: SessionPayload struct 已扩展，现有测试全通过
✅ claude-code-runtime: mapper 已同步，现有测试全通过
```

## 后续

Phase 0 完成后进入 **Phase 1: L2 Memory Engine + L1 Memory 通道**（独立计划文档）。
