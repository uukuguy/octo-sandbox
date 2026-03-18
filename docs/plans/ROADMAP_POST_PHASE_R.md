# 评估与能力提升路线图（Phase R 之后）

> 创建日期: 2026-03-18
> 基准: Phase R 完成（2210 tests, GAIA R3 最优 41.6%）
> 状态: 活跃路线图

---

## 一、当前基线

| 指标 | 数值 | 来源 |
|------|------|------|
| 单元测试 | 2210 | Phase R |
| GAIA 通过率（最优 MiniMax-M2.1） | 41.6% (62/149) | Run 011 |
| GAIA 全军覆没题 | 64/149 (43%) | R3 分析 |
| SWE-bench resolved | 3.3% | Phase Q |
| 评估数据集总量 | 1161 条 (20 JSONL) | datasets/ |
| 业界 SOTA (GAIA) | 92.36% (multi-model ensemble) | 2026-03 |
| 业界单模型裸跑参考 | ~44% (GPT-5 Mini / Claude 3.7 Sonnet) | leaderboard |

### R3 全军覆没根因分布

| 根因类别 | 数量 | 占比 |
|----------|------|------|
| 多步推理失败 | 28 | 43.8% |
| 精确搜索失败 | 17 | 26.6% |
| 文件解析失败 | 14 | 21.9% |
| YouTube 泄漏（筛选遗漏） | 5 | 7.8% |

---

## 二、12 个改进方向（完整列表）

### 类别 A：Agent 能力提升（直接提升评估分数）

#### S1: 多步 ReAct 提示工程
- **目标**: 改进 SystemPromptBuilder 的 ReAct 模板，增加 chain-of-thought、自我反思、回退策略
- **根因**: 28/64 全军覆没题是多步推理失败
- **关键文件**: `crates/octo-engine/src/context/system_prompt.rs`
- **预期收益**: GAIA +3-5%
- **工作量**: Medium (2-3 天)
- **依赖**: 无

#### S2: Web 搜索增强
- **目标**: 搜索重试策略、多查询改写、结果交叉验证
- **根因**: 17/64 全军覆没题是精确搜索失败
- **关键文件**: `crates/octo-engine/src/tools/web_search.rs`
- **预期收益**: GAIA +3-5%
- **工作量**: Medium (2-3 天)
- **依赖**: 无

#### S3: 文件解析工具
- **目标**: 添加 PDF/Excel/CSV 专用解析工具（Python-based via bash）
- **根因**: 14/64 全军覆没题是文件解析失败
- **关键文件**: `crates/octo-engine/src/tools/mod.rs`, 新建解析工具
- **预期收益**: GAIA +2-3%
- **工作量**: High (3-4 天)
- **依赖**: 需要 Python 环境

#### S4: Agent 自我纠正循环
- **目标**: AgentLoop 中增加 answer validation step — 执行后让 LLM 回顾并验证答案
- **关键文件**: `crates/octo-engine/src/agent/loop.rs`
- **预期收益**: GAIA +2-4%
- **工作量**: High (3-4 天)
- **依赖**: S1（需要好的提示词配合）

### 类别 B：评估深度（更精准的洞察）

#### S5: 分类子集评估
- **目标**: 对 6 个已有子数据集 (gaia_core/basic/file/web/reasoning/media) 分别跑评估
- **关键文件**: `config/eval/benchmark.toml`, 子集 JSONL
- **预期收益**: 精确定位薄弱环节
- **工作量**: Low (1 天)
- **依赖**: 无（子集已创建）

#### S6: 多模型 Ensemble
- **目标**: 实现 model routing（简单题小模型、难题大模型）或 majority voting
- **关键文件**: `crates/octo-engine/src/providers/chain.rs`
- **预期收益**: GAIA +5-10%（理论上接近 SOTA 方法）
- **工作量**: High (4-5 天)
- **依赖**: S5（需要先了解各模型擅长领域）

#### S7: 评估回归 CI
- **目标**: gaia_basic (31 题) 作为 CI 回归测试，每次代码变更跑一次
- **关键文件**: `.github/workflows/eval-ci.yml`
- **预期收益**: 防止能力退化
- **工作量**: Medium (2 天)
- **依赖**: S5（需要确认 basic 子集稳定性）

### 类别 C：平台与基础设施（产品化方向）

#### S8: octo-cli 交互模式
- **目标**: 实现 `octo chat` 交互模式，用户直接对话 agent
- **关键文件**: `crates/octo-cli/src/repl/`, `crates/octo-cli/src/commands/`
- **预期收益**: 提升用户体验
- **工作量**: Medium (2-3 天)
- **依赖**: 无

#### S9: MCP 工具集成
- **目标**: 在评估中通过 MCP 接入外部工具服务器，扩展 agent 能力
- **关键文件**: `crates/octo-engine/src/mcp/`, `crates/octo-eval/src/runner.rs`
- **预期收益**: GAIA +3-5%（更强工具链）
- **工作量**: Medium (2-3 天)
- **依赖**: 需要外部 MCP 服务器

#### S10: Agent 跨会话记忆
- **目标**: 利用 L2 Memory 实现评估间知识积累
- **关键文件**: `crates/octo-engine/src/memory/`
- **预期收益**: 长期能力积累
- **工作量**: Medium (2-3 天)
- **依赖**: 无

### 类别 D：工程质量（技术债务）

#### S11: SWE-bench 能力提升
- **目标**: 真实 Docker 环境 + git patch 生成，从 3.3% 提升到 10%+
- **关键文件**: `crates/octo-eval/src/benchmarks/swe_bench.rs`, Docker 配置
- **预期收益**: SWE-bench +7%
- **工作量**: High (4-5 天)
- **依赖**: Docker 环境

#### S12: 评估可视化 Dashboard
- **目标**: Web-based 评估结果历史趋势图
- **关键文件**: `crates/octo-server/src/api/`, `web/`
- **预期收益**: 可视化洞察
- **工作量**: Medium (2-3 天)
- **依赖**: 无

---

## 三、优先级排序（ROI 排序）

| 优先级 | 方向 | 类别 | ROI 理由 |
|--------|------|------|----------|
| **P0** | S2 Web 搜索增强 | A | 直接解决 26.6% 全军覆没根因，技术复杂度低 |
| **P0** | S1 ReAct 提示工程 | A | 直接解决 43.8% 全军覆没根因，零成本改进 |
| **P1** | S5 分类子集评估 | B | 低成本，精确定位后续优化方向 |
| **P1** | S3 文件解析工具 | A | 直接解决 21.9% 全军覆没根因 |
| **P2** | S4 自我纠正循环 | A | 提升推理质量，但实现复杂 |
| **P2** | S7 评估回归 CI | B | 防退化，长期价值 |
| **P3** | S9 MCP 工具集成 | C | 扩展能力，但需外部依赖 |
| **P3** | S6 多模型 Ensemble | B | 高收益但高复杂度 |
| **P3** | S8 CLI 交互模式 | C | 用户体验，非评估核心 |
| **P4** | S10 跨会话记忆 | C | 长期价值，短期不紧急 |
| **P4** | S11 SWE-bench 提升 | D | Docker 依赖重 |
| **P4** | S12 评估 Dashboard | D | 可视化辅助，非核心 |

---

## 四、推荐 Phase 分组

### Phase S: Agent 能力第一轮提升（P0 + P1）
- S1 ReAct 提示工程
- S2 Web 搜索增强
- S3 文件解析工具
- S5 分类子集评估（验证改进效果）
- **预期**: GAIA 41.6% → 50%+
- **工作量**: ~8-10 天

### Phase T: Agent 智能强化（P2）
- S4 自我纠正循环
- S7 评估回归 CI
- **预期**: GAIA 50% → 55%+
- **工作量**: ~4-5 天

### Phase U: 多模型与工具链扩展（P3）
- S6 多模型 Ensemble
- S9 MCP 工具集成
- S8 CLI 交互模式
- **预期**: GAIA 55% → 60%+, 用户体验提升
- **工作量**: ~7-9 天

### Phase V: 长期能力建设（P4）
- S10 跨会话记忆
- S11 SWE-bench 提升
- S12 评估 Dashboard
- **工作量**: ~8-10 天

---

## 五、跟踪与更新

每个 Phase 完成后更新此文件：
- [ ] Phase S 完成 → 更新基线数据
- [ ] Phase T 完成 → 更新基线数据
- [ ] Phase U 完成 → 更新基线数据
- [ ] Phase V 完成 → 更新基线数据

---

*此路线图为活跃文档，随项目进展持续更新。*
