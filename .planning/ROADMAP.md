# Grid — Roadmap

> **Milestone:** Phase 4 — Product Scope Decision
> **Brownfield context:** First GSD-managed milestone after dev-phase-manager 14-phase archive (Phase BA → Phase 4a). Historical phases冻结只读, not migrated. This roadmap covers ONLY the new milestone's narrow boundary: cleanup queue → §P5 audit → Leg A/B decision documented as ADR.
> **Granularity:** standard (3 phases — intentionally below 5-8 floor; milestone is purposely narrow per prompt constraint "Done = Leg A vs Leg B decided + documented + cleanup zeroed").
> **Done condition for milestone:** ADR-V2-024 Accepted recording 走 Leg A / Leg B / 两条腿 + CLEANUP-01..04 all closed + REVIEW_POLICY.md + GSD lifecycle dry-run passed.

## Phases

- [x] **Phase 4.0: Bootstrap & Cleanup / GSD 接管 + 队列清零** — REVIEW_POLICY.md 落地 + CLEANUP-01..04 一次性扫掉 (tracer-bullet validates GSD plumbing) ✅ 2026-04-27 (5/5 SC, 7/7 must-haves PASS)
- [ ] **Phase 4.1: Discuss & Audit / §P5 触发条件审计** — Socratic discuss + DECIDE-01 audit doc + GSD discuss→plan→execute 链路跑通验证
- [ ] **Phase 4.2: Decide & Document / 决策落定** — Phase 4.2 path plan (Leg A 硬化 OR Leg B 激活) + ADR-V2-024 Accepted 关闭 milestone

## Phase Details

### Phase 4.0: Bootstrap & Cleanup / GSD 接管 + 队列清零

**Goal**: GSD 治理底座(REVIEW_POLICY.md)就位,Phase 4a 遗留 P0/P1 cleanup 队列归零,GSD 端到端 plumbing 通过 tracer-bullet 验证。
**Depends on**: Nothing(milestone 第一个 phase)
**Requirements**: CLEANUP-01, CLEANUP-02, CLEANUP-03, CLEANUP-04, GOVERNANCE-01
**Success Criteria** (what must be TRUE):
  1. `.planning/REVIEW_POLICY.md` 文件存在,定义了 high/medium/low risk task 触发条件 + superpowers two-stage opt-in 协议(基于 Phase 4a T1-T7 实战经验,不再是抽象 SOP)
  2. `docs/design/EAASP/L1_RUNTIME_ADAPTATION_GUIDE.md` §4 chunk_type 表格 + TypeScript 示例全部使用 ADR-V2-021 canonical wire 值(`text_delta` / `tool_start` / 等 8 个),并带 `<!-- @chunk-type-sync ADR-V2-021 -->` provenance marker
  3. `docs/design/EAASP/DEFERRED_LEDGER.md` 中 D120 状态 ambiguity 解决 —— 单行 grep 即可断定 D120 真实状态(closed at 7e083c7),且 ledger preamble 加入 row-edit-on-close 约定文字以防 future-D-item 复发
  4. `docs/reviews/strategy-grid-two-leg-checklist.md` 与 `.github/CODEOWNERS` 双双存在 —— ADR-V2-023 §Enforcement 引用的 5 个检查点全部落到 checklist 文件中,CODEOWNERS 把 grid-server / grid-platform / grid-desktop / web / web-platform 五条路径标为需 dormancy-justification reviewer
  5. CLEANUP-01..04 在新 GSD plan 内分 task 走完(任何 task 失败时 superpowers two-stage 机制能正确激活,验证 REVIEW_POLICY.md 不是死文档)
**Plans**: 1 plan
  - [x] 04.0-01-PLAN.md — CLEANUP-01..04 + GOVERNANCE-01 一次性扫掉, 5 atomic-commit-per-task tracer-bullet (4-5 commits expected)
**UI hint**: no

### Phase 4.1: Discuss & Audit / §P5 触发条件审计

**Goal**: 通过 socratic discuss 系统化审计 ADR-V2-023 §P5 列出的 Leg B 激活触发条件,产出可被引用的 audit doc;同时把 GSD discuss → plan → execute → review 端到端跑通,验证本仓库 brownfield 适配性。
**Depends on**: Phase 4.0(REVIEW_POLICY.md 必须先存在,本 phase 的 audit task 会走它判定 risk level)
**Requirements**: DECIDE-01, GOVERNANCE-02, GOVERNANCE-03
**Success Criteria** (what must be TRUE):
  1. `docs/design/EAASP/adrs/decisions/2026-04-XX-leg-decision-audit.md`(或 ADR-V2-024 草稿的 §Audit 章节)存在,逐条记录 ADR-V2-023 §P5 触发条件的 yes/no/evidence(at least N concrete enterprise leads / EAASP 升级路径阻断 / 团队规模 / etc.)
  2. Audit 结论可被 Phase 4.2 直接引用 —— 即 audit doc 给出明确的 "走 Leg A 硬化 / 走 Leg B 激活 / 两条腿都推进" 中三选一推荐,而非含糊"看情况"
  3. GSD `/gsd-plan-phase` → `/gsd-execute-plan` → `/gsd-code-review` → `/gsd-end-phase` 链路在本 phase 至少跑通一次,任何不顺手处记录到 `docs/dev/WORK_LOG.md` 顶部的 "GSD adoption notes" 块
  4. `/gsd-resume-work` 在本 phase 至少触发一次(可由 `/clear` 后真实测试或 dry-run 模拟),验证 `.planning/STATE.md` + checkpoint 机制能恢复 active phase context 而不丢工作
  5. Phase 4.1 end-phase 时 `.planning/STATE.md` 反映 audit 结论 + Phase 4.2 入口路径(plan 文件名 / 主要 task 形态)
**Plans**: 1 plan
  - [x] 04.1-01-PLAN.md — DECIDE-01 audit doc + ADR-V2-024 Proposed 草稿 + GOVERNANCE-02 WORK_LOG GSD Adoption Notes + GOVERNANCE-03 `/gsd-resume-work` 中段实测 + REVIEW_POLICY draft → Active flip (7 atomic tasks, expected 6-9 commits)
**UI hint**: no

### Phase 4.2: Decide & Document / 决策落定

**Goal**: 基于 Phase 4.1 audit 结论产出 Phase 4.2 实际执行 plan(Leg A 硬化路线 OR Leg B 激活路线),并把"Phase 4 product scope 决定"作为 ADR-V2-024 走完 Proposed → Accepted 流程,关闭 Phase 4 milestone。
**Depends on**: Phase 4.1(audit doc 必须先存在,且 audit 结论已被 STATE.md 锁定)
**Requirements**: DECIDE-02, DECIDE-03
**Success Criteria** (what must be TRUE):
  1. `.planning/phases/4.2/PLAN.md` 内容反映 Phase 4.1 audit 决策方向 —— 若推荐 Leg A 硬化则 plan 列出 multi-tenant / perf / EAASP shadow sync 类 task;若 Leg B 激活则 plan 列出 grid-platform / web-platform 启动 task;若两条腿则两块都列
  2. 新 ADR(候选 ID `ADR-V2-024-phase4-product-scope-decision`)经 `/adr:new --type strategy` 创建,frontmatter F1-F4 lint 全 PASS,Decision 段明确写出选定 leg + Rationale 段引用 §P5 audit 中具体 evidence
  3. ADR-V2-024 经 `/adr:accept` 进入 Accepted 状态(F1-F4 hard gates 通过 + audit doc 已被 §References 引用)
  4. Phase 4.2 end-phase 时 PROJECT.md §Active 中的 "Phase 4 主决策" 行被划掉移入 §Validated,引用 ADR-V2-024 commit hash
  5. Milestone 关闭检查点:CLEANUP-01..04 + DECIDE-01..03 + GOVERNANCE-01..03 全部在 traceability table 标记 ✅,debt water-line 没有新增 P0/P1-active 项
**Plans**: TBD
**UI hint**: no

## Phase 之外的 milestone 关闭后续

> 这些不是本 milestone 的 phase,只作为 traceability 提示。

- **下一个 milestone** 由 `/gsd-new-milestone` 启动,内容由 ADR-V2-024 决策结果驱动:
  - 若走 Leg A 硬化:进入 multi-tenant 隔离 / perf tuning / skill catalog / EAASP shadow sync 类 phase
  - 若走 Leg B 激活:进入 `grid-platform` 多租户激活 / `web-platform/` 前端实现 / `grid-server` 商用化 / Tauri MVP 类 phase
  - 若两条腿:两份 phase plan 各自独立排期
- **不属于本 milestone 但仍需追踪的 P1 项**(由 CONCERNS.md 列出,延到下一个 milestone):D109(workflow.required_tools 不变量)、D134(shipped skill hooks nested keys)、D136(grid-runtime hook firing on probe turn)、D142/D143(EAASP_DEPLOYMENT_MODE 接入)、NEW-D2(test_chunk_type_contract 参数化)、NEW-E2(ADR enforcement.trace 补)、NEW-E3(ADR-V2-019 → Accepted)。**这些不是本 milestone 的 success criteria**,但若 Phase 4.0 cleanup 跑得快、有 capacity,可被择机捎带(由 plan-phase 评估,不在 ROADMAP success criteria 里)。

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 4.0 Bootstrap & Cleanup | 1/1 ✅ | Complete (5/5 SC, 7/7 must-haves PASS) | 2026-04-27 |
| 4.1 Discuss & Audit | 0/1 (planned 2026-04-27) | Planned — 7 atomic tasks expected 6-9 commits | - |
| 4.2 Decide & Document | 0/0 (TBD by plan-phase) | Not started | - |

## Coverage

| REQ-ID | Phase | Notes |
|--------|-------|-------|
| CLEANUP-01 | 4.0 | L1 adaptation guide §4 chunk_type sweep |
| CLEANUP-02 | 4.0 | D120 ledger ambiguity + row-edit-on-close convention |
| CLEANUP-03 | 4.0 | strategy-grid-two-leg-checklist.md 落地 |
| CLEANUP-04 | 4.0 | .github/CODEOWNERS 落地 |
| DECIDE-01 | 4.1 | §P5 触发条件 audit doc |
| DECIDE-02 | 4.2 | Phase 4.2 path PLAN.md(Leg A or Leg B or both) |
| DECIDE-03 | 4.2 | ADR-V2-024 Accepted |
| GOVERNANCE-01 | 4.0 | REVIEW_POLICY.md(GSD 接管 Step 3 同时落) |
| GOVERNANCE-02 | 4.1 | GSD discuss→plan→execute 端到端验证 |
| GOVERNANCE-03 | 4.1 | /gsd-resume-work 至少触发一次 |

**Total v1 requirements:** 10
**Mapped:** 10/10 ✓
**Orphans:** 0

## Granularity 备注

本 milestone 选 3 phase(标 standard 设定 5-8)是**有意为之**:
- Prompt 明文要求 milestone 必须窄 —— 不是 EAASP v3.0 也不是产品大改造,只是 Phase 4a → 下一个 leg 之间的 1 个决策门
- 不增加 cleanup 拆分粒度(eg. 每个 CLEANUP 各一个 phase)的原因:CLEANUP-01..04 都是文件级写入,工作量分钟到小时级,放一起更便于做 GSD plumbing 的 tracer-bullet 验证
- 不增加 decision 拆分粒度(eg. discuss / draft / accept 各一个 phase)的原因:DECIDE-02 和 DECIDE-03 是同一个 ADR 的 Proposed → Accepted 状态机,人为切两个 phase 反而割裂上下文
- 后续 milestone 由 ADR-V2-024 决定的 Leg A/B 实际执行工作,phase 数会自然回到 5-8 区间

如 plan-phase 阶段发现某个 phase task 多于 5 个 plan,可由 plan-phase 自行考虑微拆,但 ROADMAP 阶段不预拆。
