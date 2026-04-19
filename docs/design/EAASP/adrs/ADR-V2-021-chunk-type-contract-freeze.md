---
id: ADR-V2-021
title: "SendResponse.chunk_type 契约冻结（统一枚举）"
type: contract
status: Proposed
date: 2026-04-19
phase: "Phase 3 收尾 / Phase 3.5 契约强化"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace:
    - "tests/contract/cases/test_chunk_type_contract.py"
    - ".github/workflows/phase3-contract.yml"
  review_checklist: null
affected_modules:
  - "proto/eaasp/runtime/v2/common.proto"
  - "proto/eaasp/runtime/v2/runtime.proto"
  - "crates/grid-runtime/"
  - "crates/eaasp-claw-code-runtime/"
  - "crates/eaasp-goose-runtime/"
  - "lang/claude-code-runtime-python/"
  - "lang/nanobot-runtime-python/"
  - "lang/pydantic-ai-runtime-python/"
  - "lang/ccb-runtime-ts/"
  - "tools/eaasp-cli-v2/"
  - "tools/eaasp-l4-orchestration/"
related: [ADR-V2-017, ADR-V2-020]
---

# ADR-V2-021 — SendResponse.chunk_type 契约冻结（统一枚举）

**Status:** Proposed
**Date:** 2026-04-19
**Phase:** Phase 3 收尾 / Phase 3.5 契约强化
**Author:** Jiangwen Su
**Related:** ADR-V2-017（L1 生态策略），ADR-V2-020（工具命名空间契约），Phase 3 plan

---

## Context / 背景

Phase 3 L1 runtime 扩展到 7 个（grid / claude-code / goose / nanobot / pydantic-ai / claw-code / ccb），但人工 E2E 验证 nanobot 时暴露一个隐性契约漂移：**7 个 runtime 的 `SendResponse.chunk_type` 取值互不相同**，L4 / CLI 消费端只认 grid 的那套，导致 nanobot session 其实跑完了完整 workflow（5 次 tool call、最终 text 回复、anchor 写入），但 CLI 显示 `11 events, 0 chars`，看起来像"没响应"。

### 漂移实况

`proto/eaasp/runtime/v2/runtime.proto:102` 把 `chunk_type` 定义成自由 `string`，只在注释里写了"标准集合"：

```proto
string chunk_type = 1;  // "text_delta" | "thinking" | "tool_start" | "tool_result" | "done" | "error"
```

各 runtime 实际发出的取值：

| runtime | text 流 | tool 开始 | tool 结果 | done | error | thinking |
|---|---|---|---|---|---|---|
| grid | `text_delta` ✅ | `tool_start` ✅ | `tool_result` ✅ | `done` ✅ | `error` ✅ | `thinking` ✅ |
| claude-code | `text_delta` ✅ | `tool_start` ✅ | `tool_result` ✅ | `done` ✅ | `error` ✅ | — |
| nanobot | `text` ❌ | `tool_call_start` ❌ | `tool_result` ✅ | `done` ✅ | `error` ✅ | — |
| pydantic-ai | `text` ❌ | `tool_call` ❌ | `tool_result` ✅ | `done` ✅ | `error` ✅ | — |
| goose | `chunk` ❌ | `tool_call` ❌ | — | `done` ✅ | — | — |
| claw-code | `chunk` ❌ | `tool_call` ❌ | — | `done` ✅ | — | — |
| ccb | `chunk` ❌ | — | — | `done` ✅ | `error` ✅ | — |

### 为什么没被 Phase 3 契约测试抓住

Phase 3 `tests/contract/cases/` 只断言 `SendResponse` 字段**存在**（`chunk_type` 非空字符串），没断言取值属于合法集合。`make v2-phase3-e2e` 的 E2E 也不消费 chunk_type 语义（只看 `events` 列表长度），所以 112 个 pytest 全 PASS 仍放行了漂移。

### 根因

字段类型 `string` + 注释约束 ≠ 可执行契约。没有 proto enum、没有契约测试白名单，就是"约定俗成"，必然被后续 runtime 自由解读。

---

## Decision / 决定

**把 `SendResponse.chunk_type` 从自由 `string` 升级为 proto enum `ChunkType`，一次性切换所有 7 个 runtime，不保留兼容层**。

### 1. proto 层硬约束

在 `proto/eaasp/runtime/v2/common.proto` 新增 enum：

```proto
// ChunkType 定义 SendResponse 流中每个 chunk 的语义。
// 关闭枚举 — 新增取值必须先改 proto + 同步扩展契约测试白名单，
// 不允许在 runtime 层自行发送未定义值。
enum ChunkType {
  CHUNK_TYPE_UNSPECIFIED = 0;   // 禁止发送；proto 默认值占位
  CHUNK_TYPE_TEXT_DELTA  = 1;   // 流式 assistant 文本增量
  CHUNK_TYPE_THINKING    = 2;   // 扩展思考流（可选，runtime 不支持时不发）
  CHUNK_TYPE_TOOL_START  = 3;   // tool call 开始，content 为 JSON 序列化的参数
  CHUNK_TYPE_TOOL_RESULT = 4;   // tool 返回，content 为 JSON 序列化的结果
  CHUNK_TYPE_DONE        = 5;   // 本轮结束，content 可为最终汇总文本
  CHUNK_TYPE_ERROR       = 6;   // 运行期错误，error 字段必填
}
```

`runtime.proto` 的 `SendResponse` 对应改写：

```proto
message SendResponse {
  ChunkType chunk_type = 1;  // 关闭枚举，见 common.proto
  string content = 2;
  string tool_name = 3;
  string tool_id = 4;
  bool is_error = 5;
  RuntimeError error = 6;
}
```

### 2. JSON / SSE wire 值

gRPC 默认把 enum 序列化为 UPPER_SNAKE（`TEXT_DELTA`），但 L4 SSE 已在用 lowercase（claude-code / grid 习惯）。为最小化消费端改动并保持 SSE 可读性，**采用 `lower_snake_case` 作为 JSON wire 值**：

| enum | wire value |
|---|---|
| `CHUNK_TYPE_TEXT_DELTA` | `"text_delta"` |
| `CHUNK_TYPE_THINKING` | `"thinking"` |
| `CHUNK_TYPE_TOOL_START` | `"tool_start"` |
| `CHUNK_TYPE_TOOL_RESULT` | `"tool_result"` |
| `CHUNK_TYPE_DONE` | `"done"` |
| `CHUNK_TYPE_ERROR` | `"error"` |

实现方式：
- gRPC 内部走 enum int32（快速）
- L4 SSE 层写一个 `ChunkType → str` 函数（去前缀 + 小写），单点映射，不在 runtime 端重复
- CLI 直接消费 SSE 的 lowercase 字符串

### 3. 零兼容切换

grid-sandbox 当前**没有生产用户**（ADR-V2-017 §4 确认），所以不做双写、不做同义词兼容、不做过渡期。一次 PR 改完：

- proto 源 + stub 全部重生成
- 6 个 runtime 同步改（grid 已合规，改动 = 0）
- CLI + L4 消费端从 `"text_delta"` 白名单改成 `ChunkType` 枚举（或其 wire 值）
- 契约测试升级

### 4. 契约测试硬门

`tests/contract/cases/test_chunk_type_contract.py` 新增必跑用例，每个 runtime 起 session → 发 "hello" 消息 → 遍历全部 SendResponse → 断言：

```python
ALLOWED = {"text_delta", "thinking", "tool_start", "tool_result", "done", "error"}
for chunk in responses:
    assert chunk.chunk_type in ALLOWED, f"runtime {rt} emitted illegal chunk_type={chunk.chunk_type}"
    assert chunk.chunk_type != "", "empty chunk_type forbidden"
```

此用例进入 Phase 3 contract matrix（`.github/workflows/phase3-contract.yml`），每个 runtime PR 必过。新增 runtime 第一天就要满足。

### 5. ADR 约束条款

**禁止在 runtime 层自行扩展 chunk_type 取值或添加同义映射**。新增语义必须：
1. 先改 `common.proto` 新增 enum 值
2. 同步更新契约测试白名单
3. 再让 runtime 实现发送
4. 变更需单独 ADR 补充（例如 `ADR-V2-0XX — ChunkType 扩展 FOO`）

reviewer 在 PR 阶段按此条款卡合入。

---

## Consequences

### 正面

- **物理约束**：不合法取值在 proto 层直接编译失败（enum 未定义 int 无法序列化），不再靠 code review 把关
- **契约测试**：每个 runtime 第一天就过 chunk_type 白名单，新增 runtime 天然遵守
- **CLI / L4 零分支**：消费端直接 if/switch enum，不再写"同义词表"
- **跨语言一致**：proto 是唯一真相源，Rust / Python / TypeScript stub 自动一致

### 负面

- **6 runtime 代码改动**：nanobot / pydantic-ai / goose / claw-code / ccb 各改 5-10 行；claude-code 已合规；grid 已合规
- **proto 向下不兼容**：序列化的 `chunk_type` 从 string (TYPE_STRING) 变成 enum (TYPE_ENUM)，线上系统无法滚动升级（不适用，无生产用户）
- **契约测试新增**：需要起真实 runtime 实例跑（已有 matrix 基础设施，增量成本低）

### 中立

- SSE wire 值仍是 `lower_snake_case` 字符串，L4 SSE 消费端代码改动极小
- thinking 仍是可选 chunk type（runtime 不发也合法）

---

## Alternatives Considered

### A. 保持 `string` + 只加契约测试白名单

优点：不改 proto，不碰 stub
缺点：物理上仍可能发出非法值，只有运行期才失败，跨语言不一致
拒绝理由：不是"治本"。用户明确要求"统一契约能被遵守"，只有 proto enum 能从编译期拦截

### B. 让 CLI 做宽容消费（同义词表）

优点：改动最少
缺点：契约漂移被掩盖，未来每新增一个 runtime 都要更新 CLI 白名单；是反模式
拒绝理由：用户明确否决"每个都临时做一套"

### C. enum + UPPER_SNAKE wire

优点：gRPC 标准做法，Kubernetes / Envoy 等生态一致
缺点：L4 SSE 和 CLI 已用 lowercase，改成 UPPER 要动所有消费端显示逻辑；JSON 观感不如 lowercase 自然
拒绝理由：SSE / JSON 场景 lowercase snake_case 可读性更高，Google Cloud / Stripe API 的 enum over JSON 也普遍用 lowercase

---

## Implementation

见 `docs/plans/2026-04-19-v2-chunk-type-unification.md`。
