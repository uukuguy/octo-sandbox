# Phase S — Agent 能力第一轮提升

> 创建日期: 2026-03-18
> 前置: Phase R (GAIA Filtered Evaluation) 完成
> 路线图: `docs/plans/ROADMAP_POST_PHASE_R.md` 中 P0 + P1 方向
> 目标: GAIA 通过率从 41.6% 提升至 50%+

---

## 目标

针对 R3 评估中 64/149 全军覆没题的三大根因（多步推理 43.8%、精确搜索 26.6%、文件解析 21.9%），
实施四项改进，并通过分类子集评估验证效果。

---

## 背景分析（基于代码调研）

### 当前瓶颈

| 瓶颈 | 现状 | 影响 |
|------|------|------|
| **System Prompt 无 ReAct 指导** | `system_prompt.rs` Zone A 仅含角色+通用指令，无显式思维链/工具使用策略 | 多步推理失败 28 题 |
| **Web Search 无智能重试** | `web_search.rs` 直接发送原始 query，Tavily 失败后 fallback DDG，无查询改写 | 精确搜索失败 17 题 |
| **Web Fetch 返回原始 HTML** | `web_fetch.rs` 直接 `response.text()`，无内容提取/可读性处理 | agent 被 HTML 噪音淹没 |
| **File Read 仅 UTF-8** | `file_read.rs` 用 `read_to_string()`，无法读取 XLSX/PDF/DOCX | 文件解析失败 14 题 |
| **Bash Allowlist 不完整** | 缺 `unzip`、`pdftotext`，虽有 `python3` 但 agent 不知如何用 | 文件解析变通路径受阻 |

### 关键代码位置

| 文件 | 行号 | 作用 |
|------|------|------|
| `crates/octo-engine/src/context/system_prompt.rs` | 18-28, 285-330 | 核心指令 + Zone A 构建 |
| `crates/octo-engine/src/tools/web_search.rs` | 79-91, 140-161 | Tavily 搜索 + DDG fallback |
| `crates/octo-engine/src/tools/web_fetch.rs` | 139-148 | 原始内容返回 + 截断 |
| `crates/octo-engine/src/tools/file_read.rs` | 103 | `read_to_string()` UTF-8 only |
| `crates/octo-engine/src/tools/bash.rs` | 40-44, 61-70 | Allowlist + 元字符阻断 |
| `crates/octo-engine/src/tools/mod.rs` | 119-131 | `default_tools()` 注册 |
| `crates/octo-engine/src/agent/harness.rs` | 232-900+ | Agent Loop 主循环 |
| `crates/octo-engine/src/agent/loop_steps.rs` | 110-134 | 错误提示生成 |
| `crates/octo-eval/src/runner.rs` | 219-239, 260-266 | Eval 工具注入 + 附件复制 |
| `crates/octo-eval/src/benchmarks/gaia.rs` | 40-64 | GAIA 任务提示构建 |

---

## 任务分解

### G1: ReAct 提示工程 (3 tasks) — 解决多步推理失败

#### T1: 增强核心 System Prompt
- 在 `system_prompt.rs` 的核心指令中添加 ReAct 风格指导
- 增加内容：
  1. **思维链提示**: "Before taking action, reason step-by-step about what information you need and which tools to use"
  2. **工具使用策略**: "When a task requires multiple steps, plan your approach first, then execute one step at a time"
  3. **答案验证**: "Before giving your final answer, verify it by cross-checking with available evidence"
  4. **搜索策略**: "If a web search returns no relevant results, reformulate your query with different keywords"
  5. **文件处理提示**: "For binary files (xlsx, pdf, docx, zip), use `python3` with appropriate libraries or `bash` commands to extract content"
- 不改变 Zone A/B 架构，仅扩展核心指令文本
- **文件**: `crates/octo-engine/src/context/system_prompt.rs`
- **新增测试**: 验证新指令出现在 system prompt 输出中

#### T2: GAIA 专用 Agent Manifest
- 创建 `config/agents/gaia_solver.yaml` agent manifest
- 配置：
  - `role`: "Research Assistant specialized in multi-step reasoning"
  - `goal`: "Answer questions accurately using available tools"
  - `backstory`: ReAct 专家角色描述，强调逐步推理和验证
- 在 eval runner 中支持可选的 agent manifest 注入
- **文件**: `config/agents/gaia_solver.yaml`, `crates/octo-eval/src/runner.rs`
- **新增测试**: 验证 manifest 正确加载和注入

#### T3: 评估提示增强效果
- 使用 `gaia_basic` 子集 (31 题) 对比改进前后
- 配置: 1 个模型 (MiniMax-M2.1) × 31 题
- 对比 R3 baseline 同子集数据
- **运行时任务**: 需用户确认后执行

### G2: Web 搜索增强 (3 tasks) — 解决精确搜索失败

#### T4: 搜索结果质量提升
- 在 `web_search.rs` 中增强 Tavily 请求：
  1. `search_depth: "advanced"` (Tavily 高级搜索)
  2. `include_raw_content: true` (获取原始页面内容)
  3. `topic: "general"` (明确主题)
- 增加结果格式化：提取 Tavily 的 `answer` 字段更显著展示
- **文件**: `crates/octo-engine/src/tools/web_search.rs`
- **新增测试**: 验证请求参数正确、结果格式化正确

#### T5: Web Fetch 内容提取
- 在 `web_fetch.rs` 中添加基础内容提取：
  1. HTML 标签清洗：去除 `<script>`, `<style>`, `<nav>`, `<footer>`, `<header>` 标签及内容
  2. 保留语义标签内文本：`<p>`, `<h1-6>`, `<li>`, `<td>`, `<th>`, `<article>`, `<main>`
  3. 多余空白压缩
  4. 添加 `extract_content` 参数（默认 true），允许关闭
- 不引入外部依赖，用简单的正则/字符串处理实现
- **文件**: `crates/octo-engine/src/tools/web_fetch.rs`
- **新增测试**: HTML 标签清洗、空白压缩、参数控制

#### T6: 搜索结果改进效果评估
- 使用 `gaia_web` 子集 (44 题) 评估搜索改进
- 配置: 1 模型 × 44 题
- 对比 R3 baseline 同子集
- **运行时任务**: 需用户确认后执行

### G3: 文件解析能力 (3 tasks) — 解决文件解析失败

#### T7: Bash Allowlist 扩展
- 在 `bash.rs` 的 allowlist 中添加文件处理命令：
  - `unzip` — 解压 ZIP/XLSX/DOCX (本质都是 ZIP)
  - `file` — 文件类型检测
  - `xxd` — 十六进制查看
  - `pdftotext` — PDF 文本提取（如果系统有）
  - `pip3` — 安装 Python 包（仅 eval 环境需要）
- 添加 allowlist 可配置机制：`BashTool::with_extra_allowlist(Vec<String>)`
- **文件**: `crates/octo-engine/src/tools/bash.rs`
- **新增测试**: 验证扩展命令被允许、原有阻断规则不受影响

#### T8: File Read 二进制格式支持
- 在 `file_read.rs` 中添加格式检测和委托解析：
  1. 检测文件扩展名：`.xlsx`, `.csv`, `.pdf`, `.docx`, `.zip`, `.json`, `.jsonl`, `.jsonld`
  2. CSV: 直接读取，格式化为表格文本
  3. XLSX: 使用 `calamine` crate (纯 Rust) 提取为 CSV 文本
  4. PDF: 使用 `pdf-extract` crate 提取文本（或 fallback 到提示用 bash）
  5. ZIP: 列出目录结构 + 提取文本文件
  6. 其他二进制: 返回文件类型提示 + 建议使用 bash 处理
- 添加 `calamine` 和 `pdf-extract` 到 `octo-engine/Cargo.toml` (optional features)
- **文件**: `crates/octo-engine/src/tools/file_read.rs`, `crates/octo-engine/Cargo.toml`
- **新增测试**: 各格式读取、fallback 提示、特性门控

#### T9: GAIA 文件子集评估
- 使用 `gaia_file` 子集 (22 题) 评估文件解析改进
- 配置: 1 模型 × 22 题
- 对比 R3 baseline 同子集
- **运行时任务**: 需用户确认后执行

### G4: 分类子集基线评估 (2 tasks) — 精确定位薄弱环节

#### T10: 全子集基线扫描
- 对 5 个子集 (basic, file, web, reasoning, core) 用最优模型 (MiniMax-M2.1) 跑基线
- 目的：在改进前建立各子集的精确基线
- 配置: `config/eval/gaia_s_baseline.toml`
- **运行时任务**: 需用户确认后执行

#### T11: 改进后全子集对比评估
- 在 G1-G3 改进完成后，重新跑全子集对比
- 与 T10 基线 + R3 全量数据对比
- 生成改进效果报告，更新路线图基线数据
- **运行时任务**: 需用户确认后执行

### G5: 收尾 (2 tasks)

#### T12: 更新设计文档和路线图
- 更新 `docs/plans/ROADMAP_POST_PHASE_R.md` 基线数据
- 更新 `docs/design/STANDARD_BENCHMARK_DESIGN.md` 工具链章节
- 更新 `CLAUDE.md` 中相关工具描述（如果有变化）
- **文件**: 多个文档

#### T13: Checkpoint + Commit
- 运行全量测试: `cargo test --workspace -- --test-threads=1`
- 更新 `.checkpoint.json`
- git commit

---

## 依赖关系

```
T1 (system prompt) ──┐
T2 (agent manifest) ─┤
                     ├→ T3 (prompt 效果评估)
T4 (web search) ─────┤
T5 (web fetch) ──────┤→ T6 (搜索效果评估)
T7 (bash allowlist) ─┤
T8 (file read) ──────┤→ T9 (文件效果评估)
                     │
T10 (基线扫描) ──────→ [G1-G3 改进] → T11 (对比评估)
                     │
                     └→ T12 (文档) → T13 (commit)
```

### 推荐执行顺序

**批次 1** (可并行): T1 + T4 + T5 + T7 + T8 — 所有代码改进
**批次 2**: T2 — agent manifest (依赖 T1 的提示设计)
**批次 3**: T10 — 改进前基线扫描（可与批次 1 并行）
**批次 4** (需用户): T3 + T6 + T9 — 各子集效果评估
**批次 5** (需用户): T11 — 全子集对比评估
**批次 6**: T12 + T13 — 收尾

---

## 新增依赖

| Crate | 版本 | 用途 | Feature Gate |
|-------|------|------|-------------|
| `calamine` | ^0.26 | XLSX/XLS/ODS 读取 (纯 Rust) | `file-parsing` |
| `pdf-extract` | ^0.7 | PDF 文本提取 | `file-parsing` |
| `zip` | ^2.0 | ZIP 解压列目录 | `file-parsing` |

所有依赖在 `file-parsing` feature 下，默认启用，可在嵌入式/最小化场景关闭。

---

## 风险

1. **`calamine`/`pdf-extract` 编译兼容性**: 纯 Rust，预期无问题，但需验证
2. **System Prompt 膨胀**: ReAct 指令增加 ~500 token，需关注 Zone A 缓存命中率
3. **HTML 清洗准确度**: 简单正则可能误删有效内容，需要 fallback
4. **评估 API 额度**: 5 子集 × 1 模型 ≈ 164 次调用 × 2 轮 (基线+改进) = ~328 次
5. **Python 环境**: GAIA file 任务依赖 `openpyxl` 等 Python 包，评估机器需预装

---

## 成功标准

- [ ] System Prompt 包含 ReAct 指导，测试通过
- [ ] Web Search 使用 Tavily advanced 模式，测试通过
- [ ] Web Fetch 返回清洗后内容，测试通过
- [ ] File Read 支持 XLSX/PDF/CSV，测试通过
- [ ] Bash allowlist 扩展，测试通过
- [ ] 全量测试 2210+ 通过（不减少）
- [ ] GAIA 子集对比评估完成
- [ ] GAIA 总通过率 ≥ 48%（目标 50%+）
