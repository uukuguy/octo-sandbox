# Plan — chunk_type 契约统一（ADR-V2-021 落地）

**Date:** 2026-04-19
**Status:** Proposed
**ADR:** ADR-V2-021
**Phase:** Phase 3 收尾 / Phase 3.5 契约强化
**Swarm topology:** hierarchical, max-agents=8, strategy=specialized

---

## Goal

把 `SendResponse.chunk_type` 从自由 string 升级为 proto enum，一次性消除 7 个 L1 runtime 的取值漂移，让"每个 runtime 遵守同一套契约"成为 CI 硬门。

**Done 定义**：
1. proto 改成 enum，全栈 stub 重新生成
2. 7 runtime 全部只发合法 chunk_type；非法值编译/运行期失败
3. CLI + L4 SSE 消费端基于枚举，不含 fallback / 同义词
4. 契约测试用例进 CI matrix，每 runtime 每次 push 必过
5. 人工 E2E 重跑 `eaasp session run -s threshold-calibration -r nanobot-runtime ...`，CLI 显示完整 text + 合理 event 计数
6. ADR-V2-021 状态改 Accepted + 归档 MEMORY.md

---

## Scope

### 改的

- `proto/eaasp/runtime/v2/common.proto` 新增 `ChunkType` enum
- `proto/eaasp/runtime/v2/runtime.proto` `SendResponse.chunk_type` 改用 enum
- 7 runtime stub 全部重生成（Rust / Python / TypeScript）
- 6 runtime 发送端改 chunk_type 取值（grid 和 claude-code 无改动，或只改字段类型）
- L4 SSE 序列化层：enum → lowercase snake_case 字符串（一处）
- CLI `cmd_session.py` 消费端（去掉 text_delta 单值白名单）
- L4 done summary `response_text` 累加逻辑（修 `response_text=""` bug）
- 契约测试 `tests/contract/cases/test_chunk_type_contract.py`（新增）
- Phase 3 CI workflow `.github/workflows/phase3-contract.yml`（纳入新用例）
- `MEMORY.md` 归档

### 不改的

- `content` / `tool_name` / `tool_id` / `is_error` / `error` 字段语义
- 现有事件流（USER_MESSAGE / RESPONSE_CHUNK 等事件层级不动）
- tool 命名空间（ADR-V2-020 独立）
- hook envelope（ADR-V2-006 独立）
- skill 合约

---

## Task Breakdown

### S0 — proto contract freeze（串行，先导）

**S0.T1** 改 proto + 重生成 stub（**1 coder**）

- 改 `proto/eaasp/runtime/v2/common.proto`：新增 `ChunkType` enum（见 ADR §1）
- 改 `proto/eaasp/runtime/v2/runtime.proto`：`SendResponse.chunk_type` 从 `string` → `ChunkType`
- 重新运行各 runtime 的 proto 生成命令：
  - Rust：`cargo build`（build.rs 自动）覆盖 `crates/*/src/pb.rs`
  - Python：`make claude-runtime-proto` 等 make target 覆盖 `lang/*/_proto/`
  - TypeScript：`lang/ccb-runtime-ts/` 的 proto build 脚本
- 验证：全栈 `cargo check --workspace` + `pytest --collect-only` + `bun run tsc --noEmit` 全过（enum 变更导致所有赋值编译失败是**预期的**，这一步只是让 stub 先重生成，不改 runtime 实现）

**DoD**：proto 改完，stub 重生成，**此时 cargo build 和 pytest 一定挂**（后续 S1 收尾前不修复）。

**Commit**: `proto(eaasp-v2): freeze chunk_type as enum (ADR-V2-021 S0)`

---

### S1 — 6 runtime 并行改发送端（**并行 6 coder**）

每个 coder 只动一个 runtime 的 SendResponse 赋值行，范围极小。

**S1.T1 grid-runtime**（Rust）
- 文件：`crates/grid-runtime/src/harness.rs` 行 108/122/474/481/488/495/502/511/524/545
- 改动：`"text_delta".into()` → `ChunkType::TextDelta as i32` 等
- 验证：`cargo test -p grid-runtime`

**S1.T2 eaasp-claw-code-runtime**（Rust）
- 文件：`crates/eaasp-claw-code-runtime/src/service.rs` 行 92/103/115/138
- 改动：`"chunk"` → `ChunkType::TextDelta`，`"tool_call"` → `ChunkType::ToolStart`，补 `tool_result` 与 `error` 分支
- 验证：`cargo test -p eaasp-claw-code-runtime`

**S1.T3 eaasp-goose-runtime**（Rust）
- 文件：`crates/eaasp-goose-runtime/src/service.rs` 行 102/111/122/153
- 改动：同 S1.T2
- 验证：`cargo test -p eaasp-goose-runtime`

**S1.T4 claude-code-runtime**（Python）
- 文件：`lang/claude-code-runtime-python/src/claude_code_runtime/sdk_wrapper.py`
- 改动：所有 `chunk_type="text_delta"` 等改成 `chunk_type=ChunkType.TEXT_DELTA`（Python enum int）
- 验证：`cd lang/claude-code-runtime-python && .venv/bin/pytest -xvs`

**S1.T5 nanobot-runtime**（Python）
- 文件：`lang/nanobot-runtime-python/src/nanobot_runtime/service.py` 行 115/120/127/135/140
- 改动：`"text"` → `ChunkType.TEXT_DELTA`，`"tool_call_start"` → `ChunkType.TOOL_START`
- 验证：`cd lang/nanobot-runtime-python && .venv/bin/pytest -xvs`

**S1.T6 pydantic-ai-runtime**（Python）
- 文件：`lang/pydantic-ai-runtime-python/src/pydantic_ai_runtime/service.py` 行 56/59/66/73/76
- 改动：`"text"` → `ChunkType.TEXT_DELTA`，`"tool_call"` → `ChunkType.TOOL_START`
- 验证：`cd lang/pydantic-ai-runtime-python && .venv/bin/pytest -xvs`

**S1.T7 ccb-runtime**（TypeScript）
- 文件：`lang/ccb-runtime-ts/src/service.ts` 行 56/67/75
- 改动：`"chunk"` → `ChunkType.TEXT_DELTA`（TS enum），补 `tool_start / tool_result / error` 分支
- 注意：`lang/ccb-runtime-ts/src/proto/types.ts:39` 的 `chunkType: string` 类型也要同步改
- 验证：`cd lang/ccb-runtime-ts && bun test`

**DoD**（每任务）：本 runtime 单测通过；`grep chunk_type src/` 只出现合法枚举取值。

**Commit**（每任务独立）: `runtime(<name>): align chunk_type to ChunkType enum (ADR-V2-021 S1.T<n>)`

---

### S2 — consumer 消费端（并行 2 coder）

**S2.T1 L4 SSE 序列化**（Python）
- 文件：`tools/eaasp-l4-orchestration/src/eaasp_l4_orchestration/session_orchestrator.py`（chunk_type 出现点）
- 工作：
  - 写 `ChunkType → lowercase snake_case str` 单点函数（查 proto `DESCRIPTOR` 拿名字，去 `CHUNK_TYPE_` 前缀转小写）
  - stream_message 路径：enum int → 函数映射 → SSE JSON
  - done summary 里的 `response_text` 累加：所有 `ChunkType.TEXT_DELTA` 的 content 拼接（修 `response_text=""` bug）
- 验证：`cd tools/eaasp-l4-orchestration && .venv/bin/pytest -xvs tests/test_api.py tests/test_session_orchestrator.py`

**S2.T2 CLI 消费端**（Python）
- 文件：`tools/eaasp-cli-v2/src/eaasp_cli_v2/cmd_session.py` 行 205-232 / 297-320
- 工作：
  - 删除 `chunk_type == "text_delta"` 单值分支，换成白名单 set `{"text_delta", "thinking", "tool_start", "tool_result", "done", "error"}`
  - 未知 chunk_type → 打印到 stderr 的 warning（防契约漂移再次发生）
  - done summary 里的 event 计数和 chars total 直接用 L4 发的 `response_text`（S2.T1 修完后就正确）
- 验证：`cd tools/eaasp-cli-v2 && .venv/bin/pytest -xvs`

**Commit**: `l4/cli: consume ChunkType enum wire values (ADR-V2-021 S2)`

---

### S3 — 契约测试硬门（1 coder + 1 tester）

**S3.T1 新增契约用例**
- 文件：`tests/contract/cases/test_chunk_type_contract.py`（新建）
- 内容：起每个 runtime 的 live session，发 "hello" → 遍历 SendResponse 流，断言：
  - `chunk_type != CHUNK_TYPE_UNSPECIFIED`
  - wire 值 ∈ `{"text_delta", "thinking", "tool_start", "tool_result", "done", "error"}`
  - 至少有一个 `DONE` chunk（终结信号必发）
- 参数化：`@pytest.mark.parametrize("runtime", [grid, claude-code, goose, nanobot, pydantic-ai, claw-code, ccb])`
- 验证：本地能跑通所有 runtime

**S3.T2 CI 纳入**
- 文件：`.github/workflows/phase3-contract.yml`
- 加一步：`python -m pytest tests/contract/cases/test_chunk_type_contract.py --runtime=<matrix-runtime>`
- 验证：push 一个临时违规 PR（nanobot 故意发非法 chunk_type）→ CI 必 FAIL；改回 → 必 PASS

**S3.T3 完整回归**
- 本地跑 `make v2-phase3-e2e` + `make v2-phase3-e2e-rust`，确认 112 pytest 仍全 PASS
- 人工跑 `scripts/phase3-runtime-verification.sh --auto`

**Commit**: `contract: add chunk_type whitelist gate for all runtimes (ADR-V2-021 S3)`

---

### S4 — 人工 E2E 验证（1 tester）

**S4.T1** 重跑原始失败用例：

```bash
make dev-eaasp  # 另起终端
SID=$(tools/eaasp-cli-v2/.venv/bin/eaasp session run \
  -s threshold-calibration -r nanobot-runtime \
  "校准 Transformer-001 的温度阈值")
```

**期望**：
- CLI 打印完整中文校准报告（不再是空）
- 末尾 `── N events, X chars total` 的 `X > 0` 且与实际输出长度相符
- tool call 日志行正常显示（`[tool_call: scada_read_snapshot]` 等）

**S4.T2** 对其他 6 runtime 做同样冒烟，每个至少能正确显示 text 流和 tool 调用。

**S4.T3** 归档 `phase3-verification-log.txt` 新增条目。

---

### S5 — ADR 终结 & memory

**S5.T1** ADR-V2-021 状态改 Accepted，追加 Implementation Record
**S5.T2** `MEMORY.md` 加一行 `[ADR-V2-021 chunk_type 统一](project_adr_v2_021_chunk_type.md)`
**S5.T3** `project_adr_v2_021_chunk_type.md` 写本轮实施记录（改动文件清单、commit hash、E2E 证据）

---

## Risk Register

| # | 风险 | 缓解 |
|---|------|------|
| R1 | proto enum 改动导致所有 runtime stub 重生成后大量编译失败（S0 结束 → S1 完成前） | 计划：S0 单独一个 commit，S1.T1-T7 并行改完才能整体 push，不走中间可编译状态 |
| R2 | gRPC enum 和 JSON wire 小写映射代码放错层 | 只在 L4 SSE 序列化层一个地方做，runtime 端永远用 enum int |
| R3 | 契约测试需要真实 runtime 实例 → CI 资源 | 已有 phase3-contract.yml matrix 基础设施；增量 pytest 单用例成本低 |
| R4 | L4 done summary `response_text=""` 是 bug 还是设计 | 从 SSE 观察确定是 bug（text chunk content 没累加到 response_text），S2.T1 修 |
| R5 | thinking chunk 某些 runtime 不发 → 契约测试是否容忍 | 契约测试只断言"发的值合法"，不断言"必须发 thinking"；THINKING 是可选 |

---

## Verification Checklist（在 S4 完成前逐条勾）

- [ ] `cargo check --workspace` 全 PASS
- [ ] 所有 Python runtime `.venv/bin/pytest` PASS
- [ ] `bun test`（ccb）PASS
- [ ] `pytest tests/contract/cases/` 包含新 chunk_type 用例全 PASS
- [ ] `make v2-phase3-e2e` 全 PASS（112 pytest）
- [ ] `make v2-phase3-e2e-rust` PASS
- [ ] 人工 E2E nanobot session 显示完整 text + 正确 event 计数
- [ ] 人工 E2E 其他 6 runtime smoke PASS
- [ ] `grep -rn '"text"\|"tool_call_start"\|"tool_call"\|"chunk"' crates/*/src lang/*/src` 无命中（白名单外值全部清除）
- [ ] ADR-V2-021 状态 → Accepted
- [ ] MEMORY.md 更新

---

## Rollout

单 PR 推 main，一次 merge。不做 feature flag，不做双写（用户明确否决兼容两套）。
