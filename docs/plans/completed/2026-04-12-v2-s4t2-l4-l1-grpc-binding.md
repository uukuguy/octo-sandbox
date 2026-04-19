---
id: ADR-V2-004
title: "L4→L1 Real gRPC Binding（延迟到 Phase 1）"
type: record
status: Archived
date: 2026-04-12
phase: "Phase 0 MVP — S4.T2 收尾"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: "2026-04-19"
deprecated_reason: "downgraded to plan — phase-specific implementation record, not an architectural contract"
enforcement:
  level: review-only
  trace: []
  review_checklist: "docs/plans/completed/2026-04-12-v2-s4t2-l4-l1-grpc-binding.md"
affected_modules:
  - "crates/grid-runtime/"
  - "lang/claude-code-runtime-python/"
  - "tools/eaasp-l4-orchestration/"
  - "proto/eaasp/runtime/v2/common.proto"
  - "scripts/verify-v2-mvp.sh"
  - "scripts/verify-v2-mvp.py"
related: [ADR-V2-001, ADR-V2-002, ADR-V2-003]
---

> **Note:** Downgraded from ADR-V2-004 on 2026-04-19 via `/adr:downgrade`.
> Original ADR was phase-specific (Phase 0 MVP S4.T2 "4b-lite" implementation)
> and better categorized as a plan/record. Original ADR frontmatter preserved above.

# ADR-V2-004 — L4→L1 Real gRPC Binding（延迟到 Phase 1）

**Status:** Accepted / Deferred
**Date:** 2026-04-12
**Phase:** Phase 0 MVP — S4.T2 收尾
**Related:** ADR-V2-001 (EmitEvent), ADR-V2-002 (Event Stream backend), ADR-V2-003 (Event clustering)
**Supersedes (partial):** 原 `Deferred.D27` "L4 session_orchestrator L1 Initialize/Send gRPC" 的模糊描述 → 本 ADR 精化为新 Deferred **D54**

---

## 背景

Phase 0 MVP 的核心命题是"跨 session 记忆累加" —— 用户在 session-1 通过"阈值校准助手" skill 把一次校准结果写入 L2 memory，session-2 换一个 L1 runtime 时应该通过 P3 MemoryRefs 看到 session-1 的记忆并引用它。

按 `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md §8` 的验收标准，这一命题对应 15 条 E2E 断言。S3.T4 完成时 L4 `eaasp-l4-orchestration` 选择把 L1 `Initialize` / `Send` 写成"在 `session_events` 里插入 `RUNTIME_INITIALIZE_STUBBED` / `RUNTIME_SEND_STUBBED` 事件"的**事件占位**形态，理由写在 `S3.T4` 的 Deferred.D27 里：

> "Phase 1 真 L1 gRPC 客户端接入 + 两个 runtime 的 Initialize fixture 就绪"

S4.T2 的任务是实现 `scripts/verify-v2-mvp.sh` + `.py` 让 15 条断言能端到端跑通。在设计阶段，scout agent 发现如果要真正做到"L4 通过 gRPC 调 L1 runtime，runtime 通过 LLM 工具调用写 L2"的完整链路，需要的附加工作远超 S4.T2 的时间窗：

1. **Python gRPC client in L4** — 从 `proto/eaasp/runtime/v2/runtime.proto` 生成 Python 存根，在 `tools/eaasp-l4-orchestration/` 内加入 `grpcio` 依赖和 `L1RuntimeClient` 抽象；
2. **Session lifecycle 贯通** — L4 在 `create_session` 真正调用 `Initialize(SessionPayload)` 并等待 `SessionHandle` 返回，L4 `send_message` 把用户消息转成 `Send` 请求的流式响应反流到 CLI；
3. **Scoped-hook runtime executor** — SKILL.md 的 `PreToolUse` / `PostToolUse` / `Stop` hook 目前只在 helper 层有 `substitute_scoped_hooks`（S4.T2 preconditions D49），runtime 侧没有执行路径，真 LLM session 无法 enforcement（D53）；
4. **真 MCP 接入** — L4 需要告诉 L1 runtime 连接哪些 MCP server（mock-scada + L2 memory-engine），对应新的 `ConnectMcp` RPC 和 L4 → L1 的 server config 透传；
5. **LLM 非确定性** — 真走 LLM 路径的 tool-call 序列无法在 CI gate 里可靠断言 "evidence anchor 写入 + memory file 写入" 这两条（assertion 8）；LLM 可能选择不调 `memory_write_anchor`，或调用参数不匹配。

总工作量保守估计 **3–5 个 RuFlo session**，涉及跨 Rust / Python / proto / L4 orchestrator / runtime harness 的大量改动。如果强行打包进 S4.T2，Phase 0 MVP 会长期处于"半完成"状态，"MVP 可以用"的最小承诺无法兑现。

## 决策

**S4.T2 实现为 "4b-lite" 模式 —— `verify-v2-mvp` 15 条断言在 "L4-stubbed mode" 下跑通**：

- **L1 runtime 真做**（不 stub）：`grid-runtime` 和 `claude-code-runtime-python` 的 `initialize` 方法**真正消费** `payload.policy_context` (P1 读取 + log) 和 `payload.memory_refs` (P3 构建 system 前缀 + 注入 `initial_history`)。这解决了 `Deferred.D1` / `Deferred.D2` —— 不再是"字段被丢弃"，而是"字段被读取并注入 LLM 上下文"。两个 runtime 的 certifier 验证也照常跑（assertion 15）。
- **L4 保持 stub**：`session_orchestrator.create_session` / `api.send_message` 仍然是 S3.T4 的事件占位形态，不引入 Python gRPC client。
- **Verify script 补齐 L4 空洞**：assertion 8 / 11 / 12 / 13 这四条原本依赖"L1 LLM 驱动 tool call 写 L2 / L3"的断言，改由 `verify-v2-mvp.py` **直接 POST** 到 L2 / L3 REST 端点来模拟 runtime **应该**写入什么。每条这样的断言在代码里打 `# L4-STUBBED: ...` 注释，明确标记"这条断言证明的是 infra wiring，不是 L1 LLM-driven tool-call loop"。
- **新 Deferred D54** 精化原 D27 范围 —— 把"真 L4→L1 gRPC binding" 作为独立的 Phase 1 首要任务，拆成 5 个明确子项（见下文）。
- **原 D27 保留 ⏳ 状态**，在 notes 里指向本 ADR + D54 作为继承者。

## 后果

### 正面

1. **Phase 0 MVP 真能跑起来** —— `make v2-mvp-e2e` 确实返回 exit 0，15/15 断言通过，不是纸面承诺。
2. **D1 / D2 正式关闭** —— 从 2026-04-11 S2→S3 过渡扫描开始就标记的"SessionPayload 字段接收但丢弃" landmine 终于被消除。两个 L1 runtime 的 `initialize` 都能证明 P1/P3 可达。
3. **后续 session 有清晰的起跑线** —— D54 把 D27 的模糊描述拆成 5 个子项，任何新 session 看到 D54 都能精确知道 Phase 1 首个任务的 shape。
4. **Verify script 的断言边界诚实** —— 每条 L4-stubbed 断言在代码注释里都写清楚它实际证明的是哪一层，不存在"看起来像 L1 其实是 L2 旁路"的欺骗。

### 负面

1. **"跨 session 记忆"命题部分证明** —— assertion 11 确实验证了 session-2 的 `SessionPayload.memory_refs` 在 L4 handshake 后非空且包含 session-1 写入的 `memory_id`，这是**真实的 cross-session 记忆连续性**。但后续"runtime 拿到这些 refs 后是否真正影响 LLM 输出"这一步没有 runtime 侧行为测试 —— 只有 D2 注入路径的单元/集成测试证明 `initial_history` 里会出现 system 前缀。完整行为证明需要 D54 落地后、再加一条"runtime 输出引用了 memory_id" 的集成断言。
2. **"真 tool call 写 L2"命题被旁路** —— assertion 8 / 12 由 verify script 直接 POST 到 L2 来满足。这意味着"当 LLM 不调 `memory_write_anchor` 时 skill 失败"这个 Phase 0 之外的失败模式在 MVP 里无法测到。
3. **`tools/mock-scada/` 在 MVP 里未被真实调用** —— Phase 0 preconditions D47 下的 mock-scada MCP stdio server 已经建好且有 15 个单元测试，但 S4.T2 的 verify script 不启动它（因为 runtime 没有 `connect_mcp` 路径被触发）。它等 D54 完成后才会真的被 L1 runtime 连上。
4. **D49 的 runtime 侧 wiring 仍悬空** —— 原 S4.T1 Deferred.D53 记录了 "scoped-hook executor 在 runtime 侧未实现"，4b-lite 维持这一现状。verify script 也不执行 hook。

## 替代方案

设计阶段评估了三个方案，在 2026-04-12 session 里与用户确认：

### 方案 A — "4b-full"（全部真做）

完成 D1 + D2 + D27 + scoped-hook executor + 真 MCP 接入 + 真 LLM 驱动的 tool call。**拒绝理由**：工程量 3–5 个 session，LLM 非确定性使 CI gate 不可靠，用"MVP 是否 DONE"阻塞一个开放工期的集成任务会让 Phase 0 无限期拖延。

### 方案 C — "4c-mocked"（全部 bypass L1）

Verify script 完全不启动 L1 runtime，只测 L2/L3/L4/cli-v2 链路。**拒绝理由**：MVP 的核心命题是"两个 L1 runtime 都能见到 P3 MemoryRefs"。bypass 掉 L1 之后，这个命题没有任何 L1 侧证据 —— 那就不是 EAASP MVP，只是 L2-L3-L4 基础设施 smoke test。D1 / D2 也不会关闭。

### 方案 B-lite（本 ADR 的决定）

D1 + D2 真做；L4 维持 stub；verify script 在 L4 空洞处直接打 L2/L3 REST 调用。**选中理由**：兼顾了"MVP 能跑"和"D1/D2 真正关闭"，同时为 D54 的 Phase 1 精确拆分奠定基础。

## 实施约束

S4.T2 的所有改动 **只允许**以下文件被修改：

- `crates/grid-runtime/src/harness.rs` — D1/D2 wiring（∈ `initialize` method）
- `crates/grid-runtime/tests/harness_payload_integration.rs` — 新增 3 个 integration tests
- `lang/claude-code-runtime-python/src/claude_code_runtime/session.py` — `Session` dataclass 扩展
- `lang/claude-code-runtime-python/src/claude_code_runtime/service.py` — `Initialize` / `Send` wiring
- `lang/claude-code-runtime-python/tests/test_session.py` / `test_service.py` — 新增 2 个测试
- `scripts/verify-v2-mvp.sh` / `scripts/verify-v2-mvp.py` — 新建
- `scripts/assets/threshold-calibration-skill.md` / `mvp-managed-settings.json` — 新建
- `Makefile` — G1/G2/G3 + `v2-mvp-e2e` wiring
- `docs/design/EAASP/adrs/ADR-V2-004-*.md` — 本文件
- `docs/plans/2026-04-11-v2-mvp-phase0-plan.md` — Deferred 表格更新（D1/D2 → ✅，新增 D54-D61）
- `docs/plans/.checkpoint.json`

**不允许**：L4 Python gRPC client、scoped-hook executor runtime 实现、`connect_mcp` 流程、proto 变更。

## Phase 1 接替任务 —— 新 Deferred D54

原 `Deferred.D27` 的精化。D54 的 5 个子项（执行顺序可并行）：

| 子项 | 内容 | 前置 |
|---|---|---|
| D54.a | proto → Python 生成 `runtime_pb2_grpc` + `common_pb2` 到 `tools/eaasp-l4-orchestration/src/eaasp_l4_orchestration/_proto/` | 需在 L4 `pyproject.toml` 加 `grpcio` 依赖 |
| D54.b | 在 `tools/eaasp-l4-orchestration/` 新建 `l1_client.py` — `L1RuntimeClient` 抽象，封装 `Initialize(SessionPayload) → SessionHandle` 和 `Send(UserMessage) → AsyncIterator[ResponseChunk]` | D54.a 完成 |
| D54.c | `session_orchestrator.create_session` 把 `RUNTIME_INITIALIZE_STUBBED` 事件替换为：实际调用 `l1_client.initialize(payload)`，成功后写 `RUNTIME_INITIALIZED` 事件（带真 runtime 响应），失败时写 `RUNTIME_INITIALIZE_FAILED` 并把 session 转成 `failed` 状态 | D54.b + D40 (sessions state machine) |
| D54.d | `api.send_message` 把占位响应替换为流式反流 L1 `Send` 的 `ResponseChunk` | D54.c |
| D54.e | 新集成测试：`tests/integration/test_l4_l1_gprc.py` 用真 L1 runtime（grid-runtime 或 claude-code-runtime）跑一次完整 `create_session → send_message → events` 并断言 `RUNTIME_INITIALIZED` 带非空响应 | D54.d + 两个 runtime 容器化 fixture |

预计 D54 全部完成后，`verify-v2-mvp.py` 可以去掉所有 `# L4-STUBBED` 注释，assertion 8 / 12 改成读 runtime 实际写入的 L2 anchors 而不是直接 POST。

## 参考

- `/tmp/s4t2-blueprint.md` — S4.T2 4b-lite 的完整 scout 蓝图
- `docs/plans/2026-04-11-v2-mvp-phase0-plan.md` Deferred D1 / D2 / D27 / D47 / D49 / D50 / D52 / D53
- `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` §8 — 15 条 E2E 断言
- `proto/eaasp/runtime/v2/common.proto` — SessionPayload 结构
- `crates/grid-runtime/src/harness.rs` — D1/D2 实施位置
- `lang/claude-code-runtime-python/src/claude_code_runtime/service.py::Initialize` — D2-py 实施位置
- `tools/eaasp-l4-orchestration/src/eaasp_l4_orchestration/session_orchestrator.py` — D54 的主要改动位点
