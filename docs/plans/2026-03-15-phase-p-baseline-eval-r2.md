# Phase P — Baseline Evaluation Round 2（Agentic Benchmark 对标评估）

**创建时间**: 2026-03-15 19:30 GMT+8
**状态**: IN PROGRESS
**前置**: Phase O COMPLETE (2178 tests @ 9da42de)

---

## 目标

将评估体系从 LLM 能力测试重心转向 **自主智能体（Agentic）能力评估**，采用业界主流 Agentic Benchmark 作为对标层，结合 octo 自有能力评估维度，对 6 个模型执行全量基线评估。

### 评估架构：双层设计

| 层级 | 名称 | 评估维度 | 数据来源 |
|------|------|----------|----------|
| **对标层** | Agentic Benchmark Alignment | GAIA (多步推理)、SWE-bench (代码修复)、τ-bench (行为可靠性)、Terminal-Bench (终端操作)、BFCL (函数调用) | 业界标准数据集 |
| **自有层** | Octo Proprietary Capabilities | security (安全策略)、context (上下文管理)、resilience (容错恢复) | octo 自建数据集 |

### 模型矩阵（6 模型）

| 模型 | Tier | 输入成本/1M | 输出成本/1M |
|------|------|------------|------------|
| Qwen3.5-27B | economy | $0.20 | $1.56 |
| MiniMax-M2.5 | standard | $0.25 | $1.20 |
| Qwen3.5-122B | standard | $0.26 | $2.08 |
| DeepSeek-V3.2 | standard | $0.26 | $0.38 |
| Kimi-K2.5 | high | $0.45 | $2.20 |
| Claude-Sonnet-4.6 | flagship | $3.00 | $15.00 |

---

## 任务清单

### P1: eval.benchmark.toml 模型矩阵更新 ✅ DONE

- [x] 替换 Qwen3-30B → Qwen3.5-27B
- [x] 新增 MiniMax-M2.5
- [x] 保留 Qwen3.5-122B、DeepSeek-V3.2、Kimi-K2.5、Claude-Sonnet-4.6
- [x] 移除 Gemini-3-Pro

### P2: Terminal-Bench 适配器实现 ✅ DONE

- [x] P2-T1: 创建 `src/benchmarks/terminal_bench.rs` — 实现 ExternalBenchmark trait
  - 任务类型：终端命令编排、文件操作、系统管理
  - 评分：命令序列匹配 (60%) + 输出验证 (40%) - 禁止模式惩罚
  - 难度分级：L1 单命令、L2 管道组合、L3 多步编排
- [x] P2-T2: 创建 `datasets/terminal_bench.jsonl` — 30 终端操作评估任务 (L1×10, L2×10, L3×10)
- [x] P2-T3: 注册到 BenchmarkRegistry (`src/benchmarks/mod.rs`) + ScoreDetails::TerminalBench variant
- [x] P2-T4: 测试覆盖 (5 tests: deserialize, difficulty, scoring_pass, scoring_forbidden, trait)

### P3: 默认评估套件更新 ✅ DONE

- [x] P3-T1: 更新 `main.rs:764` 默认 benchmark 套件列表
  - 旧: `"tool_call,security,bfcl,context,resilience,reasoning"`
  - 新: `"gaia,swe_bench,tau_bench,terminal_bench,bfcl,security,context,resilience"`
- [x] P3-T2: reasoning 套件由 GAIA L1-L3 替代（reasoning 仍可手动指定）
- [x] P3-T3: load_suite() 通过 BenchmarkRegistry 自动路由 terminal_bench

### P4: 编译与测试验证 ✅ DONE

- [x] P4-T1: `cargo check --workspace` 编译通过
- [x] P4-T2: `cargo test --workspace -- --test-threads=1` 全量通过 (2183 tests, +5 from baseline 2178)

### P5: 执行基线评估

- [ ] P5-T1: 执行 8 suite × 6 model 全量评估
  - 命令: `cargo run -p octo-eval -- benchmark --config crates/octo-eval/eval.benchmark.toml --output eval_output/benchmark-p`
- [ ] P5-T2: 验证结果完整性（48 组合全部有输出）
- [ ] P5-T3: 生成对比报告

### P6: 报告与归档

- [ ] P6-T1: 更新 `docs/design/EVAL_BASELINE_REPORT.md` — Round 2 基线数据
- [ ] P6-T2: 更新 NEXT_SESSION_GUIDE.md
- [ ] P6-T3: 更新 .phase_stack.json 归档

---

## 关键变更：reasoning → GAIA

Phase K 的 reasoning 评估维度使用 6 个自编 Hard 难度任务（均已标记 deprecated），存在以下问题：
1. 任务数量不足、难度单一
2. 依赖不存在的文件路径
3. 无法与业界对标

**替代方案**: 使用 GAIA L1/L2/L3 作为多步推理评估维度。GAIA 已有完整适配器和 50 个任务（Phase I 实现），覆盖单步→多步→长链推理三个难度级别。

## 已知限制

| 项目 | 说明 | 影响 |
|------|------|------|
| SWE-bench mock 验证 | 当前 `score()` 仅检查 patch 格式（diff 格式匹配得 0.5 分），完整 Docker 验证待后续增强 | 分数反映 patch 生成能力，非完整修复验证 |
| GAIA 样本量 | 当前 50 个 sample 任务，非完整 GAIA 数据集 | 足够基线对比，后续可扩充 |

## 执行顺序

P1 ✅ → P2 ✅ → P3 ✅ → P4 ✅ → P5 → P6

## 预估

- **新代码**: ~300 行（Terminal-Bench 适配器 + 数据集）
- **评估任务总数**: ~327 个（现有 297 + Terminal-Bench ~30）
- **评估维度**: 8 个（5 对标 + 3 自有）
- **评估组合**: 48 组（8 suite × 6 model）
