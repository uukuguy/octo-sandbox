# EAASP v2.0 演化路径（长期记忆）

> **文档性质**：跨 Phase 的长期规划与决策备忘。每次 brainstorming 的结论追加到此文档，
> 原则上**只增量不推翻**已有决策；如需推翻，显式标注 SUPERSEDED 并附理由。
>
> **权威规范**：`docs/design/EAASP/EAASP-Design-Specification-v2.0.docx`
> （规范全文导出的 markdown：`/tmp/eaasp_v2_spec.md`，2944 行）
>
> **产品形态参考**：`docs/design/EAASP/EAASP_v2_0_Platform_Product_Forms.docx`
> **前期资产基线**：`docs/design/EAASP/GRID_CURRENT_STATE_2026-04-10.md`
>
> **创建日期**：2026-04-11
> **维护者**：Jiangwen Su + Claude

---

## 一、核心原则（不可推翻）

这些原则在任何 Phase 都不应被违反。违反需要更新本章节并记录理由。

| # | 原则 | 来源 |
|---|---|---|
| **P1** | **干净的 v2.0，无向后兼容负担**。前期工作按"是否符合 v2.0 语义"评估，不做 shim/双轨/兼容层 | 2026-04-11 brainstorming D1-D3 |
| **P2** | 终极目标是完整实现 v2.0 设计规范（5 层 + 3 管道 + 4 元范式 + 7 阶段演化） | 2026-04-11 brainstorming D2 |
| **P3** | **每个 Phase 必须人工可执行可观测可验证**。MVP 必须做精做强，skill 必须是真实能力而非占位 demo。不允许仅靠脚本断言就宣布 Phase 完成 | 2026-04-11 D4-D6, 2026-04-12 修正 |
| **P4** | 平台提供治理/知识/策略，运行时提供执行智能 — v2.0 §2.4 master boundary principle | v2.0 spec §2.4 |
| **P5** | **Governance is permanent; governance configurations are not** — hook 协议、审批链、审计管道是稳定架构元素；具体 hook/阈值/策略是会随模型能力演进的"活配置" | v2.0 spec §2.4 |
| **P6** | **Extend, never rebuild** — 每一个演化阶段都是增量叠加，不返工前一阶段 | v2.0 spec §2.4 |
| **P7** | **Deny always wins** — 任何 scope 层级的 hook 只要 deny 就一票否决 | v2.0 spec §2.4, §15.9 |
| **P8** | **所有持续演进决策都必须落在本文档或其引用的文档里** — 避免每次会话重问 | 2026-04-11 brainstorming D7 |

---

## 二、架构心智模型

### 2.1 五层 + 三管道 速查图

```
┌─────────────────────────────────────────────────────────┐
│ L5 Cowork       Event Room · Four Cards · Admin Console │  面向人
├─────────────────────────────────────────────────────────┤
│ L4 Orchestration  Event Engine · Session Orchestrator · │  编排
│                   A2A Router · Session Event Stream     │
├─────────────────────────────────────────────────────────┤
│ L3 Governance   Policy Engine · Approval Gates ·        │  治理
│                 Audit · MCP Registry · Evidence Chain   │
├─────────────────────────────────────────────────────────┤
│ L2 Assets       Skill Repo · Memory Engine ·            │  资产
│                 MCP Orchestrator · Ontology             │
├─────────────────────────────────────────────────────────┤
│ L1 Execution    T0 Managed Harness · T1 Harness ·       │  执行
│                 T2 Aligned · T3 Framework               │
└─────────────────────────────────────────────────────────┘
  ▲  Pipeline A: Hook（14 lifecycle events, deny-always-wins）
  ▲  Pipeline B: Data-flow（P1-P5 SessionPayload 下行 / 4 类上行）
  ▲  Pipeline C: Session-control（Event Room → Event → Session 三级）
```

### 2.2 四元范式 → 产品形态映射

| 元范式 | 源自 | 核心能力 | 产品形态 |
|---|---|---|---|
| I **Guarantee Engine** | Hook Pipeline | 14 lifecycle events, 4 handler types, 5-stage approval chain, OPA/Rego | Workflow-Native |
| II **Intelligence Flow** | Data-flow Pipeline | 结构化 SessionPayload P1-P5, evidence anchors, retrospective cycle | 所有形态的基础 |
| III **Collaboration Orchestrator** | Session-control Pipeline | Event Room 长寿, Session 短寿, A2A ReviewSet, 跨 session 通过 Memory | Event-Native + 多智能体评审 |
| IV **Capability Factory** | Agent Factory（v2.0 新增） | Skill Creation Pipeline, Skill Extraction, Continuous Quality Loop, Assumption Health | Factory-Native |

### 2.3 运行时分层（T0-T3）

| Tier | 特征 | Adapter 厚度 | 凭证模式 | 代表 |
|---|---|---|---|---|
| **T0** | Managed Harness（外部基础设施） | 中 | credential proxy | Anthropic Managed Agents |
| **T1** | 原生 hook + MCP + skill | 薄 | 临时预置 token | Claude Code, Claude Agent SDK, Octo, **Hermes** |
| **T2** | 原生 MCP, 无原生 hook | 中 | hook-bridge 注入 | Aider, Goose, Roo Code CLI, Cline CLI, OpenCode |
| **T3** | 框架型 | 厚 | hook-bridge 注入 | LangGraph, CrewAI, Pydantic AI, MAF, Google ADK |

### 2.4 Runtime Interface Contract（关键决策已锁定）

- **12 MUST + 4 Optional = 16 方法**（v2.0 §8.5 明确）
- Certifier **只验 12 MUST core**
- proto/trait/代码实现 **全部 16 方法**
- `emitEvent()` 是否新增为 MUST — **ADR 待定**（见第五章 Deferred ADR 表）

---

## 三、演化路径：8 Phase 对齐 5 圈能力

本项目按 v2.0 规范第 19 节的结构演化，每个 Phase 对应一个**能力圈层**的增量。

### 铁律：每个 Phase 必须人工可执行可观测可验证

> **CRITICAL — 本节是全局约束，所有 Phase 都必须遵守。**
>
> 每个 Phase 的出口标准**必须包含人工可执行、可观测、可验证的应用级演示**。
> 不允许仅靠自动化脚本断言就宣布 Phase 完成。
>
> - **可执行**：用户能在终端/UI 上发起操作，系统产生真实响应
> - **可观测**：用户能看到系统在做什么（流式输出、日志、状态变化）
> - **可验证**：用户能通过操作结果判断系统是否正确工作
>
> 自动化测试是必要补充，但**不能替代**人工可执行的验证。
> Phase 0 的教训：15 条脚本断言全部通过，但没有任何人工可执行的 agent 体验。这是一个 Infrastructure Foundation，不是 MVP。

### 圈层模型

- **圈 1**：契约与治理核心（Contract-Governance Core）
- **圈 2**：圈 1 + 资产与记忆基础（Assets-Memory Base）
- **圈 3**：圈 2 + 事件与会话流水（Event-Session Stream）
- **圈 4**：圈 3 + 演化与可观测（Evolution & Observability）
- **圈 5**：圈 4 + 领域验证（Domain Validation）+ L5 全功能

### 3.1 Phase 对齐表

| Phase | 名称 | 圈层 | 关键交付 | 人工验证标准 | 资产状态 |
|---|---|---|---|---|---|
| **Phase 0** | Infrastructure Foundation | **圈 2** | 接口契约 + 5 层服务骨架 + 15 断言脚本验证 | ⚠️ 仅脚本验证（历史遗留，已标注为 Foundation 而非 MVP） | 🟢 Completed (2026-04-12) |
| **Phase 0.5** | **MVP — 全层贯通** | **圈 2+** | L4→L1 真 gRPC + LLM agent 执行 + tool 调用 + memory 读写 + hook 触发 + 流式输出 | 用户 `eaasp-cli session send` → 看到 agent 调 tool、写 memory、流式输出结果 | 🟢 Completed (2026-04-13) |
| **Phase 0.75** | **L2 MCP 编排与部署架构** | **圈 2 补强** | L2 MCP Orchestrator 职责定义 + MCP transport 统一策略 + 容器化 runtime MCP 发现机制 + claude-code-runtime MCP 通路修复 | 三个 runtime 的 MCP tool call 全部通过真实 MCP server（非 workaround）；`eaasp-cli session send` 在 claude-code-runtime 上调用 mock-scada 成功 | 🟢 Completed (2026-04-13) |
| **Phase 1** | Event-driven foundation | 圈 3 | L4 Event Engine + Session Event Stream + L4 hooks | 用户能在 CLI 观察事件流实时更新；event 从 ingest 到 clustering 的全过程可查 | ⏸ |
| **Phase 2** | Memory and evidence | 圈 2 增强 + 圈 3 | Memory 完整三层 + Skill extraction + PreCompact | 用户能搜索/浏览 semantic 检索结果；skill extraction 产出可人工审阅 | ⏸ |
| **Phase 3** | Approval and verification | 圈 4 | 审批链 + Verifier + OPA + Sandbox Tiers | 用户能触发审批流程并看到 approve/deny 决策路径；sandbox 隔离可演示 | ⏸ |
| **Phase 4** | Multi-agent collaboration | 圈 4 | A2A Router + ReviewSet + T0 Harness | 用户能发起多 agent 评审任务并观察协作过程和汇总结果 | ⏸ |
| **Phase 5** | Complete collaboration space | 圈 5 L5 | 四卡 UI + IM bot + 回溯闭环 | 用户能在 Web UI 操作四卡界面；IM bot 可对话；回溯链可追踪 | ⏸ |
| **Phase 6** | Ecosystem expansion | 圈 5+ | Marketplace + 多租户 + SDK | 第三方开发者能用 SDK 创建/提交 skill 并在 marketplace 上架 | ⏸ |

### 3.2 Phase 0（Infrastructure Foundation）产出说明

> **名称修正**（2026-04-12）：Phase 0 原名 "MVP Validation"，已修正为 "Infrastructure Foundation"。
> 原因：验收全部依赖自动化脚本，没有人工可执行的 agent 体验，不符合 MVP 定义。
> Phase 0 的价值是验证了 5 层服务骨架、16 方法 Runtime Interface Contract、REST/gRPC 连通性。

Phase 0 的 15 条断言由 `scripts/verify-v2-mvp.sh` + `verify-v2-mvp.py` 执行，**不涉及 LLM 调用、不涉及真实 agent loop、不涉及用户交互**。脚本在 L4→L1 的空洞处直接 POST L2/L3 REST 以满足断言（ADR-V2-004 4b-lite 模式）。

Phase 0 故意**不做** Event Engine，是为了避免被以下三个未决 ADR 阻塞：

1. **`emitEvent()` ADR**（contract 方法 vs hook-bridge 副作用 vs 平台拦截器）
2. **Session Event Stream 后端选型**（Kafka vs NATS JetStream vs S3 append-only）
3. **Event clustering strategy**（需要电网 topology ontology 作为输入）

### 3.3 Phase 0.5（MVP）—— 人工可执行全层通路

Phase 0.5 是从 "Infrastructure Foundation" 到 "真正 MVP" 的最短路径。不需要 Event Engine，不需要解 ADR-V2-001/002/003，但必须关闭以下缺口使 agent 能真正运行：

| # | 缺口 | Deferred ID | 说明 |
|---|------|-------------|------|
| 1 | L4→L1 真 gRPC 调用 | D54, D27 | L4 session_orchestrator 真调 L1 Initialize / Send |
| 2 | LLM provider 配置传递 | — | L4 告诉 L1 使用哪个 provider/model |
| 3 | MCP tool 在 session 内连接 | D47 (server done) | L1 runtime connectMCP 到 mock-scada |
| 4 | Hook 真实执行 | D53, D50 | scoped-hook executor 在 Pre/Post/Stop 边界触发 |
| 5 | 流式输出回传 | — | L1 → L4 → CLI 实时显示 agent 输出 |

**人工验收标准**：

```bash
make dev-eaasp                    # 启动所有服务
eaasp-cli skill submit + promote  # 提交并推进 skill
eaasp-cli policy deploy           # 部署 managed-settings
eaasp-cli session create --skill threshold-calibration --runtime grid-runtime
eaasp-cli session send "校准 Transformer-001 的温度阈值"
# → 用户看到 agent 流式输出：调用 mock-scada 读数据 → 分析 → 写入 evidence + memory
# → hook 在 tool call 前后真实触发
# → 可用 eaasp-cli memory search 查到写入的记忆

eaasp-cli session create --runtime claude-code-runtime
eaasp-cli session send "再校准一次 Transformer-001"
# → 第二次 session 引用了上次的记忆 — 核心价值被证明
```

### 3.5 Phase 0.75 —— 平台级统一 Runtime MCP 行为

Phase 0.5 MVP 验证暴露了 MCP 通路的结构性问题：三个 runtime 虽然都通过了 MCP tool call 验收，但各自用不同的 workaround（env var / subprocess / McpBridge），**L2 和 L4 完全不参与 MCP 编排**。这不是平台级行为——L1 Runtime 不应自己找 MCP server。

**目标**：L2 管理 → L4 下发 → L1 被动接收。消除所有 runtime 的 MCP workaround。

#### 已锁定决策（2026-04-13 Brainstorming）

| ID | 决策 | 理由 |
|---|------|------|
| **D69** | L2 MCP Orchestrator Phase 0.75 只做**注册表 + 查询 API**，进程管理留 Phase 1 | dev 环境用 `dev-eaasp.sh` 手动启动够用，生产环境要 container orchestrator |
| **D70** | ConnectMCP 三 runtime 统一实现 **stdio / SSE / streamable-http** 三种 transport | proto 已定义 MCPServerConfig.transport 字段 |
| **D71** | claude-code-runtime 接受 **SDK 自管 MCP 连接**，只要配置来源统一为 L4 下发即可 | SDK 内部 MCP 实现成熟（Anthropic 官方），重写无必要 |
| **D72** | 验收标准：三 runtime MCP 配置**全部来自 L4 ConnectMCP**，**零 env var workaround** | 平台级统一行为的最低要求 |

#### 目标架构

```
L2 MCP Orchestrator (注册表 + 查询 API)
  ├── GET /v1/mcp/servers → server 列表 + transport + endpoint
  └── MCP server 分两类：
      A 类：L2 独立管理的服务进程（memory engine、外部系统）
      B 类：可随 runtime/skill 伴生的工具

L4 Session Create (编排)
  ├── 读取 skill dependencies 中的 mcp:* 依赖
  ├── 查询 L2 → 获取 server 连接信息
  └── 调用 L1 ConnectMCP RPC → 下发 server 列表

L1 Runtime (三 runtime 统一行为)
  ├── ConnectMCP 收到 MCPServerConfig 列表
  ├── 按 transport 类型建立连接
  └── 注册 MCP tools → agent 可用
```

#### 设计问题

| # | 问题 | Phase 0.75 范围 |
|---|------|----------------|
| 1 | L2 MCP 分类管理 | A/B 分类定义 + 注册表 schema + 查询 API |
| 2 | MCP transport 统一 | 三 runtime ConnectMCP 真实实现（stdio/SSE/HTTP） |
| 3 | L4 编排流程 | session create → 查询 L2 → ConnectMCP 下发 |
| 4 | L2 Memory MCP transport | Memory Engine 提供 SSE MCP transport（D67） |
| 5 | 移除 workaround | 三 runtime 移除所有 env var / subprocess 直连 |

**关联 Deferred**: D67, D68（本 Phase 关闭）；D62-D65（Phase 1）

**出口标准**：

```bash
make dev-eaasp                    # L2 MCP Orchestrator 启动
eaasp-cli mcp list                # → 显示已注册的 MCP servers

# 三 runtime 均通过 L4 ConnectMCP 获取 MCP 配置，成功调用 mock-scada
eaasp-cli session create --skill threshold-calibration --runtime grid-runtime
eaasp-cli session send "校准 Transformer-001"

eaasp-cli session create --skill threshold-calibration --runtime claude-code-runtime
eaasp-cli session send "校准 Transformer-001"

eaasp-cli session create --skill threshold-calibration --runtime hermes-runtime
eaasp-cli session send "校准 Transformer-001"
```

### 3.6 Phase 间边界原则

- Phase 0.5 → Phase 0.75：系统可真实运行 agent，但 MCP 通路依赖各 runtime 各自实现的 workaround。Phase 0.75 统一为 L2→L4→L1 平台级编排
- Phase 0.75 → Phase 1：MCP 通路统一后，Phase 1 开始前必须解 ADR-V2-001/002/003（Event Engine 相关）
- 每个后续 Phase 的验收都必须在前一 Phase 的人工可执行基础上**增量叠加**新的可观测能力，而不是推翻重来

---

## 四、已锁定决策（Decision Registry）

本表按会话时间追加，不删除。标注为 SUPERSEDED 的条目保留历史记录。

### 2026-04-11 Brainstorming（本次）

| ID | 决策 | 理由/上下文 |
|---|---|---|
| **D1** | 后续开发全部按 v2.0 规范，v1.8 封存 | 已完成 EAASP v2.0 设计规范（4373KB docx）和平台应用形态蓝图（723KB docx）的编写 |
| **D2** | 终极目标：实现 v2.0 设计规范全部内容 | 5 层 + 3 管道 + 4 元范式 + 7 阶段演化 |
| **D3** | 前期工作向 v2.0 对齐：可复用的保留、需返工的丢弃，**不做向后兼容** | 新项目无上线负担 |
| **D4** | ~~MVP 方向：Infrastructure MVP~~ **SUPERSEDED (2026-04-12)**：Phase 0 修正为 "Infrastructure Foundation"，不再称为 MVP。MVP = Phase 0.5（人工可执行全层通路）。每个 Phase 出口必须包含人工可执行可观测可验证的演示 | 原理由：避免被电网领域业务细节拖慢基座。修正理由：MVP ≠ 不完整的产品，必须可运行且能提供核心价值 |
| **D5** | MVP 范围：**圈 2 = 契约治理核心 + 资产与记忆基础** | 干净的边界："数据能存下来，能被下一个 session 捞出来继续用" |
| **D6** | MVP 的验证 skill：**阈值校准助手**（跨 session memory 累加示例） | 一个 skill 同时验证 contract / hook / skill frontmatter / evidence anchor / memory file / context assembly / cross-session |
| **D7** | 本次结论必须持久化：EVOLUTION_PATH + MVP_SCOPE + Phase 0 Plan + checkpoint | 跨会话复用，避免每次重问 |
| **D8** | L5 在 MVP 里用 **`eaasp-cli-v2`**（Python typer）作为 L5 endpoint 模拟器 | v2.0 §4.4 把 CLI 列为合法 L5 form factor |
| **D9** | 新开 `docs/design/EAASP/` 目录，v1.7/v1.8 设计文档归档到 `docs/design/Grid/archive/v1.8/`；GRID_* 文档保留在 `Grid/` 作为电网领域产品文档 | 保持 EAASP/ 目录下都是**有效**文档 |
| **D10** | 前期资产评估结果：3 KEEP + 5 REFACTOR + 3 SCRAP + ARCHIVE 若干（详见 `EAASP_v2_0_MVP_SCOPE.md` §3） | 见 MVP_SCOPE 文档 |
| **D11** | 新的 v2 proto 路径：`proto/eaasp/runtime/v2/runtime.proto`；旧 v1 proto 归档删除 | v1 到 v2 差距过大，无复用价值 |
| **D12** | 圈 2 **不做** Event Engine、**不做** Session Event Stream 真实实现（用占位 in-process append-only SQLite），**不做** L5 Web UI | 避免被未决 ADR 阻塞 |

### 2026-04-12 Brainstorming — L1 Runtime 研究 + 全局修正

| ID | 决策 | 理由/上下文 |
|---|---|---|
| **D13** | **L1 Runtime Pool 是生态覆盖，不是选最佳**。每增加一个 tier 代表就降低一类团队的接入门槛。L1 Pool 扩充是独立并行工作线，每个新 Runtime 通过 certifier 后配置加入 MVP 验证矩阵 | L1_RUNTIME_STRATEGY.md 结论 1 |
| **D14** | **claude-code-runtime = Claude Agent SDK 的 L1 包装，hermes-runtime = Hermes 的 L1 包装**。这两个已交付的 T1 实例不是"临时方案"，不需要被替换。后续是**扩充** Pool 不是替换 | L1_RUNTIME_STRATEGY.md 结论 2 |
| **D15** | **治理框架（Microsoft AGT 等）是 L3 工作线**，不是 L1 候选。作为 L3 HookBridge 可替换后端独立评估 | L1_RUNTIME_STRATEGY.md 结论 3 |
| **D16** | **Grid 是 L1 参考实现基线，不是候选**。CCB 是最值得新增的第三个 L1 实例（平行角色），继承 Anthropic Claude Code skill 生态。Grid 有 4 个真实短板（MCP 2 transport / skill 3 来源 / 无批级 hook / 无 Agent hook transport）可从 CCB/Nanobot 借鉴 | L1_RUNTIME_CANDIDATE_ANALYSIS.md §11 |
| **D17** | **Phase 0.5 MVP 三轨验证**：grid-runtime (Rust) + claude-code-runtime (Python) + hermes-runtime (Python) 全部打通 L4→L1 真 gRPC。后续需要设计 Runtime LLM Providers 的**平台级配置**（L4 管理、L1 消费） | Phase 0.5 规划讨论 |
| **D20** | **L1 Runtime LLM Provider 配置铁律**：(1) **所有 LLM API Key 在 `.env` 文件中**，通过 OpenRouter 统一接入，不要再问 key 在哪里。(2) **配置不存在就报错退出，绝不 fallback 猜测**——不跨 provider fallback（如 OPENAI→ANTHROPIC），不猜默认值。(3) 每个 runtime 读**自己约定的环境变量**：grid-runtime 读 `LLM_PROVIDER` + `OPENAI_API_KEY` + `OPENAI_BASE_URL` + `LLM_MODEL`；claude-code-runtime 读 `ANTHROPIC_API_KEY`；hermes-runtime 读 `HERMES_API_KEY`→`OPENROUTER_API_KEY`。(4) `dev-eaasp.sh` 在启动前 source `.env` 并校验所有必要变量，缺一个就退出。**此策略适用于所有 L1 Runtime，后续新增 runtime 遵循同一规则** | 2026-04-12 用户多次强调 |
| **D18** | **ADR 治理机制按最佳实践建立**（模板 + 状态机 + ID 规则 + 校验）。运行成熟后转为 Claude Code Skill（`/adr` 命令），时机到时提醒用户 make skill | ADR 管理现状分析 |
| **D19** | **T0-T3 定义需更新**，包括 EVOLUTION_PATH §2.3 + v2.0 设计规范对应章节。先产出中英文对照修正附录，供合入 spec docx。L1 Runtime Pool 研究出结论后增补各 tier 的实例说明 | L1_RUNTIME_CANDIDATE_ANALYSIS.md §13 校正 |

### 2026-04-13 Brainstorming — Phase 0.75 平台级 MCP 统一

| ID | 决策 | 理由/上下文 |
|---|---|---|
| **D69** | L2 MCP Orchestrator Phase 0.75 只做**注册表 + 查询 API**，进程管理留 Phase 1 | dev 环境用 `dev-eaasp.sh` 手动启动够用，生产环境要 container orchestrator，Phase 0.75 做进程管理意义不大 |
| **D70** | ConnectMCP 三 runtime 统一实现 **stdio / SSE / streamable-http** 三种 transport | proto 已定义 MCPServerConfig.transport 字段，三 runtime 必须按统一契约处理 |
| **D71** | claude-code-runtime 接受 **SDK 自管 MCP 连接**，只要配置来源统一为 L4 ConnectMCP 下发即可 | SDK 内部 MCP 实现成熟（Anthropic 官方），重写无必要。配置来源统一 = 平台级统一 |
| **D72** | 验收标准：三 runtime MCP 配置**全部来自 L4 ConnectMCP**，**零 env var workaround** | Phase 0.5 的 env var workaround 不是平台级行为，L1 不应自己找 MCP server |

---

## 五、ADR 注册表

### ADR 治理规则

1. **模板**：每个 ADR 使用 `docs/design/EAASP/adrs/ADR-TEMPLATE.md` 模板，包含 Status / Date / Phase / Context / Decision / Consequences / Related 结构化字段
2. **状态机**：`Proposed → Accepted → Deprecated / Superseded`，状态变更必须在本表和 ADR 文件中同步更新
3. **ID 规则**：`ADR-V2-NNN`，编号单调递增，**不复用**。已有 ADR-V2-004 ID 复用问题修复：(a) 已落地的 4b-lite 决策保留 004；(b) Memory semantic 检索重新分配为 ADR-V2-015
4. **落地位置**：`docs/design/EAASP/adrs/ADR-V2-NNN-<slug>.md`
5. **校验**：每个 ADR 记录"影响的模块/文件"清单。代码变更涉及已有 ADR 影响范围时，reviewer 需检查是否合规
6. **Skill 转化**：ADR 机制运行成熟（≥10 个 ADR 落地 + 模板稳定）后，转为 Claude Code Skill（`/adr` 命令）

### ADR 索引（按解决 Phase 排序）

| ADR ID | 主题 | 状态 | 目标 Phase | 阻塞什么 |
|---|---|---|---|---|
| **ADR-V2-001** | `emitEvent()` 是新 MUST 方法 vs hook-bridge 副作用 vs 平台拦截器 | Proposed | Phase 1 | L1→Session Event Stream 写入接口 |
| **ADR-V2-002** | Session Event Stream 后端选型（Kafka / NATS JetStream / S3 append-only） | Proposed | Phase 1 | L4 持久化平面 |
| **ADR-V2-003** | Event clustering strategies 的插件化接口（4 handler types 支持） | Proposed | Phase 1 | L4 Event Engine pipeline |
| **ADR-V2-004** | L4→L1 真 gRPC 绑定的 4b-lite scope 决策 | **Accepted** | Phase 0 ✅ | [`adrs/ADR-V2-004-l4-to-l1-real-grpc-binding.md`](adrs/ADR-V2-004-l4-to-l1-real-grpc-binding.md) |
| **ADR-V2-005** | OPA/Rego 作为 command hook backend 的部署拓扑（sidecar vs shared cluster OPA） | Proposed | Phase 3 | L3 Policy Engine |
| **ADR-V2-006** | Sandbox Isolation Tier 实现（gVisor 优先 vs Kata 优先 vs Firecracker） | Proposed | Phase 3 | L1 execution zone |
| **ADR-V2-007** | A2A ReviewSet aggregation engine 的冲突检测算法 | Proposed | Phase 4 | L4 A2A Router |
| **ADR-V2-008** | L5 Web UI 的技术选型（复用 grid-workbench 的 web/ vs 全新 web-eaasp/） | Proposed | Phase 5 | L5 Cowork 实现 |
| **ADR-V2-009** | 多租户的组织层次与 policy scope 的数据模型 | Proposed | Phase 6 | Multi-tenancy |
| **ADR-V2-010** | Runtime 认证流水线（certification pipeline）的 blind-box 质量测试设计 | Proposed | Phase 6 | 生态扩展 |
| **ADR-V2-011** | PreToolBatch 批级 hook 契约（单 tool vs 整批双轨） | Proposed | Phase 0.5/1 | HookBridge 契约扩展 |
| **ADR-V2-012** | L3 治理后端选型（Microsoft AGT vs OPA vs cedar-agent） | Proposed | Phase 1 | L3 Policy Engine 真实后端 |
| **ADR-V2-013** | L1 Runtime Pool 扩充策略 + 贡献者接入规范 | Proposed | Phase 1 | 生态开放 |
| **ADR-V2-014** | T0-T3 分层判据正式定义（含代表项目 + adapter 厚度指南） | Proposed | 立即 | L1 选型依据 |
| **ADR-V2-015** | L2 Memory Engine semantic 检索选型（pgvector / 独立向量库 / HNSW in-process）— 原 ADR-V2-004(b) 拆出 | Proposed | Phase 2 | hybrid retrieval index |

---

## 5.1 L1 Runtime 后续研究待办

> 来源：`L1_RUNTIME_CANDIDATE_ANALYSIS.md` §12.8 未决问题 + `L1_RUNTIME_STRATEGY.md` §5 工作线。
> 每项完成后在此表标注 ✅ 并附结论文档链接。

| # | 待办 | 内容 | 启动时机 | 状态 |
|---|------|------|---------|------|
| R1 | OpenCode 源码评估 | 本地 clone，确认 **T1** 归属（per-tool `tool.execute.before/after` hook + 独立 Permission Allow/Deny/Ask + MCP 3 transport + SKILL.md frontmatter）。Adapter 3-4 天。详见 `L1_RUNTIME_R1_OPENCODE_EVAL.md` | Phase 0.5 期间（并行） | ✅ 2026-04-12 |
| R2 | Agno 2.0 源码评估 | 本地 clone，确认维持 **T2**（MCP+Skills 完全达标，但 Hooks 是 agent-run 级别非 per-tool — `pre_hooks/post_hooks` 在 `agent.run()` 入口/出口各一次，无 `tool_name` 参数）。Adapter 5-7 天。详见 `L1_RUNTIME_R2_AGNO_EVAL.md` | Phase 0.5 期间（并行） | ✅ 2026-04-12 |
| R3 | Microsoft AGT 深读 | `agentmesh` + `agentmesh-mcp` Rust crate 确认存在（~8190 行），可嵌入 `grid-hook-bridge`。6 种 MCP 安全扫描（ToolPoisoning/RugPull/CrossServerAttack 等）。策略模型支持原生 YAML + OPA/Rego + Cedar 三后端。推荐方案 A：L3 Python 层引入 AGT PolicyEvaluator（2-3 人天），Phase 2+ 渐进 Rust 嵌入。详见 `R3_AGT_EVALUATION_MEMO.md` + `L1_RUNTIME_R3_AGT_EVAL_DETAIL.md` | Phase 0.5 期间 | ✅ 2026-04-12 |
| R4 | HexAgent Computer 协议评估 | 确认 **T0** 最佳开源实证。Computer 协议 6 方法（is_running/start/run/upload/download/stop），三种实现（Native/Lima VM/E2B Cloud）。Lima VM 实现 harness-tools 物理分离。Adapter 5-8 天。详见 `L1_RUNTIME_R4_HEXAGENT_EVAL.md` | Phase 0.5 期间 | ✅ 2026-04-12 |
| R5 | T0-T3 中英文修正附录 | 中英文对照文档完成，含 R1(OpenCode T1) + R2(Agno T2) 源码验证证据，T1/T2 分水岭明确定义（per-tool hook 粒度），4 tier 横向对比矩阵。详见 `L1_RUNTIME_T0_T3_TIER_APPENDIX.md` | R1+R2 完成后 | ✅ 2026-04-12 |
| R6 | 各 tier 实例说明文档 | 每个 tier 首选候选的 2-3 页架构摘要（贡献者入门包） | Phase 1 早期 | ⏸ |
| R7 | L1 贡献者指南 | `L1_RUNTIME_CONTRIBUTOR_GUIDE.md`：如何把 framework 包装成 L1 | R5+R6 完成后 | ⏸ |
| R8 | Wippy / datalayer 二次深挖 | 证据不足的两个 T2 候选，视 R1+R2 结果决定是否值得 | Phase 1（低优先级） | ⏸ |
| R9 | Google ADK / Agno bug 跟踪 | Issue #4704 / #5568 修复节奏持续观察 | 持续 | ⏸ |

**时机选择原则**：
- R1+R2 与 Phase 0.5 并行，不阻塞 MVP 开发，但结果影响 ADR-V2-013 (L1 Pool 策略) 和 ADR-V2-014 (T0-T3 定义)
- R3 不阻塞 MVP，但 Phase 1 Event Engine 设计可能参考 AGT 策略模式
- R5-R7 需要 R1+R2 结论，自然落在 Phase 1 早期

### 5.2 ADR 治理机制演进待办

> ADR 治理机制（D18）需要随实践持续演进，最终转化为 Claude Code Skill。

| # | 待办 | 内容 | 启动时机 | 状态 |
|---|------|------|---------|------|
| A1 | ADR 模板试运行 | 用 ADR-V2-014 (T0-T3 定义) 作为首个按模板撰写的 ADR，验证模板可用性 | Phase 0.5 期间 | ⏸ |
| A2 | ADR 编写流程沉淀 | 3-5 个 ADR 落地后，回顾模板和流程，修正不合理之处 | ≥5 个 ADR 落地后 | ⏸ |
| A3 | ADR 校验集成 | 在 reviewer 流程中增加 ADR 合规检查（变更涉及 ADR 影响范围时提醒） | A2 完成后 | ⏸ |
| A4 | ADR Skill 落地 | 转为 Claude Code Skill（`/adr create` / `/adr list` / `/adr check`），提醒用户 make skill | ≥10 个 ADR 落地 + 模板稳定 | ⏸ |

**演进路径**：模板试运行 → 实践验证 → 流程沉淀 → 校验集成 → Skill 自动化

---

## 六、非目标（v2.0 明确不做）

v2.0 规范第 20 章 "Design Anti-Patterns" 列出 22 个反模式。本文档摘取对实施有强约束的几条：

| # | 反模式 | 实施含义 |
|---|---|---|
| 4 | Governance without managed hooks | 不能用 prompt 来强制策略，必须走 hook |
| 13 | Using LLMs for deterministic verification | Verifier 必须是规则引擎/模拟工具，不能是 LLM |
| 14 | Agent overreach | Agent 产出 draft，不直接执行高风险操作 |
| 15 | Treating memory as a vector database | Memory file 是人可读的结构化文本，hybrid 检索是二级索引 |
| 17 | Credentials inside sandboxes for bridged runtimes | T2/T3 必须走 hook-bridge 注入凭证，不直接下发 |
| 20 | Treating model-agnosticism as an architectural problem | 模型无关通过环境变量配置，不引入 MII/Gateway 层 |

---

## 七、本文档维护规则

1. 每次 brainstorming 结束前必须追加"已锁定决策"表的新条目
2. 每次 Phase 完成必须勾选第三章 "资产状态"列的状态为 🟢 Completed
3. 每次 Phase 完成前必须**人工执行**该 Phase 的验证标准（§3.1 表中"人工验证标准"列），不能仅靠脚本
4. 每次 ADR 解决必须从第五章 Deferred 表移除并附 ADR 文档链接
5. **禁止删除**已有决策条目；推翻时显式 SUPERSEDED 标注
6. 本文档超过 500 行时拆分（但不拆散"决策注册表"）

---

## 八、引用的文档

- `docs/design/EAASP/EAASP-Design-Specification-v2.0.docx` — v2.0 权威规范（authoritative）
- `docs/design/EAASP/EAASP_v2_0_Platform_Product_Forms.docx` — 产品形态蓝图
- `docs/design/EAASP/EAASP_v2_Executive_Overview.docx` — 高管摘要
- `docs/design/EAASP/GRID_CURRENT_STATE_2026-04-10.md` — 前期资产 ground-truth 审计
- `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` — 圈 2 MVP 范围细化
- `docs/plans/2026-04-11-v2-mvp-phase0-plan.md` — Phase 0 执行计划
- `docs/design/Grid/archive/v1.8/` — v1.7/v1.8 历史设计文档（参考用，不生效）
