# Phase K — 完整真实模型对比报告

**日期**: 2026-03-14
**前置**: Phase J COMPLETE（Docker 测试修复）
**目标**: 跑通完整模型评估矩阵，产出首份有价值的多模型对比报告，为企业选型提供数据支撑

---

## 背景

经过 Phase A-J 的建设，octo-eval 已具备：
- **11 个 Suite**: tool_call, security, context, output_format, tool_boundary, reasoning, resilience, bfcl, e2e, provider, memory, swe_bench
- **11+ 种 Scorer**: ExactMatch, ToolCall, AstMatch, Behavior(11 种), Sequence, SequenceWithArgs, FunctionCallMatch, LlmJudge, Regex, NotContains, ContainsAll, PatchVerify, SweVerify
- **~270+ 评估任务**: 覆盖 10+ 维度
- **3 种运行轨道**: Engine / CLI / Server
- **Replay 零成本回归**: 录制/回放机制
- **ComparisonRunner**: 多模型对比框架

所有基础设施就绪，但从未跑过真实模型对比。本 Phase 补齐这一最后缺口。

---

## 一、模型选择策略

### 1.1 选型原则（用户确认）

- 固定 OpenRouter 端点（`.env` 中 `OPENAI_BASE_URL`）
- 企业可私有部署的分层开源模型
- 对标性能和性价比的顶级模型
- 至少 3 层对比

### 1.2 模型矩阵

参考设计文档第六节和 OpenRouter 最新数据：

| 层级 | 模型 | 定位 | 成本 ($/1M in/out) | 可私有部署 | 评估角色 |
|------|------|------|-------------------|-----------|---------|
| **T0 免费** | Qwen3 Coder 480B A35B | CI 回归 | $0/$0 | 是 | 基线/最低成本 |
| **T1 经济** | DeepSeek V3.2 | 日常编码 | $0.15/$0.75 | 是 | 经济型代表 |
| **T2 标准** | Qwen3.5-122B-A10B | 生产主力 | $0.30/$1.20 | 是 | 标准型代表 |
| **T3 高性能** | Kimi K2.5 | 复杂推理 | $0.45/$2.20 | 是 | 高性能代表 |
| **T4 旗舰** | Claude Sonnet 4.6 | 能力天花板 | $3/$15 | 否 | 旗舰对照 |

### 1.3 评估配置 (eval.toml)

```toml
[default]
timeout_secs = 120
concurrency = 1
record_traces = true
output_dir = "eval_output/benchmark"

[[models]]
name = "Qwen3-Coder-480B"
provider = "openai"
model = "qwen/qwen3-coder-480b-a35b"
tier = "free"
cost_per_1m_input = 0.0
cost_per_1m_output = 0.0

[[models]]
name = "DeepSeek-V3.2"
provider = "openai"
model = "deepseek/deepseek-chat-v3-0324"
tier = "economy"
cost_per_1m_input = 0.15
cost_per_1m_output = 0.75

[[models]]
name = "Qwen3.5-122B"
provider = "openai"
model = "qwen/qwen3.5-122b-a10b"
tier = "standard"
cost_per_1m_input = 0.30
cost_per_1m_output = 1.20

[[models]]
name = "Kimi-K2.5"
provider = "openai"
model = "moonshotai/kimi-k2.5"
tier = "high"
cost_per_1m_input = 0.45
cost_per_1m_output = 2.20

[[models]]
name = "Claude-Sonnet-4.6"
provider = "anthropic"
model = "anthropic/claude-sonnet-4-6"
tier = "flagship"
cost_per_1m_input = 3.0
cost_per_1m_output = 15.0
```

注: 所有 `provider = "openai"` 的模型通过 OpenRouter 的 OpenAI 兼容接口接入（`OPENAI_BASE_URL`），Claude 通过 OpenRouter 的 Anthropic 路由接入（同一个 endpoint）。

---

## 二、评估维度矩阵

### 2.1 核心评估 Suite

| Suite | 任务数 | 评估内容 | 成本/模型 |
|-------|--------|---------|----------|
| **tool_call** | ~58 | 工具调用精确度 (L1-L5 + AST) | ~$0.5-5 |
| **security** | ~39 | 安全策略执行力 (S1-S5) | ~$0.3-3 |
| **bfcl** | ~49 | BFCL 函数调用标准基准 | ~$0.5-5 |
| **reasoning** | ~6 | 多步推理与规划 (LlmJudge) | ~$0.2-2 |

### 2.2 差异化评估 Suite

| Suite | 任务数 | 评估内容 | 成本/模型 |
|-------|--------|---------|----------|
| **context** | ~53 | 上下文管理与降级 | ~$0.5-5 |
| **resilience** | ~20 | 弹性恢复能力 | ~$0.2-2 |
| **output_format** | ~6 | 结构化输出格式 | ~$0.1-1 |
| **tool_boundary** | ~8 | 工具边界感知 | ~$0.1-1 |

### 2.3 端到端评估

| Suite | 任务数 | 评估内容 | 成本/模型 |
|-------|--------|---------|----------|
| **swe_bench** | 50 | SWE-bench 代码修复 | ~$5-50 |
| **e2e** | 14 | E2E bug-fix (Python+Rust) | ~$1-10 |

### 2.4 成本估算

| 运行范围 | 5 模型总成本 | 说明 |
|----------|------------|------|
| 核心 Suite (152 tasks × 5) | ~$10-30 | 必跑 |
| 差异化 Suite (87 tasks × 5) | ~$5-15 | 必跑 |
| SWE-bench (50 tasks × 5) | ~$30-150 | 可选，仅跑 2-3 模型 |
| E2E (14 tasks × 5) | ~$5-25 | 需真实 Agent Loop |
| **合计** | **~$50-220** | |

---

## 三、任务分组

### K1: 评估配置准备

**K1-T1: 创建 benchmark 专用 eval.toml**

新文件: `crates/octo-eval/eval.benchmark.toml`

内容如上 1.3 节所示。包含 5 个模型的完整配置。

**K1-T2: 验证模型连通性**

运行时验证任务（用户执行）：

```bash
# 验证每个模型的连通性 — 对每个模型跑 1 个简单 tool_call 任务
cargo run -p octo-eval -- run --suite tool_call --config eval.benchmark.toml --output eval_output/connectivity_test
```

如果某个模型连接失败，调整 eval.toml 中的模型配置。

### K2: 分阶段对比评估

> 按成本从低到高分阶段执行，每阶段确认结果后再进入下一阶段

**K2-T1: 第一阶段 — 核心 Suite 对比**

运行时验证任务（用户执行）：

```bash
# 工具调用对比 (~$10)
cargo run -p octo-eval -- compare --suite tool_call --config eval.benchmark.toml --output eval_output/benchmark/tool_call

# 安全对比 (~$5)
cargo run -p octo-eval -- compare --suite security --config eval.benchmark.toml --output eval_output/benchmark/security

# BFCL 对比 (~$10)
cargo run -p octo-eval -- compare --suite bfcl --config eval.benchmark.toml --output eval_output/benchmark/bfcl
```

产出: `comparison.json` + `comparison.md` 报告

**K2-T2: 第二阶段 — 差异化 Suite 对比**

```bash
# 上下文管理
cargo run -p octo-eval -- compare --suite context --config eval.benchmark.toml --output eval_output/benchmark/context

# 弹性恢复
cargo run -p octo-eval -- compare --suite resilience --config eval.benchmark.toml --output eval_output/benchmark/resilience

# 推理能力
cargo run -p octo-eval -- compare --suite reasoning --config eval.benchmark.toml --output eval_output/benchmark/reasoning
```

**K2-T3: 第三阶段 — SWE-bench（可选，高成本）**

仅选择 3 个代表性模型（T1/T2/T4）：

```bash
# 修改 eval.toml 仅保留 3 个模型
cargo run -p octo-eval -- compare --suite swe_bench --config eval.benchmark.toml --output eval_output/benchmark/swe_bench
```

### K3: 报告汇总与分析

**K3-T1: 实现 BenchmarkAggregator**

新文件: `crates/octo-eval/src/benchmark.rs` (~150 行)

将多个 suite 的对比结果汇总为一份完整报告：

```rust
pub struct BenchmarkAggregator;

impl BenchmarkAggregator {
    /// 汇总多个 suite 的 ComparisonReport 为完整 benchmark
    pub fn aggregate(
        suite_reports: Vec<(&str, ComparisonReport)>,
    ) -> BenchmarkReport { ... }
}

pub struct BenchmarkReport {
    pub models: Vec<ModelBenchmark>,
    pub dimension_matrix: HashMap<String, HashMap<String, f64>>,  // model → suite → pass_rate
    pub cost_analysis: CostAnalysis,
    pub recommendations: Vec<Recommendation>,
}

pub struct ModelBenchmark {
    pub info: ModelInfo,
    pub overall_pass_rate: f64,
    pub overall_avg_score: f64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    pub per_suite: HashMap<String, SuiteResult>,
}

pub struct CostAnalysis {
    pub cost_per_model: HashMap<String, f64>,
    pub cost_effectiveness: HashMap<String, f64>,  // pass_rate / cost
    pub cheapest_acceptable: Option<String>,       // pass_rate > 80% 的最便宜模型
}

pub struct Recommendation {
    pub scenario: String,          // "cost_sensitive", "balanced", "performance_first"
    pub recommended_model: String,
    pub reasoning: String,
}
```

**K3-T2: 实现 Markdown 综合报告生成**

在 `benchmark.rs` 中实现 `to_markdown()`：

```markdown
# Octo Agent Benchmark Report

## 总览

| 模型 | 层级 | 工具调用 | 安全 | BFCL | 上下文 | 弹性 | 推理 | 综合 | 成本 | 性价比 |
|------|------|---------|------|------|--------|------|------|------|------|--------|
| DeepSeek V3.2 | T1 | ?% | ?% | ?% | ?% | ?% | ?% | ?% | $? | ? |
| Qwen3.5-122B | T2 | ?% | ?% | ?% | ?% | ?% | ?% | ?% | $? | ? |
| Kimi K2.5 | T3 | ?% | ?% | ?% | ?% | ?% | ?% | ?% | $? | ? |
| Claude Sonnet | T4 | ?% | ?% | ?% | ?% | ?% | ?% | ?% | $? | ? |

## 维度分析

### 模型能力敏感度

T1 vs T4 差距大的维度 → 对模型能力敏感，需要更好的 prompt/策略
T1 vs T4 差距小的维度 → 可以放心用便宜模型

### 推荐

| 场景 | 推荐模型 | 理由 |
|------|---------|------|
| 成本敏感 | ? | ? |
| 平衡型 | ? | ? |
| 性能优先 | ? | ? |
```

**K3-T3: CLI 集成 benchmark 子命令**

文件改动: `crates/octo-eval/src/main.rs` (~30 行)

新增 `benchmark` 命令：
```bash
# 汇总已有报告
cargo run -p octo-eval -- benchmark --input eval_output/benchmark --output eval_output/benchmark/final

# 或一键全跑（顺序跑所有 suite 的 compare）
cargo run -p octo-eval -- benchmark --config eval.benchmark.toml --suites tool_call,security,bfcl,context,resilience,reasoning --output eval_output/benchmark
```

### K4: Replay 基线建立

**K4-T1: 录制评估 Trace 作为回归基线**

首次真实模型评估完成后，trace 自动保存到 `eval_output/benchmark/traces/`。

将最佳模型的 trace 复制为回归基线：
```bash
cp -r eval_output/benchmark/traces eval_output/replay_baseline
```

后续 CI 用 Replay 模式零成本回归：
```bash
cargo run -p octo-eval -- run --suite tool_call --replay eval_output/replay_baseline --baseline eval_output/benchmark/tool_call/report.json
```

**K4-T2: 更新 CI 集成回归基线**

文件改动: `.github/workflows/eval-ci.yml` (~10 行)

```yaml
- name: Run regression against benchmark baseline
  if: hashFiles('eval_output/replay_baseline/traces/*.json') != ''
  run: |
    cargo run -p octo-eval -- run --suite tool_call \
      --replay eval_output/replay_baseline \
      --baseline eval_output/benchmark/tool_call/report.json \
      --output eval_output/regression
```

### K5: 文档产出

**K5-T1: 生成 EVAL_BASELINE_REPORT.md**

运行所有评估完成后，将最终的 Markdown 报告保存为设计文档：

新文件: `docs/design/EVAL_BASELINE_REPORT.md`

内容（中文，按文档规范）：
1. 评估概述：模型矩阵、Suite 覆盖、总任务数
2. 详细结果：每个维度的跨模型对比表
3. 维度分析：能力敏感度分析
4. 成本分析：各模型的 token 消耗和实际成本
5. 企业选型推荐：3 种场景的最优选择
6. 附录：评估环境配置、可复现命令

**K5-T2: 更新 AGENT_EVALUATION_DESIGN.md**

在设计文档第六节的模型评估矩阵中填入实际数据，替换所有 `?%`。

---

## 四、文件改动矩阵

| 文件 | 操作 | 行数估计 |
|------|------|---------|
| `crates/octo-eval/eval.benchmark.toml` | **新建** | ~50 |
| `crates/octo-eval/src/benchmark.rs` | **新建** | ~150 |
| `crates/octo-eval/src/lib.rs` | 修改 | +1 |
| `crates/octo-eval/src/main.rs` | 修改 | +30 |
| `.github/workflows/eval-ci.yml` | 修改 | +10 |
| `docs/design/EVAL_BASELINE_REPORT.md` | **新建** | ~200 (由评估数据生成) |
| `docs/design/AGENT_EVALUATION_DESIGN.md` | 修改 | ~20 (填入数据) |

**总计**: 3 新文件, 4 修改, ~460 行新增代码 + 报告文档

---

## 五、执行顺序与依赖

```
K1-T1 (eval.toml) ─► K1-T2 (连通性验证，用户执行)
                            │
                            ▼
                     K2-T1 (核心 Suite 对比，用户执行)
                            │
                            ▼
                     K2-T2 (差异化 Suite 对比，用户执行)
                            │
                            ▼
                     K2-T3 (SWE-bench 对比，可选，用户执行)
                            │
                            ▼
K3-T1 (BenchmarkAggregator) ─► K3-T2 (Markdown 报告) ─► K3-T3 (CLI benchmark)
                                                              │
                                                              ▼
                                                      K4-T1 (Replay 基线)
                                                              │
                                                              ▼
                                                      K4-T2 (CI 回归)
                                                              │
                                                              ▼
                                                      K5-T1 (报告文档)
                                                              │
                                                              ▼
                                                      K5-T2 (设计文档更新)
```

**关键路径**: K2 的评估执行是用户操作，需要实际运行和等待 LLM 响应

---

## 六、验收标准

### 代码验收
- [ ] `BenchmarkAggregator` 正确汇总多 suite 报告
- [ ] `cargo run -p octo-eval -- benchmark --help` 显示使用说明
- [ ] eval.benchmark.toml 包含 5 层模型配置
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过

### 评估数据验收
- [ ] 至少 4 个 suite × 3 个模型的完整对比数据
- [ ] 模型评估矩阵中 `?%` 全部填入真实数据
- [ ] 成本分析包含实际 token 消耗
- [ ] 每个维度的跨模型差异有清晰的分析

### 文档验收
- [ ] `EVAL_BASELINE_REPORT.md` 包含完整的评估报告（中文）
- [ ] 报告包含企业选型推荐（3 种场景）
- [ ] Replay 基线已建立，CI 可零成本回归
- [ ] `AGENT_EVALUATION_DESIGN.md` 第六节数据已更新

---

## 七、风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| OpenRouter API 限流 | 评估中断 | 设置 concurrency=1，加 retry 延迟 |
| 某模型不支持 tool_use | 部分 suite 跳过 | 在报告中标注 N/A |
| 成本超预算 | - | 先跑核心 Suite，确认后再扩展 |
| 模型响应不稳定 | 分数波动 | 对关键任务跑 3 次取平均 |
| SWE-bench 容器构建慢 | 评估耗时长 | SWE-bench 仅选 2-3 模型跑 |
