# octo-sandbox 下一会话指南

**最后更新**: 2026-03-14 18:20 GMT+8
**当前分支**: `main`
**当前状态**: Phase A-H COMPLETE, Phase I-K 计划已设计，待执行

---

## 项目状态：评估框架收官完成，进入 SWE-bench 阶段

评估框架 Phase A-H 全部完成。1979 tests passing @ `37680ec`。
octo-eval 已具备 10 个 Suite、11 种 Scorer、~248 评估任务、3 种运行轨道、11 种行为类型。
Phase I-K 计划文档已设计完成，进入执行阶段。

### 完成清单

| 阶段 | Tasks | 状态 | Commit |
|------|-------|------|--------|
| Wave 1-10: v1.0-v1.1 | 全部 | COMPLETE | `675155d` |
| Phase A: 轨道 A 特色评估 | 8/8 | COMPLETE | `e490da3` |
| Phase B: 评估手册 | 全部 | COMPLETE | `90017dc` |
| Phase C: octo-eval crate | 全部 | COMPLETE | `24b02d4` |
| Phase D: 多模型对比 | 10/10 | COMPLETE | `998f3b4` |
| Phase E: 评估增强 | 18/18 | COMPLETE | `3e11905` |
| Phase F: 评估任务集 | 20/23 | COMPLETE | `b4d1cd2` |
| Phase G: Deferred 补齐 | 9/9 | COMPLETE | `ca5c898` |
| **Phase H: 评估收官** | **10/10** | **COMPLETE** | **`37680ec`** |
| Phase I: SWE-bench | 0/12 | PLANNED | — |
| Phase J: Docker 修复 | 0/8 | PLANNED | — |
| Phase K: 模型报告 | 0/10 | PLANNED | — |

---

## 下一步：执行 Phase I (SWE-bench)

### 计划文档

| Phase | 文件 | 内容 |
|-------|------|------|
| **I** | `docs/plans/2026-03-14-phase-i-swebench.md` | 完整 SWE-bench 适配 |
| J | `docs/plans/2026-03-14-phase-j-docker-tests.md` | Docker 测试修复 |
| K | `docs/plans/2026-03-14-phase-k-model-benchmark.md` | 完整真实模型对比报告 |

### 启动命令

```bash
# 开始 Phase I
/start-phase "Phase I — SWE-bench"
```

---

## 基线

- **Tests**: 1979 passing @ `37680ec`
- **评估任务**: ~248 个 (10 Suite, 11 Scorer, 11 Behavior)
- **运行轨道**: Engine / CLI / Server
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **LLM 配置**: `.env` 中 OpenRouter 端点，不需要额外配置

## Phase H 交付物

- `crates/octo-eval/src/scorer.rs` — AstMatchScorer + 4 resilience behaviors
- `crates/octo-eval/src/score.rs` — AstMatch ScoreDetails variant
- `crates/octo-eval/src/datasets/loader.rs` — AST match + behavior scoring sync
- `crates/octo-eval/src/suites/resilience.rs` — ResilienceSuite 模块
- `crates/octo-eval/datasets/octo_resilience.jsonl` — 20 resilience 任务
- `crates/octo-eval/datasets/octo_context.jsonl` — 50 context 任务 (was 14)
- `crates/octo-eval/datasets/octo_tool_call.jsonl` — +10 AST 匹配任务
- `.github/workflows/eval-ci.yml` — resilience suite CI 集成
