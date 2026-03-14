# octo-sandbox 下一会话指南

**最后更新**: 2026-03-14 23:30 GMT+8
**当前分支**: `main`
**当前状态**: Phase K IN_PROGRESS — 代码任务完成，等待用户执行模型评估

---

## 项目状态：Benchmark 代码框架已就绪

评估框架 Phase A-J 全部完成，Phase K 代码任务（BenchmarkAggregator + CLI + CI）已提交。
剩余任务需要用户用真实 LLM API 执行模型评估。

```
Level 4: 端到端任务成功率 (SWE-bench 50 tasks)     → ✅
Level 3: 多轮对话+工具链协调 (GAIA 50 + τ-bench 30) → ✅
Level 2: 单次工具调用精确度 (BFCL 50 tasks)          → ✅
Level 1: 引擎基础能力 (单元测试 2021 tests)           → ✅
沙箱安全: SandboxPolicy + 审计日志                   → ✅ Phase J COMPLETE
Benchmark: BenchmarkAggregator + CLI                 → ✅ K1/K3/K4 代码完成
模型评估: 5 模型 x 6+ Suite 真实对比                 → ⏳ 等待用户执行
```

### 完成清单

| 阶段 | Tasks | 状态 | Commit |
|------|-------|------|--------|
| Wave 1-10: v1.0-v1.1 | 全部 | COMPLETE | `675155d` |
| Phase A-I: 评估框架 | 全部 | COMPLETE | `500e444` |
| Phase J: 沙箱安全体系 | 16/16 | COMPLETE | `bc25fbd` |
| **Phase K: 模型报告** | **5/12** | **IN_PROGRESS** | `6b68deb` |

---

## Phase K: 用户执行步骤

### Step 1: 验证模型连通性 (K1-T2)

```bash
# 对每个模型跑 1 个简单任务验证连通性
cargo run -p octo-eval -- compare --suite tool_call \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/connectivity_test
```

如果某个模型失败，编辑 `eval.benchmark.toml` 调整配置。

### Step 2: 核心 Suite 对比 (K2-T1)

```bash
# 工具调用
cargo run -p octo-eval -- compare --suite tool_call \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/benchmark/tool_call

# 安全
cargo run -p octo-eval -- compare --suite security \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/benchmark/security

# BFCL
cargo run -p octo-eval -- compare --suite bfcl \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/benchmark/bfcl
```

### Step 3: 差异化 Suite 对比 (K2-T2)

```bash
cargo run -p octo-eval -- compare --suite context \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/benchmark/context

cargo run -p octo-eval -- compare --suite resilience \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/benchmark/resilience

cargo run -p octo-eval -- compare --suite reasoning \
  --config crates/octo-eval/eval.benchmark.toml \
  --output eval_output/benchmark/reasoning
```

### Step 4: 一键汇总报告

```bash
# 从已有的 comparison.json 汇总
cargo run -p octo-eval -- benchmark \
  --input eval_output/benchmark \
  --output eval_output/benchmark/final
```

### Step 5: 或者一键全跑

```bash
cargo run -p octo-eval -- benchmark \
  --config crates/octo-eval/eval.benchmark.toml \
  --suites tool_call,security,bfcl,context,resilience,reasoning \
  --output eval_output/benchmark
```

### 评估完成后

用户提供评估数据后，AI 将：
1. K4-T1: 录制 Replay 基线
2. K5-T1: 生成 `docs/design/EVAL_BASELINE_REPORT.md`
3. K5-T2: 更新 `docs/design/AGENT_EVALUATION_DESIGN.md` 第六节

---

## 基线

- **Tests**: 2021 passing @ `6b68deb` (基线 2014，+7 新增)
- **评估任务**: ~297 个 (内部 167 + 外部 130)
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **LLM 配置**: `.env` 中 OpenRouter 端点
