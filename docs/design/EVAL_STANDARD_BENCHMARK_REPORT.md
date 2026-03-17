# Octo Agent 标准测试基线报告（GAIA + SWE-bench）

> 生成日期: 2026-03-17
> 评估框架版本: octo-eval 0.1.0
> 评估引擎: octo-engine (AgentLoop + ToolRegistry + SweBenchHarness)

## 一、概述

本报告记录 Octo Agent 在两个业界标准 Benchmark 上的首次完整端到端评估结果。
与之前的自定义 Suite 基线报告（`EVAL_BASELINE_REPORT.md`）不同，本次使用的是学术界/工业界公认的标准测试集，
结果可直接与业界公开数据对比。

### 评估规模

| 指标 | GAIA | SWE-bench |
|------|------|-----------|
| 模型数量 | 4 | 4 |
| 每模型任务数 | 30 | 30 |
| 总评估运行数 | 120 | 120 |
| 评分方式 | exact_match (normalize) | 官方 swebench harness (Docker) |
| 搜索工具 | R1: DDG fallback / R2: Tavily | N/A |
| Run ID | R1: 2026-03-17-008 / R2: 009 | 2026-03-17-007 |

### 模型矩阵

| 模型 | 层级 | 参数量 | 提供商 |
|------|------|--------|--------|
| Qwen3.5-9B | T1-Economy | 9B | OpenRouter (qwen/qwen3.5-9b) |
| Qwen3.5-27B | T2-Standard | 27B | OpenRouter (qwen/qwen3.5-27b) |
| Qwen3-Coder-Next | T3-HighPerf | — | OpenRouter (qwen/qwen3-coder-next) |
| MiniMax-M2.1 | T2-Standard | — | OpenRouter (minimax/minimax-m2.1) |

---

## 二、SWE-bench 评估结果

### 2.1 评估方法

- **数据集**: SWE-bench Lite (princeton-nlp)，分层采样 30 题
- **Scorer**: 官方 `swebench` 4.1.0 harness — Docker 容器内执行 `git apply` + 运行 `fail_to_pass`/`pass_to_pass` 测试
- **评分层级**:
  - `resolved = 1.0` — harness 验证通过（patch 正确解决 bug）
  - `harness_failed = 0.25` — patch 可 apply 但测试未通过
  - `patch_apply_failed = 0.10` — 有 patch 但 `git apply` 失败
  - `no_patch = 0.00` — 未生成 patch
- **Agent 配置**: max_iterations=30, timeout=600s

### 2.2 结果

| Model | Patches 生成 | Resolved | Pass Rate | Avg Score |
|-------|-------------|----------|-----------|-----------|
| **Qwen3.5-9B** | 6/30 | 1 | 3.3% | 0.050 |
| **Qwen3-Coder-Next** | 7/30 | 1 | 3.3% | 0.053 |
| **Qwen3.5-27B** | 9/30 | 1 | 3.3% | 0.060 |
| **MiniMax-M2.1** | 8/30 | 1 | 3.3% | 0.057 |
| **总计** | 30/120 | 4/120 | **3.3%** | 0.055 |

### 2.3 Harness 验证统计

| 指标 | 数值 |
|------|------|
| 总 patches 生成 | 30/120 (25%) |
| Patch apply 成功 | 4/30 (13%) |
| 测试通过 (resolved) | 4/4 (100%) |
| Patch apply 失败 | 26/30 (87%) |

### 2.4 唯一解决的任务

**`astropy__astropy-14995`** — 四个模型都成功解决了这个任务。

### 2.5 Patch Apply 失败分布（score=0.10）

| Task ID | Qwen3.5-9B | Qwen3-Coder-Next | Qwen3.5-27B | MiniMax-M2.1 |
|---------|:---:|:---:|:---:|:---:|
| django__django-11019 | ✗ | ✗ | ✗ | ✗ |
| django__django-11630 | ✗ | ✗ | ✗ | ✗ |
| django__django-11815 | ✗ | ✗ | ✗ | ✗ |
| django__django-11905 | ✗ | ✗ | ✗ | ✗ |
| django__django-12113 | ✗ | ✗ | ✗ | ✗ |
| django__django-11049 | — | — | ✗ | — |
| django__django-11422 | — | — | ✗ | ✗ |
| django__django-11564 | — | — | — | ✗ |
| django__django-11999 | — | — | ✗ | — |
| astropy__astropy-6938 | — | ✗ | — | — |

> ✗ = 生成了 patch 但 apply 失败

### 2.6 与业界对比

| 方法 | SWE-bench Lite 解题率 |
|------|----------------------|
| 业界 SOTA (Devin, SWE-Agent + Claude Sonnet) | 40–50% |
| 较好的开源 Agent | 15–25% |
| 中等水平 Agent | 5–15% |
| **Octo Agent (本次)** | **3.3%** |

### 2.7 SWE-bench 分析

1. **四模型一致性极高** — 都只解决了 astropy-14995，Django 任务全部 patch apply 失败
2. **主要瓶颈是 patch 格式质量** — 87% 的 patches 无法被 `git apply` 正确应用
3. **Django patch 系统性失败** — 非 trailing newline 问题（已修复），而是 diff 上下文/路径不匹配
4. **模型规模影响有限** — 9B 到 27B 模型解题率完全相同，说明瓶颈不在模型推理能力
5. **Agent 迭代轮次可能不足** — 30 轮 max_iterations 对复杂 Django codebase 修复可能不够

---

## 三、GAIA 评估结果

### 3.1 评估方法

- **数据集**: GAIA (General AI Assistants) validation set，分层采样 30 题（L1:L2:L3 = 30%:50%:20%）
- **Scorer**: exact_match — normalize(expected) == normalize(actual)
- **Normalize**: 去除标点、多余空格、大小写统一
- **Agent 配置**: max_iterations=30, timeout=300s
- **搜索工具**: R1 使用 DuckDuckGo (DDG) HTML fallback / R2 使用 Tavily API

### 3.2 R1 结果（DDG 搜索）

| Model | Passed | Pass Rate |
|-------|--------|-----------|
| **Qwen3-Coder-Next** | 10/30 | 33.3% |
| **Qwen3.5-27B** | 6/30 | 20.0% |
| **MiniMax-M2.1** | 4/30 | 13.3% |
| **Qwen3.5-9B** | 2/30 | 6.7% |
| **Macro-Avg** | 22/120 | **18.3%** |

### 3.3 R2 结果（Tavily 搜索）

| Model | Passed | Pass Rate |
|-------|--------|-----------|
| **MiniMax-M2.1** | 13/30 | **43.3%** |
| **Qwen3.5-27B** | 12/30 | 40.0% |
| **Qwen3-Coder-Next** | 10/30 | 33.3% |
| **Qwen3.5-9B** | 4/30 | 13.3% |
| **Macro-Avg** | 39/120 | **32.5%** |

### 3.4 搜索引擎影响对比（R1 DDG vs R2 Tavily）

| Model | R1 (DDG) | R2 (Tavily) | 变化 |
|-------|----------|-------------|------|
| **Qwen3.5-9B** | 6.7% | 13.3% | **+2.0x** |
| **Qwen3.5-27B** | 20.0% | 40.0% | **+2.0x** |
| **Qwen3-Coder-Next** | 33.3% | 33.3% | 持平 |
| **MiniMax-M2.1** | 13.3% | 43.3% | **+3.25x** |
| **Macro-Avg** | 18.3% | 32.5% | **+77%** |

### 3.5 与业界对比（GAIA Validation Set）

| 方法 | GAIA 通过率 |
|------|------------|
| 业界 SOTA (GPT-4 + tools + retrieval) | 40–60% |
| Claude-3.5-Sonnet + tools | 35–50% |
| 开源 Agent (LLaMA 70B + tools) | 15–25% |
| **Octo Agent R2 最佳 (MiniMax-M2.1)** | **43.3%** |
| **Octo Agent R2 平均** | **32.5%** |

### 3.6 GAIA 分析

1. **搜索工具质量是 GAIA 的关键变量** — Tavily vs DDG 带来 77% 的总体提升
2. **MiniMax-M2.1 受益最大** — 从 13.3% 跃升至 43.3%（+3.25x），成为 R2 最强模型
3. **Qwen3-Coder-Next 对搜索引擎不敏感** — 保持 33.3%，说明它更依赖内在推理而非外部搜索
4. **模型规模与 GAIA 正相关** — 9B < 27B，符合多步推理任务的预期
5. **MiniMax-M2.1 在 R2 中逆袭** — DDG 时排名倒数第二，Tavily 时排名第一，说明该模型善于利用高质量搜索结果

### 3.7 GAIA R2 深度分析

#### 3.7.1 任务通过分布

| 通过模型数 | 任务数 | 占比 | 说明 |
|:---------:|:------:|:----:|------|
| 4/4 | 0 | 0% | 没有所有模型都能解的"简单题" |
| 3/4 | 9 | 30% | 核心能力区：搜索+推理可解 |
| 2/4 | 4 | 13% | 能力分化区 |
| 1/4 | 4 | 13% | 模型特长区 |
| 0/4 | **13** | **43%** | Agent 能力盲区 |

#### 3.7.2 按难度分级通过率

| Level | 任务数 | q9b | q27b | coder | mm |
|:-----:|:------:|:---:|:----:|:-----:|:--:|
| L1 (简单) | 8 | 12% | 50% | **62%** | **62%** |
| L2 (多步) | 17 | 12% | **41%** | 29% | **47%** |
| L3 (复杂) | 5 | 20% | 20% | 0% | 0% |

#### 3.7.3 工具使用模式

| Model | Pass 平均工具次数 | Fail 平均工具次数 | 差异 |
|-------|:-----------------:|:-----------------:|:----:|
| **Qwen3.5-9B** | 1.2 | 1.2 | 无差异 |
| **Qwen3.5-27B** | 7.1 | 12.9 | Fail 时过度探索 |
| **Qwen3-Coder-Next** | 11.3 | 15.2 | 最多探索轮次 |
| **MiniMax-M2.1** | 7.1 | 10.2 | 高效利用工具 |

**关键发现**: Qwen3.5-9B 平均只调用 1.2 次工具，几乎不使用工具，主要靠内部知识回答，所以通过率最低。

#### 3.7.4 全军覆没任务失败原因分类（13/30 任务）

| 失败原因 | 任务数 | 说明 |
|----------|:------:|------|
| **搜索不精确 / 知识盲区** | 4 | Nature 2020 统计、EC numbers、1959 食品标准、PubChem 数据库 |
| **需要精确计算** | 3 | 鱼袋体积(0.1777m³)、国家间距离、虾长度百分比 |
| **需要图片/视觉理解** | 2 | 冰淇淋墓碑文字(OCR)、PNG 中数字提取 |
| **需要视频内容理解** | 2 | YouTube 视频中物体长度、YouTube 视频中科学家 |
| **需要 GitHub 精确操作** | 1 | numpy issue 的 Regression 标签添加日期 |
| **需要解析结构化数据** | 1 | JSONLD 文件解析 + 统计计算 |

#### 3.7.5 各模型特征画像

**Qwen3.5-9B (4/30, 13.3%) — "知识记忆型"**
- 几乎不调用工具（平均 1.2 次），依赖内部知识
- 通过的 4 题全靠搜索一次即答（duration ≤ 23s）
- 瓶颈: 模型规模太小，无法进行多步推理和工具组合

**Qwen3.5-27B (12/30, 40.0%) — "稳健均衡型"**
- L1 通过率 50%，L2 通过率 41%，L3 仅 20%
- 工具使用合理（pass 7.1 次，fail 时 12.9 次——方向错时过度探索）
- 唯一在 L3 有通过的模型之一（Claude Shannon 题）

**Qwen3-Coder-Next (10/30, 33.3%) — "推理驱动型"**
- L1 最强 (62%)，但 L3 全灭 (0%)
- 对搜索引擎不敏感（R1=R2=33.3%），依赖强推理而非搜索质量
- 工具调用最多（pass 11.3 次），善于多轮探索
- 瓶颈: L3 长链推理超出能力范围

**MiniMax-M2.1 (13/30, 43.3%) — "工具利用型"**
- L2 最强 (47%)，善于解决多步+多工具任务
- R1→R2 提升 3.25 倍（13.3%→43.3%），极度依赖搜索质量
- 唯一解决 spreadsheet 附件题（写 Python 脚本读 xlsx）和 OpenCV 题的模型
- 善于将搜索结果转化为答案，工具组合能力强

#### 3.7.6 文件附件任务分析（5/30）

| 文件格式 | 任务内容 | 通过数 | 分析 |
|----------|----------|:------:|------|
| `.docx` | Secret Santa 逻辑推理 | 3/4 | 多数模型能读取文本格式 |
| `.xlsx` | 影碟店库存查询 | 1/4 | 仅 MiniMax 用 Python 读取成功 |
| `.pdb` | 蛋白质原子距离计算 | 2/4 | 需要 Biopython 库 |
| `.png` | 图片中数字 OCR | **0/4** | **无 OCR 能力——绝对盲区** |
| `.jsonld` | ORCID 学者数据统计 | **0/4** | **解析 + 计算双重难度** |

---

## 四、综合分析

### 4.1 GAIA vs SWE-bench 难度对比

| 维度 | GAIA (R2 Tavily) | SWE-bench (Harness) |
|------|-------------------|---------------------|
| 最高通过率 | 43.3% (MiniMax) | 3.3% (all) |
| 平均通过率 | 32.5% | 3.3% |
| 模型间方差 | 大 (13.3%–43.3%) | 无 (3.3%–3.3%) |
| 搜索工具影响 | 极大 (+77%) | 无 |
| 主要瓶颈 | 推理 + 搜索质量 | Patch 格式 + 代码理解 |

### 4.2 模型综合排名

| 排名 | Model | GAIA R2 | SWE-bench | 综合评价 |
|------|-------|---------|-----------|----------|
| 1 | **MiniMax-M2.1** | **43.3%** | 3.3% | GAIA 最强，善于利用工具 |
| 2 | **Qwen3.5-27B** | 40.0% | 3.3% | 第二强，规模优势明显 |
| 3 | **Qwen3-Coder-Next** | 33.3% | 3.3% | 稳定，不依赖搜索质量 |
| 4 | **Qwen3.5-9B** | 13.3% | 3.3% | 规模限制，适合轻量场景 |

### 4.3 关键结论

1. **Octo Agent GAIA 能力达到竞争水平** — R2 平均 32.5%，最佳 43.3%，接近业界开源 Agent 水平
2. **SWE-bench 能力有待大幅提升** — 3.3% 离业界 SOTA (40-50%) 差距巨大
3. **搜索工具是低成本高回报的优化点** — 仅切换搜索引擎即可提升 77%
4. **SWE-bench 的瓶颈不在模型** — 四个模型得分完全一致，需要优化 Agent 的 patch 生成策略
5. **GAIA 对模型选择敏感，SWE-bench 不敏感** — 选型应根据目标任务类型决定

---

## 五、GAIA 数据集筛选策略

### 5.1 当前数据集分析

GAIA validation set 共 165 题，按附件和能力需求分类：

| 分类 | 任务数 | 占比 | Agent 可达性 |
|------|:------:|:----:|:------------:|
| 无附件（纯搜索+推理） | **127** | 77% | 高 |
| 文本附件（.docx/.txt/.csv/.py） | 4 | 2% | 高 |
| 表格附件（.xlsx） | 13 | 8% | 中（需 Python） |
| 结构化数据（.pdf/.pdb/.jsonld/.zip） | 8 | 5% | 低 |
| 图片附件（.png/.jpg） | **10** | 6% | **不可达（无 OCR）** |
| 音频附件（.mp3/.pptx） | 3 | 2% | **不可达（无 ASR）** |

同时，约 **13 个任务** 依赖视频内容理解（YouTube 链接），当前 Agent 无法处理。

### 5.2 筛选策略

**原则**: 排除当前 Agent 能力绝对盲区的任务，聚焦可有效区分模型能力的任务。

#### 排除条件

| 排除规则 | 排除原因 | 涉及任务数 |
|----------|----------|:----------:|
| `.png` / `.jpg` / `.gif` 附件 | 无 OCR / 多模态视觉能力 | ~10 |
| `.mp3` / `.pptx` 附件 | 无音频转写能力 | ~3 |
| 问题含 `youtube.com/watch` 或明确要求观看视频 | 无视频理解能力 | ~13 |

#### 保留条件

| 保留类型 | 理由 | 涉及任务数 |
|----------|------|:----------:|
| 无附件任务 | Agent 核心能力：搜索+推理+计算 | ~127 |
| `.docx` / `.txt` / `.csv` / `.py` 附件 | `file_read` 可直接读取 | ~4 |
| `.xlsx` 附件 | Agent 可通过 Python 脚本读取（MiniMax 已验证） | ~13 |
| `.pdb` 附件 | Agent 可通过 Biopython 读取（R2 已有 2/4 通过） | ~1 |
| `.pdf` 附件 | Agent 可通过 Python 库读取（需验证） | ~3 |
| `.jsonld` / `.zip` 附件 | 保留但标记为 Hard（解析+计算双重难度） | ~3 |

#### 预计筛选后数据集

| 指标 | 筛选前 | 筛选后 | 变化 |
|------|:------:|:------:|:----:|
| 总任务数 | 165 | ~139 | -16% |
| L1 | 53 | ~47 | -11% |
| L2 | 86 | ~72 | -16% |
| L3 | 26 | ~20 | -23% |
| 有效区分度 | 低（含不可达任务） | 高（聚焦可达任务） | 提升 |

### 5.3 采样配置更新

```toml
[gaia]
dataset = "crates/octo-eval/datasets/gaia_sample.jsonl"
scorer = "exact_match"

# 排除规则: 图片/音频/视频 依赖型任务
exclude_file_extensions = ["png", "jpg", "jpeg", "gif", "mp3", "pptx"]
exclude_question_patterns = ["youtube.com/watch", "video"]

[gaia.sampling]
quick = 10
standard = 30
full = 139       # 筛选后总数
default = "standard"
stratified_ratio = [0.3, 0.5, 0.2]  # L1:L2:L3
```

---

## 六、改进路线图

### 6.1 短期（SWE-bench 提升）

| 优先级 | 改进项 | 预期影响 |
|--------|--------|----------|
| P0 | **修复 patch 格式** — 使用 `git diff --no-color` 格式化输出 | 将 apply 成功率从 13% 提升至 60%+ |
| P0 | **增加 max_iterations** — 从 30 提升至 50-80 | 给 Agent 更多探索空间 |
| P1 | **Agent prompt 优化** — 参考 SWE-Agent 的 prompt 工程 | 引导更精确的 patch 生成 |
| P1 | **使用更大模型** — Claude-Sonnet-4.6 或 GPT-4o | 直接提升推理能力 |
| P2 | **Retrieval 增强** — codebase 索引 + 相关代码检索 | 减少 Agent 盲目探索 |

### 6.2 短期（GAIA 提升）

| 优先级 | 改进项 | 预期影响 |
|--------|--------|----------|
| P0 | **确保 Tavily 可用** — 监控 API 额度和 fallback 策略 | 维持 R2 水平 |
| P0 | **应用筛选策略** — 排除图片/音频/视频依赖任务 | 基线更准确，消除虚假 0 分 |
| P1 | **计算工具增强** — Python/calculator 集成，解锁精确计算类题目 | 预计解锁 3+ 题 |
| P1 | **Excel 读取集成** — 内置 xlsx 解析或预装 openpyxl | 解锁 13 个 xlsx 附件任务 |
| P2 | **扩大样本量** — 从 30 题扩至 full 139 题（筛选后） | 更准确的基线估计 |

### 6.3 中期

| 改进项 | 说明 |
|--------|------|
| CI 回归检测 | SWE-bench ≥ 3.3%, GAIA ≥ 32.5% 作为最低阈值 |
| 多次运行取均值 | 3-5 次运行消除随机噪声 |
| 更大模型矩阵 | 加入 Claude-Sonnet-4.6, GPT-4o, Gemini-2.5 |
| τ-bench 基线 | 补全第三个标准测试的基线数据 |
| 多模态能力 | 后续添加 OCR/ASR 后再解锁图片/音频任务 |

---

## 七、数据存档

| 数据 | 路径 |
|------|------|
| SWE-bench R4 (harness) | `eval_output/runs/2026-03-17-007/` |
| GAIA R1 (DDG) | `eval_output/runs/2026-03-17-008/` |
| GAIA R2 (Tavily) | `eval_output/runs/2026-03-17-009/` |
| 本报告 | `docs/design/EVAL_STANDARD_BENCHMARK_REPORT.md` |
| 自定义 Suite 基线 | `docs/design/EVAL_BASELINE_REPORT.md` |
| 评估配置 (SWE-bench) | `config/eval/swebench_r4.toml` |
| 评估配置 (GAIA) | `/tmp/gaia_r1.toml` (需迁移至 `config/eval/`) |
| 评估配置 (benchmark) | `config/eval/benchmark.toml` |
