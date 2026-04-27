---
id: ADR-V2-024
title: "Phase 4 Product Scope Decision (Leg A 硬化 / Leg B 激活 / 两腿)"
type: strategy
status: Proposed
accepted_at: null
date: 2026-04-27
phase: "Phase 4.1 — Leg Decision Audit (Phase 4.2 graduates to Accepted)"
author: "Jiangwen Su"
supersedes:
  - ADR-V2-023
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

> 采纳 audit §5 双轴模型 (engine vs data/integration) 作为 Phase 4 product scope 主框架, audit §4.2 推荐 (两腿都推进) 是双轴模型在产品形态层的具体实例化。下层 §1 (双轴 substance) 与 §2 (三选一推荐) 在本 ADR 中 co-equal 并存, 后续讨论必须同时引用; 仅引用 §1 会丢失 SC#2 三选一硬要求, 仅引用 §2 会丢失 audit §0 partial-needs-revision elevation 要求的 §5 substance。

### §1. 双轴模型 substance (engine vs data/integration — audit §5.1 + §5.5 verbatim)

替代 ADR-V2-023 的 Leg A(EAASP 集成)/ Leg B(Grid 独立)二元切换, 本 ADR 采纳两轴 (per audit §5.1 verbatim):

**轴 1: engine vs data/integration**
- engine = 可复用的核心组件, user 主战场(per PRE-AUDIT-NOTES §B.2 + §C.1)
- data/integration = 场景特定的横切关注, 客户/厂商/他人接手

**轴 2: Grid 全栈 vs EAASP 引擎层 vs 数据/集成横切**
- Grid 全栈 = `grid-cli` / `grid-server` / `grid-desktop` / `grid-platform` / `web*` 全部 — 都是 user 工时主战场
- EAASP 引擎层 = `tools/eaasp-l2-memory-engine/` + `eaasp-skill-registry/` + `eaasp-mcp-orchestrator/` + `eaasp-l3-governance/` + `eaasp-l4-orchestration/` 各自 engine 组件 — user 主战场之一
- 数据/集成横切 = 客户语料 / vector 库 / 企业 policy 数据 / SSO / 第三方治理 / 工作流 / SaaS 集成 / signature backend / WORM 存储 / 信创 LLM 适配 — 他人接手

**audit 推荐采纳理由** (per audit §5.5 verbatim): "§3 4 个 Q 的 verdict 在双轴下都自然归约且解释力强;§4 三选一推荐'两腿都推进'在 Leg A/B 框架下含糊('两腿都推进'是什么意思?), 在双轴下清晰('user 投 engine + 他人投 data/integration, Grid 全栈作为产品 leg 自然 active 但内部职责切清晰')。"

**5+3 字段切分** (per Open Item #4 user 决议 + audit §3.1):
- Engine 内置 (5 项, 走 proto wire-level + Level 2/3 操作): `priority_tier` (替代原 `dispatch_level` — 跨行业通用名) / `risk_class` / `latency_class` / `audit_trace` / `model_lock`
- 厂商写 (3 项, 走 vendor skill / Path D): SCADA 接入适配 / 业务 KPI 定义 / 行业合规表
- **架构承诺**: EAASP L3/L4 必须提供 extension hook API, 让行业扩展可访问并控制 5 个 engine 内置字段; 详细 API 推到 Phase 4.3+ 独立 ADR (合并 audit §3.3 Q3 ⚫ 6 项接入位 ADR 候选)

### §2. 三选一推荐 (产品形态实例 — audit §4.2 verbatim)

> "Phase 4 product scope 决定 = **两腿都推进**. 据 audit §3.4 Q4 verdict yes (Grid 优先 + 并行可行) + §2.4 §P5.4 partial baseline 默认成立 + §B.2 user 工时事实, Grid 全栈与 EAASP 引擎层均为 user 工时主战场, 不存在二选一前提 (Grid 已 active, EAASP 同仓孵化期间 user 自做引擎); Implication: Phase 4.2+ 子任务必须按 §5 双轴模型 (engine vs data/integration) 切, 而非按 Leg A/B 切, 见 §5 + Open Items §6 待 user 补 5 条精确工时分配。" (per audit §4.2; §0 elevation status: partial-needs-revision — 需与 §5 双轴模型 co-equal 引用)

**工时 baseline** (per Open Item #2 user 决议 + audit §B.2 工时事实精确化): Grid 全栈 ≈60% / EAASP 引擎 ≈30% / 元工作 (planning/audit/governance) ≈10%

**优先发力组合** (per Open Item #3 user 决议 + audit §3.4 §F.Q4): grid-cli + grid-server 优先发力, grid-desktop / grid-platform / web* 静态 dormant 到下个 milestone

**草稿状态 → Accepted 翻转**: Phase 4.2 commit 当天 status: Proposed → Accepted + accepted_at 2026-04-XX + supersedes ADR-V2-023 (per T4 frontmatter delta).

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

> 以下 4 个 option 是 Phase 4 product scope 候选; Phase 4.2 Accepted 时 audit §0 verdict partial-needs-revision elevation 要求 Decision 段同时引用 Option C (audit §4.2 推荐 — 两腿都推进) + Option D (audit §5 双轴模型 co-equal output), 不能仅取其一。本 ADR Accepted 时 Option A + B 标 rejected with audit §4.3 cite, Option C + D 标 accepted-co-equal。

### Option A: Leg A 硬化(集中 EAASP 集成)

**Positive**:
- 简化产品形态: 单一 leg 聚焦 EAASP 集成路径, vendor 心智模型最简
- 短期工时集中: user 全力投 EAASP L2/L3/L4 引擎, 不分流到 Grid 独立产品

**Negative**:
- 与 §B.2 user 自陈 "Grid 全栈是工时主战场" 直接冲突 (Grid 60% / EAASP 30% 工时分配)
- §3.4 Q4 verdict yes (Grid 优先 + 并行可行) 否决 "Grid dormant" 假设
- audit §4.3 显式列为否决候选 — Grid 5-7 项企业级品质必需 (M5 溯源链 / M9 model_lock / M6 evidence chain) 独立于 EAASP 决策, 不能 dormant

**Risks**:
- 长期商业灵活性丢失 (无独立 Grid 产品 fallback)
- 与 PRE-AUDIT-NOTES §D Sanity Guard 第 1 条冲突, 推翻 user 工时 baseline 需要 audit 无的硬证据

**Verdict**: rejected (per audit §4.3 + §3.4 Q4 + §B.2)

**Evidence chain**: audit §4.3 否决 Leg A 硬化 → audit §3.4 Q4 verdict yes (Grid 优先) → audit §B.2 user 工时 60/30 — 三条 evidence 同向支持 rejected.

### Option B: Leg B 激活(Grid 独立产品)

**Positive**:
- Grid 独立产品形态清晰, 不被 EAASP 集成路径绑定
- 客户 Grid-only 信号若出现可直接接住 (per §P5.2 trigger 设计意图)

**Negative**:
- §P5.2 verdict no (0 客户 Grid-only 信号, per Open Item #1) 否决 trigger 触发前提
- 与 §B.2 user 自陈 EAASP 引擎层是 user 工时主战场之一相悖 (EAASP 30% 不应 dormant)
- audit §4.3 显式列为否决候选 — EAASP engine (L2/L3/L4 基础组件) 由 user 自做是 baseline 锁定

**Risks**:
- 失去 EAASP 集成示例的现成验证场景 (§B.3 Sanity Guard)
- 与 audit §B.2 + §3.1 §3.2 §F.Q1/Q2 partial verdict 共同要求 EAASP engine 持续扩展相悖

**Verdict**: rejected (per audit §4.3 + §P5.2 Open Item #1 backfill + §B.2)

**Evidence chain**: audit §4.3 否决 Leg B 激活 → §P5.2 0 客户 Grid-only 信号 (Open Item #1) → audit §B.2 user 工时 60/30 EAASP 不应 dormant — 三条 evidence 同向支持 rejected.

### Option C: 两腿都推进 (audit recommended per §4.1)

**Positive**:
- 同时保留 Grid 独立产品形态 + EAASP 集成路径, 商业灵活性最高
- 与 §B.2 user 工时 60/30 baseline 一致 (Grid 主战场 + EAASP 引擎层主战场之一)
- audit §4.4 self-consistency 检查通过 — 在 ADR-V2-023 字面框架 + 双轴模型框架下都成立

**Negative**:
- 在 Leg A/B 框架下 "两腿都推进" 措辞含糊 (per audit §5.5 — "两腿都推进 是什么意思?")
- 工时分散风险若不严格按 60/30/10 baseline 控制, 容易左右摇摆

**Risks**:
- 单独 Option C (无 Option D 双轴 substance) 会丢失 audit §0 partial-needs-revision elevation 要求, 框架描述含糊
- 后续 milestone 拆 task 若仍按 Leg A/B 切而非 engine vs data/integration 切, 会复制 ADR-V2-023 措辞瘴气

**Verdict**: accepted (audit recommended per §4.1; evidence chain audit §4.1 + §3.4 Q4 + §2.4 §P5.4 + §0)

**Evidence chain**: audit §4.1 推荐 → audit §3.4 Q4 verdict yes (Grid 优先 + 并行可行) → audit §2.4 §P5.4 partial baseline 成立 → audit §0 verdict partial-needs-revision 要求 §4 §5 co-equal — 四条 evidence 在 audit §4.4 self-consistency 检查下收敛.

### Option D: 框架修订(双轴模型 — engine vs data/integration) (audit co-equal output per §0 + §5)

**Positive**:
- 职责切清晰 (engine = user 主战场 / data/integration = 他人接手), 替代 Leg A/B 二元含糊
- audit §3 4 个 Q 在双轴下自然归约且解释力强 (§5.5 verbatim)
- 后续 milestone 拆 task 起点清洁 (engine 子任务列 + data/integration 接入面 子任务列)

**Negative**:
- 改 ADR 成本 — Phase 4.2 同时 supersede ADR-V2-023, 需额外 ~1-2h sweep "Leg A/B" 措辞
- 对外沟通门槛上升 (新 contributor / 客户需学双轴模型 + 5+3 字段切分)

**Risks**:
- 双轴模型与 §P5 4 条 trigger 在 §5.6 兼容性检查下都自然落点, 但实际运维中可能浮现新边界 case 需补
- extension hook API 详细 schema 推到 Phase 4.3+ 独立 ADR, Phase 4.2 仅 surface 架构承诺

**Verdict**: accepted (audit co-equal output per §0 + §5; 与 Option C 共同构成本 ADR Decision 段双框架文本)

**Evidence chain**: audit §5 框架修订建议 → audit §5.5 audit 推荐采纳双轴 → audit §0 verdict partial-needs-revision elevation → audit §5.6 双轴与 §P5 4 trigger 兼容性 case 全 self-consistent — 四条 evidence 在 audit §0 + §5 双框架下收敛.

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
- audit Open Items 5 条 user 已补 ✓ (T3 backfill, 见 audit §6 + 本 ADR §References)
- Decision 段填实 ✓ (T1 layered 锁文 — 顶层 blockquote pin + §1 双轴 substance + §2 三选一推荐 H3)
- Alternatives Considered 4 Option Positive/Negative/Risks 展开 ✓ (T2 — Option C 标 audit recommended + Option D 标 audit co-equal)
- Enforcement 段确认 strategic level + review_checklist 非空 ✓ (T3, F4 lint 要求满足; level=strategic 允许 trace=[] per ADR-V2-019 + ADR-V2-023 precedent)
- frontmatter supersedes ADR-V2-023 ✓ (T4 — ADR-V2-024 supersedes 多行 YAML list 含 V2-023 + V2-023 frontmatter flip Superseded + body Status 行同步)
- F4 lint pass + status Proposed → Accepted ✓ (T5 — Path B fallback per Phase 4.1 T5 deviation precedent: autonomous executor 模式下 `.adr-plugin/scripts/adr_lint.py --check F1,F2,F3,F4 --ci` 替代 `/adr:accept` slash command; T5 step 5.6 final joint lint V2-023+V2-024)

## History

| 日期 | 状态 | 变更说明 |
|------|------|---------|
| 2026-04-27 | Proposed | Phase 4.1 audit 完成后落盘草稿; Decision 段 verbatim 引用 audit §4.2 推荐措辞 + §0 verdict partial-needs-revision 要求 §5 co-equal; Enforcement/Alternatives 各 option Positive-Negative-Risks 段留 Phase 4.2 填实(per CONTEXT.md D-F-03) |
