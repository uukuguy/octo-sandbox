# Strategy: Grid 两腿战略 PR Review Checklist

> **触发条件**: PR 触碰 `crates/grid-server/` / `crates/grid-platform/` / `crates/grid-desktop/` / `web/` / `web-platform/` 任一路径时，reviewer 复制本清单到 PR comment 逐项 check（CODEOWNERS 自动指派 @firmwwwee）
> **Source of truth**: `docs/design/EAASP/adrs/ADR-V2-023-grid-two-leg-product-strategy.md` §Enforcement L213-228
> **Last sync**: 2026-04-27 (Phase 4.0 CLEANUP-03 initial creation)

---

## Checklist (5 items, ADR-V2-023 §Enforcement L224-228 verbatim)

- [ ] 此 PR 是否触碰 `grid-platform` 或 `web*`？如是，是否必须（见 P2 原则，原则上当前不应动）？
- [ ] 此 PR 在 `grid-engine` / `grid-runtime` / `grid-types` / `grid-sandbox` / `grid-hook-bridge` 任何一个里加了只服务某一条腿的分支？（违反 P1）
- [ ] 此 PR 在 `tools/eaasp-*/` 里实现了 Grid 独有业务逻辑？（违反 P3）
- [ ] 此 PR 是否声称"为腿 B 准备"而实际属于 P5 未触发的预热代码？（违反 P2）
- [ ] 如果 P5 触发条件满足，是否创建了对应的 ADR 和 Phase plan？

## Reviewer 工作流

1. PR 触碰 5 路径之一 → CODEOWNERS auto-assign @firmwwwee 为 reviewer
2. Reviewer 在 PR 描述中确认任务背景属于 ADR-V2-023 §P5 4 条触发条件之一（否则按 dormancy 拒绝）
3. 复制本 5 条 checklist 到 PR comment, 逐项 ☑
4. 任一未 ☑ → block merge, 推动作者补 dormancy-justification
5. 全部 ☑ → approve

## 参考

- ADR-V2-023 (`docs/design/EAASP/adrs/ADR-V2-023-grid-two-leg-product-strategy.md`)
  - §P1 — Core first, package later
  - §P2 — 腿 A 当前绝对优先 (5 条 dormant 路径)
  - §P5 — 腿 B 激活触发条件
  - §Enforcement — 本 checklist 的 source of truth
- `.github/CODEOWNERS` (本文件 trigger 入口)
- `.planning/REVIEW_POLICY.md` (Grid 整体 review tier; 本 checklist 与 high-risk superpowers two-stage **正交不重叠**)
