# Phase L — 评估系统白盒化 + 企业级数据集

**日期**: 2026-03-15
**前置**: Phase K NEAR_COMPLETE（695 次多模型评估、5-suite benchmark 报告）
**目标**: 把 octo-eval 从黑盒判官升级为白盒调试台，同时将评估焦点从 LLM 能力转向平台能力
**设计文档**: `docs/design/EVAL_WHITEBOX_DESIGN.md`

---

## 背景

Phase K 完成了首次 5 模型 × 5 维度 = 695 次真实评估，但暴露了两个根本性问题：

1. **黑盒问题**：评估只输出 pass/fail，无法回答"为什么失败"。29 个全模型失败的任务，无法判断是任务过难、scorer bug、还是 harness 限制。

2. **定位脱节**：139 个 benchmark 任务评的是"LLM 好不好"，但企业客户选 octo-sandbox 是因为平台能力（安全策略、容灾、MCP、沙箱）。100+ 企业能力仅 18% 有评估覆盖。

---

## 任务分组

### L1: Harness 白盒化基础

**L1-T1: TraceEvent 类型定义**

文件改动: `crates/octo-eval/src/trace.rs` (新建, ~100 行)

定义 `TraceEvent` 枚举，包含 10 种事件类型：RoundStart, LlmCall, Thinking, ToolCall, Error, SecurityBlocked, ContextDegraded, BudgetSnapshot, LoopGuardVerdict, Completed。

验证: `cargo check --workspace`

**L1-T2: collect_events 全量捕获**

文件改动: `crates/octo-eval/src/runner.rs` (~80 行改动)

将 `collect_events()` 从白名单 3 事件改为全量捕获 15+ 事件到 `Vec<TraceEvent>` timeline。保持 AgentOutput 向后兼容，timeline 作为额外字段。

验证: 现有测试全通过 + 新单元测试验证 timeline 捕获

**L1-T3: EvalTrace 升级**

文件改动: `crates/octo-eval/src/recorder.rs` (~30 行改动)

给 `EvalTrace` 添加 `timeline: Vec<TraceEvent>` 字段。trace 序列化时包含完整 timeline。

验证: trace 文件包含 timeline 数据

**L1-T4: loop_guard UTF-8 boundary fix**

文件改动: `crates/octo-engine/src/agent/loop_guard.rs` (~10 行)

修复第 560 行附近的字符串截断逻辑，使用 `floor_char_boundary()` 避免截断到多字节字符中间。

验证: 构造包含中文的 1000+ 字符字符串进行截断测试

---

### L2: 失败自动归因

**L2-T1: FailureClass 枚举定义**

文件改动: `crates/octo-eval/src/failure.rs` (新建, ~80 行)

定义 FailureClass 枚举，覆盖 3 类 12 种失败模式：基础设施问题（NetworkError, Timeout, RateLimit）、Harness 问题（ScorerMismatch, EmptyOutput, HarnessError）、真实能力差距（WrongTool, WrongArgs, ReasoningError, SecurityBypassed, SecurityOverblocked, ContextOverflow, LoopDetected, InsufficientRounds）。

验证: 编译通过

**L2-T2: FailureClassifier 实现**

文件改动: `crates/octo-eval/src/failure.rs` (~200 行)

实现 `FailureClassifier::classify(timeline, score) -> Option<FailureClass>`。按优先级依次检查：空输出 → 网络错误 → 超时 → 安全拦截 → 循环检测 → 上下文溢出 → 工具不匹配 → 推理错误 → 兜底。

验证: 单元测试覆盖每种 FailureClass 的触发条件

**L2-T3: EvalScore 集成 failure_class**

文件改动: `crates/octo-eval/src/score.rs` (~20 行) + `runner.rs` (~10 行)

给 EvalScore 添加 `failure_class: Option<FailureClass>` 字段。runner 在评分后自动调用 classifier。

验证: 现有测试全通过

**L2-T4: 报告集成失败摘要**

文件改动: `crates/octo-eval/src/benchmark.rs` (~50 行) + `reporter.rs` (~30 行)

comparison.md 和 benchmark.md 新增 `## Failure Summary` 节，按 FailureClass 分类统计失败分布。

验证: 重新聚合现有 benchmark 数据，输出包含失败摘要

---

### L3: 多维评分

**L3-T1: EvalScore.dimensions 字段**

文件改动: `crates/octo-eval/src/score.rs` (~30 行)

给 EvalScore 添加 `dimensions: HashMap<String, f64>` 字段。所有现有 scorer 返回空 HashMap（向后兼容）。

验证: 现有测试全通过，JSON 序列化包含 dimensions

**L3-T2: ToolCallScorer 多维化**

文件改动: `crates/octo-eval/src/scorer.rs` (~40 行)

ToolCallScorer 在返回 score 时同时填充 dimensions：`tool_selection` (0/1), `arg_accuracy` (0-1), `efficiency` (基于 rounds)。

验证: 单元测试验证维度值

**L3-T3: BehaviorScorer 多维化**

文件改动: `crates/octo-eval/src/scorer.rs` (~40 行)

BehaviorScorer 在返回 score 时填充 dimensions：`behavior_match` (0/1), `stop_reason_correct` (0/1)。

验证: 单元测试验证维度值

---

### L4: 新 Scorer + 平台评估数据集

**L4-T1: PlatformBehaviorScorer**

文件改动: `crates/octo-eval/src/scorer.rs` (~200 行)

新 scorer 类型，基于 timeline 事件评估平台级行为。支持 `expected_behavior`: security_blocked, failover_triggered, allowed, retry_triggered。返回多维分数。

验证: 单元测试覆盖每种 expected_behavior

**L4-T2: EventSequenceScorer**

文件改动: `crates/octo-eval/src/scorer.rs` (~150 行)

新 scorer 类型，验证 timeline 中事件序列的正确性和完整性。支持通配符匹配。返回 sequence_correctness, completion_ratio 维度。

验证: 单元测试覆盖序列匹配逻辑

**L4-T3: platform_security 评估集**

文件改动: `crates/octo-eval/data/octo_platform_security.jsonl` (新建, 15-20 tasks)

覆盖：SecurityPolicy Strict/Preferred/Development 模式、AutonomyLevel 切换、路径遍历防御、命令风险评估、ActionTracker 速率限制。

验证: `cargo run -p octo-eval -- list-suites` 显示新 suite

**L4-T4: provider_resilience 评估集**

文件改动: `crates/octo-eval/data/octo_provider_resilience.jsonl` (新建, 10-15 tasks)

覆盖：provider 500 错误 → failover、429 rate limit → 退避重试、连接超时 → 健康标记、成本归属验证。

需要：FaultProvider mock 扩展支持 fault injection 配置。

验证: `cargo run -p octo-eval -- list-suites` 显示新 suite

**L4-T5: Suite 注册 + CLI 集成**

文件改动: `crates/octo-eval/src/suites/` (新 suite 模块) + `main.rs` (~20 行)

注册 platform_security 和 provider_resilience 为可选 suite。CLI `run --suite platform_security` 可用。

验证: `cargo run -p octo-eval -- list-suites` + `cargo run -p octo-eval -- run --suite platform_security --dry-run`

---

### L5: 数据集清理 + 文档更新

**L5-T1: 现有数据集清理**

文件改动: `crates/octo-eval/data/octo_reasoning.jsonl` (删除或标记 deprecated)

操作:
- reasoning 6 任务：标记为 `"status": "deprecated"`（全 Hard 无区分度）
- tool_call L1 全通过任务：标记为 `"tier": "smoke_test"`（不计入 benchmark）
- security S1 全通过任务：标记为 `"tier": "smoke_test"`
- resilience RT/ES 全 0% 任务：标记为 `"status": "pending_review"`（待 harness 审查）
- context DG 全 0% 任务：标记为 `"status": "pending_review"`

验证: benchmark 聚合时正确排除 deprecated 和 smoke_test 任务

**L5-T2: EVAL_WHITEBOX_DESIGN.md 最终化**

确认设计文档与实现一致。更新任何实现中发现的偏差。

**L5-T3: EVAL_BASELINE_REPORT.md 更新**

用新的 failure_class 数据更新基线报告，增加失败归因分析节。

---

## 任务依赖图

```
L1-T1 (TraceEvent 定义)
  ↓
L1-T2 (collect_events 全量捕获)  →  L2-T2 (FailureClassifier)
  ↓                                    ↓
L1-T3 (EvalTrace 升级)             L2-T3 (EvalScore 集成)
                                       ↓
L1-T4 (UTF-8 fix) [独立]          L2-T4 (报告失败摘要)

L3-T1 (dimensions 字段) [独立]
  ↓
L3-T2 (ToolCall 多维)  }  可并行
L3-T3 (Behavior 多维)  }

L4-T1 (PlatformBehavior)  ← 依赖 L1-T2, L3-T1
L4-T2 (EventSequence)     ← 依赖 L1-T2, L3-T1
  ↓
L4-T3 (security 数据集)  }  依赖 L4-T1
L4-T4 (provider 数据集)  }  依赖 L4-T1
L4-T5 (Suite 注册)       ← 依赖 L4-T3, L4-T4

L5-T1 (数据集清理) [独立]
L5-T2, L5-T3 (文档) ← 依赖全部完成
```

## 执行顺序

**并行组 1** (无依赖):
- L1-T1 + L1-T4 + L2-T1 + L3-T1 + L5-T1

**顺序组 2** (依赖组 1):
- L1-T2 → L1-T3

**顺序组 3** (依赖组 2):
- L2-T2 → L2-T3 → L2-T4
- L3-T2 + L3-T3 (并行)

**顺序组 4** (依赖组 3):
- L4-T1 + L4-T2 (并行)

**顺序组 5** (依赖组 4):
- L4-T3 + L4-T4 (并行) → L4-T5

**最后**:
- L5-T2 + L5-T3

---

## 预期产出

| 产出 | 说明 |
|------|------|
| TraceEvent timeline | 完整事件流，可回答"agent 做了什么、想了什么、遇到了什么" |
| FailureClassifier | 自动归因 12 种失败模式，>80% 覆盖率 |
| MultiDimScore | 每任务 3-5 维度评分，不再是纯 pass/fail |
| PlatformBehaviorScorer | 评估安全策略、容灾、审计等平台行为 |
| EventSequenceScorer | 评估 MCP/多步骤全链路正确性 |
| platform_security suite | 15-20 个企业安全评估任务 |
| provider_resilience suite | 10-15 个容灾评估任务 |
| 数据集清理 | 去除/降级 ~30 个无价值任务 |
| UTF-8 bug fix | 解除 Claude 评估时的 crash |

---

## 成本与风险

| 项目 | 估计 |
|------|------|
| 代码改动量 | ~1200 行新增, ~100 行修改 |
| 新测试 | ~30-40 个单元测试 |
| API 费用 | 首次验证运行 ~$3-5 |
| 风险: timeline 数据量 | TraceEvent 可能导致 trace 文件膨胀 → 加截断限制 |
| 风险: scorer 兼容性 | 新字段可能影响 JSON 反序列化 → 全部用 `#[serde(default)]` |
| 风险: SecurityBlocked 事件缺失 | Engine 可能未发射此事件 → 需检查并补充 |

---

## 完成标准

- [ ] `cargo test --workspace -- --test-threads=1` 全通过（baseline + 新测试）
- [ ] `cargo run -p octo-eval -- list-suites` 显示 platform_security, provider_resilience
- [ ] trace 文件包含完整 timeline（至少包含 ToolCall + Thinking + Error 事件）
- [ ] benchmark.md 包含 `## Failure Summary` 节
- [ ] EvalScore JSON 包含 `dimensions` 和 `failure_class` 字段
- [ ] loop_guard 不再 panic on 中文字符截断
