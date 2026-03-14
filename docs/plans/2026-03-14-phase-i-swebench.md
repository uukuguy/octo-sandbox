# Phase I — 完整 SWE-bench 适配

**日期**: 2026-03-14
**前置**: Phase H COMPLETE（resilience suite + AstMatch + context 扩充）
**目标**: 适配 SWE-bench Verified 数据集，实现 Issue → Agent Patch → Tests Pass 全流程，产出 octo 的首个行业标准横向对比数据

---

## 背景

SWE-bench 是当前智能体评估的金标准。它验证的是"给定一个 GitHub Issue，Agent 能否自动生成正确的代码修复"。这是 octo 评估体系中**对外说服力最强**的维度。

当前 E2E suite（14 fixtures）验证的是预制的小型 bug-fix，与 SWE-bench 的区别：
- E2E fixture: 预制的 buggy code + known fix，验证评分管线
- SWE-bench: 真实 GitHub 仓库快照 + 真实 Issue + 真实测试套件，验证 Agent 端到端能力

---

## 一、SWE-bench 数据集分析

### SWE-bench Verified 结构

每个 task 包含：
```json
{
    "instance_id": "django__django-16527",
    "repo": "django/django",
    "base_commit": "abc123...",
    "patch": "diff --git a/...",          // gold patch (用于验证)
    "test_patch": "diff --git a/...",     // 新增/修改的测试
    "problem_statement": "Issue 描述...",  // agent 的输入
    "hints_text": "...",                  // 可选提示
    "environment_setup_commit": "...",
    "FAIL_TO_PASS": "[\"test_xxx\"]",     // 修复后应通过的测试
    "PASS_TO_PASS": "[\"test_yyy\"]",     // 修复后仍应通过的测试
}
```

### 数据量

| 子集 | 任务数 | 说明 |
|------|--------|------|
| SWE-bench Full | 2,294 | 完整集 |
| SWE-bench Verified | 500 | 人工验证的高质量子集 |
| SWE-bench Lite | 300 | 更简单的子集 |

### 目标范围

**Phase I 适配 SWE-bench Lite 子集的 50 个精选任务**：
- 选择依据：Python 项目、setup 简单、测试运行快、涵盖不同难度
- 推荐仓库：django, flask, sympy, requests, pytest（生态成熟，依赖易管理）

---

## 二、技术架构

### 整体流程

```
swe_bench.jsonl (50 tasks)
       │
       ▼
  SweDatasetLoader        ── 解析 SWE-bench JSONL 格式
       │
       ▼
  SweSuite::load()        ── 生成 Vec<Box<dyn EvalTask>>
       │
       ▼
  EvalRunner::run_task()  ── Agent 生成 patch
       │
       ▼
  SweVerifier             ── Docker 沙箱内验证 patch
       │
       ├── 1. Clone repo @ base_commit
       ├── 2. Apply test_patch
       ├── 3. Apply agent patch
       ├── 4. Run FAIL_TO_PASS tests → must pass
       ├── 5. Run PASS_TO_PASS tests → must still pass
       └── 6. Score: both conditions → pass
```

### 组件设计

#### I-1: SWE-bench 数据加载器 (`datasets/swe_bench.rs`)

```rust
/// SWE-bench JSONL task record
#[derive(Debug, Deserialize)]
pub struct SweBenchRecord {
    pub instance_id: String,
    pub repo: String,
    pub base_commit: String,
    pub patch: String,                    // gold patch (仅用于离线验证)
    pub test_patch: String,               // 新增测试
    pub problem_statement: String,        // agent 输入
    pub hints_text: Option<String>,
    pub environment_setup_commit: Option<String>,
    pub fail_to_pass: String,             // JSON array string
    pub pass_to_pass: String,             // JSON array string
}

/// 将 SWE-bench record 转为 EvalTask
pub struct SweBenchTask {
    record: SweBenchRecord,
}

impl EvalTask for SweBenchTask {
    fn id(&self) -> &str { &self.record.instance_id }
    fn prompt(&self) -> &str { &self.record.problem_statement }
    fn available_tools(&self) -> Option<Vec<ToolSpec>> {
        // 提供: bash, file_read, file_write, file_edit
        Some(swe_bench_tools())
    }
    fn score(&self, output: &AgentOutput) -> EvalScore {
        // 从 output 中提取 agent 生成的 diff/patch
        // 调用 SweVerifier 验证
        // 注意: score() 是同步的，验证需要异步 Docker 调用
        // 解决方案: 在 runner.rs 中为 SWE-bench 任务特殊处理
        EvalScore::fail(0.0, ScoreDetails::Custom { message: "SWE verification pending".into() })
    }
    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: "swe_bench".into(),
            difficulty: classify_swe_difficulty(&self.record),
            expected_steps: Some(10),
            tags: vec!["swe-bench".into(), self.record.repo.clone()],
        }
    }
}

/// 根据 patch 大小和测试数量分类难度
fn classify_swe_difficulty(record: &SweBenchRecord) -> Difficulty {
    let patch_lines = record.patch.lines().count();
    let fail_tests: Vec<String> = serde_json::from_str(&record.fail_to_pass).unwrap_or_default();
    match (patch_lines, fail_tests.len()) {
        (0..=20, 1) => Difficulty::Easy,
        (0..=50, 1..=3) => Difficulty::Medium,
        _ => Difficulty::Hard,
    }
}
```

#### I-2: SWE-bench 验证器 (`swe_verifier.rs`)

```rust
/// SWE-bench patch verifier — 在 Docker 沙箱中验证 agent patch
pub struct SweVerifier {
    docker: DockerAdapter,
}

impl SweVerifier {
    /// 验证 agent 生成的 patch
    ///
    /// 流程:
    /// 1. 启动 Docker 容器 (python:3.11-slim)
    /// 2. Clone repo 到容器内
    /// 3. Checkout base_commit
    /// 4. Apply test_patch (新增测试)
    /// 5. Apply agent_patch (agent 生成的修复)
    /// 6. 安装依赖 (pip install -e .)
    /// 7. 运行 FAIL_TO_PASS 测试 → 必须全部通过
    /// 8. 运行 PASS_TO_PASS 测试 → 必须仍然通过
    /// 9. 返回 SweVerifyResult
    pub async fn verify(
        &self,
        record: &SweBenchRecord,
        agent_patch: &str,
    ) -> Result<SweVerifyResult> {
        // ... Docker 沙箱执行逻辑
    }
}

pub struct SweVerifyResult {
    pub fail_to_pass_results: Vec<TestResult>,
    pub pass_to_pass_results: Vec<TestResult>,
    pub all_fail_to_pass_passed: bool,
    pub all_pass_to_pass_passed: bool,
    pub passed: bool,  // both conditions
    pub execution_time_ms: u64,
}
```

#### I-3: SWE Suite (`suites/swe_bench.rs`)

```rust
pub struct SweBenchSuite;

impl SweBenchSuite {
    const DEFAULT_DATASET: &'static str = "datasets/swe_bench_lite.jsonl";

    pub fn load() -> Result<Vec<Box<dyn EvalTask>>> {
        let path = Self::resolve_path()?;
        load_swe_bench_as_tasks(&path)
    }

    /// Mock 模式: 用 gold patch 替代 agent patch 验证管线
    pub async fn run_mock() -> Result<EvalReport> { ... }

    /// Live 模式: Agent 生成 patch + Docker 验证
    pub async fn run_live(provider: Arc<dyn Provider>) -> Result<EvalReport> { ... }
}
```

#### I-4: Runner 集成

```rust
// runner.rs — run_task() 中为 SWE-bench 任务特殊处理
match task.metadata().category.as_str() {
    "swe_bench" => {
        // 1. Agent loop 生成 patch (通过 file_write/bash 工具)
        // 2. 从 agent output 中提取 diff
        // 3. 调用 SweVerifier::verify() 验证
        // 4. 用 SweVerifyResult 生成 EvalScore
    }
    _ => { /* 现有逻辑 */ }
}
```

---

## 三、任务分组

### I1: 数据准备 (无代码改动)

**I1-T1: 下载并精选 SWE-bench Lite 子集**

- 从 HuggingFace 下载 SWE-bench Lite 数据集
- 精选 50 个任务（按仓库和难度均衡分布）
- 转换为 `crates/octo-eval/datasets/swe_bench_lite.jsonl`

选择标准：
- 仅 Python 项目
- 单文件修改优先（降低验证复杂度）
- patch < 100 行
- 测试运行 < 60s
- 覆盖 5+ 不同仓库

目标分布：
| 仓库 | easy | medium | hard | 合计 |
|------|------|--------|------|------|
| django | 3 | 5 | 2 | 10 |
| flask | 3 | 3 | 1 | 7 |
| sympy | 2 | 4 | 2 | 8 |
| requests | 3 | 3 | 1 | 7 |
| pytest | 2 | 3 | 2 | 7 |
| 其他 | 3 | 5 | 3 | 11 |
| **合计** | **16** | **23** | **11** | **50** |

**I1-T2: 创建 SWE-bench Docker 镜像**

```dockerfile
# Dockerfile.swe-bench
FROM python:3.11-slim

RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*
RUN pip install pytest

WORKDIR /workspace
```

构建并推送到本地 registry 或保存为 tar：
```bash
docker build -t octo-swe-bench:latest -f Dockerfile.swe-bench .
```

### I2: 数据加载器

**I2-T1: 实现 `datasets/swe_bench.rs`**

新文件: `crates/octo-eval/src/datasets/swe_bench.rs` (~120 行)

内容:
- `SweBenchRecord` 结构体
- `SweBenchTask` 实现 `EvalTask`
- `load_swe_bench_as_tasks()` 函数
- `classify_swe_difficulty()` 函数
- `swe_bench_tools()` — 返回 SWE-bench 评估可用的工具集

测试: 2 个测试（加载 + 难度分类）

**I2-T2: 更新 `datasets/mod.rs`**

添加 `pub mod swe_bench;`

### I3: 验证器

**I3-T1: 实现 `swe_verifier.rs`**

新文件: `crates/octo-eval/src/swe_verifier.rs` (~200 行)

核心方法:
- `SweVerifier::new(docker: DockerAdapter)` — 创建验证器
- `SweVerifier::verify(record, agent_patch)` — 完整验证流程
- `SweVerifier::verify_with_gold(record)` — 用 gold patch 验证（mock 模式）
- `extract_patch_from_output(output: &AgentOutput)` — 从 agent 输出中提取 diff

Docker 容器内执行脚本：
```bash
#!/bin/bash
set -e
cd /workspace
git clone --depth 1 $REPO .
git checkout $BASE_COMMIT
# Apply test patch
echo "$TEST_PATCH" | git apply
# Apply agent patch
echo "$AGENT_PATCH" | git apply
# Install
pip install -e . 2>/dev/null || true
# Run FAIL_TO_PASS tests
pytest $FAIL_TO_PASS_TESTS --tb=short 2>&1
FAIL_EXIT=$?
# Run PASS_TO_PASS tests
pytest $PASS_TO_PASS_TESTS --tb=short 2>&1
PASS_EXIT=$?
echo "RESULTS:$FAIL_EXIT:$PASS_EXIT"
```

ScoreDetails 新增变体:
```rust
SweVerify {
    instance_id: String,
    fail_to_pass_passed: bool,
    pass_to_pass_passed: bool,
    fail_to_pass_count: usize,
    pass_to_pass_count: usize,
    execution_time_ms: u64,
}
```

测试:
- 1 个单测验证 patch 提取逻辑
- 1 个集成测试（需 Docker）验证 gold patch 验证流程

**I3-T2: 新增 ScoreDetails::SweVerify 变体**

文件改动: `crates/octo-eval/src/score.rs` (~10 行)

### I4: Suite + Runner 集成

**I4-T1: 实现 `suites/swe_bench.rs`**

新文件: `crates/octo-eval/src/suites/swe_bench.rs` (~80 行)

- `SweBenchSuite::load()` — 加载 JSONL 为 EvalTask
- `SweBenchSuite::run_mock()` — 用 gold patch 验证管线
- 注册到 `suites/mod.rs`

**I4-T2: Runner 集成 SWE-bench 特殊处理**

文件改动: `crates/octo-eval/src/runner.rs` (~60 行)

在 `run_task()` 中为 category="swe_bench" 的任务添加 post-scoring 验证步骤：
1. 从 AgentOutput 中提取生成的 patch（查找 file_write 工具调用或 bash 中的 diff 输出）
2. 创建 SweVerifier 实例
3. 调用 `verify()` 进行 Docker 沙箱验证
4. 用 SweVerifyResult 覆盖 score

**I4-T3: CLI 注册**

文件改动: `crates/octo-eval/src/main.rs` (~10 行)

- `load_suite()` 添加 `"swe_bench"` 分支
- `cmd_list_suites()` 添加 SWE-bench 描述
- `cmd_run_direct_suite()` 添加 `"swe_bench"` mock 模式

### I5: 验证与收尾

**I5-T1: Mock 模式端到端测试**

```bash
# 用 gold patch 验证管线（需 Docker）
cargo run -p octo-eval -- run --suite swe_bench --output eval_output/swe_bench
```

**I5-T2: 更新 eval-ci.yml**

Docker 可用时才运行 SWE-bench：
```yaml
- name: Run SWE-bench mock verification (requires Docker)
  if: env.DOCKER_AVAILABLE == 'true'
  run: cargo run -p octo-eval -- run --suite swe_bench --output eval_output/swe_bench
```

**I5-T3: 全量测试**

```bash
cargo test --workspace -- --test-threads=1
```

---

## 四、文件改动矩阵

| 文件 | 操作 | 行数估计 |
|------|------|---------|
| `crates/octo-eval/src/datasets/swe_bench.rs` | **新建** | ~120 |
| `crates/octo-eval/src/datasets/mod.rs` | 修改 | +1 |
| `crates/octo-eval/src/swe_verifier.rs` | **新建** | ~200 |
| `crates/octo-eval/src/lib.rs` | 修改 | +1 |
| `crates/octo-eval/src/score.rs` | 修改 | +10 |
| `crates/octo-eval/src/suites/swe_bench.rs` | **新建** | ~80 |
| `crates/octo-eval/src/suites/mod.rs` | 修改 | +1 |
| `crates/octo-eval/src/runner.rs` | 修改 | +60 |
| `crates/octo-eval/src/main.rs` | 修改 | +15 |
| `crates/octo-eval/datasets/swe_bench_lite.jsonl` | **新建** | ~50 行 |
| `crates/octo-eval/Dockerfile.swe-bench` | **新建** | ~10 |
| `.github/workflows/eval-ci.yml` | 修改 | +10 |

**总计**: 5 新文件, 7 修改, ~550 行新增

---

## 五、依赖

- **Docker daemon**: SWE-bench 验证必须有 Docker（Phase J 会修复 Docker 测试）
- **网络访问**: 需要 clone GitHub 仓库到容器内（首次运行时）
- **磁盘**: 每个仓库 clone ~100MB-1GB，50 个任务需要 ~10GB 临时空间

### 降级策略

如果 Docker 不可用：
1. `SweBenchSuite::run_mock()` 退化为仅验证 JSONL 加载和 patch 格式
2. 跳过实际的容器化验证
3. CI 中通过环境变量 `DOCKER_AVAILABLE` 条件执行

---

## 六、验收标准

- [ ] `swe_bench_lite.jsonl` 包含 50 个精选任务
- [ ] `SweBenchTask` 正确实现 `EvalTask` trait
- [ ] `SweVerifier::verify_with_gold()` 对 gold patch 返回 passed=true（需 Docker）
- [ ] `cargo run -p octo-eval -- list-suites` 显示 swe_bench
- [ ] `cargo run -p octo-eval -- run --suite swe_bench` mock 模式可运行
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过
- [ ] Docker 不可用时优雅降级（不崩溃）
