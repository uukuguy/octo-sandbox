# Phase E — 评估框架增强（Runner 加固 + 新套件 + 多轨道）

**日期**: 2026-03-14
**前置**: Phase D Multi-Model Comparison (COMPLETE @ 998f3b4)
**目标**: 补全评估设计文档中未实现的核心能力，使 octo-eval 从"可用"提升至"生产级"

---

## 一、差距分析

Phase A-D 完成了评估框架骨架和 3 模型对比，但设计文档（`AGENT_EVALUATION_DESIGN.md`）要求的核心能力仍有 ~60% 未实现：

### 已完成 (Phase A-D)

- EvalTask trait + EvalRunner（Engine 模式）
- 5 种 Scorer（ExactMatch/ToolCall/Behavior/Sequence/auto）
- ComparisonRunner + 多模型报告（JSON + Markdown）
- MockProvider + ReplayProvider
- EvalRecorder（save/load）
- 3 套件 43 任务（tool_call 23 / security 14 / context 6）
- CLI（list-suites / run / compare）
- 1870 tests passing

### 未完成（本 Phase 目标）

| 缺口 | 设计文档章节 | 优先级 |
|------|-------------|--------|
| Recorder 未接入 Runner | 5.5 Mock/Replay | P0 |
| Timeout 未强制执行 | 5.2 EvalRunner | P0 |
| 并发评估未实现 | 5.2 EvalRunner | P1 |
| 回归检测（对比基线） | 5.2 报告 | P1 |
| memory.rs 套件 | 4 评估维度·记忆检索 | P1 |
| provider.rs 套件 | 4 特色·Provider Chain | P1 |
| e2e.rs 套件 | 4 Level 4 端到端 | P1 |
| LlmJudge 评分器 | 5.3 scorer | P1 |
| EvalTarget::Cli | 5.2 轨道 B-1 | P2 |
| EvalTarget::Server | 5.2 轨道 B-2 | P2 |
| BFCL 数据集适配器 | 5.4 datasets | P2 |
| eval.toml 配置文件 | 5.4 Provider 配置 | P2 |
| CI 集成（Replay 模式） | 3.4 投入产出 | P2 |
| Per-task tool allowlists | 5.3 EvalTask | P2 |

---

## 二、任务分组

### Phase E1: Runner 加固（无外部依赖，快速出成果）

**E1-T1: Recorder 集成到 Runner**
- `EvalRunner` 构造时接受 `Option<EvalRecorder>`
- `config.record_traces = true` 时，`run_task()` 结束后自动调用 `recorder.save_trace()`
- `run_suite()` 结束后自动调用 `recorder.save_summary()`
- 文件改动: `runner.rs` ~25 行
- 测试: 1 个新测试验证 trace 文件生成

**E1-T2: Timeout 强制执行**
- `run_task()` 用 `tokio::time::timeout(Duration::from_secs(config.timeout_secs), ...)` 包裹
- 超时返回 `TaskResult { score: EvalScore { passed: false, score: 0.0, details: ScoreDetails::Timeout }, ... }`
- `ScoreDetails` 枚举增加 `Timeout` 变体
- 文件改动: `runner.rs` ~15 行, `score.rs` ~5 行
- 测试: 1 个新测试（MockProvider 延迟触发超时）

**E1-T3: 并发评估**
- `run_suite()` 使用 `futures::stream::iter().buffer_unordered(config.concurrency)` 替代顺序循环
- `concurrency = 1` 时行为不变（向后兼容）
- 注意: `eprintln!` 进度日志在并发时交错是可接受的
- 文件改动: `runner.rs` ~30 行
- 依赖: `futures` crate（已在 workspace 中）
- 测试: 1 个新测试验证并发 > 1 时任务并行执行

**E1-T4: Per-task Tool Allowlists**
- `JsonlTask` 解析 `"tools"` 字段（可选 `Vec<String>`）
- `available_tools()` 返回 `Some(tools)` 时，`run_task()` 过滤 ToolRegistry
- 文件改动: `runner.rs` ~20 行, `datasets/loader.rs` ~15 行
- 测试: 1 个新测试验证工具过滤

**E1-T5: 回归检测**
- `Reporter` 增加 `diff_report(current: &EvalReport, baseline: &EvalReport) -> RegressionReport`
- 输出格式: 每个任务标注 `IMPROVED` / `REGRESSED` / `UNCHANGED`
- 总体输出: `pass_rate: 82.6% → 85.2% (▲+2.6%)` 或 `▼-1.3%`
- CLI 增加 `--baseline <path>` 参数
- 文件改动: `reporter.rs` ~80 行, `main.rs` ~15 行
- 测试: 2 个新测试

**E1-T6: 测试验证 + Checkpoint**
- `cargo test --workspace -- --test-threads=1` 全量通过
- `cargo check --workspace` 无 warning
- 更新 checkpoint

---

### Phase E2: 新评估套件（octo 差异化能力验证）

**E2-T1: LlmJudge 评分器**
- 新增 `LlmJudgeScorer` in `scorer.rs`
- 输入: task prompt + agent output + rubric（评分标准文本）
- 流程: 构造评判 prompt → 调用 judge provider → 解析返回的 JSON 分数
- 评判 prompt 模板:
  ```
  You are an evaluation judge. Score the following agent output on a scale of 0.0 to 1.0.

  ## Task
  {task_prompt}

  ## Agent Output
  {agent_output}

  ## Rubric
  {rubric}

  Respond with JSON: {"score": 0.0-1.0, "reasoning": "..."}
  ```
- JsonlTask 中通过 `"scorer": "llm_judge"` + `"rubric": "..."` 字段触发
- 文件改动: `scorer.rs` ~80 行, `datasets/loader.rs` ~10 行
- 测试: 2 个新测试（MockProvider 模拟 judge 响应）

**E2-T2: Provider 容错套件（provider.rs）**
- 新文件 `suites/provider.rs`
- **纯 Mock 测试**，不需要真实 LLM，可在 CI 中运行
- 任务设计（10 个任务）:
  - `prov-R1-01`: 主 Provider 返回 429 → 验证 exponential backoff
  - `prov-R1-02`: 主 Provider 返回 500 → 验证重试
  - `prov-R1-03`: 主 Provider 返回 401 → 验证不重试（认证错误）
  - `prov-R2-01`: 主 Provider 超时 → 验证 failover 到备用
  - `prov-R2-02`: 主 Provider 持续失败 → 验证完整 failover 链
  - `prov-R3-01`: 429 + Retry-After header → 验证尊重 header
  - `prov-R3-02`: 间歇性失败（第 1,3 次失败，第 2 次成功）→ 验证恢复
  - `prov-R3-03`: 所有 Provider 都失败 → 验证优雅降级
  - `prov-R4-01`: failover 后数据一致性验证
  - `prov-R4-02`: ProviderChain 负载均衡验证
- 评分: BehaviorScorer（验证行为模式）
- **注意**: 此套件直接测试 octo-engine Provider 层，不走 Agent Loop
- 实现方式: 自定义 `ProviderEvalRunner` 直接调用 ProviderChain
- 文件改动: 新文件 `suites/provider.rs` ~150 行, `datasets/octo_provider.jsonl` ~10 tasks
- 测试: 3 个新测试

**E2-T3: 记忆一致性套件（memory.rs）**
- 新文件 `suites/memory.rs`
- 验证 octo-engine 四层记忆系统的存取一致性
- 任务设计（12 个任务）:
  - `mem-L0-01~03`: WorkingMemory — 同一对话内存取一致性（3 tasks）
  - `mem-L1-01~03`: SessionMemory — 跨轮次记忆持久性（3 tasks）
  - `mem-L2-01~03`: MemoryStore — 长期存储检索精度（3 tasks）
  - `mem-KG-01~03`: KnowledgeGraph — 实体关系图查询（3 tasks）
- 评分: ExactMatch（检索结果精确匹配）+ LlmJudge（语义相似性）
- **实现挑战**: 需要模拟 session 切换
  - 方案: 扩展 `EvalRunner` 增加 `run_multi_turn(tasks: &[EvalTask])` 方法
  - 或: 每个 task 的 prompt 自包含（写入 + 检索在同一 prompt 中）
- 文件改动: 新文件 `suites/memory.rs` ~180 行, `datasets/octo_memory.jsonl` ~12 tasks
- 测试: 3 个新测试

**E2-T4: 端到端编程套件（e2e.rs）**
- 新文件 `suites/e2e.rs`
- 简化版 SWE-bench: 给定代码 + bug → Agent 修复 → 验证
- 任务设计（8 个任务）:
  - `e2e-B1-01~02`: 简单 bug 修复（off-by-one, typo）
  - `e2e-B2-01~02`: 逻辑 bug（条件反转, 边界处理）
  - `e2e-B3-01~02`: 多文件协调修改
  - `e2e-B4-01~02`: 复杂重构（函数签名变更 + 调用点更新）
- 评分: 自定义 `PatchVerifyScorer`
  - 流程: Agent 输出 → 提取 file_write 调用 → 写入临时目录 → 跑预定义测试
  - 需要: `tests/e2e_fixtures/` 存放测试项目
- 文件改动: 新文件 `suites/e2e.rs` ~200 行, `scorer.rs` ~60 行（PatchVerifyScorer）
- Fixture 文件: `datasets/e2e_fixtures/` ~8 个小项目
- 测试: 2 个新测试

**E2-T5: 套件注册与测试验证**
- 更新 `suites/mod.rs` 注册 provider/memory/e2e 三个新套件
- 更新 `main.rs` 的 `list-suites` 和 `load_suite()` 逻辑
- `cargo test --workspace -- --test-threads=1` 全量通过
- 更新 checkpoint

---

### Phase E3: 多轨道 + 外部 Benchmark（长期价值）

**E3-T1: EvalTarget::Cli 子进程模式**
- 取消 `EvalTarget::Cli(CliConfig)` 的注释
- `CliConfig`: `binary_path`, `args`, `timeout`, `env`
- `EvalRunner::run_task_cli()`:
  1. 启动 `octo-cli` 子进程
  2. stdin 写入 prompt
  3. stdout 收集输出（JSON 格式）
  4. 解析为 `AgentOutput`
- 前提: `octo-cli` 需要支持 `--json-output` 模式
- 文件改动: `runner.rs` ~80 行, `config.rs` ~20 行

**E3-T2: EvalTarget::Server HTTP 模式**
- 取消 `EvalTarget::Server(ServerConfig)` 的注释
- `ServerConfig`: `base_url`, `api_key`
- `EvalRunner::run_task_server()`:
  1. `POST /api/sessions` 创建 session
  2. `POST /api/sessions/{id}/messages` 发送 prompt
  3. 通过 WebSocket 或轮询收集响应
  4. `DELETE /api/sessions/{id}` 清理
- 文件改动: `runner.rs` ~100 行, `config.rs` ~20 行
- 依赖: `reqwest`（已在 workspace）

**E3-T3: BFCL 数据集适配器**
- 新文件 `datasets/bfcl.rs`
- 从 BFCL JSON 格式转换为 octo `EvalTask`:
  - `question` → `prompt`
  - `function` → `available_tools`（转为 ToolDefinition）
  - `ground_truth` → AstMatch scorer 的期望值
- 新增 `AstMatchScorer`: 解析 Python 函数调用语法，比较函数名 + 参数
- 文件改动: 新文件 `datasets/bfcl.rs` ~150 行, `scorer.rs` ~60 行

**E3-T4: eval.toml 配置文件**
- 支持 TOML 配置替代环境变量
- 优先级: eval.toml < env vars < CLI args
- 格式:
  ```toml
  [default]
  timeout_secs = 120
  concurrency = 4
  record_traces = true
  output_dir = "eval_output"

  [[models]]
  name = "DeepSeek-V3"
  provider = "openai"
  model = "deepseek/deepseek-chat-v3-0324"
  tier = "economy"
  base_url = "https://openrouter.ai/api/v1"

  [[models]]
  name = "Claude-Sonnet-4.6"
  provider = "anthropic"
  model = "claude-sonnet-4-6-20250514"
  tier = "flagship"
  ```
- 文件改动: `config.rs` ~80 行, `main.rs` ~20 行
- 依赖: `toml` crate

**E3-T5: CI 集成（GitHub Actions）**
- 新文件 `.github/workflows/eval.yml`
- 使用 Replay 模式（$0 成本）跑评估回归
- 触发: PR push + nightly
- 步骤: build → replay 模式跑 3 套件 → 对比 baseline → 注释 PR
- 文件改动: 新 YAML ~60 行

**E3-T6: 测试验证 + Checkpoint**
- 全量测试通过
- 更新 checkpoint

---

## 三、执行顺序

```
Phase E1 (E1-T1 ~ E1-T6)     Runner 加固
  ↓ 预计 2-3 天，~200 行代码 + 6 tests
Phase E2 (E2-T1 ~ E2-T5)     新评估套件
  ↓ 预计 3-5 天，~650 行代码 + 40 tasks + 13 tests
Phase E3 (E3-T1 ~ E3-T6)     多轨道 + 外部 Benchmark
    预计 5-7 天，~600 行代码
```

### 依赖关系

```
E1-T1 (Recorder) ─┐
E1-T2 (Timeout)  ─┤─ 可并行
E1-T4 (Allowlist) ┘
E1-T3 (Concurrency) ─ 独立
E1-T5 (Regression) ─ 独立
E1-T6 (验证) ─ 依赖 E1-T1~T5 全部完成

E2-T1 (LlmJudge) ─┐
E2-T2 (Provider)  ─┤─ 可并行
E2-T3 (Memory)    ─┘
E2-T4 (E2E) ─ 依赖 E2-T1（需要 LlmJudge 或 PatchVerify）
E2-T5 (注册) ─ 依赖 E2-T1~T4 全部完成

E3-T1 (CLI) ─┐
E3-T2 (Server) ─┤─ 可并行
E3-T3 (BFCL)   ─┤
E3-T4 (TOML)   ─┘
E3-T5 (CI) ─ 依赖 E3-T1~T4
E3-T6 (验证) ─ 依赖全部完成
```

---

## 四、验收标准

| Phase | 验收标准 |
|-------|---------|
| E1 | `cargo test -p octo-eval` 全通过，新增 6 tests，Recorder 自动生成 trace 文件，timeout 可触发，并发模式工作 |
| E2 | 新增 3 套件 + 1 评分器，总任务数 43→83（+40），Mock 模式全部可跑，LlmJudge 在 e2e 套件工作 |
| E3 | `octo-eval run --target cli` 和 `--target server` 可用，BFCL 100 题导入成功，CI Replay 流水线通过 |

---

## 五、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Memory 套件需要 session 切换 | E2-T3 实现复杂度高 | 先用单 prompt 自包含方案（写入+检索同一 prompt） |
| E2E 套件需要临时文件系统 | 测试隔离性 | 使用 `tempdir` crate 创建临时项目 |
| LlmJudge 引入评判成本 | 每次评估额外 LLM 调用 | 仅 e2e 套件使用，其他套件用确定性 scorer |
| BFCL Python AST 解析 | Rust 中解析 Python 语法复杂 | 简化为字符串匹配 + 正则，不用完整 AST |
| CLI/Server 模式需要进程管理 | E3-T1/T2 实现复杂 | CLI 先实现，Server 可延期 |
