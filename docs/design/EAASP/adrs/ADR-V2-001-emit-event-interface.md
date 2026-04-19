---
id: ADR-V2-001
title: "EmitEvent 接口形式：混合拦截器 + OPTIONAL RPC"
type: contract
status: Accepted
date: 2026-04-13
phase: "Phase 1 — Event-driven Foundation"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace:
    - "tools/eaasp-l4-orchestration/tests/test_event_interceptor.py"
    - "tools/eaasp-l4-orchestration/tests/test_event_api.py"
    - "tools/eaasp-l4-orchestration/tests/test_event_integration.py"
    - "tools/eaasp-l4-orchestration/tests/test_event_handlers.py"
    - "tests/contract/contract_v1/test_proto_shape.py"
  review_checklist: null
affected_modules:
  - "proto/eaasp/runtime/v2/runtime.proto"
  - "tools/eaasp-l4-orchestration/"
  - "crates/grid-runtime/"
  - "lang/claude-code-runtime-python/"
  - "lang/hermes-runtime-python/"
related: [ADR-V2-002, ADR-V2-003, ADR-V2-004]
---

# ADR-V2-001 — EmitEvent 接口形式：混合拦截器 + OPTIONAL RPC

**Status:** Accepted
**Date:** 2026-04-13
**Phase:** Phase 1 — Event-driven Foundation
**Related:** ADR-V2-002 (Event Stream backend), ADR-V2-003 (Event clustering), ADR-V2-004 (L4→L1 gRPC)
**Blocks:** L1→Session Event Stream 写入接口

---

## 背景

Phase 0.75 完成后，三个 L1 runtime 均可通过 L4 ConnectMCP 获取 MCP 配置并运行 agent。但事件可观测性缺失——L4 只有 session lifecycle events（SESSION_CREATED / USER_MESSAGE / POST_SESSION_END），没有 agent 运行过程中的细粒度事件（tool call / thinking / token usage）。

proto 已定义 `rpc EmitEvent(EventStreamEntry) returns (Empty)` 作为 PLACEHOLDER（Phase 0 runtimes 全部 no-op）。Phase 1 需要决定：

1. EmitEvent 是升级为 MUST 方法（certifier 强制验证），还是保持 OPTIONAL？
2. L1 runtime 如何将事件传递给 L4 Event Engine？
3. 不同 tier 的 runtime（T1 Managed / T2 Aligned / T3 Wrapped）是否用同一种机制？

### 三种候选方案

| 方案 | 描述 | T1 适合度 | T2/T3 适合度 | Phase 1 工作量 |
|------|------|---------|-------------|-------------|
| **A. MUST 方法** | EmitEvent 升级为 certifier 核心方法（12→13 MUST） | ⭐⭐⭐⭐⭐ | ⭐⭐（增加认证门槛） | 大（所有 runtime 必须实装） |
| **B. Hook-bridge 副作用** | 事件通过 hook-bridge gRPC sidecar 写入 L4 | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 中（扩展 hook-bridge） |
| **C. 平台拦截器** | L4 从现有 RPC（OnToolCall/OnToolResult/OnStop）自动提取事件 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | 小（零 runtime 改造） |

---

## 决策

**采用混合方案 A+C**：平台拦截器兜底 + EmitEvent 保持 OPTIONAL（T1 必须实装）。

### 具体设计

```
事件来源 1（拦截器 — 所有 runtime 自动覆盖）:
  L1 OnToolCall RPC ──→ L4 session_orchestrator ──→ 自动提取 PRE_TOOL_USE 事件
  L1 OnToolResult RPC ──→ L4 session_orchestrator ──→ 自动提取 POST_TOOL_USE 事件
  L1 OnStop RPC ──→ L4 session_orchestrator ──→ 自动提取 STOP 事件

事件来源 2（EmitEvent OPTIONAL — T1 runtime 主动 emit 更丰富的事件）:
  L1 EmitEvent RPC ──→ L4 Event Engine ──→ 补充 SESSION_START / THINKING / TOKEN_USAGE 等
```

### 规则

1. **EmitEvent 保持 OPTIONAL** — certifier MUST 核心方法维持 12 个不变
2. **T1 runtime（grid-runtime）必须实装 EmitEvent** — 作为平台示范实现，emit 全部 9 种 L1 事件
3. **T2/T3 runtime 可不实装 EmitEvent** — L4 拦截器已覆盖 PRE_TOOL_USE / POST_TOOL_USE / STOP 三种核心事件
4. **L4 拦截器在 session_orchestrator 层实现** — 不修改 proto，不增加 runtime 负担
5. **去重由 Event Engine Deduplicator 处理** — 如果 T1 同时 emit + 拦截器提取，Deduplicator 合并
6. **EmitEvent 调用语义：fire-and-forget** — 失败不阻塞 agent loop，runtime 本地缓冲 + 重试

### 拦截器提取规则

| 现有 RPC | 提取的 HookEventType | payload 来源 |
|---------|---------------------|-------------|
| `OnToolCall(ToolCallEvent)` | `PRE_TOOL_USE` | tool_name, arguments_json |
| `OnToolResult(ToolResultEvent)` | `POST_TOOL_USE` / `POST_TOOL_USE_FAILURE` | tool_name, result_json, error |
| `OnStop(StopEvent)` | `STOP` | reason, final_output |
| `Initialize` 成功 | `SESSION_START` | session_id, runtime_name |
| `Terminate` 调用 | `POST_SESSION_END` | session_id |

### EmitEvent 补充事件（T1 runtime 额外 emit）

| HookEventType | 触发点 | 拦截器无法覆盖原因 |
|---------------|--------|------------------|
| `USER_PROMPT_SUBMIT` | 用户消息到达 runtime | 拦截器在 L4 Send 层已知，但 emit 可附加 runtime 视角的 metadata |
| `PRE_COMPACT` | context window 压缩前 | 拦截器完全不可见 |
| `SUBAGENT_STOP` | 子 agent 完成 | 拦截器完全不可见 |
| `PERMISSION_REQUEST` | 工具权限请求 | 拦截器完全不可见 |

### L4 接收端点

EmitEvent 的 L4 接收侧有两种路径：

1. **gRPC 反向调用**（Phase 1 不做）：L4 作为 gRPC server 接收 L1 的 EmitEvent 调用。需要 L1→L4 的 gRPC channel，当前架构只有 L4→L1。
2. **REST fallback**（Phase 1 采用）：L1 runtime 通过 HTTP POST `/v1/events/ingest` 发送事件到 L4。简单、无需新 gRPC channel。

```
Phase 1 事件传递路径：
  grid-runtime (Rust) ──HTTP POST──→ L4 /v1/events/ingest ──→ Event Engine
  claude-code-runtime ──不 emit（拦截器覆盖核心事件）
  hermes-runtime ──不 emit（拦截器覆盖核心事件）
```

---

## 后果

### 正面

1. **零 runtime 改造启动** — 拦截器立即在 Phase 0.75 的基础上提供 3 种核心事件
2. **渐进式增强** — T1 先实装 EmitEvent，Phase 2+ 逐步扩展到 T2/T3
3. **certifier 不膨胀** — MUST 维持 12，不增加生态准入门槛
4. **去重统一** — Event Engine Deduplicator 处理拦截器 + EmitEvent 的重叠

### 负面

1. **T2/T3 事件粒度受限** — 只有 tool call / result / stop，缺失 thinking / token_usage
2. **拦截器 + EmitEvent 双源** — Deduplicator 需要处理同一事件的两份拷贝
3. **REST fallback 延迟** — HTTP POST 比 gRPC stream 慢，但对 fire-and-forget 可接受

### 新增 Deferred

| ID | 描述 | 目标 Phase |
|---|------|-----------|
| **D73** | Event Room 推迟，Phase 1 session = event 容器 | Phase 4 |
| **D74** | EmitEvent gRPC 反向通道（L1→L4 gRPC server） | Phase 2 |

---

## Proto 变更

无需修改 proto — `EmitEvent` 和 `EventStreamEntry` 已定义。只需移除注释中的 "PLACEHOLDER" 标记。
