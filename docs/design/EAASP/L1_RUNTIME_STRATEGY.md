# EAASP L1 Runtime 战略指引

> **文档性质**：指导后续 EAASP 开发的**决策与行动依据**
> **关系说明**：本文档是 `L1_RUNTIME_CANDIDATE_ANALYSIS.md` 经多轮校正后沉淀的**结论文件**。前者保留作为推导过程存档（13 节 1262 行），本文档只保留经过验证的核心判断。
> **创建日期**：2026-04-12
> **适用阶段**：EAASP v2.0 Phase 0 收尾 → Phase 1 启动

---

## 一、3 条战略性结论

### 结论 1：L1 Runtime Pool 的目标是**生态开放**，不是"选最佳"

EAASP 的 L1 Runtime 不应该被当成"选一个最好的"。L1 的**真实目的**是让使用不同技术栈、不同 agent 框架的团队都能找到**最接近他们现有资产**的起点，把自己的 agent 接入 EAASP。

因此 L1 Runtime Pool 的价值**不在于挑冠军**，而在于**覆盖面足够宽**——Rust 团队、Python 团队、TypeScript 团队、.NET 团队、已有 LangChain 资产的团队、已有 Claude Code 资产的团队，都能在 Pool 里找到自己的路径。

**这决定了后续开发路径**：不做"替换 claude-code-runtime / hermes-runtime"的事，而是做"**扩充** L1 Runtime Pool"的事——每增加一种 tier 代表就降低一类团队的接入成本。

### 结论 2：claude-code-runtime = Claude Agent SDK 的 L1 包装

这是本轮分析中最关键的身份澄清：

- `lang/claude-code-runtime-python/` 就是 Anthropic 官方 `claude-agent-sdk` Python 库的 EAASP gRPC 适配器
- `lang/hermes-runtime-python/` 就是 Hermes agent 的 EAASP gRPC 适配器
- **这 2 个已经跑通的 runtime 就是 EAASP T1 的实证**，不需要"寻找候选来替换它们"

**EAASP 目前 T1 已交付状态**：2 个生产可运行的 L1 实例（Python），都对齐了 MCP + Skills + Hooks 三件套。

### 结论 3：治理框架是 **L3 工作线**，不是 L1 候选

`Microsoft Agent Governance Toolkit` 这类项目在本轮调研中浮现，但它们**不是 L1 候选**——它们是 **L3 HookBridge 的可替换后端**。

这个认知对 EAASP 架构的意义可能大于任何 L1 选型：如果 EAASP L3 对接 Microsoft AGT 成功，所有现有 agent 框架（LangChain / CrewAI / AutoGen / OpenAI Agents / LlamaIndex）一次性获得 AGT 提供的 OWASP Agentic Top 10 全套治理能力——**EAASP 不需要自己造策略库**。

因此治理框架对接应该作为**独立工作线**，和 L1 Runtime Pool 扩充**平行推进**。

---

## 二、T0-T3 判据（本轮校正后的最终定义）

> 判据本身已与团队负责人多轮确认，作为后续所有 L1 选型、候选归类、adapter 厚度估算的依据。

### T0 — Harness-Tools 容器分离型

**定义**：agent 主体（harness）和 tools 执行环境（容器/VM/远程 sandbox）**物理分离**，通过解耦协议（Computer Protocol / Sandbox API / RPC）通信。凭证和治理策略通过协议层注入，不内嵌在 harness 或 tools 任一侧。

**为什么需要这个 tier**：支持"agent 在云端，tools 在客户侧内网"这类跨信任域部署，以及 tools 容器独立缩放/隔离/换掉的生产需求。

**判别特征**：
- harness 和 tools 在不同进程/容器/甚至不同机器
- 协议层是解耦关键，而不是共享库
- tools 容器可以被替换而不影响 harness

**EAASP 本体呼应**：
- `docs/design/SANDBOX_EXECUTION_DESIGN.md` 四种沙箱执行模式
- `grid-engine/src/sandbox/{docker,wasm,external,subprocess}.rs` 已有接口基础
- Grid 的 `external` sandbox 适配器已含部分 T0 理念

### T1 — 完整三件套 + 直接对齐 EAASP（薄 adapter）

**定义**：runtime 原生提供 **MCP Client + Skills (Markdown+YAML frontmatter) + Hooks (PreToolUse/PostToolUse)** 三件套，**且三件套直接对齐 EAASP 规范要求**。adapter 薄——只做协议转发。

**判别特征**（每一项都必须满足）：

| 三件套维度 | T1 要求 |
|---|---|
| **MCP** | 原生 MCP Client (stdio + SSE 最少)，能消费 EAASP SessionPayload.mcp_servers 5-block |
| **Skills** | Markdown + YAML frontmatter 格式，`name/description/version/allowed-tools` 字段，能无损加载 EAASP Skill v2 扩展字段（`runtime_affinity/access_scope/scoped_hooks/dependencies`）或无损映射 |
| **Hooks** | function-call 级别（per-tool）的 PreToolUse/PostToolUse 拦截点，返回语义能映射 `{Allow / Deny / Modify}` 三元决策 |

**Adapter 厚度**：1-2 天完成协议包装

**T1 已交付实例**（EAASP 当前状态）：
- ✅ `claude-code-runtime` (Python, Claude Agent SDK 包装)
- ✅ `hermes-runtime` (Python, Hermes agent 包装)

### T2 — 智能体框架，三件套某部分不完整（中 adapter）

**定义**：runtime 是完整的智能体框架，但 MCP/Skills/Hooks 三件套中**至少一项不完整或不对齐 EAASP 规范**。adapter 要补齐缺失部分 + 做映射转换。

**典型不完整形态**：

| 缺失维度 | 典型代表 | Adapter 需补齐 |
|---|---|---|
| 无 Skill manifest（只有 recipe/代码注册） | Goose | 写 Markdown frontmatter → Recipe 映射层 |
| Hook 粒度不对（批级、非 per-tool） | Nanobot | 拆分/聚合到单 tool hook |
| Server 层薄弱（channel bridge 为主） | Nanobot | 包 FastAPI/gRPC server 层 |
| 无 MCP（纯代码 tool） | 部分 framework | 写 MCP client 适配层 |

**Adapter 厚度**：3-7 天（协议包装 + 维度补齐）

**关键认知**：T1/T2 分水岭**不是**"有无 hook"（2025-2026 主流 runtime 都有 hook 了），而是**三件套的对齐完整度**。Agno 2.0、OpenCode 等新发现候选的 tier 归属需要源码验证。

### T3 — 传统 AI Framework（厚 adapter）

**定义**：根本没有"agent runtime"概念，是 Python/TS **库**。agent 抽象是图节点（LangGraph）/ crew（CrewAI）/ conversation（AutoGen）/ decorator function（Pydantic AI）。通常没有 MCP，hook 语义错位（图节点级 / conversation 级 / 无 hook）。

**Adapter 厚度**：1-3 周（造 per-tool 拦截 + MCP 适配 + skill loader + 会话管理）

**判别细节**：这类框架的"agent"概念和 EAASP 的 "session + tool + hook + skill" 模型**语义错位**——强行适配会在 16 方法 gRPC 契约上产生大量 impedance mismatch。T3 候选值得做 L1 的前提是**给已有该框架资产的团队一条接入路径**，而不是"选最佳 L1"。

---

## 三、当前候选池快照（按 tier + 实证深度）

### T0 候选（harness-tools 分离）

| 候选 | 语言 | 证据深度 | 备注 |
|---|---|---|---|
| **HexAgent** (`github.com/UnicomAI/hexagent`) | Python | web 调研 | Computer 协议，LocalNative/LocalVM/RemoteE2B 可插拔，T0 理念最佳开源实证 |
| Anthropic Computer Use | 商业 | web 调研 | 理念同源，闭源 |
| E2B.dev | Python/TS | 生态已知 | Cloud sandbox 作为远程 tool 容器 |
| Browserbase | - | web 调研 | 远程 headless 浏览器 |

**EAASP 当前状态**：T0 未交付，Phase 0 明确不做。

### T1 候选（完整三件套对齐）

| 候选 | 语言 | 实证状态 |
|---|---|---|
| ✅ **claude-code-runtime** (Claude Agent SDK 包装) | Python | **已交付，生产可运行** |
| ✅ **hermes-runtime** | Python | **已交付，生产可运行** |
| CCB (Claude Code Best) | TS/Bun | 源码已读，MCP 4 transport + 5 skill 来源最完整，**候选扩充 T1 的 TS 代表** |
| OpenCode | TS | web 调研，client/server 架构 + `tool.execute.before/after`，**tier 归属待源码验证** |
| claw-code | Rust | 源码已读，hook 规范对齐最严但**无 skill 无 server**，严格说三件套不完整，可能是 T2 |
| Claude Agent SDK (官方) | Python/TS | 已是 claude-code-runtime 的上游 |

**EAASP 当前状态**：T1 已交付 2 个（都是 Python），**待补充其他语言栈代表**。

### T2 候选（智能体框架，部分不完整）

| 候选 | 语言 | 不完整维度 |
|---|---|---|
| **Goose** | Rust | skill 维度：用 Recipe 非 Markdown frontmatter。MCP + hook 都原生（rmcp + ToolConfirmationRouter）。Block 生产验证。**Rust 团队首选 T2 起点** |
| **Agno 2.0 / AgentOS** | Python | tier 归属待源码验证（可能上移到 T1）。`pre_hook/post_hook` 含 background 非阻塞 + MCPTools + AgentOS FastAPI server |
| **Nanobot** | Python | hook 是批级（非 per-tool）。但 SKILL.md 格式最接近 EAASP。批级 hook 有独特价值（事务/预算/组合风险） |
| Aider / Cline CLI / Roo Code CLI | Python/TS | 未深入分析 |
| HexAgent | Python | Computer 协议如果不作为 T0 主特征，它也算 T2 |

**EAASP 当前状态**：T2 未交付。

### T3 候选（传统 AI Framework）

| 候选 | 语言 | 适配难度 | 备注 |
|---|---|---|---|
| **Pydantic AI** | Python | 中 | T3 里 hook 最干净（decorator function filter）+ 原生 MCP，语义与 Anthropic hook 对齐。**已有 Pydantic 生态团队首选** |
| Semantic Kernel | .NET/Py | 中 | Function Invocation Filter + 2025-03 原生 MCP。**.NET 团队独立入口** |
| LangGraph | Python | 高 | LangGraph Platform GA + 2025-07 MCP。**但图节点级 hook 语义错位** |
| CrewAI | Python | 高 | workflow 静态，hook 语义错位 |
| AutoGen | Python | 高 | conversation 级，hook 缺失 |
| MAF (Microsoft Agent Framework) | .NET/Py | 未评估 | 原文点名 |
| Google ADK | Python | 不推荐 | `before_tool_callback` 存在但 live path bug |
| LlamaIndex | Python | 不推荐 | 定位 RAG 不是 agent runtime |

**EAASP 当前状态**：T3 未交付。

### 治理框架候选（L3 工作线，平行于 L1 Pool）

| 候选 | License | 覆盖 | 对接方式 |
|---|---|---|---|
| **Microsoft Agent Governance Toolkit** | MIT | OWASP Agentic Top 10 全 10 项 + 9500+ 测试 | `agentmesh-mcp` Rust crate 可嵌入 `grid-hook-bridge` |
| Open Policy Agent (OPA) | Apache 2.0 | Rego DSL 通用策略 | HTTP 调用或嵌入 |
| Permit.io cedar-agent | Apache 2.0 | Cedar 策略 | HTTP 调用 |

**EAASP 当前状态**：治理框架工作线**尚未启动**。

---

## 四、生态覆盖现状与空白

按"团队技术栈接入路径"视角看 L1 Runtime Pool 的覆盖度：

| 团队背景 | 可选起点 | 状态 |
|---|---|---|
| **Python + Claude 生态** | claude-code-runtime (T1) | ✅ 已可用 |
| **Python + Hermes 生态** | hermes-runtime (T1) | ✅ 已可用 |
| **Python + 传统 AI/ML** | Pydantic AI (T3) / Agno 2.0 (T2?) / LangGraph (T3) | ⚠️ 候选已识别，未交付 |
| **Rust 系统团队** | Goose (T2) / Grid 自研 | ⚠️ Goose 未交付；Grid 是 EAASP 内核不是独立 L1 候选 |
| **TypeScript 团队** | CCB (T1?) / OpenCode (T1?) | ⚠️ 候选已识别，未交付 |
| **.NET 团队** | Semantic Kernel (T3) / Microsoft Agent Framework (T3) | ⚠️ 候选已识别，未交付 |
| **已有 LangChain 生态** | LangGraph (T3) | ⚠️ 候选已识别，未交付 |
| **合规/治理优先** | Microsoft AGT (L3 工作线) | ⚠️ 独立工作线，未启动 |
| **Go 团队** | 无 | ❌ **生态空白** |
| **Java 团队**（Spring AI 等） | 无 | ❌ **生态空白** |
| **跨信任域部署** | HexAgent (T0) | ❌ Phase 0 不做 |

**关键空白**：
1. **Rust 生态 T1** — Grid 是内核不是独立 L1 adapter，Rust 团队需要一个可以拿来包装自己 agent 的起点
2. **TypeScript 生态任意 tier** — CCB / OpenCode 都没交付
3. **Go / Java / .NET 全空白**
4. **治理框架工作线未启动**

---

## 五、后续开发路径图

### Phase 0 收尾（S4-S5）

**不做**：L1 Runtime Pool 扩充、治理框架对接、T0 实例
**继续做**：
- S4.T2 threshold-calibration E2E 验证（已规划）
- S4.T3 阶段收尾
- 保持 claude-code-runtime + hermes-runtime 双 T1 基线稳定

**原因**：Phase 0 核心目的是**跑通协议和三件套基线**，L1 Runtime Pool 的扩充是 Phase 1+ 的事。

### Phase 1 启动（预计 S5 之后）

按优先级分 3 条并行工作线：

#### 工作线 A：L1 Runtime Pool 生态扩充

| 优先级 | 任务 | 目的 | 工作量估计 |
|---|---|---|---|
| P0 | **确认 OpenCode / Agno 2.0 的 tier 归属**（本地 clone 源码评估 2 天） | 影响 T1/T2 候选清单准确性 | 2 天 |
| P0 | **为每个 tier 的首选候选做 1 份"贡献者入门包"**（架构摘要 + 接入步骤 2-3 页） | 降低外部团队接入成本 | 每份 1-2 天 × 4 份 |
| P1 | **交付 1 个 TypeScript T1 实例**（CCB 或 OpenCode，待 P0 任务确定） | 补齐 TS 生态空白 | 1-2 周 spike |
| P1 | **交付 1 个 Rust T2 实例**（Goose adapter） | 补齐 Rust 生态空白 | 2 周 |
| P2 | **交付 1 个 Python T3 实例**（Pydantic AI adapter） | 为 Pydantic 生态团队提供起点 | 2-3 周（厚 adapter） |
| P3 | 调研 Go/Java 生态候选（Spring AI 等） | 补齐语言空白 | 1 周调研 |

#### 工作线 B：治理框架对接评估（独立）

| 优先级 | 任务 | 触发条件 |
|---|---|---|
| P1 | **读 Microsoft AGT 官方白皮书 + `agentmesh-mcp` Rust crate 接口面** | S4 完成后 |
| P1 | **评估 `hook_bridge.proto` 与 AGT 协议的差距** | 依赖前一项 |
| P2 | **POC：EAASP L3 对接 Microsoft AGT** | 依赖前两项结果 |
| P3 | 对比 OPA / cedar-agent 作为备选后端 | POC 完成后 |

#### 工作线 C：T0 模式启动（可选，视资源）

| 优先级 | 任务 |
|---|---|
| P2 | **深度评估 HexAgent Computer 协议**（本地 clone 源码 2-3 天） |
| P3 | 评估 Grid `external` sandbox 是否可直接作为 T0 L1 实例的基础 |
| P3 | 规划 Phase 1 是否把 T0 作为独立 milestone |

### Phase 2+

基于 Phase 1 的成果决定：
- 是否把 L1 Runtime Pool 开放为外部贡献项目
- 是否把 Microsoft AGT 作为 EAASP 默认 L3 后端
- 是否把 T0 作为生产级部署模式

---

## 六、开发原则（从本次分析中沉淀）

### 原则 1：L1 Runtime Pool 扩充只看"覆盖度 + 质量"，不搞"最佳排名"

每个 tier 都应该有**多个**候选实例，分别对应不同的团队技术栈背景。不要因为某个候选"次一点"就拒绝交付——只要它是某个技术栈团队的最佳入口，就有价值。

### 原则 2：先补证据再做结论

本次分析最大的教训是：**Web 调研结果必须用源码验证才能作为决策依据**。subagent 的输出只是线索，多次出现的错误（把 CCB 判成"IDE 插件"、把 Agno 默认归到 T2）都是因为没有源码验证就下结论。

**行动约束**：
- 任何"tier 归属判断"必须有源码证据（文件路径+关键函数名）
- 任何"候选评分"必须有一手验证，不能只依赖 README
- 任何"替换现有实现"的建议都需要 2 天以上的 spike 验证

### 原则 3：治理框架对接是独立优先级

不要把治理框架的选型混入 L1 选型讨论。它们是**两条独立工作线**，各自有独立的决策依据和优先级。

### 原则 4：分层判据是"adapter 厚度 + 对齐度"，不是"server 最强"或"生产验证度"

T0-T3 的分层是**实施难度分层**，不是"哪个好"分层。Goose 有 Block 生产验证、CCB 有最完整 MCP client、HexAgent 有独创 Computer 协议——它们各自回答的是**不同 tier 的问题**，不能横向比。

### 原则 5：前 12 节分析文档保留作为推导过程存档

`L1_RUNTIME_CANDIDATE_ANALYSIS.md` (13 节 1262 行) 保留作为推导过程的**完整记录**——包括被纠正的错误结论、多轮校正痕迹、未验证候选等。它不是"给人看的决策文档"，是"审计溯源文档"。本文档才是给人看的。

---

## 七、下一步具体行动（按时间顺序）

### 本会话之后立即可做（P0，低成本）

1. **更新 memory**：保存本文档的核心结论作为跨会话 memory（`project_eaasp_v2_l1_runtime_strategy.md`）
2. **在 `docs/plans/` 下创建 Phase 1 L1 Runtime Pool 扩充计划草稿**，把"工作线 A/B/C"落地为具体 task
3. **提交本文档作为 Phase 0 的 deliverable**（commit 信息说明本文档替代前文档作为决策依据）

### S4 阶段内（P1，2-3 天）

1. **本地 clone OpenCode 和 Agno 2.0** 做 2 天源码评估，确定 tier 归属（填补未决问题 #2 和 #3）
2. **深读 Microsoft AGT 官方白皮书** 1 天，产出一份对接可行性备忘录
3. **在 `docs/plans/` 创建"L1 Runtime 贡献者指南"计划草稿**（不是立刻写指南本身，而是先规划它的内容结构）

### S4 结束后（P2，Phase 1 启动）

按工作线 A/B/C 并行推进。

---

## 八、文档维护

| 项目 | 说明 |
|---|---|
| 创建日期 | 2026-04-12 |
| 基于 | `L1_RUNTIME_CANDIDATE_ANALYSIS.md` 13 节调研（作为推导过程存档） |
| 更新触发 | OpenCode/Agno tier 归属确认 / Microsoft AGT 对接评估完成 / Phase 1 L1 Pool 扩充任何一个实例交付 |
| 相关文档 | `EAASP_v2_0_EVOLUTION_PATH.md`（本文档不修改原文，原文更新由独立决策触发）|
| 相关 memory | `memory/project_eaasp_v2_l1_runtime_strategy.md`（待创建，保存本文档核心结论） |
| 适用读者 | EAASP 核心开发者、外部贡献者、架构评审者 |
