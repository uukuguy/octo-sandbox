# GAIA / SWE-bench 标准验证设计方案

**日期**: 2026-03-16
**状态**: 已确认，准备实施
**前置**: Phase Q1 (TaskRecord scoring bug fix, commit 0e2ac3e)
**关联**: `AGENT_EVALUATION_DESIGN.md`, `EVAL_WHITEBOX_DESIGN.md`

---

## 一、目标

将 GAIA 和 SWE-bench 评估从"内部实验性"提升到**可对标业界 SOTA 的标准验证**。

核心原则：
- **数据集来自官方**，格式可适配但内容不可篡改
- **评分标准对标官方**，不做 fallback、不做宽松匹配
- **工具链完整真实**，所有 agent 内置工具必须是真实实现
- **Docker 是标准能力**，不是障碍

---

## 二、业界 SOTA 参照（2026-03）

### GAIA（General AI Assistants）

| 排名 | Agent | 组织 | Overall | L1 | L2 | L3 | 日期 |
|------|-------|------|---------|-----|-----|-----|------|
| 1 | OPS-Agentic-Search | Alibaba Cloud | 92.36% | 98.92 | 90.57 | 85.71 | 2026-03 |
| 2 | openJiuwen-deepagent | openJiuwen | 91.69% | 98.92 | 88.68 | 87.76 | 2026-02 |
| 4 | Nemotron-ToolOrchestra | NVIDIA | 90.37% | 96.77 | 86.79 | 89.80 | 2026-01 |
| — | Human | — | 92% | — | — | — | — |
| ref | 单模型裸跑 (GPT-5 Mini) | — | ~44.8% | — | — | — | — |
| ref | 单模型裸跑 (Claude 3.7 Sonnet) | — | ~43.9% | — | — | — | — |

**关键洞察**: 顶级 agent 系统 (multi-model ensemble + 真实工具链) 已接近人类水平。单模型裸跑约 44%。octo-eval 属于单模型 + 真实工具链，合理目标 30-50%。

### SWE-bench

| 变体 | Top Score | 状态 | 说明 |
|------|-----------|------|------|
| **Verified** (500) | 79.2% (Claude Opus 4.6 Thinking) | 饱和+污染 | 70%+ 已无区分度 |
| **Lite** (300) | ~55% | 活跃 | 推荐入门 |
| **Pro** (1865) | ~43.6% (Claude 4.5 Sonnet) | 活跃(推荐) | 多语言，最有区分度 |

**关键洞察**: SWE-bench Verified 已被数据污染（OpenAI 确认前沿模型训练数据包含测试集）。Lite 是实际可行的入门选择，Pro 是未来目标。

---

## 三、GAIA 标准验证方案

### 3.1 数据集

- **来源**: HuggingFace `gaia-benchmark/GAIA` validation split
- **现有**: `datasets/gaia_sample.jsonl` — 165 tasks，已从官方转换
- **分布**: L1:53, L2:86, L3:26
- **附件**: 38 tasks 有文件附件，已下载至 `datasets/gaia_files/`
- **全部 165 条 final_answer 非空** — 已验证

### 3.2 工具链

Agent 使用 `octo_engine::tools::default_tools()` 提供的真实工具：

| 工具 | 实现文件 | 状态 | GAIA 用途 |
|------|---------|------|-----------|
| `bash` | `tools/bash.rs` | 真实执行 | 数据处理、计算 |
| `file_read` | `tools/file_read.rs` | 真实执行 | 读取附件 |
| `file_write` | `tools/file_write.rs` | 真实执行 | 保存中间结果 |
| `file_edit` | `tools/file_edit.rs` | 真实执行 | — |
| `glob` | `tools/glob.rs` | 真实执行 | 文件查找 |
| `grep` | `tools/grep.rs` | 真实执行 | 内容搜索 |
| `find` | `tools/find.rs` | 真实执行 | 文件查找 |
| `web_search` | `tools/web_search.rs` | 真实 DDG 搜索 | L2/L3 信息检索 |
| `web_fetch` | `tools/web_fetch.rs` | 真实 HTTP (含 SSRF 防护) | 网页内容获取 |

**注意**: `EvalMockTool` 仅用于注入 tau-bench 等 domain-specific 工具，**不会覆盖已有的真实工具**（`runner.rs:213` 有检查）。

### 3.3 Scorer — Exact Match

**当前问题**: `gaia.rs:123` 使用 `contains` 匹配，导致假阳性（如 expected="4", actual="I tried 4 approaches" 也算通过）。

**改为标准 exact match**:

```rust
fn normalize_answer(s: &str) -> String {
    s.trim()
     .to_lowercase()
     .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';')
     .trim()
     .to_string()
}

fn score(&self, output: &AgentOutput) -> EvalScore {
    let actual = output.messages.last()
        .map(|m| m.text_content())
        .unwrap_or_default();

    let normalized_expected = normalize_answer(&self.record.final_answer);
    let normalized_actual = normalize_answer(&actual);

    let passed = normalized_actual == normalized_expected;
    // ...
}
```

与官方 GAIA 评估一致：exact match after normalization。

### 3.4 分层采样策略

通过 TOML 配置支持不同场景：

```toml
[gaia]
dataset = "datasets/gaia_sample.jsonl"

[gaia.sampling]
# 预设: quick(快速验证), standard(标准评估), full(完整评估)
presets.quick = 10
presets.standard = 30
presets.full = 165
default = "standard"
# 分层比例 L1:L2:L3
stratified_ratio = [0.3, 0.5, 0.2]
```

采样实现：按 level 分层，每层按比例随机抽取，确保各难度级别有代表性。

### 3.5 web_search 可靠性

当前 `web_search.rs` 使用 DuckDuckGo HTML 搜索 + 手写 HTML 解析器。需验证：
- DDG 是否返回有效结果（可能被 rate limit 或 blocked）
- HTML 解析器是否能正确提取 title/url/snippet
- 如果 DDG 不可用，需要实现备选搜索引擎（如 Tavily API）

---

## 四、SWE-bench 标准验证方案

### 4.1 数据集

- **来源**: HuggingFace `princeton-nlp/SWE-bench_Lite`
- **规模**: 300 test + 23 dev
- **范围**: 11 个 Python 仓库 (django, sympy, scikit-learn, matplotlib, pytest, sphinx, xarray, astropy, pylint, requests, flask)
- **当前问题**: 现有 `swe_bench_lite.jsonl` 50 条是 Phase I 时合成的，**必须替换为官方数据**

官方数据集字段：

```
instance_id        — "django__django-16527"
repo               — "django/django"
base_commit        — commit hash
patch              — gold patch (不给 agent 看)
test_patch         — 测试 patch
problem_statement  — GitHub issue 描述
hints_text         — 可选提示
FAIL_TO_PASS       — JSON 数组，修复后应通过的测试
PASS_TO_PASS       — JSON 数组，不应被破坏的测试
version            — 仓库版本号
```

octo-eval JSONL 适配：保留所有官方字段，字段名映射 `FAIL_TO_PASS` → `fail_to_pass`。

### 4.2 评估流程

```
┌─────────────────────────────────────────────────────┐
│                  octo-eval runner                    │
│                                                     │
│  1. 加载 SWE-bench Lite 数据集                       │
│  2. 对每个 instance:                                 │
│     a. 构建/拉取 sweb.instance Docker 镜像           │
│     b. 启动容器, working_dir = /testbed              │
│     c. Agent 在容器内执行:                            │
│        - 看到: problem_statement + hints_text        │
│        - 工具: bash, file_read, file_write,          │
│                file_edit, grep, glob                 │
│        - 探索代码 → 定位 bug → 生成 fix              │
│     d. 从 agent output 提取 model_patch (diff)       │
│     e. 写入 predictions.jsonl                        │
│                                                     │
│  3. 调用官方 swebench harness 验证:                   │
│     python -m swebench.harness.run_evaluation \      │
│       --dataset_name princeton-nlp/SWE-bench_Lite \  │
│       --predictions_path predictions.jsonl \         │
│       --max_workers 4                                │
│                                                     │
│  4. 解析 harness 输出 → EvalScore                    │
│     - FAIL_TO_PASS 全过 AND PASS_TO_PASS 全过        │
│       → resolved = 1.0                              │
│     - 否则 → 0.0                                    │
└─────────────────────────────────────────────────────┘
```

### 4.3 Docker 镜像体系

SWE-bench 使用官方 harness 的**三层镜像体系**：

```
sweb.base.x86_64       — Ubuntu + Python + Conda/Pyenv
  └── sweb.env.{repo}  — 某 repo 某 version 的完整环境 (pip install -e .)
      └── sweb.eval.{instance_id}  — git reset --hard base_commit
```

- 镜像由 `swebench.harness` 自动构建和管理
- macOS ARM: 使用 `--namespace ''` 本地构建
- 首次构建较慢（需 clone repo + install deps），后续可缓存 (`--cache_level=env`)

**与 octo-sandbox 标准镜像的关系**: 两套独立体系，互不干扰。

### 4.4 Scorer

**不做自研 scorer。** 直接调用官方 `swebench.harness.run_evaluation`，结果可直接对标 leaderboard。

评分逻辑：
- `resolved` = FAIL_TO_PASS 全部通过 + PASS_TO_PASS 全部通过
- Score = 1.0 (resolved) 或 0.0 (not resolved)
- 无中间分数，对错分明

### 4.5 Prediction 格式

Agent 输出需转换为官方 prediction 格式：

```jsonl
{
  "instance_id": "django__django-16527",
  "model_name_or_path": "octo-agent/claude-sonnet-4-6",
  "model_patch": "diff --git a/django/db/models/query.py b/django/db/models/query.py\n..."
}
```

需要从 agent 的工具调用输出中提取 diff。策略：
1. Agent prompt 中明确要求: "完成修复后，用 `bash` 工具执行 `git diff` 输出最终 patch"
2. eval runner 从最后的 `bash` tool_call 中提取 diff 内容
3. 如果 agent 直接用 `file_write`/`file_edit` 修改了文件，则在容器内执行 `git diff` 获取 patch

### 4.6 分层采样

```toml
[swe_bench]
dataset = "datasets/swe_bench_lite.jsonl"

[swe_bench.sampling]
presets.quick = 10
presets.standard = 30
presets.full = 300
default = "standard"
```

quick=10 用于开发验证，standard=30 用于日常评估，full=300 用于正式报告。

---

## 五、Docker 标准镜像体系

### 5.1 设计策略

**方案 B: 官方语言镜像 + 公共基础包脚本**

每个镜像基于对应语言的官方 Docker 镜像，通过共享的 `install-base.sh` 安装公共基础包。

优势：
- 语言环境由官方维护，质量有保证
- 公共包通过脚本统一，一处修改全部受益
- 镜像体积更小（利用官方 base layer 缓存）
- 不需要维护自建 base 镜像的发布流程

### 5.2 公共基础包

`docker/sandbox-images/install-base.sh` (Debian/Ubuntu):
```bash
#!/bin/bash
set -euo pipefail
apt-get update && apt-get install -y --no-install-recommends \
    git curl wget jq ca-certificates \
    build-essential pkg-config \
    ripgrep fd-find tree \
    unzip zip tar gzip \
    && rm -rf /var/lib/apt/lists/*
```

`docker/sandbox-images/install-base-alpine.sh` (Alpine):
```bash
#!/bin/sh
set -euo pipefail
apk add --no-cache \
    git curl wget jq ca-certificates \
    build-base pkgconf \
    ripgrep fd tree \
    zip unzip tar gzip bash coreutils
```

### 5.3 镜像清单

| 镜像 | Base | 额外安装 | 标签 | 用途 |
|------|------|----------|------|------|
| **python** | `python:3.12-slim-bookworm` | 公共包 + requests/pandas/openpyxl/PyPDF2/beautifulsoup4/sympy/scipy/numpy/Pillow | `octo-sandbox/python:1.0` | GAIA, 通用 Python |
| **rust** | `rust:1.92-bookworm` | 公共包 | `octo-sandbox/rust:1.0` | Rust 项目 |
| **nodejs** | `node:22-bookworm-slim` | 公共包 + typescript | `octo-sandbox/nodejs:1.0` | JS/TS 项目 |
| **bash** | `alpine:3.21` | Alpine 公共包 | `octo-sandbox/bash:1.0` | Shell 脚本 |
| **general** | `python:3.12-slim-bookworm` | 公共包 + node22 + Python 数据包 | `octo-sandbox/general:1.0` | 多语言 agent 任务 |
| **swebench** | `python:3.12-slim-bookworm` | 公共包 + pytest/tox/pre-commit | `octo-sandbox/swebench:1.0` | SWE-bench 评估 |

### 5.4 ImageRegistry 升级

```rust
// crates/octo-engine/src/sandbox/docker.rs
impl ImageRegistry {
    pub fn default_registry() -> Self {
        let mut images = HashMap::new();
        images.insert("python".into(),     "octo-sandbox/python:1.0".into());
        images.insert("rust".into(),       "octo-sandbox/rust:1.0".into());
        images.insert("node".into(),       "octo-sandbox/nodejs:1.0".into());
        images.insert("javascript".into(), "octo-sandbox/nodejs:1.0".into());
        images.insert("typescript".into(), "octo-sandbox/nodejs:1.0".into());
        images.insert("bash".into(),       "octo-sandbox/bash:1.0".into());
        images.insert("sh".into(),         "octo-sandbox/bash:1.0".into());
        images.insert("general".into(),    "octo-sandbox/general:1.0".into());
        images.insert("swebench".into(),   "octo-sandbox/swebench:1.0".into());
        Self { images }
    }
}
```

### 5.5 构建与管理

```bash
# docker/sandbox-images/build.sh
# 一键构建所有镜像

# Makefile 集成
make docker-build          # 构建全部
make docker-build-python   # 构建单个
make docker-list           # 列出已构建
make docker-clean          # 清理
```

---

## 六、配置体系

### 6.1 评估配置格式

统一的 TOML 配置用于 benchmark 运行：

```toml
# config/eval/benchmark.toml

[general]
output_dir = "eval_output/runs"
max_rounds = 20
timeout_seconds = 300

# === 模型集 ===
[models]
quick = ["claude-sonnet-4-6"]
standard = ["claude-sonnet-4-6", "qwen3-30b-a3b", "mistral-small-3.2"]
full = [
    "claude-sonnet-4-6", "claude-opus-4-6",
    "qwen3-30b-a3b", "mistral-small-3.2",
    "minimax-m2.1"
]
default = "standard"

# === GAIA ===
[gaia]
dataset = "datasets/gaia_sample.jsonl"
scorer = "exact_match"

[gaia.sampling]
presets = { quick = 10, standard = 30, full = 165 }
default = "standard"
stratified_ratio = [0.3, 0.5, 0.2]  # L1:L2:L3

# === SWE-bench ===
[swe_bench]
dataset = "datasets/swe_bench_lite.jsonl"
scorer = "swebench_harness"
docker_required = true

[swe_bench.sampling]
presets = { quick = 10, standard = 30, full = 300 }
default = "standard"

[swe_bench.harness]
# 官方 swebench Python 包配置
dataset_name = "princeton-nlp/SWE-bench_Lite"
max_workers = 4
cache_level = "env"
namespace = ""  # macOS ARM 本地构建
```

### 6.2 CLI 用法

```bash
# GAIA 评估
octo eval benchmark --suite gaia --sampling quick --models quick
octo eval benchmark --suite gaia --sampling full --models standard

# SWE-bench 评估
octo eval benchmark --suite swe_bench --sampling quick --models quick
octo eval benchmark --suite swe_bench --sampling standard --models standard
```

---

## 七、与现有系统的集成点

### 7.1 eval runner 改动

| 文件 | 改动 |
|------|------|
| `benchmarks/gaia.rs` | scorer 改 exact match |
| `benchmarks/swe_bench.rs` | 数据集字段对齐官方, scorer 调用 harness |
| `runner.rs` | SWE-bench 任务在 Docker 容器内启动 agent |
| `comparison.rs` | TaskRecord 中的 scoring_data 已修复 (Q1) |
| `config.rs` | 新增采样/模型集配置解析 |

### 7.2 不改动的部分

- `default_tools()` — 已完整，所有工具真实可用
- `EvalMockTool` — 仅用于 tau-bench, 不影响 GAIA/SWE-bench
- `AgentLoop` — 不需要改动
- `ProviderConfig` — 不需要改动

---

## 八、风险与依赖

| 风险 | 影响 | 缓解 |
|------|------|------|
| DDG 搜索被 rate limit | GAIA L2/L3 分数低 | 验证后如需要，增加 Tavily 搜索引擎 |
| SWE-bench Docker 首次构建慢 | 评估耗时长 | 预构建 + 缓存 |
| macOS ARM Docker 兼容性 | 镜像构建失败 | `--namespace ''` 本地构建 |
| swebench Python 包版本 | harness API 变更 | pin 版本 |
| Agent 无法生成有效 diff | SWE-bench 分数 0% | prompt engineering + 验证 |

---

## 九、预期基线

### GAIA (单模型 + 真实工具链)

| 模型 | 预期 Overall | 预期 L1 | 预期 L2 | 预期 L3 |
|------|-------------|---------|---------|---------|
| Claude Sonnet 4.6 | 30-45% | 50-70% | 25-40% | 10-25% |
| Qwen3-30B | 20-35% | 40-55% | 15-30% | 5-15% |
| Mistral Small 3.2 | 15-25% | 30-45% | 10-20% | 3-10% |

注：以上为**单模型裸跑**预期，参考 PricePerToken 数据 (GPT-5 Mini 44.8%, Claude 3.7 Sonnet 43.9%)。

### SWE-bench Lite (单模型 + 真实 Docker 验证)

| 模型 | 预期 Resolved |
|------|--------------|
| Claude Sonnet 4.6 | 15-35% |
| Qwen3-30B | 5-15% |
| Mistral Small 3.2 | 3-10% |

注：参考 SWE-bench Verified 上单模型 + SWE-agent scaffold 的公开分数。
