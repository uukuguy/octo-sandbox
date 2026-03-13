# Phase D — 多模型对比评估

**日期**: 2026-03-13
**前置**: Phase C octo-eval Crate (COMPLETE @ 24b02d4)
**目标**: 补齐多模型对比基础设施，使 octo-eval 能跑 T1-T4 模型矩阵并产出对比报告

---

## 缺口分析

Phase C 交付了单模型评估全流程，但 Phase D 需要以下新增能力：

1. EvalReport 缺少模型元数据（名称/层级/成本）
2. EvalConfig 缺少多模型对比配置
3. 没有多模型批量运行器（ComparisonRunner）
4. Reporter 只支持单模型报告，不支持横向对比
5. 没有 `main.rs` 可执行入口（`cargo run -p octo-eval`）
6. 没有多模型对比的集成测试

---

## 任务分组

### GroupA-P0: 模型元数据与配置扩展（可并行）

**T1: ModelInfo 与 EvalReport 扩展**
- 在 `task.rs` 或新文件 `model.rs` 中定义 `ModelInfo` struct（name, tier, provider, cost_per_1m_input, cost_per_1m_output）
- 在 `EvalReport` 中增加 `model: Option<ModelInfo>` 字段
- 在 `TaskResult` 中确保 token 数据足以计算成本
- 新增 `EvalReport::estimated_cost()` 方法

**T2: EvalConfig 多模型扩展**
- 在 `config.rs` 中增加 `MultiModelConfig` struct
- 包含 `models: Vec<ModelEntry>`，每个 entry 含 EngineConfig + ModelInfo
- 增加 `fallback_to_mock: bool` 字段
- 确保向后兼容（单模型配置仍可用）

### GroupB-P0: 对比运行器（依赖 GroupA）

**T3: ComparisonRunner 实现**
- 新文件 `comparison.rs`
- `ComparisonRunner::run_comparison(config, tasks) -> ComparisonReport`
- 对每个模型创建 `EvalRunner`，依次跑同一批 tasks
- `ComparisonReport` 包含 `Vec<(ModelInfo, EvalReport)>` 和跨模型统计

**T4: 对比报告生成器**
- 扩展 `reporter.rs` 添加 `ComparisonReporter`
- `to_comparison_markdown()` — 生成多模型横向对比 Markdown 表
- `to_comparison_json()` — JSON 格式对比数据
- 包含：按模型对比、按维度对比、成本效益分析

### GroupC-P1: 可执行入口与测试（依赖 GroupB）

**T5: main.rs 命令行入口**
- 在 `crates/octo-eval/` 增加 `src/main.rs` (binary)
- 更新 `Cargo.toml` 添加 `[[bin]]` 配置
- 子命令：`run` (单模型)、`compare` (多模型对比)、`list-suites` (列出套件)
- 从 `.env` / `eval.toml` 加载配置
- 输出 JSON/Markdown 报告到文件

**T6: 多模型对比集成测试**
- 使用 MockProvider 模拟多模型场景
- 验证 ComparisonRunner 输出正确
- 验证对比报告生成（JSON + Markdown）
- 验证成本计算

**T7: Workspace 验证 + Checkpoint**
- `cargo test --workspace -- --test-threads=1` 全量通过
- `cargo check --workspace` 无 warning
- 更新 checkpoint

---

## 执行顺序

```
GroupA-P0 (T1, T2 并行)
    ↓
GroupB-P0 (T3, T4 并行)
    ↓
GroupC-P1 (T5 → T6 → T7 串行)
```

## 预期产出

- `model.rs` — ModelInfo、ModelTier 定义
- `comparison.rs` — ComparisonRunner、ComparisonReport
- `reporter.rs` — 扩展 ComparisonReporter
- `main.rs` — CLI 入口 (cargo run -p octo-eval)
- 集成测试覆盖多模型对比场景
- 全部测试通过（预期 ~1880+ tests）
