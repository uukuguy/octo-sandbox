# octo-sandbox 下一会话指南

**最后更新**: 2026-03-14 17:30 GMT+8
**当前分支**: `main`
**当前状态**: Phase A-G COMPLETE, Phase H-K 计划已设计，待执行

---

## 项目状态：评估框架建设完成，进入收官阶段

评估框架 Phase A-G 全部完成。1962 tests passing @ `ca5c898`。
octo-eval 已具备 9 个 Suite、10 种 Scorer、~218 评估任务、3 种运行轨道。
Phase H-K 计划文档已设计完成，进入执行阶段。

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
| **Phase H: 评估收官** | **0/10** | **PLANNED** | — |
| Phase I: SWE-bench | 0/12 | PLANNED | — |
| Phase J: Docker 修复 | 0/8 | PLANNED | — |
| Phase K: 模型报告 | 0/10 | PLANNED | — |

---

## 下一步：执行 Phase H

### 计划文档

| Phase | 文件 | 内容 |
|-------|------|------|
| **H** | `docs/plans/2026-03-14-phase-h-eval-capstone.md` | 特色评估 + AstMatch + Context 扩充 |
| I | `docs/plans/2026-03-14-phase-i-swebench.md` | 完整 SWE-bench 适配 |
| J | `docs/plans/2026-03-14-phase-j-docker-tests.md` | Docker 测试修复 |
| K | `docs/plans/2026-03-14-phase-k-model-benchmark.md` | 完整真实模型对比报告 |

### Phase H 任务概览

| 任务组 | 内容 | 预计新增 |
|--------|------|---------|
| H1 | 4 种新 Behavior + resilience Suite + 20 JSONL 案例 | ~90 行 |
| H2 | Context 案例扩充至 50+ 任务 | ~20 行 |
| H3 | AstMatch Scorer + 10 AST 匹配案例 | ~100 行 |
| H4 | 测试 + CI + CLI 更新 | ~50 行 |

### 启动命令

```bash
# 恢复计划执行
/resume-plan

# 或直接开始 Phase H
/start-phase "Phase H — Eval Capstone"
```

---

## 基线

- **Tests**: 1962 passing @ `ca5c898`
- **评估任务**: ~218 个 (9 Suite, 10 Scorer)
- **运行轨道**: Engine / CLI / Server
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **LLM 配置**: `.env` 中 OpenRouter 端点，不需要额外配置
