# EAASP L1 Runtime 分层判据修正附录
# EAASP L1 Runtime Tier Classification Criteria — Amendment Appendix

> 供合入 v2.0 设计规范 §8.5
> For incorporation into v2.0 Design Specification §8.5
>
> 基于源码验证结论（R1 OpenCode + R2 Agno），更新于 2026-04-12
> Based on source-code verification findings (R1 OpenCode + R2 Agno), updated 2026-04-12

---

## T0 — Harness-Tools 容器分离型 / Harness-Tools Container Isolation

### 定义 / Definition

Agent 主体（harness）和 tools 执行环境（容器/VM/远程 sandbox）**物理分离**，通过解耦协议（Computer Protocol / Sandbox API / RPC）通信。凭证和治理策略通过协议层注入，不内嵌在 harness 或 tools 任一侧。

The agent body (harness) and the tools execution environment (container / VM / remote sandbox) are **physically separated**, communicating via a decoupled protocol (Computer Protocol / Sandbox API / RPC). Credentials and governance policies are injected through the protocol layer, not embedded in either the harness or the tools side.

### 判别特征 / Distinguishing Features

| 特征 / Feature | 说明 / Description |
|---|---|
| 进程/容器分离 / Process-container separation | Harness 和 tools 在不同进程/容器/甚至不同机器 / Harness and tools reside in different processes, containers, or even different machines |
| 协议层解耦 / Protocol-layer decoupling | 协议层是解耦关键，而不是共享库 / The protocol layer is the decoupling mechanism, not a shared library |
| Tools 可替换性 / Tools replaceability | Tools 容器可以被替换而不影响 harness / Tool containers can be replaced without affecting the harness |

### 适用场景 / Use Cases

支持"agent 在云端，tools 在客户侧内网"这类**跨信任域部署**，以及 tools 容器**独立缩放/隔离/替换**的生产需求。

Supports cross-trust-domain deployments such as "agent in the cloud, tools in the customer's intranet", as well as production requirements for independent scaling, isolation, and replacement of tool containers.

### EAASP 本体呼应 / EAASP Alignment

- `SANDBOX_EXECUTION_DESIGN.md` 四种沙箱执行模式 / Four sandbox execution modes
- `grid-engine/src/sandbox/{docker,wasm,external,subprocess}.rs` 已有接口基础 / Existing interface foundation
- Grid 的 `external` sandbox 适配器已含部分 T0 理念 / Grid's `external` sandbox adapter already embodies partial T0 concepts

### 代表项目 / Representative Projects

| 项目 / Project | 语言 / Language | 证据深度 / Evidence Depth | 备注 / Notes |
|---|---|---|---|
| **HexAgent** (`github.com/UnicomAI/hexagent`) | Python | Web 调研 / Web research | Computer 协议，LocalNative/LocalVM/RemoteE2B 可插拔 / Computer protocol, pluggable adapters — best open-source T0 exemplar |
| Anthropic Computer Use | 商业 / Commercial | Web 调研 / Web research | 理念同源，闭源 / Same conceptual origin, closed-source |
| E2B.dev | Python/TS | 生态已知 / Known ecosystem | 云端 sandbox 作为远程 tool 容器 / Cloud sandbox as remote tool container |
| Browserbase | - | Web 调研 / Web research | 远程 headless 浏览器 / Remote headless browser |

**EAASP 当前状态 / Current Status**：T0 未交付，Phase 0 明确不做。 / T0 not delivered; explicitly excluded from Phase 0.

---

## T1 — 完整三件套 + 薄 Adapter / Complete Triad + Thin Adapter

### 定义 / Definition

Runtime 原生提供 **MCP Client + Skills (Markdown+YAML frontmatter) + Hooks (PreToolUse/PostToolUse)** 三件套，**且三件套直接对齐 EAASP 规范要求**。Adapter 薄——只做协议转发。

The runtime natively provides the **MCP Client + Skills (Markdown+YAML frontmatter) + Hooks (PreToolUse/PostToolUse)** triad, **and the triad directly aligns with EAASP specification requirements**. The adapter is thin — protocol forwarding only.

### 三件套判别矩阵 / Triad Qualification Matrix

| 维度 / Dimension | T1 要求 / T1 Requirement | 说明 / Description |
|---|---|---|
| **MCP** | 原生 MCP Client（stdio + SSE 最少） / Native MCP Client (stdio + SSE minimum) | 能消费 EAASP `SessionPayload.mcp_servers` 5-block / Must consume EAASP `SessionPayload.mcp_servers` 5-block |
| **Skills** | Markdown + YAML frontmatter 格式 / Markdown + YAML frontmatter format | `name/description/version/allowed-tools` 字段；能无损加载或映射 EAASP Skill v2 扩展字段（`runtime_affinity/access_scope/scoped_hooks/dependencies`） / Core fields + lossless loading or mapping of EAASP Skill v2 extension fields |
| **Hooks** | function-call 级别（per-tool）的 PreToolUse/PostToolUse 拦截点 / Per-tool-call PreToolUse/PostToolUse interception points | 返回语义能映射 `{Allow / Deny / Modify}` 三元决策 / Return semantics mappable to `{Allow / Deny / Modify}` ternary decision |

**Adapter 厚度 / Adapter Thickness**：1-4 天完成协议包装 / 1-4 days for protocol wrapping

### 代表项目 / Representative Projects

| 项目 / Project | 语言 / Language | 实证状态 / Verification Status | Adapter 厚度 / Adapter Thickness |
|---|---|---|---|
| **claude-code-runtime** (Claude Agent SDK 包装) | Python | **已交付，生产可运行** / Delivered, production-ready | 已完成 / Done |
| **hermes-runtime** | Python | **已交付，生产可运行** / Delivered, production-ready | 已完成 / Done |
| **OpenCode** | TypeScript | **R1 源码验证 T1** / R1 source-verified T1 | 3-4 天 / 3-4 days |
| CCB (Claude Code Best) | TypeScript/Bun | 源码已读，待正式评估 / Source read, pending formal evaluation | 待评估 / TBD |
| claw-code | Rust | 源码已读，hook 规范对齐最严但无 skill 无 server / Source read, strict hook alignment but no skill/server | 可能 T2 / Possibly T2 |
| Claude Agent SDK (官方) | Python/TS | 已是 claude-code-runtime 的上游 / Upstream of claude-code-runtime | N/A |

### OpenCode T1 判定关键证据 / OpenCode T1 Key Evidence

> 来源：`L1_RUNTIME_R1_OPENCODE_EVAL.md`（2026-04-12 源码验证）
> Source: `L1_RUNTIME_R1_OPENCODE_EVAL.md` (2026-04-12 source verification)

**项目概况 / Project Overview**：TypeScript 5.8 + Bun 1.3 + Effect-TS，~60,407 行，20+ LLM Provider，Vercel AI SDK

| 维度 / Dimension | 状态 / Status | 关键证据 / Key Evidence |
|---|---|---|
| **MCP** | **完全达标** / Fully qualified | stdio + SSE + Streamable HTTP 三种 transport 全覆盖；OAuth 2.0 完整流程；多 Server 并发初始化 + namespace 隔离。文件：`packages/opencode/src/mcp/index.ts` (927 行) |
| **Skills** | **格式达标，字段需扩展** / Format qualified, fields need extension | Markdown + YAML frontmatter（gray-matter 解析），已有 `name/description`，缺 `version/allowed-tools/v2 扩展字段`。扩展工作量 0.5 天。文件：`packages/opencode/src/skill/index.ts` |
| **Hooks** | **两系统组合达标** / Qualified via two-system composition | per-tool `tool.execute.before/after`（Plugin 系统）提供 **Modify** 语义；独立 Permission 系统（`permission/evaluate.ts`）提供 **Allow/Deny/Ask** 语义。EAASP adapter 需桥接两系统以覆盖完整 Allow/Deny/Modify 三元决策 |

**Adapter 厚度明细 / Adapter Thickness Breakdown**:

| 适配项 / Adaptation Item | 工作量 / Effort | 说明 / Description |
|---|---|---|
| SessionPayload 注入 / SessionPayload injection | 0.5d | `SystemPrompt.provider()` 已有系统提示构建点 / Existing system prompt build point |
| MCP Server 消费 / MCP Server consumption | 0.5d | `MCP.add(name, config)` API 已就绪 / API ready |
| Skill frontmatter 扩展 / Skill frontmatter extension | 0.5d | 在 `Skill.Info` schema 添加 v2 字段 / Add v2 fields to schema |
| Hook-EAASP 桥接 / Hook-EAASP bridge | 1d | 创建 `EaaspHookBridge` 桥接 Plugin hook + Permission 系统 / Bridge Plugin hooks + Permission system |
| Telemetry 上报 / Telemetry reporting | 0.5d | `SessionProcessor` 已有 token/cost 事件 / Existing token/cost events |
| gRPC RuntimeService | 1d | 16 方法 gRPC 服务包装 / 16-method gRPC service wrapper |
| **合计 / Total** | **3-4d** | |

**EAASP 关键贡献 / Key Contributions to EAASP**：填补 L1 Runtime Pool 的 TypeScript 生态空白；20+ LLM Provider 覆盖远超现有 runtime；Effect-TS Service/Layer 架构天然支持可测试性；企业级 Permission 系统（Rule-based Allow/Deny/Ask + 持久化审批记录）。

Fills the TypeScript ecosystem gap in the L1 Runtime Pool; 20+ LLM providers far exceeding existing runtimes; Effect-TS Service/Layer architecture naturally supports testability; enterprise-grade Permission system (Rule-based Allow/Deny/Ask + persistent approval records).

---

## T2 — 智能体框架，部分不完整 / Agent Framework, Partially Incomplete

### 定义 / Definition

Runtime 是完整的智能体框架，但 MCP/Skills/Hooks 三件套中**至少一项不完整或不对齐 EAASP 规范**。Adapter 要补齐缺失部分 + 做映射转换。

The runtime is a complete agent framework, but **at least one item in the MCP/Skills/Hooks triad is incomplete or misaligned with the EAASP specification**. The adapter must fill in the missing parts and perform mapping transformations.

**Adapter 厚度 / Adapter Thickness**：3-7 天（协议包装 + 维度补齐）/ 3-7 days (protocol wrapping + dimension completion)

### 典型不完整形态 / Typical Incompleteness Patterns

| 缺失维度 / Missing Dimension | 典型代表 / Typical Representative | Adapter 需补齐 / Adapter Must Provide |
|---|---|---|
| 无 Skill manifest（只有 recipe/代码注册）/ No Skill manifest (recipe/code registration only) | Goose | Markdown frontmatter-Recipe 映射层 / Markdown frontmatter-to-Recipe mapping layer |
| Hook 粒度不对（批级/run 级，非 per-tool）/ Hook granularity mismatch (batch/run-level, not per-tool) | Nanobot, **Agno 2.0** | 拆分/聚合到单 tool hook / Split/aggregate to per-tool hooks |
| Server 层薄弱（channel bridge 为主）/ Thin server layer (channel bridge only) | Nanobot | 包 FastAPI/gRPC server 层 / Wrap FastAPI/gRPC server layer |
| 无 MCP（纯代码 tool）/ No MCP (code-only tools) | 部分 framework / Some frameworks | MCP client 适配层 / MCP client adaptation layer |

### 代表项目 / Representative Projects

| 项目 / Project | 语言 / Language | 不完整维度 / Incomplete Dimension | 实证状态 / Verification Status |
|---|---|---|---|
| **Agno 2.0 / AgentOS** | Python | Hooks: agent-run 级别非 per-tool / Hooks: agent-run level, not per-tool | **R2 源码验证 T2** / R2 source-verified T2 |
| **Goose** | Rust | Skills: 用 Recipe 非 Markdown frontmatter / Skills: Recipe, not Markdown frontmatter | 待深度验证 / Pending deep verification |
| **Nanobot** | Python | Hooks 批级 + Server 层薄弱 / Batch-level hooks + thin server layer | 待深度验证 / Pending deep verification |
| Aider / Cline CLI / Roo Code CLI | Python/TS | 未深入分析 / Not deeply analyzed | 待评估 / Pending |
| HexAgent | Python | 若不作为 T0 主特征，也算 T2 / If not classified by T0 primary feature, also T2 | 待评估 / Pending |

### Agno T2 判定关键证据 / Agno T2 Key Evidence

> 来源：`L1_RUNTIME_R2_AGNO_EVAL.md`（2026-04-12 源码验证）
> Source: `L1_RUNTIME_R2_AGNO_EVAL.md` (2026-04-12 source verification)

**项目概况 / Project Overview**：Python 3.7+ + Pydantic + FastAPI (AgentOS)，v2.5.16（前身 Phidata），~289,000 行，40+ LLM Provider，120+ 内置工具

| 维度 / Dimension | 状态 / Status | 关键证据 / Key Evidence |
|---|---|---|
| **MCP** | **完全达标** / Fully qualified | stdio + SSE + Streamable HTTP 三种 transport；生产级实现（session 管理、TTL 清理、动态 header、per-run session 隔离）；`MultiMCPTools` 支持多 Server。文件：`libs/agno/agno/tools/mcp/mcp.py` (663+ 行) |
| **Skills** | **完全达标（甚至超标）** / Fully qualified (even exceeding) | SKILL.md + YAML frontmatter + `allowed-tools`；validator 校验 `{name, description, license, allowed-tools, metadata, compatibility}`；XML 格式 system prompt 注入。文件：`libs/agno/agno/skills/` |
| **Hooks** | **不达标** / Not qualified | `pre_hooks/post_hooks` 在 `agent.run()` 入口/出口各**一次**，签名无 `tool_name` 参数。仅有 agent-run 级别粒度，**无 per-tool 拦截** |

**Hooks 差距明细 / Hooks Gap Detail**:

| EAASP 要求 / EAASP Requirement | Agno 实现 / Agno Implementation | 差距 / Gap |
|---|---|---|
| PreToolUse per-tool 拦截 / PreToolUse per-tool interception | `pre_hooks` 在 run 入口一次 / `pre_hooks` once at run entry | **缺失** / Missing |
| PostToolUse per-tool 拦截 / PostToolUse per-tool interception | `post_hooks` 在 run 结束一次 / `post_hooks` once at run exit | **缺失** / Missing |
| Allow/Deny/Modify 返回值 / Allow/Deny/Modify return value | Guardrail 仅支持 Deny (raise) / Guardrail supports Deny only (raise) | **Modify 缺失** / Modify missing |
| `tool_name` 过滤 / `tool_name` filtering | 无 / None | **缺失** / Missing |

**替代机制（不等价）/ Alternative Mechanisms (Not Equivalent)**:
- `requires_confirmation` / `external_execution`：用户交互级 HITL 暂停，非程序化 hook / User-interaction-level HITL pause, not programmatic hooks
- `@approval` 装饰器：审批系统，也是 HITL / Approval system, also HITL
- Guardrails：只能对 run input/output 做检查，非 per-tool / Run input/output checks only, not per-tool

**Adapter 厚度明细 / Adapter Thickness Breakdown**:

| 适配项 / Adaptation Item | 工作量 / Effort | 说明 / Description |
|---|---|---|
| PerToolUse Hook 注入点 / PerToolUse hook injection points | 2-3d | 在 `_run.py` tool-call loop 中注入拦截点（侵入式改造）/ Inject interception in `_run.py` tool-call loop (invasive modification) |
| Hook 参数扩展 / Hook parameter extension | 1d | 在 hook callback 签名中传递 `tool_name/tool_args/tool_call_id` / Pass tool context in hook callback signature |
| ScopedHookHandler 桥接 / ScopedHookHandler bridge | 1-2d | EAASP ScopedHook-Agno hook 翻译 / Translate EAASP ScopedHook to Agno internal hooks |
| SessionPayload 映射 / SessionPayload mapping | 1d | 5-block SessionPayload-Agent 初始化参数 / Map 5-block SessionPayload to Agent init params |
| gRPC RuntimeService | 2-3d | 16 方法 gRPC 服务 / 16-method gRPC service |
| Telemetry 采集 / Telemetry collection | 1d | Agno 内部 metrics-L3 telemetry / Forward internal metrics to L3 |
| **合计 / Total** | **5-7d** | 接近 T2 上界，但 MCP/Skills 零成本 / Near T2 upper bound, but MCP/Skills zero-cost |

**EAASP 关键贡献 / Key Contributions to EAASP**：120+ 内置工具覆盖企业集成；40+ LLM Provider（含本地部署 Ollama/vLLM/llama.cpp）；原生 Team 多 Agent 编排 + Workflow DAG；AgentOS FastAPI Server + 多接口（A2A, Slack, Telegram, WhatsApp）；内置 Knowledge/RAG + 多层 Memory + Eval 框架。Python AI/ML 生态天然覆盖——已有 Python AI/ML 资产的团队的最自然 L1 接入起点。

120+ built-in tools covering enterprise integrations; 40+ LLM providers (including local deployment via Ollama/vLLM/llama.cpp); native Team multi-agent orchestration + Workflow DAG; AgentOS FastAPI server + multi-interface (A2A, Slack, Telegram, WhatsApp); built-in Knowledge/RAG + multi-layer memory + eval framework. Natural coverage of the Python AI/ML ecosystem — the most natural L1 entry point for teams with existing Python AI/ML assets.

### T1/T2 分水岭 / T1-T2 Watershed

T1/T2 分水岭**不是**"有无 hook"——2025-2026 主流 runtime 都有 hook 了——而是 **per-tool hook 粒度**和**三件套的对齐完整度**。

The T1/T2 watershed is **not** "whether hooks exist" — all mainstream runtimes in 2025-2026 have hooks — but rather the **per-tool hook granularity** and **alignment completeness of the triad**.

具体判别标准 / Specific criteria:

| 判别点 / Criterion | T1 | T2 |
|---|---|---|
| Hook 触发粒度 / Hook trigger granularity | 每次 tool call 触发（per-tool）/ Fires on every tool call (per-tool) | 每次 agent run 触发或批级（per-run/batch）/ Fires per agent run or batch-level |
| Hook 参数含 tool 上下文 / Hook params include tool context | `tool_name`, `tool_args`, `tool_call_id` 可用 / Available | 仅 run 级别上下文 / Run-level context only |
| 三元决策覆盖 / Ternary decision coverage | Allow + Deny + Modify 三者可达（可通过组合系统）/ All three reachable (even via combined systems) | 至少一项缺失或需侵入式改造 / At least one missing or requiring invasive modification |
| Adapter 工作量 / Adapter effort | 1-4 天 / 1-4 days | 3-7 天 / 3-7 days |

**源码验证实例 / Source-verified examples**:
- **OpenCode (T1)**：`tool.execute.before/after` = per-tool hook + Permission 系统 = Allow/Deny。两系统组合覆盖三元决策，adapter 仅需桥接。
  `tool.execute.before/after` = per-tool hook + Permission system = Allow/Deny. Combined systems cover ternary decision; adapter only needs bridging.
- **Agno 2.0 (T2)**：`pre_hooks/post_hooks` = agent-run 级别，签名无 `tool_name`。需在 `_run.py` tool-call loop 中侵入式注入拦截点（2-3 天）。
  `pre_hooks/post_hooks` = agent-run level, signature lacks `tool_name`. Requires invasive injection of interception points in `_run.py` tool-call loop (2-3 days).

---

## T3 — 传统 AI Framework / Legacy AI Framework

### 定义 / Definition

根本没有"agent runtime"概念，是 Python/TS **库**。Agent 抽象是图节点（LangGraph）/ crew（CrewAI）/ conversation（AutoGen）/ decorator function（Pydantic AI）。通常没有 MCP，hook 语义错位（图节点级 / conversation 级 / 无 hook）。

Fundamentally lacks the concept of an "agent runtime" — it is a Python/TS **library**. The agent abstraction is a graph node (LangGraph) / crew (CrewAI) / conversation (AutoGen) / decorator function (Pydantic AI). Typically no MCP; hook semantics are misaligned (graph-node-level / conversation-level / absent).

**Adapter 厚度 / Adapter Thickness**：1-3 周（造 per-tool 拦截 + MCP 适配 + skill loader + 会话管理）/ 1-3 weeks (build per-tool interception + MCP adaptation + skill loader + session management)

### 判别细节 / Classification Detail

这类框架的"agent"概念和 EAASP 的 "session + tool + hook + skill" 模型**语义错位**——强行适配会在 16 方法 gRPC 契约上产生大量 impedance mismatch。T3 候选值得做 L1 的前提是**给已有该框架资产的团队一条接入路径**，而不是"选最佳 L1"。

The "agent" concept in these frameworks has a **semantic mismatch** with the EAASP "session + tool + hook + skill" model — forcing adaptation generates significant impedance mismatch across the 16-method gRPC contract. A T3 candidate is worth building as an L1 only to **provide an onboarding path for teams with existing framework assets**, not to "select the best L1".

### 代表项目 / Representative Projects

| 项目 / Project | 语言 / Language | 适配难度 / Adaptation Difficulty | 备注 / Notes |
|---|---|---|---|
| **Pydantic AI** | Python | 中 / Medium | T3 里 hook 最干净（decorator function filter）+ 原生 MCP。已有 Pydantic 生态团队首选 / Cleanest hooks in T3 (decorator function filter) + native MCP. Preferred for teams with Pydantic ecosystem |
| **Semantic Kernel** | .NET/Python | 中 / Medium | Function Invocation Filter + 2025-03 原生 MCP。.NET 团队独立入口 / Function Invocation Filter + native MCP since 2025-03. Independent entry for .NET teams |
| **LangGraph** | Python | 高 / High | LangGraph Platform GA + 2025-07 MCP。但图节点级 hook 语义错位 / LangGraph Platform GA + MCP since 2025-07. But graph-node-level hook semantic mismatch |
| CrewAI | Python | 高 / High | workflow 静态，hook 语义错位 / Static workflow, hook semantic mismatch |
| AutoGen | Python | 高 / High | conversation 级，hook 缺失 / Conversation-level, hooks absent |
| MAF (Microsoft Agent Framework) | .NET/Python | 未评估 / Not evaluated | 原文点名 / Named in original |
| Google ADK | Python | 不推荐 / Not recommended | `before_tool_callback` 存在但 live path bug / Exists but live path bug |
| LlamaIndex | Python | 不推荐 / Not recommended | 定位 RAG 不是 agent runtime / Positioned as RAG, not agent runtime |

**EAASP 当前状态 / Current Status**：T3 未交付。本轮评估（R1/R2）未涉及 T3 候选的源码验证。 / T3 not delivered. No T3 candidate source-code verification was conducted in this round (R1/R2).

---

## 分层总结矩阵 / Tier Summary Matrix

| Tier | Adapter 厚度 / Adapter Thickness | 三件套要求 / Triad Requirement | 已交付 / Delivered | 源码验证 / Source-verified |
|---|---|---|---|---|
| **T0** | 协议层开发 / Protocol layer development | N/A（分离架构为主特征）/ N/A (separation architecture is primary feature) | 无 / None | 无 / None |
| **T1** | 1-4 天 / 1-4 days | MCP+Skills+Hooks 三件套完整对齐 / Complete triad alignment | claude-code-runtime, hermes-runtime (Python) | OpenCode (TS) = T1 |
| **T2** | 3-7 天 / 3-7 days | 至少一项不完整 / At least one item incomplete | 无 / None | Agno 2.0 (Python) = T2 |
| **T3** | 1-3 周 / 1-3 weeks | 语义错位，需大量适配 / Semantic mismatch, heavy adaptation | 无 / None | 无 / None |

---

## 变更记录 / Change Log

| 日期 / Date | 变更 / Change | 来源 / Source |
|---|---|---|
| 2026-04-12 | 初版：基于 `L1_RUNTIME_STRATEGY.md` §二 的 T0-T3 定义，翻译为中英文对照格式 / Initial: bilingual translation of T0-T3 definitions from `L1_RUNTIME_STRATEGY.md` §2 | `L1_RUNTIME_STRATEGY.md` |
| 2026-04-12 | R1 OpenCode 源码验证 — 确认 **T1**（per-tool hook + Permission 组合覆盖三元决策，adapter 3-4d）/ R1 OpenCode source verification — confirmed **T1** (per-tool hook + Permission combined ternary decision, adapter 3-4d) | `L1_RUNTIME_R1_OPENCODE_EVAL.md` |
| 2026-04-12 | R2 Agno 2.0 源码验证 — 确认维持 **T2**（MCP+Skills 超标，Hooks 为 agent-run 级别非 per-tool，adapter 5-7d）/ R2 Agno 2.0 source verification — confirmed **T2** (MCP+Skills exceeding, Hooks at agent-run level not per-tool, adapter 5-7d) | `L1_RUNTIME_R2_AGNO_EVAL.md` |
