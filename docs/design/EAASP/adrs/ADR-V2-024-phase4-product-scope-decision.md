---
id: ADR-V2-024
title: "Phase 4 Product Scope Decision (Leg A 硬化 / Leg B 激活 / 两腿)"
type: strategy
status: Proposed
accepted_at: null
date: 2026-04-27
phase: "Phase 4.1 — Leg Decision Audit (Phase 4.2 graduates to Accepted)"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: strategic
  trace: []
  review_checklist: "docs/design/EAASP/adrs/ADR-V2-024-phase4-product-scope-decision.md"
affected_modules:
  - "crates/grid-engine/"
  - "crates/grid-runtime/"
  - "crates/grid-platform/"
  - "crates/grid-server/"
  - "crates/grid-desktop/"
  - "tools/eaasp-*/"
related:
  - ADR-V2-017
  - ADR-V2-019
  - ADR-V2-023
---

# ADR-V2-024 — Phase 4 Product Scope Decision (Leg A 硬化 / Leg B 激活 / 两腿)

**Status:** Proposed
**Date:** 2026-04-27
**Phase:** Phase 4.1 — Leg Decision Audit (Phase 4.2 graduates to Accepted)
**Author:** Jiangwen Su
**Related:** ADR-V2-017 (L1 ecosystem), ADR-V2-019 (L1 deployment), ADR-V2-023 (双腿战略 — 本 ADR 候选修订对象)

---

## Context / 背景

ADR-V2-023 (Accepted 2026-04-19) 用 Leg A (EAASP 集成) / Leg B (Grid 独立) 二元框架切产品形态, §P5 列出 4 条 Leg B 激活触发条件。Phase 4.1 audit (`docs/design/EAASP/adrs/decisions/2026-04-27-leg-decision-audit.md`) 系统化检查这 4 条 trigger + 4 条 §F audit agenda (来自 `.planning/phases/4.1-PRE-AUDIT-NOTES.md` §F), 浮现两类发现:

1. §P5 4 条 trigger 在"EAASP 同仓孵化"现状下语义部分弱化 (audit §2 — partial 覆盖率 50%, §0 verdict = partial-needs-revision)
2. user 心智模型实际是 "engine vs data/integration" 双轴, 与 ADR-V2-023 字面 Leg A/B 切角不同 (PRE-AUDIT-NOTES §C.1; audit §5 双轴模型框架修订建议)

ADR-V2-024 落盘 Phase 4 product scope 决定, 喂 Phase 4.2 PLAN.md 路径。

## Decision / 决策

> "Phase 4 product scope 决定 = **两腿都推进**. 据 audit §3.4 Q4 verdict yes (Grid 优先 + 并行可行) + §2.4 §P5.4 partial baseline 默认成立 + §B.2 user 工时事实, Grid 全栈与 EAASP 引擎层均为 user 工时主战场, 不存在二选一前提 (Grid 已 active, EAASP 同仓孵化期间 user 自做引擎); Implication: Phase 4.2+ 子任务必须按 §5 双轴模型 (engine vs data/integration) 切, 而非按 Leg A/B 切, 见 §5 + Open Items §6 待 user 补 5 条精确工时分配。"

(per audit §4.2; §0 elevation status: partial-needs-revision — 需与 §5 双轴模型 co-equal 引用)

具体决策:
- **三选一选择**: 两腿都推进 (per audit §4.1 推荐, evidence chain 见 audit §4.1 + §3.4 Q4 + §2.4 §P5.4 + §0)
- **框架修订**: 是否采纳 audit §5 框架修订建议 (engine vs data/integration 双轴) — Phase 4.2 决定, 但本 audit §0 verdict partial-needs-revision elevation 要求 Phase 4.2 ADR-V2-024 §Decision 段必须**同时**引用 §4.2 推荐措辞 (产品形态切角) + §5.5 双轴模型 substance (职责切角), 不能仅取其一
- **Rationale**: 引用 audit doc §2 §P5 4 条 verdict (1 partial 语义弱化 + 1 unknown + 1 no + 1 partial baseline) + audit doc §3 §F Q1-Q4 verdict (2 yes + 2 partial); 详见 References 段 audit doc 链接

**草稿状态**: Phase 4.1 落盘 Proposed; Decision 段 verbatim 引用 audit §4.2 (audit recommended per §4.1 — 见下方 Alternatives Considered 段 Option C 标注); Phase 4.2 翻 Accepted 时锁定文本 + 填 Enforcement 段 + 决定是否 supersede ADR-V2-023 (per D-F-05)。

## Consequences / 后果

引用 audit doc §3 §F Q1-Q4 答案 (verbatim from audit doc):
- **Q1 (EAASP 调度 vertical)** verdict partial (并行倾向) → 影响 EAASP engine 工时倍增风险, audit §3.1 建议 4-5 项 vertical 字段 engine 吸收 (dispatch_level / risk_class / latency_class) + ~3-4 项业务规则厂商写
- **Q2 (governance workflow)** verdict partial (做基础引擎部分) → 影响 EAASP 企业市场差异化, audit §3.2 建议 governance engine (R1-R9 lint / HITL workflow / lifecycle 5 阶段 / multi-dim index 引擎) 由 user 做, policy 数据 / 业务规则 / 第三方治理系统适配 由企业 / 厂商接入
- **Q3 (⚫ 6 项接入位)** verdict yes → 影响客户/厂商集成顺畅度, audit §3.3 建议 Phase 4.3 / 后续 milestone 显式文档化为 EAASP extension surface contract (单独 ADR 候选), 作为客户/厂商集成参考
- **Q4 (Grid vs EAASP 优先级)** verdict yes (Grid 优先 + 并行可行) → 影响 user 工时分配, audit §3.4 建议 Phase 4.2 决定 grid-cli / grid-server / grid-desktop / grid-platform / web* 五者中先发力 2 个

### Positive
- (待 Phase 4.2 填; 取决于 Decision 选择)

### Negative
- (待 Phase 4.2 填)

### Risks
- (待 Phase 4.2 填)

## Affected Modules / 影响范围

| Module | Impact |
|--------|--------|
| `crates/grid-engine/` | 共享核心, 无论 Decision 选哪条都受影响 |
| `crates/grid-runtime/` | L1 实现, Leg A 入口 |
| `crates/grid-platform/` | dormant 状态是否解锁取决于 Decision; audit §4.2 推荐"不再标 dormant 但优先级低于核心" |
| `crates/grid-server/` | Grid 核心组件, audit §3.4 Q4 verdict yes 直接推进 |
| `crates/grid-desktop/` | dormant 状态是否解锁取决于 Decision |
| `tools/eaasp-*/` | shadow vs production 路径取决于 Decision; audit §3.1-§3.2 建议 user 持续扩展引擎层 |
| `docs/design/EAASP/adrs/ADR-V2-023-grid-two-leg-product-strategy.md` | 是否 supersede 取决于 Phase 4.2 决议 (D-F-05) |

## Alternatives Considered / 候选方案

> 以下 4 个 option 是 Phase 4 product scope 候选; audit §4 推荐其一, audit §0 + §5 决定是否升级双轴模型为主结论. 本 ADR Proposed 状态下据 audit §4.1 phrase ("两腿都推进") 在 Option C 标注 "(audit recommended per §4.1)"; audit §0 verdict partial-needs-revision 要求 Option D 标 "(audit co-equal output per §0 + §5)"。

### Option A: Leg A 硬化(集中 EAASP 集成)
TBD by Phase 4.2 (per audit §4.3 候选立场; audit §3.4 Q4 yes (Grid 优先) + PRE-AUDIT-NOTES §B.2 + §B.3 否决此 option, 与 baseline §D Sanity Guard 第 1 条冲突)

### Option B: Leg B 激活(Grid 独立产品)
TBD by Phase 4.2 (per audit §4.3 候选立场; 与 §B.2 user 自陈 EAASP 引擎层是 user 工时主战场之一相悖, audit 无硬证据推翻 baseline §D 第 4-5 条 Sanity Guard)

### Option C: 两腿都推进 (audit recommended per §4.1)
TBD by Phase 4.2 (per audit §4.1 推荐立场; evidence chain: §3.4 Q4 yes + §2.4 §P5.4 partial baseline + §B.2 user 工时事实 + §0 verdict partial-needs-revision; 见 audit §4.4 self-consistency 检查)

### Option D: 框架修订(双轴模型 — engine vs data/integration) (audit co-equal output per §0 + §5)
TBD by Phase 4.2 (per audit §5 框架修订建议; audit §0 verdict partial-needs-revision elevation 要求 Phase 4.2 ADR §Decision 段同时读 Option C 与 Option D, 不能仅取其一)

## References / 参考

- **`docs/design/EAASP/adrs/decisions/2026-04-27-leg-decision-audit.md`** — Phase 4.1 audit doc(本 ADR Decision 段直接来源, 不复制内容避免双 SoT 漂移; per CONTEXT.md D-A-02)
- ADR-V2-023 §P5 (L155-162) — 候选修订对象(是否 supersede 推到 Phase 4.2 决定; per D-F-05)
- `.planning/phases/4.1-PRE-AUDIT-NOTES.md` §A-§F — Phase 4.1 audit baseline + agenda
- `docs/external-review/2026-04-26-eaasp-skill-spec-coverage-internal.md` — §F Q1/Q2 evidence 素材
- ADR-V2-017 (L1 ecosystem) — related, L1 runtime 生态策略
- ADR-V2-019 (L1 deployment) — related, L1 部署模型 (Proposed-with-empty-trace 形态参考)

## Implementation

### Phase 4.1 (本 ADR 落盘):
- 创建 audit doc + 本 ADR Proposed 草稿 (per CONTEXT.md D-F-01..03)
- F1-F3 lint pass (per D-F-04)
- audit Open Items 列出待 user 补 5 条 (audit §6)

### Phase 4.2 (Accepted 翻转):
- audit Open Items user 补足 (per D-E-04; ADR-block:yes hard-stop §P5.2 必须先补)
- Decision 段填实 (verbatim 替换 §4.2 引用为锁定文本 + 双轴模型 substance 段)
- Enforcement 段填(F4 lint 要求)
- Alternatives Considered 各选项展开 (Positive / Negative / Risks)
- 决定是否 supersede ADR-V2-023 (per D-F-05)
- F4 lint pass + `/adr:accept ADR-V2-024`

## History

| 日期 | 状态 | 变更说明 |
|------|------|---------|
| 2026-04-27 | Proposed | Phase 4.1 audit 完成后落盘草稿; Decision 段 verbatim 引用 audit §4.2 推荐措辞 + §0 verdict partial-needs-revision 要求 §5 co-equal; Enforcement/Alternatives 各 option Positive-Negative-Risks 段留 Phase 4.2 填实(per CONTEXT.md D-F-03) |
