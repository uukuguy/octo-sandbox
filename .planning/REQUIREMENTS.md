# Grid — Requirements

> **Brownfield context**: 14 archived phases (Phase BA → Phase 4a) under dev-phase-manager already shipped EAASP v2.0 functional baseline. This REQUIREMENTS.md scopes the **first GSD-managed milestone** ("Phase 4 — Product Scope Decision") + Phase 4a project review's P0/P1 cleanup queue. Historical Validated capabilities are listed in PROJECT.md §Validated for context but not re-tracked here.

---

## v1 Requirements (Milestone: Phase 4 — Product Scope Decision)

### A. Pre-Phase-4 Cleanup (carry-over from Phase 4a project review 2026-04-26)

- [x] **CLEANUP-01**: 修复 `docs/design/EAASP/L1_RUNTIME_ADAPTATION_GUIDE.md` §4 stale chunk_type wire 值 —— 用 ADR-V2-021 §4 的合法 enum 值(TEXT_DELTA / TOOL_START / WORKFLOW_CONTINUATION 等 8 个,加 ADR-V2-021 引用)替换 "text" / "tool_call" / "hook_fired" / "pre_compact" 字符串。新 L1 runtime 作者直接读到正确 wire 契约。
- [x] **CLEANUP-02**: 澄清 D120 状态 —— 调查 `DEFERRED_LEDGER.md` 中 D120 标 "P1-defer → Phase 2.5 W1" 与 `phase_stack.json` Phase 2.5 标 100% complete 的矛盾,决定 D120 是真已 closed 还是 silent descope。如已 closed,补 close-out trace;如仍 open,加显式 ETA。Output: ledger row 状态明确,可被搜索断言。
- [x] **CLEANUP-03**: 创建 `docs/reviews/strategy-grid-two-leg-checklist.md` —— ADR-V2-023 §Enforcement 引用了此文件但磁盘上不存在。文件应规范化 reviewer 在 PR 触碰 `grid-platform/` `grid-server/` `grid-desktop/` `web*/` 时的 dormancy-justification 检查清单。
- [x] **CLEANUP-04**: 创建/补充 `.github/CODEOWNERS` 把 Leg-B crate / web 目录强制要求 reviewer 走 §P5 dormancy-justification 流程。

### B. Phase 4 主决策(Leg A vs Leg B per ADR-V2-023 §P5)

- [ ] **DECIDE-01**: 完成 ADR-V2-023 §P5 触发条件审计 —— 系统化检查 §P5 列出的 Leg B 激活条件(at least N concrete enterprise leads / EAASP 升级路径阻断 / 团队规模 / 等)是否满足。Output: 一份 `docs/design/EAASP/adrs/decisions/2026-04-XX-leg-decision-audit.md`(or 写进新 ADR-V2-024 草稿)记录每条触发条件的 yes/no/evidence。
- [ ] **DECIDE-02**: 基于 §P5 审计结果,在 Phase 4.1 socratic discuss 收尾后,定 Phase 4 后续走向 —— 若 Leg A 继续硬化,产出 Phase 4.2 Leg-A-roadmap;若 Leg B 激活,产出 Phase 4.2 Leg-B-activation-plan。Output: `.planning/phases/4.2/PLAN.md` 内容反映决策。
- [ ] **DECIDE-03**: 决策文档化 —— 新建 ADR(V2-024 候选)记录 "Phase 4 product scope 决定走 Leg A / Leg B / 两条腿都推进" 的最终选择 + Rationale。Status = Accepted。

### C. Workflow & Governance Bootstrap(GSD 体系基建)

- [x] **GOVERNANCE-01**: `.planning/REVIEW_POLICY.md` 落地 —— 定义 high / medium / low risk task 触发条件,以及 superpowers two-stage review opt-in 协议(Phase 4 期间起草,基于 Phase 4a T1-T7 实战经验)。
- [ ] **GOVERNANCE-02**: 第一个 GSD-managed phase(Phase 4.1 discuss)完整跑通 —— 验证 GSD 的 discuss → plan → execute → review → end-phase 链路在本仓库 brownfield 上可工作,记录任何不顺手处。
- [ ] **GOVERNANCE-03**: 第一次跨 phase 状态恢复测试 —— 通过 `/gsd-resume-work` 在 `/clear` 之后恢复 active phase context。验证 STATE.md + checkpoint 机制可靠。

---

## v2 Requirements (deferred — 后续 milestone 处理)

> 这些是 Phase 4 决策结果出来后才能定义具体 task 的项,以 milestone 占位符形式列。

- v2 — **若 Leg A 继续**:multi-tenant isolation hardening / perf tuning / skill catalog 扩展 / EAASP shadow sync 自动化
- v2 — **若 Leg B 激活**:`grid-platform` 多租户激活 / `web-platform/` 前端实现 / 单租户 `grid-server` 商用化路径 / Tauri `grid-desktop` MVP
- v2 — Phase 4 哪条线都涵盖的:harness.rs 抽 sub-fn(可读性) / TUI key_handler.rs 拆分 / grid-eval EvalRunner 抽象
- v2 — 文档规整:`docs/design/*.md` root-level 60+ legacy 文件迁档至 `docs/design/LEGACY/` 子目录(或加 `[STALE]` header)
- v2 — `lang/hermes-runtime-python` 对齐 hatchling + ≥3.12(frozen 但成本极低)

## Out of Scope

- **`grid-sandbox` 仓库改名** —— per ADR-V2-023 §P6,Leg B 激活前不动
- **`git push origin main`** —— 累积 ~14 unpushed commits,push 时机由人决策
- **132 个历史 plan 文件 + 14 archived phase 迁入 GSD ROADMAP.md** —— 冻结只读,git history 为准
- **F4 lint 52 module-overlap 警告 reconcile** —— 已确认无 Decision-text 矛盾,advisory-only
- **Phase 0–2.5 sign_off_commit 字段 retrofit** —— 历史不完美接受,git log 已记录
- **EAASP 上游 sync 机制自动化** —— 上游团队独立项目
- **`docs/dev/WORK_LOG.md` 替换为 `.planning/STATE.md`** —— 二者并存,WORK_LOG 是 history,STATE 是 current
- **`docs/design/EAASP/DEFERRED_LEDGER.md` 迁入 GSD backlog** —— ledger 为 SSOT 保留(GSD 例外,Key Decision 已锁)

---

## Traceability

> Filled by `/gsd-roadmapper` 2026-04-26 — ROADMAP.md generated。每条 REQ-ID 1-to-1 映射到 ROADMAP.md `Phase Details` 中一个 phase。

| REQ-ID | Phase | Notes |
|--------|-------|-------|
| CLEANUP-01 | Phase 4.0 | L1_RUNTIME_ADAPTATION_GUIDE.md §4 chunk_type sweep + ADR-V2-021 provenance marker |
| CLEANUP-02 | Phase 4.0 | DEFERRED_LEDGER D120 status + row-edit-on-close convention(NEW-C6 顺手解决) |
| CLEANUP-03 | Phase 4.0 | docs/reviews/strategy-grid-two-leg-checklist.md 落地 |
| CLEANUP-04 | Phase 4.0 | .github/CODEOWNERS Leg-B dormancy reviewer 强制 |
| DECIDE-01 | Phase 4.1 | ADR-V2-023 §P5 触发条件 audit doc(可单文件 OR ADR-V2-024 草稿 §Audit) |
| DECIDE-02 | Phase 4.2 | .planning/phases/4.2/PLAN.md 反映 Leg A / Leg B / 两条腿 路径 |
| DECIDE-03 | Phase 4.2 | ADR-V2-024 Accepted (Proposed → Accepted 同 phase 走完) |
| GOVERNANCE-01 | Phase 4.0 | REVIEW_POLICY.md 与 cleanup 一起做 tracer-bullet 验证 GSD plumbing |
| GOVERNANCE-02 | Phase 4.1 | GSD discuss → plan → execute → review 端到端跑通本仓库 brownfield |
| GOVERNANCE-03 | Phase 4.1 | /gsd-resume-work 在本 phase 至少触发一次(/clear 实测 OR dry-run) |

---

*Requirements 来源:Phase 4a project review(2026-04-26) + 用户在 GSD 接管 questioning 中的 Active selection + ADR-V2-023 §P5 决策框架。*
