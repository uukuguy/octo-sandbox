# octo-sandbox 下一会话指南

**最后更新**: 2026-03-14 20:25 GMT+8
**当前分支**: `main`
**当前状态**: Phase A-I COMPLETE, Phase J/K PLANNED

---

## 项目状态：评估框架四级覆盖完成

评估框架 Phase A-I 全部完成。1992 tests passing @ `500e444`。
octo-eval 已具备完整的四级评估层次覆盖：

```
Level 4: 端到端任务成功率 (SWE-bench 50 tasks)     → ✅
Level 3: 多轮对话+工具链协调 (GAIA 50 + τ-bench 30) → ✅
Level 2: 单次工具调用精确度 (BFCL 50 tasks)          → ✅
Level 1: 引擎基础能力 (单元测试 1992 tests)           → ✅
```

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
| Phase H: 评估收官 | 10/10 | COMPLETE | `37680ec` |
| Phase I: 外部 Benchmark | 13/13 | COMPLETE | `500e444` |
| Phase J: Docker 修复 | 0/8 | PLANNED | — |
| Phase K: 模型报告 | 0/10 | PLANNED | — |

---

## 下一步：Phase J 或 Phase K

### Phase J: Docker 测试修复
- SWE-bench 从 mock 模式升级为真实 Docker 沙箱验证
- 修复 5 个 Docker 测试 (pre-existing failure)
- 实现 `SweVerifier` 的完整 Docker 验证管线

### Phase K: 多模型对比报告
- 跨 GAIA/SWE-bench/τ-bench 的多模型对比
- 生成评估报告 (JSON + Markdown)
- 模型排行榜

### 关键代码路径

| 组件 | 文件 | 说明 |
|------|------|------|
| 抽象层 | `src/benchmarks/mod.rs` | ExternalBenchmark + Registry |
| GAIA | `src/benchmarks/gaia.rs` | 50 tasks, L1-L3 |
| SWE-bench | `src/benchmarks/swe_bench.rs` | 50 tasks, 8 repos |
| τ-bench | `src/benchmarks/tau_bench.rs` | 30 tasks, pass^k=8 |
| 数据集 | `datasets/gaia_sample.jsonl` | GAIA 评估数据 |
| 数据集 | `datasets/swe_bench_lite.jsonl` | SWE-bench 评估数据 |
| 数据集 | `datasets/tau_bench_retail.jsonl` | τ-bench 评估数据 |
| CI | `.github/workflows/eval-ci.yml` | 含 3 个外部 benchmark 步骤 |

---

## 基线

- **Tests**: 1992 passing @ `500e444`
- **评估任务**: ~297 个 (内部 167 + 外部 130)
- **Benchmark**: GAIA (50) + SWE-bench (50) + τ-bench (30) + BFCL (50) + 内部 (117)
- **运行轨道**: Engine / CLI / Server
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **LLM 配置**: `.env` 中 OpenRouter 端点，不需要额外配置
