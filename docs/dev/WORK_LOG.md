# octo-sandbox 工作日志

## Phase O — Deferred 暂缓项全解锁 (2026-03-15)

### 完成内容

Phase O 目标：解决 Phase M-a/M-b/N 累积的全部 10 个暂缓项。15/15 任务完成。

**G1: TUI Input Widget 抽取** (O-T1~T6)
- 抽取 `TextInput` 可复用组件 (`tui/widgets/text_input.rs`)
- ChatScreen 重构使用 TextInput widget
- Eval shortcut dialogs (M-b_D1)、filter popup (M-b_D2)
- Memory 搜索交互 (N_D2)
- Watch 实时进度条 with Gauge (M-a_D3)

**G2: ProviderChain Failover Trace** (O-T7~T9)
- FailoverTrace 数据结构 (ring buffer) 在 `providers/chain.rs`
- ChainProvider complete()/stream() 方法插桩记录 failover 轨迹
- Provider Inspector 可视化 (N_D3)

**G3: Session Event 广播** (O-T10~T13)
- SessionEvent enum + EventBus (`session/events.rs`)
- WS SessionUpdate 消息推送
- DevAgent TUI event-driven refresh (N_D1)

**G4: Workbench 收尾** (O-T14~T15)
- Workbench 模式审计 vs 设计文档 §6.9.2 (N_D4)
- 3 个计划文档中所有 deferred 状态更新为已完成

### 测试结果

- **2178 tests pass**（基线 2126，+52 新增）
- 0 failures, 0 remaining deferred items
- 5 commits merged

### 暂缓项解决矩阵

| 暂缓项 | 来源 | 解决任务 |
|--------|------|----------|
| M-a_D3: watch 实时进度条 | Phase M-a | G1-T6 |
| M-b_D1: Eval shortcut dialogs | Phase M-b | G1-T3 |
| M-b_D2: Eval filter popup | Phase M-b | G1-T4 |
| N_D1: Session 实时数据流 | Phase N | G3-T10~T13 |
| N_D2: Memory 搜索交互 | Phase N | G1-T5 |
| N_D3: Provider failover 可视化 | Phase N | G2-T7~T9 |
| N_D4: 完整 Workbench 模式 | Phase N | G4-T14 |

---

## Phase N — Agent Debug Panel (2026-03-15)

### 完成内容

- DevAgentScreen 全功能调试面板 (`tui/screens/dev_agent.rs`)
- AgentFocus 枚举、InspectorPanel、DevAgentScreen 结构
- 7/7 任务完成，+30 tests (2096→2126)

---

## Phase M-b — TUI Dual-View + Eval Panel (2026-03-15)

### 完成内容

- TUI 双视图模式 (ViewMode::Ops / ViewMode::Dev)
- DevEvalScreen 评估面板 (`tui/screens/dev_eval.rs`)
- OpsTab / DevTask 枚举，TUI 事件系统
- 8/8 任务完成，+38 tests (2058→2096)

---

## Phase M-a — Eval Management CLI Unification (2026-03-15)

### 完成内容

- RunStore 持久化 + EvalCommands (11 个子命令)
- handle_eval 路由统一
- 12/12 任务完成，+8 tests (2050→2058)

---

## Phase L — Eval Whitebox + Enterprise Dataset (2026-03-15)

### 完成内容

- L1: TraceEvent (10 variants) + EvalTrace.timeline + UTF-8 修复
- L2: FailureClass (14 variants) + FailureClassifier
- L3: EvalScore.dimensions 多维化 + ToolCallScorer/BehaviorCheckScorer
- L4: PlatformBehaviorScorer + EventSequenceScorer + 27 新评估任务
- L5: 数据集标注 + 设计文档最终化
- 18/18 任务完成，+29 tests (2021→2050)

---

## Phase K — 完整真实模型对比报告 (2026-03-14)

### 完成内容（代码任务）

**K1-T1: 评估配置文件** (@ 6b68deb)
- 新建 `crates/octo-eval/eval.benchmark.toml` — 5 层模型矩阵
- T0 免费: Qwen3-Coder-480B (0/0 $/1M)
- T1 经济: DeepSeek-V3.2 (0.15/0.75 $/1M)
- T2 标准: Qwen3.5-122B (0.30/1.20 $/1M)
- T3 高性能: Kimi-K2.5 (0.45/2.20 $/1M)
- T4 旗舰: Claude-Sonnet-4.6 (3.0/15.0 $/1M)

**K3-T1/T2: BenchmarkAggregator** (@ 6b68deb)
- 新建 `crates/octo-eval/src/benchmark.rs` (~340 行)
- `BenchmarkAggregator::aggregate()` — 汇总多 Suite ComparisonReport
- `ModelBenchmark` — 每模型综合 pass_rate、avg_score、token 消耗、成本
- `CostAnalysis` — 成本效益分析，自动找出 >80% pass_rate 的最便宜模型
- `Recommendation` — 3 种场景推荐 (cost_sensitive/balanced/performance_first)
- `to_markdown()` — 综合报告含维度敏感度分析 (HIGH/MEDIUM/LOW)
- 7 个单元测试覆盖聚合、成本分析、推荐、Markdown/JSON 生成

**K3-T3: CLI benchmark 命令** (@ 6b68deb)
- 修改 `crates/octo-eval/src/main.rs` — 新增 `benchmark` 子命令
- Mode 1: `--suites tool_call,security,...` — 运行所有 suite 的 compare 并汇总
- Mode 2: `--input eval_output/benchmark` — 从已有 comparison.json 聚合

**K4-T2: CI 集成** (@ 6b68deb)
- 修改 `.github/workflows/eval-ci.yml` — 新增 benchmark regression step

### 文件变更矩阵

| 文件 | 操作 | 行数 |
|------|------|------|
| `crates/octo-eval/eval.benchmark.toml` | **新建** | 38 |
| `crates/octo-eval/src/benchmark.rs` | **新建** | ~340 |
| `crates/octo-eval/src/lib.rs` | 修改 | +1 |
| `crates/octo-eval/src/main.rs` | 修改 | +170 |
| `.github/workflows/eval-ci.yml` | 修改 | +6 |

### 测试结果

- 2021 tests passing (基线 2014，+7 新增)
- 新增 benchmark 模块测试: 7 个 (aggregate_empty, aggregate_single_suite, aggregate_multiple_suites, recommendations_generated, cost_analysis, markdown_generation, json_generation)

### 待完成（需用户执行）

- K1-T2: 模型连通性验证 — 需真实 API 调用
- K2-T1/T2/T3: 核心/差异化/SWE-bench Suite 对比 — 需真实 LLM 评估
- K4-T1: 录制 Replay 基线 — 评估完成后
- K5-T1/T2: 文档产出 — 评估数据就绪后

---

## Phase J — 沙箱安全体系建设 (2026-03-14)

### 完成内容

**J1: SandboxPolicy 策略引擎** (@ 4570365)
- 新增 `SandboxPolicy` 枚举 (Strict/Preferred/Development) 到 `traits.rs`
- Strict 为默认值：仅允许 Docker/WASM 执行，拒绝 Subprocess
- 新增 `PolicyDenied` 错误变体到 `SandboxError`
- `SandboxRouter` 集成策略执行：`with_policy()`, `resolve_fallback()`
- 更新 BashTool 使用 Development 策略
- 10 个新策略测试 + 更新现有测试适配策略

**J2: Docker 预置镜像与语言检测** (@ 5553c27)
- 创建 `docker/sandbox-images/Dockerfile.python` (python:3.12-slim-bookworm)
- 创建 `docker/sandbox-images/Dockerfile.rust` (rust:1.82-bookworm)
- 新增 `ImageRegistry` 结构体（8 种语言映射）
- DockerAdapter `execute()` 使用 language 参数自动选择镜像

**J3: DockerAdapter 测试加固** (@ 5553c27)
- `ContainerGuard` RAII 结构体确保测试清理
- `require_docker()` 辅助函数提供清晰 skip 消息
- Docker 环境诊断测试

**J4: WASM/WASI CLI 执行器** (@ 5553c27)
- 新增 `execute_wasi_cli()` 使用 wasmtime_wasi preview1
- WASI 上下文：args, stdin MemoryInputPipe, stdout/stderr 捕获
- 通过 `language="wasi-cli"` 或 `code` 前缀 `wasi://` 触发
- I32Exit 退出码处理
- 3 个新 WASI 测试

**J5: 沙箱审计日志** (@ 5553c27)
- 新增 `SandboxAuditEvent` (7 种 SandboxAction，SHA-256 代码哈希)
- 工厂方法：`execution()`, `policy_deny()`, `degradation()`
- `to_audit_event()` 转换到通用 AuditEvent 用于 hash-chain 存储
- `AuditStorage` 新增 `query_sandbox_events()` 和 `query_policy_denials()`
- 7 个审计测试

**J6/J7: Docker 测试修复与 CI 集成** (@ 45a7342)
- eval-ci.yml 新增 `docker-sandbox-tests` job
- 运行策略、审计、WASM、Docker 四组沙箱测试
- 容器泄漏检测步骤
- 新增 `octo-sandbox` 路径触发 CI

### 测试结果

- **2014 tests pass**（基线 1992，+22 新增）
- 0 failures, 3 ignored
- 新增测试分布：10 策略 + 7 审计 + 3 WASI + 2 Docker 辅助

### 文件变更矩阵

| 文件 | 操作 |
|------|------|
| `crates/octo-engine/src/sandbox/traits.rs` | 修改 (+SandboxPolicy, +PolicyDenied) |
| `crates/octo-engine/src/sandbox/router.rs` | 修改 (+policy 集成, +fallback) |
| `crates/octo-engine/src/sandbox/docker.rs` | 修改 (+ImageRegistry, language 路由) |
| `crates/octo-engine/src/sandbox/wasm.rs` | 修改 (+WASI CLI executor) |
| `crates/octo-engine/src/sandbox/audit.rs` | **新建** (SandboxAuditEvent) |
| `crates/octo-engine/src/sandbox/mod.rs` | 修改 (+re-exports) |
| `crates/octo-engine/src/audit/storage.rs` | 修改 (+sandbox queries) |
| `crates/octo-engine/src/tools/bash.rs` | 修改 (Development policy) |
| `docker/sandbox-images/Dockerfile.python` | **新建** |
| `docker/sandbox-images/Dockerfile.rust` | **新建** |
| `.github/workflows/eval-ci.yml` | 修改 (+docker-sandbox-tests job) |

---

## Phase I — External Benchmark Adapters (2026-03-14)

### 完成内容

**I1: ExternalBenchmark 抽象层** (@ 2e0d365)
- 定义 `ExternalBenchmark` trait (6 方法) + `BenchmarkVerifier` trait + `MetricDefinition` 系统
- 实现 `BenchmarkRegistry` 注册表，支持动态查找和列举
- 创建 GAIA / SWE-bench / τ-bench 三个骨架 adapter 实现
- 新增 `ScoreDetails` 变体: `GaiaMatch`, `SweVerify`, `PassK`
- CLI `load_suite()` 和 `list-suites` 集成外部 benchmark 动态加载

**I2: GAIA Benchmark 数据集** (@ 5512f4f)
- 创建 `gaia_sample.jsonl` — 50 个多步推理任务
- 分布: L1 (Easy) 20 个, L2 (Medium) 20 个, L3 (Hard) 10 个
- 覆盖: 数学, 地理, 科学, 历史, 文学, 技术等领域
- 工具: web_search, calculator, file_read, code_execution, database_query, api_call

**I3: SWE-bench Benchmark 数据集** (@ 5512f4f)
- 创建 `swe_bench_lite.jsonl` — 50 个代码修复任务
- 覆盖 8 个仓库: django (10), flask (7), sympy (8), requests (7), pytest (7), scikit-learn (3), matplotlib (8)
- 包含真实格式的 unified diff patch + test patch + problem statement
- 难度按 patch 大小和测试数量自动分类

**I4: τ-bench Benchmark 数据集** (@ 5512f4f)
- 创建 `tau_bench_retail.jsonl` — 30 个零售场景任务
- 分布: 退货 (10), 查询 (10), 修改 (10)
- 每条任务包含 policy_rules, expected_actions, expected_db_state
- pass^k=8 一致性指标

**I5: 验证与 CI 集成** (@ 57ca310)
- eval-ci.yml 新增 GAIA / SWE-bench / τ-bench 运行步骤
- SWE-bench 通过 DOCKER_AVAILABLE 环境变量条件执行
- 更新 eval_integration.rs 跳过外部 benchmark 文件验证
- 全量测试通过: 1992 tests (+13)

### 技术变更

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/benchmarks/mod.rs` | 已有 | ExternalBenchmark trait + Registry (~110 行) |
| `src/benchmarks/gaia.rs` | 已有 | GAIA adapter (247 行, 含 4 个测试) |
| `src/benchmarks/swe_bench.rs` | 已有 | SWE-bench adapter (248 行, 含 3 个测试) |
| `src/benchmarks/tau_bench.rs` | 已有 | τ-bench adapter (266 行, 含 4 个测试) |
| `datasets/gaia_sample.jsonl` | 新建 | 50 GAIA 任务 |
| `datasets/swe_bench_lite.jsonl` | 新建 | 50 SWE-bench 任务 |
| `datasets/tau_bench_retail.jsonl` | 新建 | 30 τ-bench 任务 |
| `tests/eval_integration.rs` | 修改 | 添加 is_external_benchmark_file() |
| `.github/workflows/eval-ci.yml` | 修改 | +3 benchmark 步骤 |

### 测试结果

- octo-eval 单元测试: 28/28 通过
- workspace 全量测试: 1992/1992 通过
- 无 deferred 项

### 评估层次覆盖

```
Level 4: 端到端任务成功率 (SWE-bench 50 tasks)     → ✅ 已实现
Level 3: 多轮对话+工具链协调 (GAIA 50 + τ-bench 30) → ✅ 已实现
Level 2: 单次工具调用精确度 (BFCL 50 tasks)          → ✅ 已有
Level 1: 引擎基础能力 (单元测试 1992 tests)           → ✅ 已有
```

### 下一步

- Phase J: Docker 测试修复 → SWE-bench 从 mock 升级为真实验证
- Phase K: 跨 GAIA/SWE-bench/τ-bench 的多模型对比报告

---

## Phase H — Eval Capstone (2026-03-14)

### 完成内容

**H1: Resilience Suite + 新行为类型**
- 在 BehaviorScorer 中新增 4 种行为模式: retry_success, emergency_stopped, canary_detected, text_tool_recovered
- 同步更新 loader.rs 中的 score_behavior() 函数
- 创建 ResilienceSuite 模块 (resilience.rs) 和 20 条 JSONL 评估任务
- 注册到 mod.rs / main.rs / CLI help

**H2: Context 扩充**
- octo_context.jsonl 从 14 扩充到 50 条任务
- 新增 8 个评估维度: CX5 (degradation), CX6 (token budget), CX7 (long prompt), CX8 (multi-turn), CX9 (prioritization), CX10 (recovery), CX11 (format consistency), CX12 (information density)

**H3: AstMatch Scorer**
- 实现 AstMatchScorer，支持深层 JSON 结构比较
- 功能: 嵌套对象递归比较、数组顺序无关匹配、类型强转 (strict_types=false)、null=缺失语义、额外字段容忍
- 新增 AstMatch variant 到 ScoreDetails enum
- 在 auto_scorer() 中集成 "ast_match" scorer 覆盖
- 10 条 AST 匹配测试用例添加到 octo_tool_call.jsonl

**H4: 验证与 CI**
- eval-ci.yml 新增 resilience suite 运行步骤
- CLI list-suites 帮助文本更新
- 全量测试通过: 1979 tests (+17)

### 技术变更

| 文件 | 变更 |
|------|------|
| `crates/octo-eval/src/scorer.rs` | +4 behavior branches, +AstMatchScorer (~130 LOC), +16 tests |
| `crates/octo-eval/src/score.rs` | +AstMatch ScoreDetails variant |
| `crates/octo-eval/src/datasets/loader.rs` | +score_ast_match(), +strict_types field, +4 behaviors |
| `crates/octo-eval/src/suites/resilience.rs` | 新文件, ResilienceSuite 实现 |
| `crates/octo-eval/src/suites/mod.rs` | +resilience 导出 |
| `crates/octo-eval/src/main.rs` | +resilience import/load/help |
| `crates/octo-eval/datasets/octo_resilience.jsonl` | 新文件, 20 tasks |
| `crates/octo-eval/datasets/octo_context.jsonl` | 14→50 tasks |
| `crates/octo-eval/datasets/octo_tool_call.jsonl` | +10 AST tasks |
| `.github/workflows/eval-ci.yml` | +resilience suite step |

### 测试结果

- 全量: 1979 tests passing (was 1962)
- Docker tests: 5 excluded (Docker daemon not running)
- 编译无 warning

### 遗留问题

- 无

### 下一步

- Phase I: SWE-bench 适配 (12 tasks)
- Phase J: Docker 测试修复 (8 tasks)
- Phase K: 完整模型对比报告 (10 tasks)
