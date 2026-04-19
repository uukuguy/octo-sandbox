---
id: ADR-V2-005
title: "Session 级工具容器隔离（Tool Sandbox Container）"
type: contract
status: Accepted
date: 2026-04-13
phase: "Phase 0.5 MVP 全层贯通 — hermes-runtime 验证"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace:
    - "lang/hermes-runtime-python/tests/test_mcp_bridge.py"
    - "tools/mock-scada/tests/test_server.py"
    - "tools/eaasp-l4-orchestration/tests/test_session_orchestrator.py"
  review_checklist: null
affected_modules:
  - "tools/mock-scada/"
  - "lang/hermes-runtime-python/"
  - "crates/grid-runtime/"
  - "tools/eaasp-l4-orchestration/"
related: [ADR-V2-004, ADR-V2-019]
---

# ADR-V2-005 — Session 级工具容器隔离（Tool Sandbox Container）

**Status:** Accepted
**Date:** 2026-04-13
**Phase:** Phase 0.5 MVP 全层贯通 — hermes-runtime 验证
**Related:** ADR-V2-004 (L4→L1 gRPC), L1_RUNTIME_T0_T3_COMPLETE.md, EAASP_v2_0_EVOLUTION_PATH.md

---

## 背景

EAASP L1 Runtime 容器化后面临一个核心问题：**MCP Server（如 mock-scada）是 skill 的动态依赖，不能打包进 runtime 容器镜像**。

- Skill 的 MCP 依赖在 L2 skill-registry 管理，每个 skill 不同
- MCP Server 有自己的运行环境依赖（Python、Node.js 等）
- Runtime 容器应只包含 harness 逻辑，不应膨胀为全能环境

Anthropic Managed Agents 的方案是为每个 session 启动一个**工具容器（tool sandbox）**，实现 harness 与 tools 的物理隔离。EAASP 的多 Tier、多 Runtime 场景比 CC 更复杂，但核心思路可以复用。

## 决策

### 1. 架构模型：Sibling Container + Network MCP

```
┌─────────────────────────────────────────────────────┐
│  L4 Orchestration                                   │
│  create_session(skill, runtime_id)                  │
│    ├── 1. 从 skill-registry 获取 skill dependencies │
│    ├── 2. 解析 MCP server 配置列表                   │
│    ├── 3. 启动 tool-sandbox 容器                     │
│    │      (每个 MCP server 一个进程)                 │
│    ├── 4. 调用 L1 ConnectMCP(servers)                │
│    │      transport=sse, url=tool-sandbox:port       │
│    └── 5. L1 runtime 通过 network MCP 连接工具       │
└─────────────────────────────────────────────────────┘

┌──────────────────┐    SSE/HTTP     ┌──────────────────┐
│  L1 Runtime      │ ◄────────────► │  Tool Sandbox    │
│  (hermes-runtime │                 │  Container       │
│   容器)           │                 │                  │
│                  │                 │  mock-scada:8090 │
│  AIAgent         │                 │  l2-memory:8091  │
│    ↓ tool_call   │                 │  (future tools)  │
│    → MCP client ─┼─── SSE ───────►│                  │
│    ← result ─────┼─── SSE ────────│                  │
└──────────────────┘                 └──────────────────┘
```

**不使用 Docker-in-Docker**：避免 Docker socket mount 带来的安全风险。由 L4（或 dev-eaasp.sh）在宿主机层面编排容器。

### 2. 生命周期

| 阶段 | 动作 | 责任方 |
|------|------|--------|
| Session 创建 | 解析 skill dependencies → 启动 tool-sandbox 容器 | L4 (生产) / dev-eaasp.sh (开发) |
| Session 初始化 | ConnectMCP(transport=sse, url=sandbox:port) | L4 → L1 |
| Session 运行 | L1 runtime 通过 SSE 调用 MCP tools | L1 |
| Session 终止 | 停止 tool-sandbox 容器 | L4 / dev-eaasp.sh |

### 3. MCP Transport 选择

| Transport | 适用场景 | Phase 0.5 使用 |
|-----------|---------|---------------|
| **stdio** | 同容器/同进程内 | grid-runtime 裸跑（现有） |
| **SSE / streamable-http** | 跨容器网络 | hermes-runtime 容器化 |

mock-scada 当前只支持 stdio。需要加 SSE transport 包装层。方案：用 `fastmcp` 或 `mcp` SDK 的 SSE server 模式暴露现有 mock-scada 工具。

### 4. 跨 Tier 适配

| Tier | 工具容器策略 | MCP Transport |
|------|-------------|---------------|
| **T0 Native** | 平台预设工具容器镜像 | stdio (同容器) 或 SSE (sibling) |
| **T1 Embedded** | 可同容器 (Rust binary 轻量) 或 sibling | stdio 优先 |
| **T2 Aligned** | **sibling container (本 ADR 验证)** | SSE |
| **T3 Governed** | 纯网络 MCP，无容器控制 | SSE / streamable-http |

### 5. Tool Sandbox 容器镜像

**策略：通用基础镜像 + 动态注入 MCP server 代码**

```dockerfile
# eaasp-tool-sandbox:latest — 通用工具沙箱基础镜像
FROM python:3.12-slim
RUN pip install fastmcp uvicorn
# MCP server 代码通过 volume mount 或 init-container 注入
WORKDIR /opt/tools
CMD ["python", "-m", "tool_runner"]
```

开发环境用 volume mount：
```bash
docker run -v $PROJECT_ROOT/tools/mock-scada:/opt/tools/mock-scada \
    eaasp-tool-sandbox:latest
```

生产环境用 OCI artifact 或 init-container 注入。

## Phase 0.5 验证范围 (Minimal)

仅验证核心链路可行性，不做完整的 session 级生命周期管理：

1. **mock-scada 加 SSE transport** — 复用 fastmcp 或手动 SSE wrapper
2. **dev-eaasp.sh 启动 tool-sandbox 容器** — 固定启动，非 per-session
3. **hermes-runtime ConnectMCP 实现** — 通过 SSE 连接 tool-sandbox
4. **hermes-agent MCP tool 注入** — tool schema → agent.tools + handle_function_call 拦截
5. **端到端验证** — hermes-runtime session 能调 mock-scada scada_read_snapshot

**不做**：
- Per-session 容器生命周期（固定启动）
- 动态镜像构建
- 多 MCP server 容器编排
- T0/T1 的工具容器（它们继续裸跑 stdio）

## 后果

### 正面
- 验证 Managed Agents 工具容器隔离模式在 EAASP 多 Tier 体系中的可行性
- 为 Phase 1 的容器编排设计提供实证基础
- mock-scada SSE transport 同时支持裸跑和容器两种部署模式

### 负面 / 风险
- mock-scada SSE wrapper 增加一层间接性
- 网络 MCP 比 stdio 多了序列化开销和故障模式（网络超时、连接断开）
- hermes-agent 的 handle_function_call monkey-patch 叠加（governance + MCP）需要仔细排序

### Deferred
- **D62**: Per-session tool-sandbox 容器生命周期管理（L4 编排）
- **D63**: Tool-sandbox 通用基础镜像 + OCI artifact 分发
- **D64**: T0/T1 runtime 的工具容器化（当前继续 stdio）
- **D65**: MCP server 多实例 / 连接池
