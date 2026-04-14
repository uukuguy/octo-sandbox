# S1.T1 — D87 修复实施计划：grid-engine agent loop 过早退出

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 修复 grid-engine `harness.rs:1169` 错误退出条件，让 agent loop 在 LLM 持续返回 tool_use 时继续推进多步工作流，且通过 grid-runtime + threshold-calibration 真实 E2E 验收。

**Architecture:** 单点修改 grid-engine 主循环判断逻辑——把 `stop_reason != ToolUse || tool_uses.is_empty()` 简化为 `tool_uses.is_empty()`，只看事实（有没有 tool_use），不看 LLM 的"叙事性"信号 (`stop_reason`)。这符合所有 LLM 厂商（Anthropic / OpenAI / Mistral）的标准 agentic loop 语义。修复必须通过 regression test、单 tool 不回归、grid-engine 全量测试、**真实 E2E 跑通 threshold-calibration**才算完成。

**Tech Stack:** Rust 1.75+（grid-engine） / Python 3.12+（grid-runtime + L4 + CLI） / SQLite（事件存储） / cargo test --test-threads=1 / Makefile EAASP 启动链

**ADR:** ADR-V2-016 (Proposed → Accepted on E2E pass)

---

## 任务依赖图

```
Task 1 (锁定 baseline)
   ↓
Task 2 (复现 D87 bug)  — regression test FAIL
   ↓
Task 3 (修复代码)
   ↓
Task 4 (regression test PASS)
   ↓
Task 5 (单 tool 不回归)
   ↓
Task 6 (grid-engine 全量测试无回归)
   ↓
Task 7 (E2E 准备：构建 + 启动栈)
   ↓
Task 8 (E2E 执行：grid-runtime + threshold-calibration)
   ↓
Task 9 (E2E 验收：≥4 个 PRE_TOOL_USE)
   ↓
Task 10 (升级 ADR-V2-016 Accepted + 更新 EVOLUTION_PATH + WORK_LOG)
   ↓
Task 11 (最终 commit + checkpoint)
```

如果 Task 8 或 Task 9 失败 → STOP，**不要**继续打补丁。回到根因分析（开 issue 或 brainstorming），ADR-V2-016 保持 Proposed。

---

## Task 1: 锁定 baseline 测试现状

**Files:** 无修改，仅观察

**Step 1: 确认当前 grid-engine 编译通过**

Run:
```bash
cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox
cargo check -p grid-engine 2>&1 | tail -10
```

Expected: `Finished` 输出，0 errors。

**Step 2: 确认 d87 测试文件存在且 ignore 状态**

Run:
```bash
grep -n "#\[ignore" crates/grid-engine/tests/d87_multi_step_workflow_regression.rs
```

Expected:
```
251:#[ignore = "D87 pending fix — grid-engine harness.rs:1169 terminates loop on text-only response"]
```

**Step 3: 确认 baseline 单 tool 测试当前 PASS**

Run:
```bash
cargo test -p grid-engine --test d87_multi_step_workflow_regression test_d87_single_tool_workflow_still_works -- --test-threads=1 --nocapture 2>&1 | tail -20
```

Expected: `test result: ok. 1 passed`

记录基线 git commit：
```bash
git rev-parse HEAD
```
存到 PLAN_CONTEXT 备查。

---

## Task 2: 复现 D87 bug（regression test FAIL）

**Files:**
- Modify: `crates/grid-engine/tests/d87_multi_step_workflow_regression.rs:250-251`

**Step 1: 去掉 `#[ignore]` 属性**

Read 文件 L249-252，确认上下文，然后用 Edit 工具：

old_string:
```rust
/// This test is `#[ignore]`'d so CI doesn't fail. Remove `#[ignore]` when
/// starting D87 fix work to lock in the regression.
#[tokio::test]
#[ignore = "D87 pending fix — grid-engine harness.rs:1169 terminates loop on text-only response"]
async fn test_d87_multi_step_workflow_no_early_exit() {
```

new_string:
```rust
/// D87 fix landed: this test now runs in CI and locks in the multi-step behavior.
#[tokio::test]
async fn test_d87_multi_step_workflow_no_early_exit() {
```

**Step 2: 跑测试，确认 FAIL 复现 bug**

Run:
```bash
cargo test -p grid-engine --test d87_multi_step_workflow_regression test_d87_multi_step_workflow_no_early_exit -- --test-threads=1 --nocapture 2>&1 | tail -40
```

Expected: **FAIL**，错误信息含：
```
D87: Expected 3 tool calls (read_data, search_history, write_result), got 1
```
（或 got 0 / got 2 — 任何 < 3 都说明 bug 复现）

**Step 3: 不 commit，直接进 Task 3 改代码**

理由：测试 FAIL + 代码未改 = 中间状态，避免引入 broken commit。

---

## Task 3: 修复 harness.rs:1169 退出条件

**Files:**
- Modify: `crates/grid-engine/src/agent/harness.rs:1169`

**Step 1: Read 上下文确认行号**

Run Read tool on `crates/grid-engine/src/agent/harness.rs` offset=1165 limit=10。

确认 L1169 的当前内容是：
```rust
if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
```

**Step 2: 单点修改判断条件**

用 Edit 工具：

old_string:
```rust
        // --- If no tool uses: check for continuation or finalize ---
        if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
```

new_string:
```rust
        // --- If no tool uses: check for continuation or finalize ---
        // ADR-V2-016: agentic loop 只看 tool_uses 事实，不看 stop_reason 叙事信号。
        // LLM 在中间轮次返回 EndTurn + tool_use 也应继续执行；
        // 真正的退出条件是「本轮无任何 tool_use」。
        if tool_uses.is_empty() {
```

**Step 3: 编译检查**

Run:
```bash
cargo check -p grid-engine 2>&1 | tail -10
```

Expected: `Finished` 输出，0 errors。

**Step 4: 不 commit，进 Task 4 验证修复**

---

## Task 4: 验证 D87 regression test PASS

**Files:** 无修改

**Step 1: 跑 regression test**

Run:
```bash
cargo test -p grid-engine --test d87_multi_step_workflow_regression test_d87_multi_step_workflow_no_early_exit -- --test-threads=1 --nocapture 2>&1 | tail -20
```

Expected: **PASS**，`test result: ok. 1 passed`

**Step 2: 检查 tool_starts 数组**

Expected 输出包含（如果 nocapture 显示）：
```
Tool calls seen: ["read_data", "search_history", "write_result"]
```
或类似 3 个 tool 全部被调用的证据。

**Step 3: 如果 FAIL：STOP 进入根因分析**

不要打第二个补丁。可能：
- harness 有其他相关分支也 short-circuit
- mock provider 行为不符预期
- masker / blob_store 干扰

→ 暂停，开 brainstorming session 重新分析，ADR-V2-016 保持 Proposed。

---

## Task 5: 验证单 tool workflow 不回归

**Files:** 无修改

**Step 1: 跑 baseline 测试**

Run:
```bash
cargo test -p grid-engine --test d87_multi_step_workflow_regression test_d87_single_tool_workflow_still_works -- --test-threads=1 --nocapture 2>&1 | tail -15
```

Expected: **PASS**

**Step 2: 如果 FAIL**

说明修复破坏了单 tool 场景。STOP，回到 Task 3 重新评估。
可能原因：单 tool 后 LLM 返回 final text，新逻辑下应仍正确退出（`tool_uses.is_empty()` true）。如失败说明读漏了什么，需要重新分析 harness 代码路径。

---

## Task 6: grid-engine 全量测试无回归

**Files:** 无修改

**Step 1: 跑 grid-engine 所有测试**

Run:
```bash
cargo test -p grid-engine -- --test-threads=1 2>&1 | tail -30
```

Expected: 所有 test PASS（包括之前 ignored 的 d87 现在也 run）。

记录 PASS / FAIL 数。基线对照 MEMORY.md "Test Execution" 条目：grid-engine 应继承 2476 全工作区 PASS 状态对应的子集。

**Step 2: 如果有 FAIL**

逐个看哪些测试与 D87 修复相关：
- 如果是依赖 `stop_reason != ToolUse` 短路退出的测试 → 可能是测试设计缺陷（耦合了 bug 行为），需要修测试不是修代码
- 如果是无关测试 → 修复引入了 side effect，回到 Task 3 重新评估

不在不知根因的情况下重写。

**Step 3: PASS 后才 commit 一次中间快照**

```bash
git add crates/grid-engine/src/agent/harness.rs \
        crates/grid-engine/tests/d87_multi_step_workflow_regression.rs
git commit -m "$(cat <<'EOF'
fix(grid-engine): D87 — agent loop only exits on empty tool_uses (ADR-V2-016)

Replace `stop_reason != ToolUse || tool_uses.is_empty()` with
`tool_uses.is_empty()` at harness.rs:1169. Agentic loop should observe the
fact (any tool_use blocks present?) rather than the LLM's narrative signal
(stop_reason).

Regression test: tests/d87_multi_step_workflow_regression.rs no longer
#[ignore]. Single-tool baseline still passes.

ADR-V2-016 status: Proposed (E2E verification pending).

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>
EOF
)"
```

---

## Task 7: E2E 准备 — 构建 + 启动 EAASP 栈

**Files:** 无修改

**Step 1: 检查 Makefile 提供的 EAASP 启动目标**

Run:
```bash
make help 2>&1 | grep -iE "eaasp|grid-runtime|claude-runtime|verify" | head -20
```

记下可用的命令（如 `make verify-dual-runtime` / `make l3-start` / `make l4-start` 等）。

**Step 2: 确认 ANTHROPIC_API_KEY 在环境变量**

Run:
```bash
env | grep -E "ANTHROPIC|OPENAI" | sed 's/=.*/=***REDACTED***/'
```

Expected: 看到 `ANTHROPIC_API_KEY=...` 或 `OPENAI_API_KEY=...`（OpenRouter 路由）。
若都没有 → STOP，请用户在 shell 设置 key 后重试（不要主动 cat .env 文件）。

**Step 3: Release 编译 grid-runtime**

Run:
```bash
cargo build --release -p grid-runtime 2>&1 | tail -10
```

Expected: `Finished release` 输出，0 errors。

**Step 4: 确认 threshold-calibration skill 存在**

Run:
```bash
ls examples/skills/threshold-calibration/SKILL.md
cat examples/skills/threshold-calibration/SKILL.md | head -30
```

确认 SKILL.md 含 `runtime_affinity` 和 `dependencies`（mock-scada + l2-memory）字段。

---

## Task 8: E2E 执行 — grid-runtime + threshold-calibration session

**Files:** 无修改

**Step 1: 用户确认启动方式**

**这是需要用户介入的 Runtime Verification 任务**（CLAUDE.md "Runtime Verification" 规则）。Claude 不直接运行长程后台进程，而是给出执行指令请用户运行。

向用户输出：

```
## Runtime Verification 任务：D87 修复 E2E 验收

### 前置
- ANTHROPIC_API_KEY 已配置
- grid-runtime release binary 已构建（Task 7 完成）

### 启动 EAASP 服务栈

请在三个独立终端运行（顺序无关，但都需启动）：

终端 1（L2 Memory）:
   make l2-memory-start

终端 2（L3 Governance + L4 Orchestration + Skill Registry）:
   make l3-start &
   make skill-registry-start &
   make l4-start &
   wait

终端 3（grid-runtime，端口 50051）:
   ./target/release/grid-runtime

确认所有服务监听端口（按 ADR-V2-015 / port rule，>=10000 except runtime gRPC 50051）：
   lsof -iTCP -sTCP:LISTEN | grep -E "18081|18083|18084|18085|50051"

应看到 5 个端口监听。

### 创建 session + 触发 workflow

终端 4（CLI 操作）:
   cd /Users/sujiangwen/sandbox/LLM/speechless.ai/SGAI/grid-sandbox
   eaasp session create --skill threshold-calibration --runtime grid-runtime
   # 输出形如：Session created: sess_xxxxxxxxx
   # 记下 SID，下面用

   eaasp session send <SID> "校准 Transformer-001 的温度阈值"
   # 等待返回（可能 30-60s）

### 收集事件流

   eaasp session events <SID> --format json > /tmp/d87-e2e-events.json
   cat /tmp/d87-e2e-events.json | jq '[.events[] | select(.event_type=="PRE_TOOL_USE")] | length'

请把以上命令的输出粘贴回来，特别是：
1. 服务启动是否成功（5 端口 LISTEN）
2. session create 返回的 SID
3. session send 是否报错
4. PRE_TOOL_USE 计数（关键验收数字）
5. /tmp/d87-e2e-events.json 文件路径（如有需要供进一步检查）
```

**Step 2: 等待用户反馈**

不要主动 sleep / poll。等用户粘贴回执行结果。

---

## Task 9: E2E 验收判定

**Files:** 无修改

**Step 1: 收到用户反馈后，分类判定**

| PRE_TOOL_USE 数 | workflow 完成度 | 判定 |
|----------------|---------------|------|
| **≥ 4** | 含 memory_write_anchor + memory_write_file | ✅ **PASS** — D87 修复成功 |
| 2-3 | 中途停下 | ⚠️ **PARTIAL** — D87 修复部分有效，但有其他遗留问题 |
| 1 | 仍只调一个 tool | ❌ **FAIL** — D87 修复无效 |
| 0 | 完全没调 tool | ❌ **FAIL** — 上游 MCP 连接或 skill 加载失败（不是 D87 范围） |

**Step 2: 如果 PASS（PRE_TOOL_USE ≥ 4）**

- 进 Task 10（升 Accepted + 文档收尾）
- 把事件计数和 SID 记入 WORK_LOG

**Step 3: 如果 PARTIAL 或 FAIL**

- **STOP，不打补丁**
- 把事件 JSON 用 jq 抽取关键信息：
  ```bash
  jq '[.events[] | {seq, event_type, payload}]' /tmp/d87-e2e-events.json | head -100
  ```
- 开新 brainstorming session 分析根因（ADR-V2-016 保持 Proposed）
- 可能根因：
  - LLM 模型本身行为问题（不调 tool）→ 换模型试
  - mock-scada / l2-memory MCP 连接失败 → 与 D87 无关
  - harness 还有其他短路分支 → 重新读代码

**Step 4: 不在 FAIL 状态下进 Task 10**

---

## Task 10: 升级 ADR-V2-016 + 文档收尾

**Files:**
- Modify: `docs/design/EAASP/adrs/ADR-V2-016-agent-loop-generic-principle.md`
- Modify: `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md`
- Modify: `docs/main/WORK_LOG.md`
- Modify: `docs/plans/.checkpoint.json`

**Step 1: ADR-V2-016 Status: Proposed → Accepted**

Edit `docs/design/EAASP/adrs/ADR-V2-016-agent-loop-generic-principle.md`:

old_string:
```
**Status:** Proposed（E2E 验收通过后升 Accepted）
```

new_string:
```
**Status:** Accepted (2026-04-14)
```

并在文末 References 新增 E2E 验收记录段：

```markdown
## E2E Verification Record / E2E 验收记录

- **Date**: 2026-04-14
- **Setup**: grid-runtime (release) + threshold-calibration skill + mock-scada MCP + l2-memory MCP
- **Result**: PRE_TOOL_USE count = <N> (≥4 required), workflow 走完含 memory_write_anchor / memory_write_file
- **Session ID**: sess_<填入>
- **Event log**: 归档（如有）
- **Conclusion**: ADR-V2-016 fix verified end-to-end, status Accepted
```

**Step 2: EVOLUTION_PATH ADR 注册表更新**

Edit `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md`:

把 ADR-V2-016 行的 Status 改为 **Accepted (2026-04-14)**。

**Step 3: WORK_LOG 新增 Phase 2 S1.T1 完成条目**

Edit `docs/main/WORK_LOG.md`：在文件**最上方**插入新段：

```markdown
## 2026-04-14 — Phase 2 S1.T1 D87 修复完成

### 会话概要

修复 grid-engine agent loop 过早退出 bug（D87，CRITICAL）。
按 ADR-V2-016 通用 agentic loop 原则，把 `harness.rs:1169` 退出条件从
`stop_reason != ToolUse || tool_uses.is_empty()` 简化为 `tool_uses.is_empty()`。
单点修改 + regression test 锁定 + 真实 E2E 验收通过。

### 技术变更

| 文件 | 改动 |
|------|------|
| `crates/grid-engine/src/agent/harness.rs:1169` | 退出条件简化为 `tool_uses.is_empty()` |
| `crates/grid-engine/tests/d87_multi_step_workflow_regression.rs` | 去 `#[ignore]`，CI 永久锁定 |
| `docs/design/EAASP/adrs/ADR-V2-016-*.md` | Proposed → Accepted |

### 测试结果

- regression test (`test_d87_multi_step_workflow_no_early_exit`): PASS（修复前 FAIL）
- baseline (`test_d87_single_tool_workflow_still_works`): PASS（无回归）
- grid-engine 全量: <数字> PASS / 0 FAIL

### E2E 验证

- grid-runtime + threshold-calibration session sess_<填入>
- PRE_TOOL_USE 数: <N>（≥4 通过）
- workflow 走完: ✅ 含 memory_write_anchor 调用

### Phase 2 进度

- S0 done (ADR-V2-015 + ADR-V2-016)
- S1: 1/5 done（D87），剩余 D88 / D86 / D83 / D85
- S2 可与 S1 并行启动
```

**Step 4: checkpoint.json 更新**

Edit `docs/plans/.checkpoint.json`：

`completed_tasks` 加入 `"S1.T1 (D87 fix verified E2E)"`，`current_task` 改为剩余 S1 任务（如 `"S1.T2 (D88 hermes stdio MCP)"`）。

`prerequisite_adrs.ADR-V2-016` 改为 `"Accepted 2026-04-14 — E2E 验证通过"`.

---

## Task 11: 最终 commit 和 phase checkpoint

**Files:** 无修改

**Step 1: Git status 检查**

Run:
```bash
git status -sb
```

应看到 4 个 modified 文档（ADR / EVOLUTION_PATH / WORK_LOG / checkpoint）+ 可能的 brainstorming 设计文档。

**Step 2: 单次 commit 收口**

```bash
git add docs/design/EAASP/adrs/ADR-V2-015-l2-memory-semantic-retrieval.md \
        docs/design/EAASP/adrs/ADR-V2-016-agent-loop-generic-principle.md \
        docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md \
        docs/plans/2026-04-14-phase2-s0-brainstorming-design.md \
        docs/plans/2026-04-14-s1t1-d87-fix-implementation.md \
        docs/plans/.checkpoint.json \
        docs/main/WORK_LOG.md \
        docs/dev/.phase_stack.json

git commit -m "$(cat <<'EOF'
docs(eaasp): Phase 2 S0+S1.T1 done — ADR-V2-015 Accepted, ADR-V2-016 Accepted (E2E verified)

S0 (ADR decisions):
- ADR-V2-015: L2 Memory semantic retrieval (HNSW in-process + 4-layer abstraction
  + pgvector migration triggers T1/T2/T3) — Accepted
- ADR-V2-016: Agent loop generic principle (only check tool_uses.is_empty(),
  ignore stop_reason narrative signal) — Accepted after E2E verification

S1.T1 (D87 fix):
- harness.rs:1169 simplified, regression test live (no #[ignore])
- grid-runtime + threshold-calibration session: <N> PRE_TOOL_USE, workflow完成

Phase 2 progress: S0 done, S1 1/5 done. S2 can start in parallel.

Generated-By: Claude (claude-sonnet-4-6) via Claude Code CLI
Co-Authored-By: claude-flow <ruv@ruv.net>
EOF
)"
```

**Step 3: 跑 /checkpoint-progress 固化**

向用户提示：

```
S1.T1 完成。建议运行 /dev-phase-manager:checkpoint-progress 固化 Phase 2 进度，
然后选择并行启动 S1.T2 (D88) 还是 S2.T1 (Memory 增强)，或两者并行。
```

---

## 失败回退路径

如果任意 Task 失败：

| 失败点 | 回退动作 |
|-------|---------|
| Task 4 (regression FAIL) | 不打第二个补丁；`git checkout` 还原 harness.rs，开 brainstorming |
| Task 5 (单 tool 回归) | 同上，root cause 重新分析 |
| Task 6 (其他测试 FAIL) | 评估是否测试耦合了 bug 行为（修测试）vs 修复有 side effect（修代码） |
| Task 8 (启动失败) | 用户反馈端口冲突 / 配置缺失 → 调整启动脚本，不在 D87 范围 |
| Task 9 (E2E PARTIAL/FAIL) | ADR-V2-016 保持 Proposed，开新 brainstorming，不补丁 |

**不要在不理解失败原因时尝试"再改一下试试"**。每次失败都是信息——root cause 不清就 STOP。

---

## 验收总清单

- [ ] Task 1: baseline 编译 + 单 tool 测试 PASS
- [ ] Task 2: regression test FAIL 复现 D87 bug
- [ ] Task 3: harness.rs:1169 单点修改 + cargo check 通过
- [ ] Task 4: regression test PASS（D87 修复证明）
- [ ] Task 5: 单 tool baseline 仍 PASS（无回归）
- [ ] Task 6: grid-engine 全量测试无回归 + 中间 commit
- [ ] Task 7: release binary 构建 + 环境变量确认
- [ ] Task 8: 用户反馈 EAASP 栈启动 + session 创建
- [ ] Task 9: PRE_TOOL_USE ≥ 4 + workflow 走完
- [ ] Task 10: ADR Accepted + EVOLUTION_PATH + WORK_LOG + checkpoint 更新
- [ ] Task 11: 单次 commit 收口

---

## 关于第三方 skill 引用

- @superpowers:executing-plans — 执行此计划时使用
- @superpowers:systematic-debugging — Task 4/5/6/9 失败时使用
- 不依赖其他 skill
