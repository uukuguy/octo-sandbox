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
| **P3** | **MVP 必须做精做强**，skill 必须是真实能力而非占位 demo | 2026-04-11 brainstorming D4-D6 |
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

## 三、演化路径：7 Phase 对齐 5 圈能力

本项目按 v2.0 规范第 19 节的 7 Phase 结构演化，每个 Phase 对应一个**能力圈层**的增量。
"圈"是 2026-04-11 brainstorming 引入的心智模型：

- **圈 1**：契约与治理核心（Contract-Governance Core）
- **圈 2**：圈 1 + 资产与记忆基础（Assets-Memory Base）← **MVP 终点**
- **圈 3**：圈 2 + 事件与会话流水（Event-Session Stream）
- **圈 4**：圈 3 + 演化与可观测（Evolution & Observability）
- **圈 5**：圈 4 + 领域验证（Domain Validation）+ L5 全功能

### 3.1 Phase 对齐表

| Phase | v2.0 规范名称 | 圈层 | 关键交付 | 资产状态 |
|---|---|---|---|---|
| **Phase 0** | MVP Validation（Infrastructure MVP） | **圈 2** | 完整 L4→L3→L2→L1 链路 + Memory Engine + "阈值校准助手" skill 跑通 | 🟢 Completed (2026-04-12) |
| **Phase 1** | Event-driven foundation（weeks 1-6） | 圈 3 的一部分 | L4 Event Engine (ingest→dedup→cluster→state machine) + Session Event Stream + L4 hooks (EventReceived/PreSessionCreate/PostSessionEnd) | ⏸ Phase 0 完成后启动 |
| **Phase 2** | Memory and evidence（weeks 5-12） | 圈 2 增强 + 圈 3 | Memory Engine 完整三层 + Skill extraction + PreCompact 通过 event stream 可逆 | ⏸ |
| **Phase 3** | Approval and verification（weeks 10-18） | 圈 4 的一部分 | 5 阶段审批链 + Deterministic Verifier + OPA/Rego + Egress Control + Sandbox Isolation Tiers | ⏸ |
| **Phase 4** | Multi-agent collaboration（weeks 16-24） | 圈 4 | A2A Router + ReviewSet + T0 Managed Harness 作为 runtime tier | ⏸ |
| **Phase 5** | Complete collaboration space（weeks 22-30） | 圈 5 的 L5 部分 | L5 四卡渲染引擎 + IM bot + 回溯闭环 + Assumption Health Dashboard + Hook Shadow Mode | ⏸ |
| **Phase 6** | Ecosystem expansion（ongoing） | 圈 5 + | Runtime 认证流水线 + Skill marketplace + 多租户 + API SDK + Ontology Service + 渐进式策略松绑 + 分层凭证隔离 | ⏸ |

### 3.2 Phase 0 ↔ Phase 1 边界

圈 2 故意**不做** Event Engine，这是为了避免 Phase 0 被以下三个决策阻塞：

1. **`emitEvent()` ADR**（contract 方法 vs hook-bridge 副作用 vs 平台拦截器）
2. **Session Event Stream 后端选型**（Kafka vs NATS JetStream vs S3 append-only）
3. **Event clustering strategy**（需要电网 topology ontology 作为输入）

Phase 0 的 `eaasp-cli-v2` 会**手动触发** session 创建和事件对象（绕过 Event Engine），
以便跑通"记忆累加"链路。Event Engine 的真实实现推迟到 Phase 1。

---

## 四、已锁定决策（Decision Registry）

本表按会话时间追加，不删除。标注为 SUPERSEDED 的条目保留历史记录。

### 2026-04-11 Brainstorming（本次）

| ID | 决策 | 理由/上下文 |
|---|---|---|
| **D1** | 后续开发全部按 v2.0 规范，v1.8 封存 | 已完成 EAASP v2.0 设计规范（4373KB docx）和平台应用形态蓝图（723KB docx）的编写 |
| **D2** | 终极目标：实现 v2.0 设计规范全部内容 | 5 层 + 3 管道 + 4 元范式 + 7 阶段演化 |
| **D3** | 前期工作向 v2.0 对齐：可复用的保留、需返工的丢弃，**不做向后兼容** | 新项目无上线负担 |
| **D4** | MVP 方向：**Infrastructure MVP**（骨架做精做强，不追求领域产品） | 避免被电网领域业务细节拖慢基座 |
| **D5** | MVP 范围：**圈 2 = 契约治理核心 + 资产与记忆基础** | 干净的边界："数据能存下来，能被下一个 session 捞出来继续用" |
| **D6** | MVP 的验证 skill：**阈值校准助手**（跨 session memory 累加示例） | 一个 skill 同时验证 contract / hook / skill frontmatter / evidence anchor / memory file / context assembly / cross-session |
| **D7** | 本次结论必须持久化：EVOLUTION_PATH + MVP_SCOPE + Phase 0 Plan + checkpoint | 跨会话复用，避免每次重问 |
| **D8** | L5 在 MVP 里用 **`eaasp-cli-v2`**（Python typer）作为 L5 endpoint 模拟器 | v2.0 §4.4 把 CLI 列为合法 L5 form factor |
| **D9** | 新开 `docs/design/EAASP/` 目录，v1.7/v1.8 设计文档归档到 `docs/design/Grid/archive/v1.8/`；GRID_* 文档保留在 `Grid/` 作为电网领域产品文档 | 保持 EAASP/ 目录下都是**有效**文档 |
| **D10** | 前期资产评估结果：3 KEEP + 5 REFACTOR + 3 SCRAP + ARCHIVE 若干（详见 `EAASP_v2_0_MVP_SCOPE.md` §3） | 见 MVP_SCOPE 文档 |
| **D11** | 新的 v2 proto 路径：`proto/eaasp/runtime/v2/runtime.proto`；旧 v1 proto 归档删除 | v1 到 v2 差距过大，无复用价值 |
| **D12** | 圈 2 **不做** Event Engine、**不做** Session Event Stream 真实实现（用占位 in-process append-only SQLite），**不做** L5 Web UI | 避免被未决 ADR 阻塞 |

---

## 五、Deferred ADRs（按解决 Phase 排序）

以下决策在 Phase 0 MVP 里**显式不解**，各 Phase 到达时必须先解决相应 ADR。

| ADR ID | 主题 | 目标 Phase | 阻塞什么 |
|---|---|---|---|
| **ADR-V2-001** | `emitEvent()` 是新 MUST 方法 vs hook-bridge 副作用 vs 平台拦截器 | Phase 1 | L1→Session Event Stream 写入接口 |
| **ADR-V2-002** | Session Event Stream 后端选型（Kafka / NATS JetStream / S3 append-only） | Phase 1 | L4 持久化平面 |
| **ADR-V2-003** | Event clustering strategies 的插件化接口（4 handler types 支持） | Phase 1 | L4 Event Engine pipeline |
| **ADR-V2-004** | L2 Memory Engine semantic 检索选型（pgvector / 独立向量库 / HNSW in-process） | Phase 2 增强 | hybrid retrieval index |
| **ADR-V2-005** | OPA/Rego 作为 command hook backend 的部署拓扑（sidecar vs shared cluster OPA） | Phase 3 | L3 Policy Engine |
| **ADR-V2-006** | Sandbox Isolation Tier 实现（gVisor 优先 vs Kata 优先 vs Firecracker） | Phase 3 | L1 execution zone |
| **ADR-V2-007** | A2A ReviewSet aggregation engine 的冲突检测算法 | Phase 4 | L4 A2A Router |
| **ADR-V2-008** | L5 Web UI 的技术选型（复用 grid-workbench 的 web/ vs 全新 web-eaasp/） | Phase 5 | L5 Cowork 实现 |
| **ADR-V2-009** | 多租户的组织层次与 policy scope 的数据模型 | Phase 6 | Multi-tenancy |
| **ADR-V2-010** | Runtime 认证流水线（certification pipeline）的 blind-box 质量测试设计 | Phase 6 | 生态扩展 |

**规则**：每个 ADR 落地时在 `docs/design/EAASP/adrs/ADR-V2-XXX.md` 写文档，本文档只做索引。
（`adrs/` 目录在第一个 ADR 落地时再建，避免先挖空坑。）

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
3. 每次 ADR 解决必须从第五章 Deferred 表移除并附 ADR 文档链接
4. **禁止删除**已有决策条目；推翻时显式 SUPERSEDED 标注
5. 本文档超过 500 行时拆分（但不拆散"决策注册表"）

---

## 八、引用的文档

- `docs/design/EAASP/EAASP-Design-Specification-v2.0.docx` — v2.0 权威规范（authoritative）
- `docs/design/EAASP/EAASP_v2_0_Platform_Product_Forms.docx` — 产品形态蓝图
- `docs/design/EAASP/EAASP_v2_Executive_Overview.docx` — 高管摘要
- `docs/design/EAASP/GRID_CURRENT_STATE_2026-04-10.md` — 前期资产 ground-truth 审计
- `docs/design/EAASP/EAASP_v2_0_MVP_SCOPE.md` — 圈 2 MVP 范围细化
- `docs/plans/2026-04-11-v2-mvp-phase0-plan.md` — Phase 0 执行计划
- `docs/design/Grid/archive/v1.8/` — v1.7/v1.8 历史设计文档（参考用，不生效）
