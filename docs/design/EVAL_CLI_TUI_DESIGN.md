# 评估管理 CLI + 开发调试 TUI 设计

> Phase M-a / M-b / N 整体规划
> 状态：设计方案
> 日期：2026-03-15

---

## 一、背景与目标

Phase L 完成了评估系统白盒化（TraceEvent、FailureClass、多维评分），但白盒数据仅输出到文件，缺乏交互式管理能力。同时 octo-cli TUI 当前只有运维视图（12 Tab），尚未落地 AGENT_CLI_DESIGN.md §6.9.2 中规划的开发调试视图。

### 目标

1. **octo-eval 版本化存储** — 每次评估运行纳入版本管理，可追溯、可比较
2. **octo-cli `octo eval` 命令族** — 统一命令入口覆盖评估全生命周期
3. **TUI 双视图架构** — Ops（运维）/ Dev（开发调试）两个视图，各自聚焦
4. **Dev-Eval 面板** — 三栏面板式评估管理，TraceEvent timeline + 归因分析

### 职责边界

```
octo-eval（评估引擎 — 独立可运行）     octo-cli（统一入口 + 开发调试平台）
┌──────────────────────────┐           ┌─────────────────────────────────┐
│ - 评估执行全过程           │           │ - `octo eval` 命令行代理调用     │
│ - Scorer / Suite          │           │ - TUI Eval 面板：交互式浏览      │
│ - Trace 采集              │◄── lib ──►│ - Trace Timeline 可视化          │
│ - 归因分析                │           │ - 失败归因钻取                   │
│ - 版本化报告生成           │           │ - 持续调试跟踪（watch/diff）     │
│ - 独立二进制              │           │ - 评估历史管理                   │
└──────────────────────────┘           └─────────────────────────────────┘
```

octo-eval 是**能力引擎**，独立运行产出完整报告；octo-cli 通过 lib 依赖调用 octo-eval，提供命令行 + TUI 两层交互界面管理评估全生命周期。

---

## 二、版本化存储模型

### 2.1 目录结构

```
eval_output/
├── runs/
│   ├── 2026-03-15-001/            # run-id = 日期-序号
│   │   ├── manifest.json          # 运行元数据
│   │   ├── report.json            # 完整评估报告
│   │   ├── report.md              # Markdown 报告
│   │   ├── traces/                # 该次运行的所有 trace
│   │   │   ├── trace_tc-L1-01.json
│   │   │   └── eval_traces.jsonl
│   │   └── comparison.json        # 多模型对比（如有）
│   ├── 2026-03-15-002/
│   └── 2026-03-16-001/
└── latest -> runs/2026-03-16-001  # 软链接指向最新
```

### 2.2 manifest.json 结构

```json
{
  "run_id": "2026-03-15-001",
  "tag": null,
  "timestamp": "2026-03-15T14:30:00+08:00",
  "command": "compare",
  "suite": "tool_call",
  "models": ["Qwen3-30B", "DeepSeek-V3"],
  "git_commit": "f28ad6c",
  "git_branch": "main",
  "task_count": 23,
  "passed": 15,
  "pass_rate": 0.652,
  "avg_score": 0.72,
  "duration_ms": 45000,
  "total_tokens": 128000,
  "estimated_cost": 0.85,
  "eval_config_hash": "a1b2c3d4",
  "failure_summary": {
    "by_class": {"WrongTool": 3, "WrongArgs": 4, "Timeout": 1},
    "total_classified": 8,
    "total_unclassified": 0
  }
}
```

### 2.3 run-id 生成策略

- **默认**：日期-序号，格式 `YYYY-MM-DD-NNN`，自动递增
- **可选 tag**：`--tag baseline-v1` 生成 `2026-03-15-001` 并在 manifest 中记录 tag
- **Git commit** 自动记录在 manifest 中，不需放入 run-id

### 2.4 实现位置

在 `octo-eval` crate 中新增 `run_store.rs` 模块：

```rust
pub struct RunStore {
    base_dir: PathBuf,  // eval_output/runs/
}

impl RunStore {
    pub fn next_run_id(&self) -> String;           // 生成下一个 run-id
    pub fn save_run(&self, manifest, report, traces) -> Result<PathBuf>;
    pub fn list_runs(&self) -> Result<Vec<RunManifest>>;
    pub fn load_run(&self, run_id: &str) -> Result<EvalRun>;
    pub fn diff_runs(&self, a: &str, b: &str) -> Result<RegressionReport>;
    pub fn update_latest_link(&self, run_id: &str) -> Result<()>;
}
```

---

## 三、`octo eval` 命令族

### 3.1 命令矩阵

| 阶段 | 命令 | 作用 | 对应 octo-eval 功能 |
|------|------|------|-------------------|
| **准备** | `octo eval list` | 列出可用 suites/tasks | `cmd_list_suites()` |
| | `octo eval config` | 查看/验证 eval.toml 配置 | 新增 |
| **执行** | `octo eval run --suite X` | 单模型运行 | `cmd_run()` |
| | `octo eval compare --suite X` | 多模型对比 | `cmd_compare()` |
| | `octo eval benchmark --suites A,B,C` | 跨 suite 聚合 | `cmd_benchmark()` |
| **分析** | `octo eval report <run-id>` | 查看某次运行的报告 | RunStore + Reporter |
| | `octo eval trace <run-id> <task-id>` | 查看某任务的 TraceEvent timeline | RunStore + EvalTrace |
| | `octo eval diagnose <run-id>` | 失败归因分析汇总 | RunStore + FailureSummary |
| | `octo eval diff <run-a> <run-b>` | 两次运行的 regression 对比 | RunStore + Reporter::diff_report |
| **跟踪** | `octo eval history` | 评估运行历史列表 | RunStore::list_runs |
| | `octo eval watch --suite X` | 持续运行 + 实时展示进度 | 新增流式输出 |

### 3.2 命令行参数设计

```
octo eval run --suite tool_call [--tag baseline-v1] [--config eval.toml] [--target engine|cli|server]
octo eval compare --suite tool_call [--tag v2-compare]
octo eval benchmark --suites tool_call,security,reasoning [--tag full-benchmark]
octo eval report 2026-03-15-001 [--format json|markdown|text]
octo eval trace 2026-03-15-001 tc-L2-03 [--full]
octo eval diagnose 2026-03-15-001 [--category infrastructure|harness|capability]
octo eval diff 2026-03-15-001 2026-03-16-001
octo eval history [--limit 20] [--suite tool_call] [--since 2026-03-01]
octo eval watch --suite tool_call [--interval 60]
```

### 3.3 实现位置

`crates/octo-cli/src/commands/eval.rs` — 新增模块，所有命令通过 lib 调用 octo-eval。

```rust
// lib.rs 新增
pub enum Commands {
    // ... 现有命令 ...
    /// Evaluation management
    Eval {
        #[command(subcommand)]
        action: EvalCommands,
    },
}
```

octo-cli Cargo.toml 新增依赖：`octo-eval = { path = "../octo-eval" }`

---

## 四、TUI 双视图架构

### 4.1 视图切换

- `Ctrl+O` — 切换到 Ops（运维）视图
- `Ctrl+D` — 切换到 Dev（开发调试）视图
- 启动时默认进 Dev 视图

### 4.2 Ops 视图（运维）

Tab 分页模式，精简为 6 个 Tab：

```
 Dashboard | Agents | Sessions | MCP | Security | Logs
```

保持现有 Tab 切换交互（Tab/Shift+Tab、数字键 1-6）。

Welcome 和 Settings 作为叠加层（`?` 帮助、`,` 设置），不占 Tab 位。

### 4.3 Dev 视图（开发调试）

**2 个主任务 + Inspector 子面板**，面板式布局：

```
 ① Agent 调试    ② Eval 评估
```

数字键 1-2 切换主任务。每个任务有专属的三栏面板布局。

### 4.4 层次关系

```
Agent 调试（主体）
├── Skill/Tool  — Agent 调用的能力扩展
├── MCP         — Agent 的工具来源
├── Provider    — Agent 的 LLM 后端
└── Memory      — Agent 的记忆基础设施

Eval 评估（独立维度）
└── 从外部评估 Agent 整体表现，横切所有组件
```

Agent 的 4 个子系统在 Inspector 中按需查看（`S/M/P/R` 单键切换）。

---

## 五、Dev-Agent 调试面板

### 5.1 三栏布局

```
┌─ Agent 调试 ────────────────────────────────────────────────┐
│ Sessions        │ Conversation         │ Inspector [S]      │
│                 │                      │                    │
│ > session-01    │ User: 帮我重构...     │ -- Skills --       │
│   session-02    │ Agent: [thinking]    │ debugger    loaded │
│   session-03    │   需要读取文件...     │ git-helper  loaded │
│                 │ Agent: [tool_call]   │ test-gen    error  │
│                 │   file_read -> OK    │                    │
│                 │ Agent: 好的，我来... │ Trigger Log:       │
│                 │                      │ debugger matched   │
│                 │                      │ pattern: "重构"    │
├─────────────────┤                      ├────────────────────┤
│ Context: 62%    │                      │ [S]kill [M]cp      │
│ ========--      │                      │ [P]rovider [R]mem  │
└─────────────────┴──────────────────────┴────────────────────┘
```

### 5.2 Inspector 子面板

| 按键 | 子面板 | 关注内容 |
|------|--------|---------|
| `S` | Skill | Skill manifest 列表、触发日志、执行状态 |
| `M` | MCP | 服务器连接状态、工具发现列表、stdio/SSE 日志 |
| `P` | Provider | 模型列表、请求/响应、token/cost、failover 链路 |
| `R` | Memory | L0-L2 层内容浏览、KG 实体/关系、检索测试 |

### 5.3 导航

- `h/l` 或左右箭头 — 三栏间切换焦点
- `j/k` 或上下箭头 — 当前栏内滚动
- `Enter` — 展开/选中
- `Esc` — 返回上层
- `S/M/P/R` — 切换 Inspector 子面板

---

## 六、Dev-Eval 评估面板

### 6.1 三栏布局

```
┌─ Eval 评估 ─────────────────────────────────────────────────┐
│ Run History          │ Task Results          │ Detail        │
│                      │                       │               │
│ > 03-16-001          │ OK tc-L1-01  1.0 120ms│ -- Timeline --│
│   tool_call          │ OK tc-L1-02  1.0  95ms│ [0ms] Round#1 │
│   65.2%  15/23       │ NG tc-L2-01  0.3 340ms│ [50ms] LlmCall│
│                      │ >NG tc-L2-03 0.0 500ms│ [60ms] Tool OK│
│   03-15-002          │ OK tc-L3-01  0.8 180ms│ [810ms] Block │
│   security           │                       │               │
│   78.6%  11/14       │ -- Dimensions --      │ -- Failure -- │
│                      │ tool_selection: 1.0    │ WrongArgs     │
│   03-15-001          │ arg_accuracy:   0.0    │ tool: bash    │
│   benchmark          │ efficiency:     0.0    │ missing --flag│
│   56.8%              │                       │               │
├──────────────────────┤ -- Failure Summary -- │               │
│ [r]un [d]iff         │ WrongTool: 3          │ [j/k] scroll  │
│ [/]filter [t]ag      │ WrongArgs: 4          │ [Enter] expand│
│                      │ Timeout: 1            │ [Esc] back    │
└──────────────────────┴───────────────────────┴───────────────┘
```

### 6.2 三栏联动

- **左栏**（Run History）：选中某次 run -> 中栏更新 task 列表
- **中栏**（Task Results）：选中某个 task -> 右栏更新 timeline + 归因
- **右栏**（Detail Inspector）：TraceEvent 时间线 + FailureClass + 多维评分

### 6.3 快捷操作

| 按键 | 作用 |
|------|------|
| `r` | 触发新的评估运行 |
| `d` | diff 当前选中的 run 与另一个 run |
| `/` | 过滤（按 suite、日期、tag） |
| `t` | 给当前 run 添加 tag |
| `Enter` | 展开 trace 事件详情 |
| `Esc` | 收起/返回 |

---

## 七、实施分批

### Phase M-a：评估管理 CLI（纯命令行）

| 任务 | 内容 | 涉及 crate |
|------|------|-----------|
| Ma-T1 | RunStore 版本化存储 + manifest.json | octo-eval |
| Ma-T2 | 现有 cmd_run/compare/benchmark 集成 RunStore | octo-eval |
| Ma-T3 | `octo eval` 命令族注册（clap subcommand） | octo-cli |
| Ma-T4 | eval list/config/run/compare/benchmark 命令实现 | octo-cli |
| Ma-T5 | eval history/report/trace/diagnose/diff 命令实现 | octo-cli |
| Ma-T6 | eval watch 持续运行 + 流式进度输出 | octo-cli |
| Ma-T7 | latest 软链接 + tag 支持 | octo-eval |
| Ma-T8 | 测试 + 文档 | 全部 |

### Phase M-b：TUI 双视图 + Eval 面板

| 任务 | 内容 | 涉及模块 |
|------|------|---------|
| Mb-T1 | ViewMode 枚举（Ops/Dev） + Ctrl+O/D 切换 | tui/mod.rs |
| Mb-T2 | Ops 视图：精简为 6 Tab | tui/screens/ |
| Mb-T3 | Dev 视图框架：2 主任务选择器 | tui/screens/ |
| Mb-T4 | Dev-Eval 面板：三栏布局 + Run History | tui/screens/eval.rs |
| Mb-T5 | Dev-Eval 面板：Task Results + Dimensions | tui/screens/eval.rs |
| Mb-T6 | Dev-Eval 面板：Detail Inspector（Timeline + Failure） | tui/screens/eval.rs |
| Mb-T7 | Dev-Eval 联动：三栏选择联动 + 快捷键 | tui/screens/eval.rs |
| Mb-T8 | 测试 + Welcome/Settings 叠加层 | 全部 |

### Phase N（后续）：Workbench Agent 调试面板

| 任务 | 内容 |
|------|------|
| N-T1 | Dev-Agent 三栏布局（Sessions + Conversation + Inspector） |
| N-T2 | Inspector 子面板：Skill |
| N-T3 | Inspector 子面板：MCP |
| N-T4 | Inspector 子面板：Provider |
| N-T5 | Inspector 子面板：Memory |
| N-T6 | Context 使用率实时条 |
| N-T7 | 落地 AGENT_CLI_DESIGN.md §6.9.2 完整 Workbench 模式 |

---

## 八、依赖关系

```
Phase M-a（CLI 命令）  ← 无前置依赖，可立即开始
      │
      ▼
Phase M-b（TUI Eval）  ← 依赖 M-a 的 RunStore
      │
      ▼
Phase N（Agent 调试）   ← 依赖 M-b 的 TUI 双视图框架
```

---

## 九、设计决策记录

| 编号 | 决策 | 理由 |
|------|------|------|
| D1 | octo-eval 保持独立二进制 | 单一职责，可独立运行完整评估 |
| D2 | octo-cli 通过 lib 依赖调用 octo-eval | 复用评估引擎，避免重复实现 |
| D3 | run-id 采用日期序号 + 可选 tag | 自动生成零成本，tag 满足里程碑标记 |
| D4 | TUI 双视图用 Ctrl+O/D 切换 | 避免 Fn 键兼容性问题 |
| D5 | Dev 视图只有 2 个主任务 | Agent 调试是主体，Eval 是独立维度，子系统在 Inspector 中 |
| D6 | Dev 视图采用面板式布局而非 Tab | 开发调试需要聚焦工作整体性，多信息同屏 |
| D7 | Ops 视图保持 Tab 分页 | 运维用顺序分页逐个查看可接受 |
| D8 | 分批实施 M-a -> M-b -> N | 逐步交付价值，降低风险 |
