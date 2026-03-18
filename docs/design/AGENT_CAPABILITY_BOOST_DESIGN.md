# Agent 能力提升设计方案 (Phase S)

**日期**: 2026-03-18
**状态**: 设计完成，待实施
**前置**: Phase R (GAIA Filtered Evaluation, commit 49cb6da)
**关联**: `AGENT_EVALUATION_DESIGN.md`, `STANDARD_BENCHMARK_DESIGN.md`, `EVAL_WHITEBOX_DESIGN.md`

---

## 一、问题定义

### 1.1 GAIA R3 评估诊断

Phase R 完成了 GAIA 149 题 × 4 模型的全量评估，最优模型 MiniMax-M2.1 通过率 41.6%。
**64/149 题 (43%) 全军覆没**，根因分类如下：

| 根因 | 数量 | 占比 | 代码瓶颈 |
|------|:----:|:----:|----------|
| 多步推理失败 | 28 | 43.8% | System Prompt 无 ReAct 指导 |
| 精确搜索失败 | 17 | 26.6% | Web Search 无查询改写/重试 |
| 文件解析失败 | 14 | 21.9% | File Read 仅 UTF-8，无二进制支持 |
| YouTube 筛选遗漏 | 5 | 7.8% | 过滤规则不完善（已处理） |

### 1.2 与业界 SOTA 对比

| 系统类型 | GAIA 通过率 | 代表 |
|----------|:---------:|------|
| Multi-model ensemble + 真实工具链 | 92.36% | OPS-Agentic-Search (阿里云) |
| 单模型裸跑（顶级闭源） | ~44% | GPT-5 Mini / Claude 3.7 Sonnet |
| **octo-eval (当前)** | **41.6%** | MiniMax-M2.1 (开源) |
| **octo-eval (目标)** | **50%+** | 改进后 |

用开源模型达到闭源裸跑水平 (44%)，并通过工具链改进超越之 (50%+)。

---

## 二、设计方案

### 2.1 System Prompt ReAct 增强

**现状**: `system_prompt.rs:18-28` 核心指令仅含通用规则（安全、格式），无推理策略。
Agent 的 ReAct 行为完全依赖 LLM 自身的 tool_use 训练，缺少任务级指导。

**改进**: 在 Zone A 核心指令中增加结构化推理指导。

```
# 推理策略 (Reasoning Strategy)

## 思维链
- 面对复杂问题时，先分解为子步骤，再逐步执行
- 每个工具调用前说明目的："I need to search for X because..."
- 每个工具结果后评估："This tells me Y, next I need Z"

## 验证与回退
- 给出最终答案前，回顾证据链是否完整
- 如果搜索无结果，用不同关键词重试（至少 2 次不同表述）
- 如果文件无法直接读取，尝试 bash + python3 处理

## 工具使用指南
- 二进制文件 (xlsx, pdf, docx, zip): 用 bash + python3 处理
- 网页内容: 用 web_fetch 获取，自动清洗 HTML
- 精确数据: 先 web_search 找来源，再 web_fetch 验证
```

**影响范围**: 所有使用 `SystemPromptBuilder` 的场景（eval + 正常对话）。
增量约 500 token，Zone A prompt caching 仍有效。

### 2.2 Web 搜索增强

#### 2.2.1 Tavily Advanced 模式

**现状**: `web_search.rs:79-91` 发送基础请求，`search_depth` 未指定（默认 basic）。

**改进**:

```rust
// 当前
json!({ "query": query, "max_results": max_results, "include_answer": true })

// 改进后
json!({
    "query": query,
    "max_results": max_results,
    "include_answer": true,
    "search_depth": "advanced",        // 深度搜索，更准确
    "include_raw_content": false,       // 不需要原始 HTML（web_fetch 已有）
    "topic": "general"                  // 明确主题
})
```

**代价**: Tavily advanced 模式每次约 2 credits（basic 1 credit），成本翻倍但准确率更高。

#### 2.2.2 Web Fetch 内容提取

**现状**: `web_fetch.rs:139` 直接返回 `response.text()`，包含大量 HTML 标签、JS、CSS。

**改进**: 添加轻量级 HTML 清洗（纯 Rust，无外部依赖）。

**清洗规则**:
1. 移除 `<script>...</script>`, `<style>...</style>` 标签及内容
2. 移除 `<nav>`, `<footer>`, `<header>`, `<aside>` 标签及内容
3. 保留语义标签文本: `<p>`, `<h1-6>`, `<li>`, `<td>`, `<th>`, `<article>`, `<main>`
4. HTML 实体解码: `&amp;` → `&`, `&lt;` → `<` 等
5. 多余空行/空白压缩

**API 兼容性**: 新增 `extract_content` 参数（默认 `true`），设为 `false` 返回原始内容。

**实现方式**: 字符串状态机遍历，不引入 HTML parser crate。
预期将 50KB HTML 压缩至 5-10KB 有效文本，大幅减少上下文占用。

### 2.3 文件解析能力

#### 2.3.1 Bash Allowlist 扩展

**现状**: `bash.rs:40-44` 白名单缺少文件处理命令。

**新增命令**:

| 命令 | 用途 | 风险评估 |
|------|------|----------|
| `unzip` | 解压 ZIP/XLSX/DOCX | 低 — 只解压到当前目录 |
| `file` | 文件类型检测 | 极低 — 只读 |
| `xxd` | 十六进制查看 | 极低 — 只读 |
| `pdftotext` | PDF 文本提取 | 低 — 只读+写文本 |
| `pip3` | 安装 Python 包 | 中 — 仅评估环境 |

**设计**: `BashTool::with_extra_allowlist(Vec<String>)` 构造方法，允许运行时扩展白名单。
默认白名单不变，eval runner 可注入额外命令。

#### 2.3.2 File Read 二进制格式支持

**现状**: `file_read.rs:103` 使用 `tokio::fs::read_to_string()`，非 UTF-8 直接报错。

**改进架构**:

```
file_read(path)
  ├─ 检测扩展名
  ├─ .csv → 直接读取，保持原格式
  ├─ .json/.jsonl/.jsonld → 直接读取，格式化输出
  ├─ .xlsx/.xls/.ods → calamine crate 提取为 CSV 表格文本
  ├─ .pdf → pdf-extract crate 提取文本
  ├─ .zip → zip crate 列出目录 + 提取文本文件
  ├─ .docx/.pptx → zip crate 解压 → 提取 XML 中文本
  └─ 其他二进制 → 返回文件类型提示 + 建议用 bash 处理
```

**新增依赖** (均为 optional feature `file-parsing`):

| Crate | 版本 | 大小 | 用途 |
|-------|------|------|------|
| `calamine` | ^0.26 | 纯 Rust | XLSX/XLS/ODS 读取 |
| `pdf-extract` | ^0.7 | 纯 Rust + C deps | PDF 文本提取 |
| `zip` | ^2.0 | 纯 Rust | ZIP 解压 |

**Feature Gate**: `file-parsing` feature 默认启用。
在 `Cargo.toml` 中:

```toml
[features]
default = ["file-parsing"]
file-parsing = ["dep:calamine", "dep:pdf-extract", "dep:zip"]
```

**Fallback**: 当 feature 关闭时，二进制文件返回提示文本：
"This is a binary file ({ext}). Use `bash` tool with `python3` to parse it."

### 2.4 GAIA Agent Manifest

创建 `config/agents/gaia_solver.yaml`:

```yaml
name: gaia-solver
role: Research Assistant
goal: Answer questions accurately by combining web search, file analysis, and multi-step reasoning
backstory: |
  You are an expert research assistant. You excel at:
  1. Breaking complex questions into manageable steps
  2. Using web search to find accurate, up-to-date information
  3. Parsing files (spreadsheets, PDFs, documents) to extract data
  4. Cross-checking facts from multiple sources before answering
  5. Giving precise, concise final answers

  When you encounter difficulties:
  - Try different search queries if the first attempt fails
  - Use python3 via bash for complex data processing
  - Verify numerical answers by recalculating
```

在 eval runner 中添加可选 manifest 配置:
- `benchmark.toml` 新增 `agent_manifest` 字段
- Runner 加载 manifest 并注入到 `AgentLoopConfig`

---

## 三、分类子集评估策略

### 3.1 子集概览

| 子集 | 数量 | 特征 | 对应改进 |
|------|:----:|------|----------|
| `gaia_basic` | 31 | 基础推理，无附件/搜索 | S1 提示工程 |
| `gaia_file` | 22 | 需要文件解析 | S3 文件解析 |
| `gaia_web` | 44 | 需要网络搜索 | S2 搜索增强 |
| `gaia_reasoning` | 45 | 多步推理 | S1 + S4 |
| `gaia_core` | 142 | 排除 media 的可评估集 | 综合 |

### 3.2 评估流程

```
Phase 1: 改进前基线 (T10)
  └─ MiniMax-M2.1 × 5 子集 → 各子集通过率

Phase 2: 实施改进 (G1-G3)
  └─ T1-T8 代码变更

Phase 3: 改进后对比 (T3/T6/T9/T11)
  └─ 同模型 × 同子集 → Delta 分析

Phase 4: 全量验证
  └─ gaia_core (142题) × MiniMax-M2.1 → 最终通过率
```

### 3.3 预期 Delta

| 子集 | R3 预估基线 | 改进后预期 | 提升来源 |
|------|:---------:|:--------:|----------|
| basic | ~55% | ~60% | 更好的推理指导 |
| file | ~15% | ~35% | XLSX/PDF 解析 + bash 扩展 |
| web | ~35% | ~50% | Tavily advanced + HTML 清洗 |
| reasoning | ~30% | ~40% | ReAct 提示 + 验证策略 |
| **core** | **~42%** | **~50%** | 综合提升 |

---

## 四、风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|:----:|:----:|------|
| `calamine` 编译失败 | 低 | 高 | 纯 Rust，已广泛使用；fallback 到 bash+python |
| `pdf-extract` C 依赖 | 中 | 中 | CI 需安装 poppler-dev；或降级为 bash pdftotext |
| System Prompt 膨胀 | 低 | 中 | ~500 token 增量，Zone A 缓存仍有效 |
| HTML 清洗误删内容 | 中 | 低 | `extract_content=false` 参数可关闭 |
| 评估 API 额度 | 低 | 中 | ~328 次调用，成本约 $3-5 |

---

## 五、验收标准

### 代码质量
- [ ] 全量测试 ≥ 2210（不减少）
- [ ] 新增测试覆盖所有改进点
- [ ] `cargo clippy --workspace -- -D warnings` 通过
- [ ] Feature gate 正确：`file-parsing` 可独立关闭

### 功能验证
- [ ] System Prompt 包含 ReAct 指导文本
- [ ] Web Search 使用 `search_depth: "advanced"`
- [ ] Web Fetch 返回清洗后内容（默认开启）
- [ ] File Read 支持 XLSX → CSV 文本输出
- [ ] File Read 支持 PDF → 文本输出
- [ ] Bash allowlist 包含 `unzip`, `file`, `xxd`

### 评估结果
- [ ] 各子集通过率均有提升（或持平）
- [ ] `gaia_core` 总通过率 ≥ 48%（目标 50%+）

---

*本文档为 Phase S 的架构级设计方案，具体任务分解见 `docs/plans/2026-03-18-phase-s-agent-capability-boost.md`。*
