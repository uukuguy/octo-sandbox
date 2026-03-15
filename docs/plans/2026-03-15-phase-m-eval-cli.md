# Phase M-a: 评估管理 CLI 统一

> 设计文档：`docs/design/EVAL_CLI_TUI_DESIGN.md`
> 前置：Phase L（评估系统白盒化）已完成
> 目标：octo-eval 版本化存储 + octo-cli `octo eval` 命令族

---

## 任务清单

### Group 1: 版本化存储基础

#### Ma-T1: RunStore 版本化存储模块

**文件**: `crates/octo-eval/src/run_store.rs`

新增 RunStore 模块，管理评估运行的版本化存储：

```rust
pub struct RunManifest {
    pub run_id: String,
    pub tag: Option<String>,
    pub timestamp: String,
    pub command: String,          // run | compare | benchmark
    pub suite: String,
    pub models: Vec<String>,
    pub git_commit: String,
    pub git_branch: String,
    pub task_count: usize,
    pub passed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
    pub duration_ms: u64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    pub eval_config_hash: String,
    pub failure_summary: FailureSummary,
}

pub struct RunData {
    pub manifest: RunManifest,
    pub report: Option<DetailedReport>,
    pub comparison: Option<ComparisonReport>,
    pub traces: Vec<EvalTrace>,
}

pub struct RunStore {
    base_dir: PathBuf,  // eval_output/runs/
}

impl RunStore {
    pub fn new(base_dir: PathBuf) -> Result<Self>;
    pub fn next_run_id(&self) -> String;
    pub fn save_run(&self, run: &RunData) -> Result<PathBuf>;
    pub fn list_runs(&self, filter: &RunFilter) -> Result<Vec<RunManifest>>;
    pub fn load_run(&self, run_id: &str) -> Result<RunData>;
    pub fn load_manifest(&self, run_id: &str) -> Result<RunManifest>;
    pub fn update_latest_link(&self, run_id: &str) -> Result<()>;
    pub fn tag_run(&self, run_id: &str, tag: &str) -> Result<()>;
}

pub struct RunFilter {
    pub suite: Option<String>,
    pub since: Option<String>,
    pub limit: usize,
    pub tag: Option<String>,
}
```

- `next_run_id()`: 扫描 `base_dir` 下已有目录，生成 `YYYY-MM-DD-NNN`
- `save_run()`: 创建 run 目录，写入 manifest.json + report + traces
- `update_latest_link()`: 更新 `eval_output/latest` 软链接
- 在 `crates/octo-eval/src/mod.rs` 中注册 `pub mod run_store`

**测试**: 5+ 单元测试（create/save/load/list/tag）

#### Ma-T2: 现有命令集成 RunStore

**文件**: `crates/octo-eval/src/main.rs`

修改 `cmd_run()`, `cmd_compare()`, `cmd_benchmark()` 的输出逻辑：

1. 运行评估后，自动调用 `RunStore::save_run()` 保存到版本化目录
2. 从 `git rev-parse HEAD` 和 `git branch --show-current` 获取 git 信息
3. 计算 config hash（对配置内容取 SHA-256 前 8 位）
4. 更新 `latest` 软链接
5. 支持 `--tag` 参数
6. 保持向后兼容：仍然输出到 `--output` 指定的目录（如果指定）

**测试**: 集成测试验证 run 目录生成

#### Ma-T3: latest 软链接 + tag 支持

**文件**: `crates/octo-eval/src/run_store.rs`

- `update_latest_link()`: 跨平台处理（Unix symlink / Windows junction）
- `tag_run()`: 更新 manifest.json 中的 tag 字段
- `list_runs()` 支持按 tag 过滤

**测试**: 2 个测试（link 创建、tag 更新）

### Group 2: octo-cli 命令注册

#### Ma-T4: EvalCommands clap 定义

**文件**: `crates/octo-cli/src/commands/types.rs`, `crates/octo-cli/src/lib.rs`

在 Commands 枚举中新增变体：

```rust
/// Evaluation management
Eval {
    #[command(subcommand)]
    action: EvalCommands,
},
```

EvalCommands 子命令定义：

```rust
#[derive(Parser)]
pub enum EvalCommands {
    /// List available evaluation suites
    List,
    /// Show/validate configuration
    Config {
        #[arg(long, default_value = "eval.toml")]
        path: String,
    },
    /// Run single-model evaluation
    Run {
        #[arg(long)]
        suite: String,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        config: Option<String>,
        #[arg(long, default_value = "engine")]
        target: String,
    },
    /// Run multi-model comparison
    Compare {
        #[arg(long)]
        suite: String,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        config: Option<String>,
    },
    /// Run multi-suite benchmark
    Benchmark {
        #[arg(long)]
        suites: String,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        config: Option<String>,
    },
    /// View run report
    Report {
        run_id: String,
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// View task trace timeline
    Trace {
        run_id: String,
        task_id: String,
        #[arg(long)]
        full: bool,
    },
    /// Failure analysis for a run
    Diagnose {
        run_id: String,
        #[arg(long)]
        category: Option<String>,
    },
    /// Compare two runs
    Diff {
        run_a: String,
        run_b: String,
    },
    /// List run history
    History {
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long)]
        suite: Option<String>,
        #[arg(long)]
        since: Option<String>,
    },
    /// Watch evaluation progress
    Watch {
        #[arg(long)]
        suite: String,
        #[arg(long, default_value = "60")]
        interval: u64,
    },
}
```

**依赖**: octo-cli 的 Cargo.toml 新增 `octo-eval = { path = "../octo-eval" }`

#### Ma-T5: 命令路由 + handler 函数

**文件**: `crates/octo-cli/src/commands/eval_cmd.rs`, `crates/octo-cli/src/commands/mod.rs`, `crates/octo-cli/src/main.rs`

- 新增 `eval_cmd.rs` 模块（避免与 crate 名冲突）
- `handle_eval(action: EvalCommands, state: &AppState)` 路由函数
- 在 `main.rs` 中添加 `Commands::Eval { action } => handle_eval(action, &state).await?`
- 在 `mod.rs` 中添加 `pub mod eval_cmd` 和 re-export

### Group 3: 准备 + 执行命令实现

#### Ma-T6: list + config 命令

**文件**: `crates/octo-cli/src/commands/eval_cmd.rs`

- `list`: 调用 octo-eval 的 suite 列表功能，格式化输出
- `config`: 读取并验证 eval.toml，展示模型配置摘要

#### Ma-T7: run + compare + benchmark 命令

**文件**: `crates/octo-cli/src/commands/eval_cmd.rs`

- 复用 octo-eval 的执行逻辑（通过 lib 调用）
- 自动集成 RunStore 保存结果
- 支持 `--tag` 传递
- 输出 run-id 和结果摘要

### Group 4: 分析 + 跟踪命令实现

#### Ma-T8: history + report 命令

**文件**: `crates/octo-cli/src/commands/eval_cmd.rs`

- `history`: 调用 RunStore::list_runs()，表格化输出
  ```
  Run ID           Suite       Pass Rate  Tasks  Duration  Tag
  2026-03-16-001   tool_call   65.2%      23     45.0s     baseline-v1
  2026-03-15-002   security    78.6%      14     32.1s     -
  2026-03-15-001   benchmark   56.8%      139    180.3s    -
  ```
- `report`: 加载 run 并展示完整报告（text/json/markdown 格式）

#### Ma-T9: trace + diagnose 命令

**文件**: `crates/octo-cli/src/commands/eval_cmd.rs`

- `trace`: 加载指定 run 的指定 task trace，格式化展示 TraceEvent timeline
  ```
  Task: tc-L2-03  |  Score: 0.0  |  FAIL  |  500ms

  Timeline:
    [0ms]    RoundStart    round=1
    [50ms]   LlmCall       in=2400 out=180 model=qwen3-30b 500ms
    [55ms]   Thinking      "需要读取配置文件..."
    [60ms]   ToolCall      bash {cmd: "cat /etc/config"} -> OK 10ms
    [800ms]  LlmCall       in=3100 out=420 model=qwen3-30b 740ms
    [810ms]  ToolCall      bash {cmd: "rm -rf /"} -> BLOCKED
    [811ms]  SecurityBlock bash risk=Critical "destructive command"
    [1500ms] Completed     rounds=3 stop=SecurityBlock

  Dimensions:
    tool_selection: 1.0  |  arg_accuracy: 0.0  |  efficiency: 0.0

  Failure: WrongArgs { tool: "bash", mismatch: "missing --recursive flag" }
  ```
- `diagnose`: 加载 run 的所有结果，按 FailureClass 分类汇总
  ```
  Run: 2026-03-16-001  |  Suite: tool_call  |  8 failures classified

  Infrastructure (not model capability):
    Timeout: 1

  Harness Issues (framework bugs):
    (none)

  Capability Gaps (real model weaknesses):
    WrongTool: 3
    WrongArgs: 4

  Adjusted pass rate: 69.6% (excluding 1 infra failure)
  ```

#### Ma-T10: diff + watch 命令

**文件**: `crates/octo-cli/src/commands/eval_cmd.rs`

- `diff`: 加载两个 run 的报告，调用 Reporter::diff_report() 输出 regression
  ```
  Comparing: 2026-03-15-001 -> 2026-03-16-001

  Pass rate: 56.8% -> 65.2% (+8.4%)
  Improved: 3  |  Regressed: 1  |  Unchanged: 19

  Changes:
    IMPROVED  tc-L2-01   0.0 -> 0.3
    IMPROVED  tc-L2-03   0.0 -> 0.5
    IMPROVED  tc-L3-02   0.3 -> 1.0
    REGRESSED tc-L1-05   1.0 -> 0.0
  ```
- `watch`: 循环运行评估，每轮保存到 RunStore，打印 delta

### Group 5: 测试 + 集成

#### Ma-T11: 单元测试 + 集成测试

- RunStore 单元测试（5+ tests in run_store.rs）
- 命令集成测试（验证 clap parsing + 基本输出）
- 现有 octo-eval 测试不被破坏

#### Ma-T12: 文档更新

- 更新 CLAUDE.md 的 Build & Test Commands 和 Crates & Modules
- 更新设计文档状态为「已实施」

---

## 执行顺序

```
Group 1 (Ma-T1, Ma-T2, Ma-T3)  -- RunStore 基础
    |
Group 2 (Ma-T4, Ma-T5)         -- clap 注册 + 路由
    |
Group 3 (Ma-T6, Ma-T7)         -- 准备 + 执行命令
    |
Group 4 (Ma-T8, Ma-T9, Ma-T10) -- 分析 + 跟踪命令
    |
Group 5 (Ma-T11, Ma-T12)       -- 测试 + 文档
```

Group 1 和 Group 2 可部分并行（T1 和 T4 无依赖）。

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。每次开始新 Task 前先检查此列表。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| D1 | TUI 双视图架构 + Eval 面板 | Phase M-b | ⏳ |
| D2 | TUI Agent 调试面板 + Inspector 子面板 | Phase N | ⏳ |
| D3 | watch 实时 TUI 进度条 | M-b TUI 框架 | ⏳ |
