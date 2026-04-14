# ADR-V2-016 — Agent Loop 通用性原则

**Status:** Proposed（E2E 验收通过后升 Accepted）
**Date:** 2026-04-14
**Phase:** Phase 2 — Memory and Evidence
**Author:** Claude + 用户 brainstorming 决策
**Related:** D87 regression test (`crates/grid-engine/tests/d87_multi_step_workflow_regression.rs`)

---

## Context / 背景

Phase 1 E2E 验证 (2026-04-14) 暴露了 **D87 (CRITICAL)**：grid-runtime 在 threshold-calibration skill 下只执行 step 1 (`scada_read_snapshot`) 后停止，跳过 step 2-6。
对比 claude-code-runtime 自主完成全部 6 步。

**根因定位** (EVOLUTION_PATH L298)：

```rust
// crates/grid-engine/src/agent/harness.rs:1169
if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
    // finalize + return  ← 过于激进
}
```

LLM 返回任意非 `ToolUse` 的 `stop_reason` 就退出 loop。

### 讨论过程中被否决的方向

在 brainstorming 过程中一度提出以下方向，**均被用户否决**：

- ❌ skill frontmatter 加 `completion_marker`（"任务硬编码到架构"）
- ❌ skill frontmatter 加 `min_tool_calls`（"违反 agentic 精神"）
- ❌ runtime 自动注入 "Continue" user message（"为特定任务硬加动作"）
- ❌ system prompt 加"不要中途问用户"指令（"假设性太强"）

**用户原则**：**agentic loop 是通用机制，不能为特定任务硬编码动作**。

## Decision / 决策

### 核心原则（两条规则）

1. **LLM 响应里有 tool_use** → 执行工具 → tool_result 追加到上下文 → 继续
2. **LLM 响应里无 tool_use** → 循环结束

**不考虑 stop_reason**。`stop_reason` 是 LLM 的"叙事性"信号（"我这轮说完了"），不是 loop 控制信号。loop 控制**只看 tool_uses 事实**。

### 具体修复

`crates/grid-engine/src/agent/harness.rs:1169`：

```rust
// 错（当前）
if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
    // finalize + return
}

// 对（修复方向）
if tool_uses.is_empty() {
    // finalize + return
}
```

### 与现有机制的关系

grid-engine 已有三层防死锁机制（核实有效，**不改动**）：

| 机制 | 位置 | 职责 |
|------|------|------|
| **LoopGuard** | `loop_guard.rs` + `harness.rs:1447` | 防同 tool 同 args 重复调用 |
| **StuckDetector / SelfRepair** | `self_repair.rs` + `harness.rs:2074` | 防连续失败 / 无进展 |
| **max_rounds 硬上限** | `for round in 0..max_rounds` (L420) | 循环轮次熔断 |

**D87 与防死锁无关**——三个 guard 都在且有效。D87 是**正常退出条件错误**，不是失控。

### 不做的事（明确范围）

- ❌ 不改 skill frontmatter schema
- ❌ 不改 system prompt 模板
- ❌ 不加 completion_marker / min_tool_calls / completion_policy
- ❌ 不注入 "Continue" user message
- ❌ 不改 max_iterations 默认值（50 保持）
- ❌ 不改三层 guard 机制

**仅修一行判断逻辑**（加测试覆盖）。

### 验收标准（E2E 必须通过才升 Accepted）

本 ADR 采用 **Proposed**，修复完成后执行以下验收：

1. **Regression test**：`d87_multi_step_workflow_regression.rs` 去 `#[ignore]` → `cargo test -p grid-engine d87` PASS
2. **单 tool workflow 不回归**：`test_d87_single_tool_workflow_still_works` 仍 PASS
3. **真实 E2E**：
   - grid-runtime 跑 threshold-calibration skill
   - 事件流 ≥ 4 个 PRE_TOOL_USE
   - Skill workflow 走完（看到 memory_write_anchor / memory_write_file 调用）
4. **grid-engine 全量测试**：`cargo test -p grid-engine --test-threads=1` 无回归

**任何一项失败 → ADR 保持 Proposed + 打开新的根因分析**（不 silent 调整方案）。

验收通过后更新 Status 为 **Accepted** 并在 EVOLUTION_PATH 注册表标注。

## Consequences / 后果

### Positive

- ✅ **通用性**——符合所有厂商（Anthropic / OpenAI / Mistral）的标准 agentic loop 语义
- ✅ **最小改动**——单点修复，不污染 skill / prompt / config schema
- ✅ **可测试**——regression test 已锁定，修复成功条件明确
- ✅ **与三层 guard 正交**——不改动防死锁机制，分工清晰

### Negative

- ⚠️ **max_iterations 更易触顶**——原本 L1169 过早退出掩盖了部分 LLM 冗长行为，修复后可能更常见触发 max_rounds 熔断
  - **缓解**：50 轮对正常多步 skill 绰绰有余；超 50 轮的 skill 应显式覆盖 `max_iterations`

### Risks

- 🚨 **LLM 可能陷入新的失败模式**（比如每轮只调 1 个 tool 无意义重复）
  - **缓解**：LoopGuard 已覆盖"同 tool 同 args 重复"
- 🚨 **修复后 E2E 仍失败**（说明根因不止 L1169）
  - **缓解**：ADR 保持 Proposed，强制打开新根因分析，不 silent 补丁

## Affected Modules / 影响范围

| Module | Impact |
|--------|--------|
| `crates/grid-engine/src/agent/harness.rs:1169` | 修改：退出条件简化为 `tool_uses.is_empty()` |
| `crates/grid-engine/tests/d87_multi_step_workflow_regression.rs` | 修改：去掉 `#[ignore]` |
| `examples/skills/threshold-calibration/` | **不改**（通用修复不依赖 skill 改动） |
| `tools/eaasp-l4-orchestration/` | **不改**（事件流格式不变） |

## Alternatives Considered / 候选方案

### Option A: skill-specific 完成 marker（已否决）

要求 skill 在 frontmatter 声明 `completion_marker: "<task_complete/>"`，runtime 看到 marker 才退出。

**否决理由**：把任务语义硬编码到架构，违反通用性。

### Option B: runtime 注入 "Continue" user message（已否决）

LLM 返回 text-only 时 runtime 自动注入 `User: Continue with the next step.`。

**否决理由**：为"继续执行"硬加专门动作，破坏 agentic loop 的通用性。

### Option C: system prompt 加"不要中途问用户"指令（已否决）

在 system prompt 模板里加"multi-step skill 执行时不要中途询问"。

**否决理由**：假设所有 skill 都是"不应中途问"，不成立。某些 skill 本来就该中途停下征求人类确认。

### Option D: 只修 `tool_uses.is_empty()` 退出条件（选用）

通用 agentic loop 标准行为：有 tool 就执行，没 tool 就退出。`stop_reason` 不参与决策。

**选用理由**：
- 通用性最强，符合所有 LLM 厂商语义
- 最小改动，可测性最强
- 不引入新概念到 skill / prompt / config

## References / 参考

- D87 详细分析：EVOLUTION_PATH §Deferred L298
- Regression test：`crates/grid-engine/tests/d87_multi_step_workflow_regression.rs`
- Brainstorming 设计文档：`docs/plans/2026-04-14-phase2-s0-brainstorming-design.md`
- Phase 1 WORK_LOG：`docs/main/WORK_LOG.md` (2026-04-14 条目)
- 现有防死锁机制代码：
  - `crates/grid-engine/src/agent/loop_guard.rs`
  - `crates/grid-engine/src/agent/self_repair.rs`
  - `crates/grid-engine/src/agent/harness.rs:420` (for loop)
