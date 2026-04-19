---
id: ADR-V2-018
title: "PreCompact Hook 协议 + 迭代式 Summary 复用 + 跨压缩 Token 预算"
type: contract
status: Accepted
date: 2026-04-15
phase: "Phase 2 — Memory and Evidence (S3.T1 实施依据)"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: contract-test
  trace: []
  review_checklist: null
affected_modules:
  - "proto/eaasp/runtime/v2/runtime.proto"
  - "proto/eaasp/hook.proto"
  - "crates/grid-engine/"
  - "lang/claude-code-runtime-python/"
related: [ADR-V2-006, ADR-V2-016, ADR-V2-017]
---

# ADR-V2-018 — PreCompact Hook 协议 + 迭代式 Summary 复用 + 跨压缩 Token 预算

**Status:** Accepted
**Date:** 2026-04-15
**Phase:** Phase 2 — Memory and Evidence (S3.T1 实施依据)
**Author:** Jiangwen Su (orchestrated by claude-flow swarm-1776204938811)
**Related:** ADR-V2-016 (agent loop 通用原则), ADR-V2-017 (L1 runtime 生态), ADR-V2-006 (hook envelope 契约 — 待 S3.T5 起草)

---

## Context / 背景

Phase 2 S3.T1 任务原计划"新建 `crates/grid-engine/src/agent/context_compressor.rs`"，但实际代码审计（scout blueprint，2026-04-15）发现：

1. `crates/grid-engine/src/context/compaction_pipeline.rs` 已存在 730 行 LLM-based 压缩管线（9 段 prompt、PTL 重试），**新建独立模块会重复实现**
2. `proto/eaasp/runtime/v2/runtime.proto` 已定义 `HookEventType::PRE_COMPACT = 8`，**但 `hook.proto` 的 `HookEvent.event` oneof 缺 `PreCompactHook` 分支**
3. `SessionSummaryStore` 已通过 `CompactionContext::session_summary_store` 字段挂入 pipeline，**但 `compact()` 既不读也不写**——迭代式 summary 复用是最大语义空洞
4. `harness.rs` 当前压缩触发**仅靠 `is_prompt_too_long(error)` 字符串匹配**，不消费 `error_classifier::FailoverReason::ContextOverflow`，且 `HookPoint::ContextDegraded` 在压缩**之后**才 fire
5. 跨压缩 token 预算（plan §S3.T1 引用 claude-code `taskBudgetRemaining`）在 grid-engine **完全缺失**

同时，Phase 2.5 即将启动 `goose-runtime`（ADR-V2-017 轨道 3 对比首选）。如果 PreCompact 的 hook payload schema 不在 `hook.proto` 里定义干净，goose 实现时会抄到不一致的 schema，破坏 L1 契约统一性。

---

## Decision / 决策

### D1. PreCompact hook 协议 — 在 `hook.proto` 加 `PreCompactHook` oneof 字段

```protobuf
message HookEvent {
  // ... 现有字段 ...
  oneof event {
    PreToolCallHook pre_tool_call = 10;
    PostToolResultHook post_tool_result = 11;
    StopHook stop = 12;
    SessionStartHook session_start = 13;
    SessionEndHook session_end = 14;
    PrePolicyDeployHook pre_policy_deploy = 15;
    PreApprovalHook pre_approval = 16;
    EventReceivedHook event_received = 17;
    PreCompactHook pre_compact = 18;     // ⭐ 新增
  }
}

message PreCompactHook {
  string trigger = 1;             // "proactive_threshold" | "reactive_413" | "reactive_overflow"
  uint64 estimated_tokens = 2;    // 当前 context 估算 token 数
  uint64 context_window = 3;      // 当前模型 context window
  uint32 usage_pct = 4;           // 0-100，触发时使用百分比
  uint32 messages_to_compact = 5; // 中段消息条数（不含 head/tail）
  uint32 messages_total = 6;      // 全部消息条数
  bool reuses_prior_summary = 7;  // 是否复用上次 summary
  uint32 prior_summary_count = 8; // 历史 summary 复用了几条（M1 仅取 latest = 0 或 1）
}
```

**Decision Rationale**:
- 字段 18 与现有 enum 顺序一致（10-17 已用，跳过反而会污染历史变更日志）
- `trigger` 用枚举字符串而非 protobuf enum，让未来加触发原因（如 "user_command"、"scheduled"）无需 schema 变更
- `usage_pct` 用 0-100 整数避免 protobuf 浮点跨语言精度问题
- `reuses_prior_summary` + `prior_summary_count` 让 L4 审计能识别"这次压缩是从原始消息总结，还是叠加在历史 summary 之上"——影响 L4 对压缩质量的评估算法

**MVP audit-only**: hook 不允许 mutate prompt 或消息，只供 L3/L4 旁路日志/审计/告警。Mutate 协议挪到 ADR-V2-006（S3.T5 起草），跟 PreToolUse mutate 一起设计。

### D2. Hook 触发时机 — `generate_summary()` LLM 调用之**前**

`HookPoint::ContextDegraded`（现位于 `crates/grid-engine/src/hooks/mod.rs:35-36`，在压缩**之后** fire）改名为 `HookPoint::PostCompact`（保留语义但点出时机），同时新增 `HookPoint::PreCompact` 在 `compaction_pipeline.rs` 调用 `generate_summary()` 之前 fire。

**两个 hook 都保留**——L3/L4 审计可同时观测压缩前的 plan 和压缩后的 result。

### D3. 迭代式 Summary 复用 — 线性链，仅复用最近一次

**Rule**: `compact()` 调用时
1. 若 `CompactionContext::session_summary_store` 提供，先 `get_latest(session_id)` 取上次压缩产出的 summary
2. 若有，**作为 system message 前置注入** `to_summarize` 序列首位（不算入 head 保护，因为它本身就是上次的精简产物）
3. 压缩成功后，调用 `session_summary_store.save(session_id, new_summary, ...)` 覆盖（upsert）

**为什么线性而非 DAG**:
- Plan 文本说"复用**上次** summary"（singular），没说累积所有历史
- DAG 实现要 `(session_id, compaction_round)` 双主键 + 检索策略，本期不必要的复杂度
- 线性链已经能实现 plan 关键收益："不是从头总结"，summary 长度收敛而非 N²

**Schema**: 沿用 `SessionSummaryStore` 现有 `session_summaries` 表（一行一 session，upsert on conflict）。历史保留延后 Phase 3。

### D4. 跨压缩 Token 预算 — `task_budget_remaining` 全任务级别

引入 `task_budget_remaining: u64` 作为 harness loop 的初始上界（默认值 = `model.context_window * MAX_TURNS`，上限可配），每次 API 响应后从真实 `usage.input_tokens + usage.output_tokens` 扣减，**压缩不重置**。

**Rule**:
- 压缩只重排 messages，不影响"全任务还能用多少 token"的 budget
- 当 `task_budget_remaining < ESTIMATED_NEXT_TURN_COST` 时，loop 应主动结束（避免 OOM 或费用爆炸）
- `task_budget_remaining` 与 `TokenEscalation`（`agent/token_escalation.rs`）正交：后者管"这一轮 max_tokens 升级"，前者管"全任务 token 总账"

**Decision Rationale**:
- Plan 引用 claude-code `taskBudgetRemaining` 是 loop-local 不重置——在 grid-engine 等价于 harness loop 局部变量
- 选总任务预算 (a) 而非 per-turn 预算 (b)，因为 (b) 在压缩后等价于"按当前压缩后 size 重新算"，反而违反 plan "不重置"语义
- 真实 token 来自 `usage.input_tokens + usage.output_tokens`（provider response），不用 chars/4 估算——`ContextBudgetManager::update_actual_usage()` 已经存了，加 getter 即可

### D5. Proactive + Reactive 共存 — 不同 ratio

| Trigger | summary_ratio (默认) | trigger 条件 |
|---------|---------------------|--------------|
| Proactive | **0.2** (aggressive) | `usage_pct >= proactive_threshold_pct` (默认 75) |
| Reactive | **0.5** (conservative) | `is_prompt_too_long(err)` OR `FailoverReason::{ContextOverflow,PayloadTooLarge}` |

**Reactive 守卫**: harness loop 持有 `attempted_reactive_compact: bool`，触发一次后 set true；同 turn 内即使再 413 也不再触发 reactive，让 retry / fallback chain 接管。Loop 进入下一 turn 时 reset。

### D6. Tail/Head 保护 — Token-based

| 区域 | 保护规则 |
|------|---------|
| Head | system message + 第 1 个 user/assistant pair（最多 2 条非 system 消息）跳过 summarize |
| Tail | 反向累计 token 直到 `tail_protect_tokens`（默认 20000），到达即停 |
| Middle | 其余全部喂 summarizer |

`find_head_boundary()` 和 `find_tail_boundary()` 作为 `compaction_pipeline.rs` 私有函数实现。

### D7. Summarizer Model — 复用 session provider，仅允许同 provider 内换 model

`CompactionPipelineConfig::compact_model: Option<String>` 已存在。MVP 决定：
- 若 `compact_model = Some("claude-haiku-4-5-20251001")` 等同 provider 内的便宜 model，直接用
- 若 `compact_model = Some("gpt-4o-mini")` 但 session 是 Claude，**报错**（M2 增强：跨 provider 自动 routing → Phase 3）
- 默认 `None` → 用 session model

---

## Consequences / 后果

### Positive

- ✅ `hook.proto` schema 一次到位，goose-runtime / nanobot-runtime 抄就行
- ✅ L3/L4 审计有 typed event，可做"哪个 session 压缩频率高"等聚合分析
- ✅ 迭代式复用避免长 session 的 O(N²) 总结成本（每次都从头）
- ✅ 跨压缩预算让"无限 loop 烧钱"不再可能
- ✅ Proactive + reactive 共存让常见路径快（proactive 触发），异常路径稳（reactive 兜底）

### Negative / Tradeoffs

- ⚠️ proto 改动需要重生 4 个 Python pb2（非阻塞，CI 跑一次）
- ⚠️ `HookPoint::ContextDegraded → PostCompact` rename 是 breaking change，但目前**无 hook 注册到 ContextDegraded**（grep 全代码无 `HookPoint::ContextDegraded` 注册点，只有 enum 定义），实际零 impact
- ⚠️ Linear summary chain 在极长 session（30+ 压缩）下会丢失早期细节——可接受，因为 plan 接受"summary 是 lossy"
- ⚠️ Audit-only hook MVP 阶段无法实现"L4 拒绝压缩"等高级流程——延后 ADR-V2-006

### Migration

- `HookPoint::ContextDegraded` rename → `PostCompact`：grep 全 workspace 无注册点（只 enum 定义 + 一处 fire 调用），重命名安全
- `CompactionPipelineConfig` 新增字段全部带 `Default` impl，下游代码不需改

---

## Decision Evolution

- **Original (plan 2026-04-14)**: 新建 `agent/context_compressor.rs` 模块
- **Scout audit (2026-04-15)**: 发现 `compaction_pipeline.rs` 已实现 60% 功能，新建会重复
- **First proposal**: Rust-only，proto 字段保留给 claude-code 空着
- **Refined (after consequence analysis)**: proto 一次改完 + 重生所有 pb2，实现仍 Rust-only（claude-code-runtime 由 Anthropic SDK 处理 compaction，无 grid 实现路径）

---

## E2E Verification Plan (S3.T1 acceptance)

1. `cargo test -p grid-engine compaction -- --test-threads=1` → 7 new tests PASS
2. `cargo test -p grid-engine harness -- --test-threads=1` → 现有 + 新增 reactive/proactive 集成测试 PASS
3. 手工：构造 80% context window 长对话 → 触发 proactive，观察日志 `trigger=proactive_threshold`
4. 手工：构造 413 错误 → 触发 reactive，观察 `trigger=reactive_413`，第二次同 turn 413 不再触发
5. 手工：跑同 session 两轮压缩 → 第二次 PRE_COMPACT event `reuses_prior_summary=true, prior_summary_count=1`

---

## References

- Plan: `docs/plans/2026-04-14-v2-phase2-plan.md` §S3.T1
- Blueprint: `docs/plans/2026-04-15-s3t1-blueprint.md`
- Pattern source: `docs/design/EAASP/AGENT_LOOP_PATTERNS_TO_ADOPT.md` items #5, #6, #9
- Root cause: `docs/design/EAASP/AGENT_LOOP_ROOT_CAUSE_ANALYSIS.md`
- Related ADRs: ADR-V2-016, ADR-V2-017
- Future ADR: ADR-V2-006 (hook envelope mutate protocol — S3.T5)
