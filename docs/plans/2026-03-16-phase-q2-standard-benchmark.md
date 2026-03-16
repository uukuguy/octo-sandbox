# Phase Q2 — GAIA & SWE-bench 标准验证

**日期**: 2026-03-16
**前置**: Phase Q1 (TaskRecord scoring bug fix, commit 0e2ac3e)
**设计文档**: `docs/design/STANDARD_BENCHMARK_DESIGN.md`
**目标**: 将 GAIA 和 SWE-bench 评估对标业界标准，获得有意义的基线分数

---

## 任务总览

| 组 | 名称 | Tasks | 优先级 |
|----|------|-------|--------|
| G1 | Docker 标准镜像体系 | T1-T4 | P0 |
| G2 | GAIA 标准化 | T5-T8 | P0 |
| G3 | SWE-bench 标准化 | T9-T13 | P0 |
| G4 | 配置统一 + 基线 | T14-T15 | P1 |

**总计**: 15 tasks

---

## G1: Docker 标准镜像体系

### T1: 公共基础包脚本

**文件**: `docker/sandbox-images/install-base.sh`, `docker/sandbox-images/install-base-alpine.sh`

- [ ] 创建 `install-base.sh` — Debian/Ubuntu 公共基础包
  - git, curl, wget, jq, ca-certificates
  - build-essential, pkg-config
  - ripgrep, fd-find, tree
  - unzip, zip, tar, gzip
- [ ] 创建 `install-base-alpine.sh` — Alpine 公共基础包
  - 同等功能的 Alpine 包名
  - 额外: bash, coreutils (Alpine 默认无)

### T2: 6 个 Dockerfile

**目录**: `docker/sandbox-images/`

- [ ] `Dockerfile.python` — FROM python:3.12-slim-bookworm + 公共包 + 数据科学包 (requests, pandas, openpyxl, PyPDF2, beautifulsoup4, sympy, scipy, numpy, Pillow)
- [ ] `Dockerfile.rust` — FROM rust:1.92-bookworm + 公共包
- [ ] `Dockerfile.nodejs` — FROM node:22-bookworm-slim + 公共包 + typescript
- [ ] `Dockerfile.bash` — FROM alpine:3.21 + Alpine 公共包
- [ ] `Dockerfile.general` — FROM python:3.12-slim-bookworm + 公共包 + node22 + Python 数据包
- [ ] `Dockerfile.swebench` — FROM python:3.12-slim-bookworm + 公共包 + pytest + tox + pre-commit

所有镜像:
- 非 root 用户 `sandbox`, WORKDIR `/workspace`
- Label: `org.octo-sandbox.type`, `org.octo-sandbox.version`

### T3: 构建脚本 + Makefile

- [ ] `docker/sandbox-images/build.sh` — 一键构建所有/单个镜像
  - 用法: `./build.sh [python|rust|nodejs|bash|general|swebench|all]`
  - 构建 tag: `octo-sandbox/{name}:1.0`
- [ ] Makefile targets:
  - `docker-build` — 构建全部
  - `docker-build-python` / `docker-build-rust` / ... — 构建单个
  - `docker-list` — 列出已构建的 octo-sandbox 镜像
  - `docker-clean` — 删除所有 octo-sandbox 镜像

### T4: ImageRegistry 升级 + 测试

**文件**: `crates/octo-engine/src/sandbox/docker.rs`

- [ ] `ImageRegistry::default_registry()` 改为 octo-sandbox 镜像
  - python → `octo-sandbox/python:1.0`
  - rust → `octo-sandbox/rust:1.0`
  - node/javascript/typescript → `octo-sandbox/nodejs:1.0`
  - bash/sh → `octo-sandbox/bash:1.0`
  - general → `octo-sandbox/general:1.0`
  - swebench → `octo-sandbox/swebench:1.0`
- [ ] 单元测试验证 resolve() 映射正确
- [ ] `cargo test --workspace -- --test-threads=1`

---

## G2: GAIA 标准化

### T5: Scorer 改 exact match

**文件**: `crates/octo-eval/src/benchmarks/gaia.rs`

- [ ] 新增 `normalize_answer()` 函数: trim → lowercase → strip 尾部标点 (.,;)
- [ ] `GaiaTask::score()`: 改 `contains` → `==` (exact match)
- [ ] `comparison.rs` 中 TaskRecord 的 GAIA scoring dispatch 同步更新
- [ ] 更新测试: pass/fail case 验证 exact match 行为
- [ ] `cargo test --workspace -- --test-threads=1`

### T6: 分层采样 + 模型集配置

**文件**: `config/eval/benchmark.toml`, `crates/octo-eval/src/config.rs`

- [ ] 设计 TOML 配置格式:
  - `[gaia.sampling]` — presets (quick=10, standard=30, full=165)
  - `[gaia.sampling]` — stratified_ratio [0.3, 0.5, 0.2]
  - `[models]` — quick/standard/full 模型集预设
- [ ] 实现采样逻辑: 按 level 分层随机抽取
- [ ] CLI 参数: `--sampling quick|standard|full`, `--models quick|standard|full`
- [ ] 测试采样分布正确性

### T7: web_search DDG 验证/修复

**文件**: `crates/octo-engine/src/tools/web_search.rs`

- [ ] 集成测试: 发送真实 DDG 搜索请求，验证返回结果
- [ ] 验证 HTML 解析器能正确提取 title/url/snippet
- [ ] 如果 DDG 有 rate limit / block:
  - 添加 User-Agent 轮换
  - 或增加 Tavily API 作为可配搜索后端
- [ ] 确保 GAIA L2/L3 任务能获得有效搜索结果

### T8: 跑 GAIA baseline

- [ ] 使用 standard=30 采样, 单模型 (claude-sonnet-4-6) 快速验证
- [ ] 确认 exact match scorer 正常工作
- [ ] 确认 web_search 在实际 GAIA 任务中返回有效结果
- [ ] 记录基线分数，与 SOTA 对比分析

---

## G3: SWE-bench 标准化

### T9: 下载官方 SWE-bench Lite 数据集

- [ ] Python 脚本: 从 HuggingFace 下载 `princeton-nlp/SWE-bench_Lite` test split
- [ ] 转为 octo-eval JSONL 格式:
  - 字段映射: `FAIL_TO_PASS` → `fail_to_pass`, `PASS_TO_PASS` → `pass_to_pass`
  - 保留所有官方字段
- [ ] 替换 `datasets/swe_bench_lite.jsonl` (备份旧文件)
- [ ] 更新 `SweBenchRecord` struct 对齐官方字段 (version, issue_url 等)
- [ ] 验证: 解析所有 300 条无报错

### T10: SWE-bench Scorer 改为调用官方 harness

**文件**: `crates/octo-eval/src/benchmarks/swe_bench.rs`

- [ ] 删除当前的静态 patch 分析 scorer (has_diff_header, references_repo 等)
- [ ] 新 scorer 流程:
  1. 收集 agent 输出的 model_patch
  2. 写入 predictions.jsonl (官方格式)
  3. 调用 `python -m swebench.harness.run_evaluation`
  4. 解析 harness 结果 → EvalScore (resolved=1.0 / 0.0)
- [ ] 前置依赖: `pip install swebench` (在 swebench Docker 镜像或本机)

### T11: eval runner Docker 集成

**文件**: `crates/octo-eval/src/runner.rs`

- [ ] SWE-bench 任务检测: 当 task 来自 SWE-bench suite 时使用 Docker 模式
- [ ] Docker 模式流程:
  1. 检查 Docker daemon 可用
  2. 使用 swebench harness 构建 instance Docker 镜像
  3. 在容器内设置 agent 的 working_dir = `/testbed`
  4. Agent 在容器内执行工具 (bash, file_read, file_write, file_edit, grep, glob)
  5. 任务完成后在容器内执行 `git diff` 获取 patch
- [ ] 容器生命周期管理 (创建/执行/清理)

### T12: Agent prompt 设计

- [ ] SWE-bench 专用 system prompt:
  - 角色: "You are a software engineer fixing a bug in a Python project."
  - 说明: "Read the problem statement, explore the codebase, identify the bug, and fix it."
  - 输出要求: "After fixing, run `git diff` to show your changes."
- [ ] prompt 中不包含 gold patch, test_patch, FAIL_TO_PASS 等信息
- [ ] prompt 中包含 hints_text (如果非空)

### T13: 跑 SWE-bench baseline

- [ ] quick=10 采样, 单模型 (claude-sonnet-4-6)
- [ ] 确认 Docker 构建 + agent 容器执行 + patch 提取 + harness 验证全链路
- [ ] 记录基线分数
- [ ] 分析 agent 行为: 是否能正确探索代码、定位 bug、生成有效 patch

---

## G4: 配置统一 + 基线报告

### T14: 统一 eval benchmark TOML 配置

**文件**: `config/eval/benchmark.toml`

- [ ] 统一格式: 所有 benchmark suite 的采样/模型/Docker 配置
- [ ] 移除旧的 `eval.benchmark.toml` / `eval.benchmark.mini.toml` (已在 git status 中标记为 deleted)
- [ ] 文档化配置字段

### T15: 基线报告

**文件**: `eval_output/` 下生成

- [ ] GAIA standard=30 × standard models 结果
- [ ] SWE-bench quick=10 × quick model 结果
- [ ] 对标 SOTA 的 context 注释
- [ ] 更新 `docs/design/EVAL_BASELINE_REPORT.md`

---

## 验收标准

- [ ] 6 个 Docker 镜像全部可构建 (`make docker-build`)
- [ ] GAIA scorer 为 exact match, 测试通过
- [ ] SWE-bench 数据集为官方 300 条
- [ ] SWE-bench scorer 调用官方 harness, 对错分明
- [ ] GAIA baseline 有有效分数 (非全 0 或全 1)
- [ ] SWE-bench baseline 全链路跑通 (Docker → agent → patch → harness)
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过
- [ ] 所有新代码有测试覆盖

---

## 依赖

- Docker daemon 运行中
- `pip install swebench` (SWE-bench 官方 Python 包)
- HuggingFace 数据集访问 (`datasets` Python 包)
- 网络访问 (DDG 搜索, HuggingFace 下载)
- LLM API keys (已配置在 shell 环境变量中)
