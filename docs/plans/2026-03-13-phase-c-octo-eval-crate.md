# Phase C — octo-eval 评估框架 Crate

**日期**: 2026-03-13
**前置**: Phase A (评估测试, e490da3) + Phase B (评估手册, 90017dc)
**设计文档**: `docs/design/AGENT_EVALUATION_DESIGN.md` 第五节
**目标**: 创建 `crates/octo-eval/` 框架骨架，实现轨道 A (Engine 自动化评估) 核心能力

---

## 任务分解

### GroupA-P0: Crate 骨架 + 核心 Trait (并行)

#### T1 — Cargo.toml + lib.rs + 基础类型

创建 `crates/octo-eval/` 目录结构和 Cargo.toml。

**文件**:
- `crates/octo-eval/Cargo.toml`
- `crates/octo-eval/src/lib.rs`
- `crates/octo-eval/src/task.rs`
- `crates/octo-eval/src/score.rs`
- `crates/octo-eval/src/config.rs`

**内容**:

`Cargo.toml`:
```toml
[package]
name = "octo-eval"
edition.workspace = true
version.workspace = true

[dependencies]
octo-types = { workspace = true }
octo-engine = { workspace = true, default-features = false, features = [] }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
async-trait = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

`task.rs` — 核心 EvalTask trait:
```rust
/// 评估任务定义
pub trait EvalTask: Send + Sync {
    fn id(&self) -> &str;
    fn prompt(&self) -> &str;
    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>>;
    fn score(&self, output: &AgentOutput) -> EvalScore;
    fn metadata(&self) -> TaskMetadata;
}

/// Agent 输出（评估用）
pub struct AgentOutput {
    pub messages: Vec<octo_types::message::ChatMessage>,
    pub tool_calls: Vec<ToolCallRecord>,
    pub rounds: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    pub stop_reason: String,
}

/// 工具调用记录
pub struct ToolCallRecord {
    pub name: String,
    pub input: serde_json::Value,
    pub output: String,
    pub is_error: bool,
    pub duration_ms: u64,
}

/// 任务元数据
pub struct TaskMetadata {
    pub category: String,
    pub difficulty: Difficulty,
    pub expected_steps: Option<u32>,
    pub tags: Vec<String>,
}

pub enum Difficulty { Easy, Medium, Hard }
```

`score.rs`:
```rust
pub struct EvalScore {
    pub passed: bool,
    pub score: f64,  // 0.0 - 1.0
    pub details: ScoreDetails,
}

pub enum ScoreDetails {
    ExactMatch { expected: String, actual: String },
    ToolCallMatch { expected_tool: String, actual_tool: Option<String>, arg_match_rate: f64 },
    SequenceMatch { expected_len: usize, matched: usize },
    BehaviorCheck { expected_behavior: String, observed: bool },
    Custom { message: String },
}
```

`config.rs`:
```rust
pub struct EvalConfig {
    pub target: EvalTarget,
    pub concurrency: usize,
    pub timeout_secs: u64,
    pub record_traces: bool,
    pub output_dir: PathBuf,
}

pub enum EvalTarget {
    Engine(EngineConfig),  // 轨道 A
    // Cli(CliConfig),     // 轨道 B-1 (Phase D)
    // Server(ServerConfig), // 轨道 B-2 (Phase D)
}

pub struct EngineConfig {
    pub provider_config: octo_types::provider::ProviderConfig,
    pub model: String,
    pub max_tokens: u32,
    pub max_iterations: u32,
}
```

`lib.rs`:
```rust
pub mod task;
pub mod score;
pub mod config;
pub mod runner;
pub mod recorder;
pub mod reporter;
pub mod datasets;
pub mod suites;
pub mod mock_provider;
```

**验证**: `cargo check -p octo-eval`

---

#### T2 — EvalRunner 评估执行器

创建评估执行器，驱动 Engine 运行评估任务。

**文件**:
- `crates/octo-eval/src/runner.rs`

**关键设计**:
- `EvalRunner::new(config: EvalConfig)` 构造
- `run_task(task: &dyn EvalTask) -> TaskResult` 执行单任务
- `run_suite(tasks: &[Box<dyn EvalTask>]) -> EvalReport` 批量执行
- 内部使用 `run_agent_loop()` (from `octo_engine::agent::harness`) 驱动单次评估
- 收集 `AgentEvent` 流，转换为 `AgentOutput`
- 调用 `task.score(output)` 生成分数

**与 engine 的集成点**:
```rust
use octo_engine::agent::harness::{run_agent_loop, AgentLoopConfig};
use octo_engine::providers::traits::Provider;
use octo_engine::providers::create_provider;
use octo_engine::tools::{ToolRegistry, default_tools};
```

构造 `AgentLoopConfig`:
- `provider` → 从 `EvalConfig.target.EngineConfig` 创建
- `tools` → `default_tools()` 或任务指定子集
- `model`, `max_tokens`, `max_iterations` → 从 config
- 其余字段用 Default

收集事件流:
```rust
let stream = run_agent_loop(loop_config, messages);
pin_mut!(stream);
let mut output = AgentOutput::default();
while let Some(event) = stream.next().await {
    match event {
        AgentEvent::ToolStart { .. } => { /* 记录 */ }
        AgentEvent::ToolResult { .. } => { /* 记录 */ }
        AgentEvent::Completed(result) => { /* 提取最终状态 */ }
        _ => {}
    }
}
```

**验证**: `cargo check -p octo-eval`

---

#### T3 — MockProvider + ReplayProvider

实现 Mock/Replay 机制，支持零成本 CI 回归。

**文件**:
- `crates/octo-eval/src/mock_provider.rs`

**MockProvider**:
```rust
pub struct MockProvider {
    responses: Vec<CompletionResponse>,
    cursor: AtomicUsize,
    call_log: Mutex<Vec<CompletionRequest>>,
}

impl MockProvider {
    pub fn new(responses: Vec<CompletionResponse>) -> Self;
    pub fn with_tool_call(tool_name: &str, tool_input: Value) -> Self;  // 便捷构造
    pub fn with_text(text: &str) -> Self;  // 便捷构造
    pub fn call_count(&self) -> usize;
    pub fn calls(&self) -> Vec<CompletionRequest>;
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, _request: CompletionRequest) -> Result<CompletionStream>;
}
```

**ReplayProvider**:
```rust
pub struct ReplayProvider {
    interactions: Vec<RecordedInteraction>,
    cursor: AtomicUsize,
}

pub struct RecordedInteraction {
    pub request_hash: String,  // request 的 hash（用于匹配）
    pub response: CompletionResponse,
    pub latency_ms: u64,
}

impl ReplayProvider {
    pub fn from_jsonl(path: &Path) -> Result<Self>;
    pub fn save_jsonl(interactions: &[RecordedInteraction], path: &Path) -> Result<()>;
}
```

**验证**: `cargo check -p octo-eval` + 单元测试

---

### GroupB-P1: 数据与评分 (并行)

#### T4 — JSONL 数据集加载器

**文件**:
- `crates/octo-eval/src/datasets/mod.rs`
- `crates/octo-eval/src/datasets/loader.rs`
- `crates/octo-eval/datasets/` (数据文件目录)

**功能**:
- `load_jsonl(path: &Path) -> Result<Vec<JsonlTask>>` — 统一 JSONL 格式加载
- `JsonlTask` 结构体 → 实现 `EvalTask` trait
- 支持的字段: `id`, `prompt`, `expected_tool`, `expected_args`, `expected_behavior`, `category`, `difficulty`

**初始数据集** (各 3-5 条，证明格式可用):
- `datasets/octo_tool_call.jsonl` — 工具调用精确度 (file_read, bash, web_fetch)
- `datasets/octo_security.jsonl` — 安全指令拒绝 (rm -rf, path traversal)
- `datasets/octo_context.jsonl` — Context 降级触发

**验证**: 加载 + 解析单元测试

---

#### T5 — 评分策略实现

**文件**:
- `crates/octo-eval/src/scorer.rs`

**评分器**:
```rust
pub trait Scorer: Send + Sync {
    fn score(&self, task_def: &serde_json::Value, output: &AgentOutput) -> EvalScore;
}

pub struct ExactMatchScorer;      // 精确匹配 expected_output
pub struct ToolCallScorer;        // 匹配 expected_tool + expected_args
pub struct BehaviorScorer;        // 检查 expected_behavior (rejected, context_degraded 等)
pub struct SequenceScorer;        // 匹配工具调用序列
```

**自动选择**:
```rust
pub fn auto_scorer(task_def: &serde_json::Value) -> Box<dyn Scorer> {
    if task_def.get("expected_tool").is_some() { return Box::new(ToolCallScorer); }
    if task_def.get("expected_behavior").is_some() { return Box::new(BehaviorScorer); }
    if task_def.get("expected_sequence").is_some() { return Box::new(SequenceScorer); }
    Box::new(ExactMatchScorer)
}
```

**验证**: 单元测试覆盖各评分器

---

### GroupC-P1: 报告 + 记录 (依赖 GroupA)

#### T6 — Recorder (Trace 记录器)

**文件**:
- `crates/octo-eval/src/recorder.rs`

**功能**:
- `EvalRecorder::new(output_dir: PathBuf)` 构造
- `record_interaction(request: &CompletionRequest, response: &CompletionResponse)` — 录制 LLM 交互
- `save_trace(task_id: &str, trace: &EvalTrace) -> Result<PathBuf>` — 保存完整 trace
- `load_trace(path: &Path) -> Result<EvalTrace>` — 加载 trace 用于 replay

```rust
pub struct EvalTrace {
    pub task_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub interactions: Vec<RecordedInteraction>,
    pub output: AgentOutput,
    pub score: EvalScore,
}
```

**验证**: 写入/读取 roundtrip 测试

---

#### T7 — Reporter (报告生成)

**文件**:
- `crates/octo-eval/src/reporter.rs`

**功能**:
- `EvalReport` 结构体 (from 设计文档 5.3 节)
- `Reporter::to_json(report: &EvalReport) -> String`
- `Reporter::to_markdown(report: &EvalReport) -> String`
- 按 category 分组统计
- 按 difficulty 分组统计
- Token 用量汇总
- 延迟统计 (min/max/avg/p95)

**验证**: 单元测试 + 示例输出

---

### GroupD-Serial: 集成验证

#### T8 — 评估套件 (Suites)

**文件**:
- `crates/octo-eval/src/suites/mod.rs`
- `crates/octo-eval/src/suites/tool_call.rs`
- `crates/octo-eval/src/suites/security.rs`

**功能**:
- 预定义评估套件，封装 JSONL 加载 + 评分器选择
- `ToolCallSuite::load() -> Vec<Box<dyn EvalTask>>`
- `SecuritySuite::load() -> Vec<Box<dyn EvalTask>>`

**验证**: `cargo check -p octo-eval`

---

#### T9 — 集成测试 + cargo test 验证

**文件**:
- `crates/octo-eval/tests/eval_integration.rs`

**测试内容**:
- 使用 MockProvider 运行完整评估流程: 加载 JSONL → 创建 Runner → 执行 → 评分 → 生成报告
- 验证 ReplayProvider 的录制/回放 roundtrip
- 验证报告 JSON/Markdown 输出格式

**验证**: `cargo test -p octo-eval -- --test-threads=1`

---

#### T10 — workspace 全量验证 + checkpoint 更新

- `cargo check --workspace` 确保无编译错误
- `cargo test --workspace -- --test-threads=1` 确保无回归
- 更新 `docs/plans/.checkpoint.json`

**验证**: 所有 1807+ 测试通过

---

## 执行策略

```
GroupA-P0 (T1, T2, T3)  ←── 并行，3 个 subagent
         ↓
GroupB-P1 (T4, T5)      ←── 并行，2 个 subagent
         ↓
GroupC-P1 (T6, T7)      ←── 并行，2 个 subagent
         ↓
GroupD-Serial (T8, T9, T10) ←── 串行
```

**预计**: 10 个任务，subagent-driven-development 模式

## 关键约束

1. **octo-eval 只依赖 octo-engine 和 octo-types**，不依赖 octo-server/octo-cli
2. **octo-engine 的 default-features = false**，避免拉入 WASM/Docker 依赖（评估不需要沙箱）
3. **Mock/Replay 优先**——所有集成测试用 MockProvider，不需要真实 LLM API key
4. **JSONL 数据集放在 `crates/octo-eval/datasets/`**，不放在根目录
5. **`--test-threads=1`** 避免 Tokio runtime 冲突
