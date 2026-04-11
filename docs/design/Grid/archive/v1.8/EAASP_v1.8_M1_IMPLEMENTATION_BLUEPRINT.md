# EAASP v1.8 M1 实施蓝图

> **版本**: v1.0
> **创建日期**: 2026-04-10
> **基线**: EAASP v1.8 设计规范 + Milestone 1 "Operator Edition" + 三类元范式分析
> **前置**: Phase BI 完成（hermes-runtime T2 Aligned L1 Runtime）
> **状态**: 设计确认，待实施

---

## 一、文档定位

本文档是 EAASP v1.8 Milestone 1 的**统一整体设计蓝图**，整合了以下讨论成果：

1. **产品线划分** — Grid 个人工具 vs EAASP 企业平台的明确边界
2. **应用类型矩阵** — 六种应用方向按三类元范式归类，含适用性判断
3. **技术栈分层** — Rust (L1+L2) / Python (L3+L4) / TypeScript (L5)
4. **部署架构** — 三个服务的职责与通信关系
5. **Gap 分析** — 现有 codebase 与 M1 目标的逐层差距
6. **实施路径** — 四阶段渐进式开发计划

所有架构决策均基于 EAASP v1.8 完整设计规范（`EAASP_Design_Specification_v1_8.docx`）和 Milestone 1 运营版规范（`EAASP_v1_8_Milestone_1_Operator_Edition.docx`）。

---

## 二、产品族定位

### 2.1 核心区分：Grid vs EAASP

Grid 和 EAASP 是两个独立产品，共享底层引擎，但产品形态、部署方式、治理级别完全不同。

```
Grid = 个人 AI 开发工具
  面向：单个开发者
  核心：grid-engine (AgentRuntime + AgentLoop)
  特点：无治理层，灵活快速，本地执行

EAASP = 企业自主智能体支撑平台
  面向：团队/企业（5人到数百人）
  核心：五层架构 + 三条纵向管线
  特点：强治理，事件驱动，证据链，持久记忆
```

### 2.2 产品矩阵

```
┌────────────────────────────────────┬──────────────────────────────────────┐
│         Grid（个人工具）            │         EAASP（企业平台）             │
├────────────────────────────────────┼──────────────────────────────────────┤
│                                    │                                      │
│  grid CLI (Rust)                   │  eaasp-cli (Python)                  │
│    ask / run / agent / session     │    event / session / policies /      │
│    本地直连 grid-engine            │    skills / memory / audit           │
│                                    │    连接 eaasp-server API             │
│                                    │    可替代 L5 驱动全链路验证           │
│                                    │                                      │
│  grid Studio (Rust+TUI)           │  eaasp-web (TS/React)               │
│    TUI 终端 + Web Dashboard        │    事件室 + 四卡 + Admin Console     │
│    本地直连 grid-engine            │    连接 eaasp-server WebSocket/REST  │
│                                    │                                      │
│  grid-web (TS/React)              │  eaasp-server (Python FastAPI)       │
│    现有 web/ workbench             │    L3 治理 + L4 编排                 │
│    连接 grid-server REST/WS       │                                      │
│                                    │  eaasp-runtime-server (Rust Axum)   │
│  grid-server (Rust Axum)          │    L1 运行时 + L2 资产服务            │
│    为 grid-web 提供 API           │    gRPC + REST + MCP                 │
│    嵌入 grid-engine               │                                      │
│                                    │                                      │
├────────────────────────────────────┴──────────────────────────────────────┤
│                         共享底座                                          │
│  grid-engine: AgentRuntime, AgentLoop, Memory, MCP, Tools, Session       │
│  grid-types: 共享类型定义                                                 │
│  grid-sandbox: 沙箱适配器                                                 │
│  grid-runtime: RuntimeContract trait + proto 定义                        │
│  grid-hook-bridge: Hook 桥接 (gRPC/InProcess)                           │
│  grid-eval: 评估框架                                                     │
└───────────────────────────────────────────────────────────────────────────┘
```

### 2.3 Crate / Package 依赖图

```
grid-types          (0) — 共享类型定义，无内部依赖
    ↓
grid-sandbox        (1) — 沙箱运行时适配器
grid-engine         (1) — 核心引擎：agent, memory, mcp, tools, session, security
grid-eval           (1) — 评估框架
grid-hook-bridge    (1) — Hook 桥接（gRPC/InProcess）
grid-runtime        (1) — L1 运行时接口契约 + GridHarness
    ↓
grid-cli            (2) — Grid CLI 产品 (grid ask/run)
                          Grid Studio 产品 (grid-studio TUI+Dashboard, feature=studio)
grid-server         (2) — Grid Web 的 HTTP/WS 后端
grid-desktop        (2) — Grid Desktop (Tauri 桌面 app)

eaasp-runtime-server(2) — EAASP L1+L2 服务 (Rust Axum)
                          依赖: grid-types, grid-engine, grid-sandbox, grid-runtime

eaasp-server        (3) — EAASP L3+L4 服务 (Python FastAPI)
                          调用: eaasp-runtime-server (gRPC + REST)

eaasp-cli           (3) — EAASP 平台管理 CLI (Python)
                          调用: eaasp-server (REST)

eaasp-web           (3) — EAASP L5 前端 (React/TS)
                          调用: eaasp-server (WebSocket/SSE + REST)
```

### 2.4 关键规则

- Grid CLI/Studio **不经过治理层**，直连 grid-engine 的 AgentRuntime
- EAASP 所有 Agent 执行 **必须经过 L3 治理层**，受 managed hooks 约束
- 四个产品共享 grid-engine，但 EAASP 在 grid-engine 之上增加 L3/L4/L5 编排层
- grid-server 为 Grid 产品服务，**不参与 EAASP 架构**
- eaasp-runtime-server 是 grid-server 的 EAASP 版本，承载 L1+L2

---

## 三、EAASP 应用类型矩阵

### 3.1 三类元范式

EAASP v1.8 的三条纵向管线（Hook 管线、数据流管线、会话控制管线）对应三种应用元范式。每种应用类型主要依赖其中一条管线，但三条管线的产出互为输入，形成增强循环。

### 3.2 六种应用方向

**元范式 I：保障引擎（Hook 管线主导）**

| # | 方向 | 核心价值 | 依赖的平台能力 | 上线阶段 |
|---|------|---------|--------------|---------|
| **A** | **工作流执行** | Skill 即流程，Hook 即保障 | 五阶审批链、Skill scoped hooks、managed-settings | M1(advisory) → M2(full) |
| **B** | **合规审计** | 证据链可追溯，治理自动化 | 证据锚点、审计服务、遥测管线、MCP 安全分级 | M1(基础) → M2(完整) |

**元范式 II：智能流（数据流管线主导）**

| # | 方向 | 核心价值 | 依赖的平台能力 | 上线阶段 |
|---|------|---------|--------------|---------|
| **C** | **事件驱动运营中心** | 告警聚类→事件室→主动服务 | 事件引擎、EventRoom、四卡推送 | M1 核心场景 |
| **D** | **深度研究** | 结论有据可查，记忆跨会话积累 | Memory Engine、证据锚点、上下文拼装 | M1(keyword) → M2(semantic) |
| **E** | **知识图谱副驾驶** | 组织大脑，越用越聪明 | Ontology Service + Memory 反馈闭环 | M3（依赖 Ontology Service） |

**元范式 III：协作编排（会话控制管线主导）**

| # | 方向 | 核心价值 | 依赖的平台能力 | 上线阶段 |
|---|------|---------|--------------|---------|
| **F** | **培训与模拟** | 真实事件场景 + advisory 安全模式 | 事件室、advisory mode、记忆回放 | M1（天然支持） |

### 3.3 砍掉的方向及去向

**创新平台**（原方向1）→ 归入 **Grid CLI/Studio 产品线**。创新强调开放探索和快速迭代，EAASP 的强项是治理和确定性。"deny always wins" 原则与"自由创新"的需求方向相反。开放探索、低延迟迭代、多运行时切换这些需求在个人工具中满足，不需要 EAASP 的重型治理架构。

**多 Agent 决策支持**（原方向6）→ 保留为 **M3+ 远期愿景**。A2A ReviewSet 机制作为架构预留存在于 v1.8 规范中，但 M1/M2 都不涉及。结论汇聚的置信度加权、冲突检测、分歧呈现——每一项都是研究级难题。当 M2 的多运行时和审批链成熟后，再评估落地时机。

### 3.4 应用 = Skill + Hook + MCP + 事件源

**EAASP 平台本身不直接"是"某个应用。** 平台提供五层架构和三条管线。应用是企业客户或实施团队基于平台**配置和开发**出来的具体业务解决方案。开发方式：

1. **写 Skill**（YAML frontmatter + 自然语言 prose）— 定义"Agent 应该怎么做"
2. **配策略**（managed-settings Hook 规则）— 定义"Agent 不能做什么 / 必须做什么"
3. **接数据源**（MCP 连接器配置）— 定义"Agent 能连接哪些外部系统"
4. **配事件源**（Webhook 配置）— 定义"什么外部事件触发 Agent 工作"

不需要写平台代码（Rust/Python），只需要配置和编写 Skill 文件。

### 3.5 各应用方向的业务场景举例

#### A. 工作流执行 — 采购审批流程

```
开发内容：
  1. 一个 workflow-skill: procurement-approval.md
     - prose 指导 Agent 如何审核采购申请
     - frontmatter hooks: PreToolUse 拦截超权限金额
  2. MCP 连接器: 对接 ERP 系统（查价格、查库存、查预算）
  3. 策略配置: managed hook "金额>10万必须走审批链"

用户体验：
  员工在 IM 里说"帮我做一个采购申请，100台服务器"
  → Agent 查 ERP 价格/库存 → 生成采购方案(Draft)
  → 审批卡推送给部门经理 → 批准后 Agent 提交 ERP
```

#### B. 合规审计 — 贷款审批合规检查

```
开发内容：
  1. 一个 domain-skill: loan-compliance.md
     - 银行信贷合规规则（贷款比例、客户资质标准）
  2. PostToolUse prompt hook: 评估每个 Agent 输出是否违反监管规定
  3. Stop hook: 检查证据覆盖率是否达到 95%
  4. MCP 连接器: 对接征信系统、监管规则库

用户体验：
  信贷经理提交贷款申请材料
  → Agent 调取征信数据(挂载证据锚点)
  → 逐条比对监管规则(确定性校核器)
  → 输出合规报告：每个结论附带证据链
  → 审计部门可随时追溯"为什么批了这笔贷款"
```

#### C. 事件驱动运营中心 — IT 运维监控

```
开发内容：
  1. Webhook 事件源: 对接 Prometheus AlertManager
  2. 一个 workflow-skill: server-incident-analysis.md
  3. 事件引擎聚类规则: "同一服务3分钟内的告警合并"
  4. MCP 连接器: 对接 Grafana（查指标）、CMDB（查拓扑）

用户体验：
  凌晨3点，Prometheus 推送5条告警（CPU、内存、磁盘、网络、进程）
  → 事件引擎聚合为一个"服务器 X 资源耗尽事件"
  → Agent 自动分析根因（查 Grafana 指标趋势、查最近部署记录）
  → 事件卡推送到值班群："服务器X内存泄漏，疑似昨晚部署的 v2.3.1"
  → 值班工程师一键确认或转派
```

#### D. 深度研究 — 竞品分析报告

```
开发内容：
  1. 一个 workflow-skill: competitive-analysis.md
     - 研究方法论、输出结构模板、质量标准
  2. MCP 连接器: Web 搜索、专利数据库、行业报告库
  3. Stop hook: 检查"每个结论是否引用至少2个独立证据源"

用户体验：
  产品经理说"帮我做竞品A的深度分析"
  → Agent 多轮搜索（Web、专利、财报）
  → 每次获取关键数据 → 写入证据锚点（网页快照hash、检索时间）
  → 输出报告：市场份额(证据:ANC-001)、技术路线(证据:ANC-002)
  → 下次再问同类问题，Memory Engine 自动注入上次的研究成果
```

#### E. 知识图谱副驾驶 — 组织大脑（M3）

```
依赖 Ontology Service（M3 才有），此处仅描述愿景：
  - Ontology 提供结构化知识（设备/人员/流程/组织/规则及关系）
  - Memory Engine 积累运营经验（偏好/阈值/复盘教训）
  - 员工提问 → Agent 查 Ontology 获取结构 + 查 Memory 获取经验
  - 每次执行产生的知识反馈回 Memory，组织大脑越用越聪明
```

#### F. 培训与模拟 — 电力调度员应急演练

```
开发内容：
  1. 一个 workflow-skill: emergency-drill.md
     - 模拟场景脚本（配变过载、线路故障等）
  2. Mock MCP 连接器: 模拟 SCADA 数据（非真实系统）
  3. managed hook: advisory mode 强制开启（绝不操作真实设备）
  4. 评分 Stop hook: 对比学员操作与标准流程的偏差

用户体验：
  新调度员进入"培训模式"事件室
  → 系统注入模拟告警："10kV 馈线A过载 96%"
  → 学员和 Agent 协作分析（Agent 引导思考，不直接给答案）
  → 学员做出操作决策 → 系统评分
  → 操作经验写入个人 Memory（错误模式、学习进度）
```

### 3.6 三条管线的增强循环

三条管线的产出互为输入，形成增强循环：

```
保障引擎(Hook管线) 产生遥测和证据链
    → 喂入 智能流(数据流管线) 作为结构化运营数据
        → 构建组织记忆，使 协作编排(会话控制管线) 在多会话编排时上下文更丰富
            → 产出更高质量的结论，形成更好的证据
                → 回到 保障引擎 做更精准的合规审计
```

M1 同时覆盖四个应用方向（A-advisory、B-基础、C-核心、D-keyword、F-天然），不是因为做了四件事，而是因为底座（事件引擎 + Memory Engine + Hook 管线）是共享的。

---

## 四、技术栈按层划分

### 4.1 核心原则

**L1/L2 用 Rust 做高性能底座，L3/L4 用 Python 做业务编排，L5 用 TypeScript 做前端，层间通过 gRPC + REST + WebSocket 通信。**

### 4.2 各层技术选型

```
┌─────────────────────────────────────────────────────────────┐
│ L5 协作层    TypeScript (React)                              │
│   Web Portal / Admin Console / IM Bot                       │
│   通信: WebSocket/SSE ← L4                                  │
├─────────────────────────────────────────────────────────────┤
│ L4 编排层    Python (FastAPI)                                │
│   事件引擎 / 会话编排器 / 上下文拼装 / A2A路由(M3)           │
│   通信: REST → L3 (内部调用), REST/gRPC → L1+L2             │
├─────────────────────────────────────────────────────────────┤
│ L3 治理层    Python (FastAPI)                                │
│   策略引擎 / 审批闸门 / 审计服务 / 证据链验证                │
│   通信: 与 L4 同进程(内部函数调用), gRPC → L1, REST → L2    │
├─────────────────────────────────────────────────────────────┤
│ L2 资产层    Rust (Axum)                                     │
│   Skill Registry / MCP Orchestrator / Memory Engine          │
│   通信: REST ← L3/L4, MCP ← L1                             │
├─────────────────────────────────────────────────────────────┤
│ L1 执行层    Rust (grid-engine / grid-runtime)               │
│   AgentLoop / Hook执行 / 工具调用 / 沙箱                    │
│   通信: gRPC ← L3, MCP → L2                                 │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 选型理由

| 层 | 语言 | 理由 |
|----|------|------|
| **L1** | **Rust** | 已有 grid-engine 2476+ tests，AgentLoop 是计算密集型，Rust 性能和安全性必要 |
| **L2** | **Rust** | Memory Engine 需要高并发读写 + 索引性能。Skill Registry 已是 Rust。L2 的主要消费者是 L1（高频 MCP 读写），应离 L1 近 |
| **L3** | **Python** | 策略编译、审计查询、证据链验证都是业务逻辑密集型。企业团队扩展策略规则、对接外部合规系统更方便 |
| **L4** | **Python** | 事件引擎的聚类规则、意图路由、上下文拼装需要灵活的业务逻辑。Python 生态的 Webhook 处理、消息队列对接更成熟 |
| **L5** | **TypeScript** | 前端 React 技术栈已确定 |

### 4.4 L2 的归属：逻辑架构 vs 物理部署

**L2 在逻辑架构上是一个统一的资产层，但物理上与 L1 同进程部署。** 这不矛盾——v1.8 规范明确声明"五层是逻辑架构，不等于五个必须独立部署的物理系统"。

L2 与 L1 同进程的理由：
- L2 的主要消费者是 L1（Agent 执行中高频读 Skill、写证据、读记忆）
- L3/L4 对 L2 只是低频管理操作（CRUD、策略编译时查 Skill、会话创建时查 Memory）
- 同进程 MCP 通信（stdio）延迟远低于跨进程 REST

L2 的各子系统在代码上保持模块独立，任何时候可按负载需要拆为独立微服务。

---

## 五、EAASP 部署架构

### 5.1 三个服务

```
eaasp-web (TS/React)                    L5 协作层
    ↕ WebSocket/SSE + REST
eaasp-server (Python FastAPI)           L3 + L4 编排治理
    ↕ REST + gRPC (单向调用)
eaasp-runtime-server (Rust Axum)        L1 + L2 执行与资产
    内部: L1 AgentLoop ←MCP→ L2 Skill/Memory/MCP
```

### 5.2 服务间通信

```
eaasp-server → eaasp-runtime-server 的调用（L4/L3 → L1/L2）：

  1. 三方握手: L4 → L1 gRPC Initialize(SessionPayload)
  2. 发送消息: L4 → L1 gRPC Send(message)
  3. 终止会话: L4 → L1 gRPC Terminate()
  4. 查 Memory: L4 → L2 REST GET /api/v1/memory/search
  5. 查 Skill:  L3 → L2 REST GET /api/v1/skills/{id}
  6. 证据验证: L3 → L2 REST GET /api/v1/memory/anchors

eaasp-runtime-server → eaasp-server 的调用：
  无主动调用。L1 遥测通过 async PostToolUse HTTP hook 推送到
  eaasp-server 的 L3 审计端点（hook 配置驱动）。
  L1 流式输出通过 gRPC 响应流回传到 eaasp-server L4。

依赖方向：单向。eaasp-server 依赖 eaasp-runtime-server。
eaasp-runtime-server 可独立运行（也服务 Grid CLI/Studio 场景）。
```

### 5.3 eaasp-runtime-server 内部模块

```
eaasp-runtime-server (Rust Axum):
  ├── l1/
  │   ├── grpc.rs              gRPC RuntimeContract (端口 50051)
  │   ├── runtime.rs           AgentRuntime 封装
  │   └── adapters/            运行时适配器 (T1/T2/T3)
  ├── l2/
  │   ├── skills/              Skill Registry (REST + MCP Server)
  │   ├── memory/              Memory Engine (REST + MCP Server)
  │   │   ├── anchors.rs       证据锚点库 (append-only)
  │   │   ├── files.rs         文件化记忆 (versioned)
  │   │   ├── index.rs         关键词检索索引 (SQLite FTS5)
  │   │   └── mcp_server.rs    6 MCP tools
  │   ├── mcp/                 MCP Orchestrator
  │   └── ontology/            Ontology Service 网关 (M3)
  ├── api/                     REST API 路由
  └── main.rs                  统一入口

M1 开发阶段: 全部在一个进程 (端口 3001 REST + 50051 gRPC)
M2+ 生产:   可按负载拆分 (L1 按并发会话扩, L2.Memory 按写入量扩)
```

### 5.4 eaasp-server 内部模块

```
eaasp-server (Python FastAPI):
  ├── orchestration/           L4 编排层
  │   ├── event_engine.py      事件引擎 (Webhook接入/去重/聚类/状态机)
  │   ├── session_orchestrator.py  会话编排器 (1:N, 上下文拼装)
  │   ├── event_room.py        事件室数据模型
  │   └── hooks.py             L4 Hook 触发点 (EventReceived, PreSessionCreate, PostSessionEnd)
  ├── governance/              L3 治理层
  │   ├── policy_engine.py     策略引擎 + 编译器
  │   ├── audit_service.py     审计服务
  │   ├── evidence_chain.py    证据链验证
  │   └── advisory_hook.py     Advisory-mode write-block hook
  ├── api/                     REST API 路由
  │   ├── events.py            /api/v1/events/*
  │   ├── sessions.py          /api/v1/sessions/*
  │   ├── policies.py          /api/v1/policies/*
  │   ├── audit.py             /api/v1/audit/*
  │   └── governance.py        /api/v1/governance/*
  ├── ws/                      WebSocket/SSE 推送 (供 L5)
  ├── grpc_client/             连接 L1 的 gRPC client
  ├── l2_client/               连接 L2 的 REST client
  └── main.py                  FastAPI 入口

L3 与 L4 在 M1 阶段同进程部署，L4 直接函数调用 L3。
当规模增大需要独立扩展时（如审计服务写入量大），可拆为独立微服务。
```

### 5.5 eaasp-cli 职能

eaasp-cli 有两类职能：平台管理（运维面）和业务验证（替代 L5 驱动全链路）。

```
1. 平台管理（运维面）
   eaasp policies deploy policy.yaml    # 部署策略
   eaasp policies test --sandbox        # 沙箱测试策略
   eaasp skills submit ./my-skill.md    # 提交 Skill
   eaasp skills promote SK-001          # 晋升 Skill
   eaasp memory search "配变过载"        # 搜索记忆
   eaasp audit query --event EVT-001    # 审计查询
   eaasp status                         # 平台状态

2. 业务验证（替代 L5 驱动全链路）
   eaasp event inject webhook.json       # 模拟 Webhook 注入事件
   eaasp event show EVT-001 --cards      # 终端渲染四卡（文本版）
   eaasp session create --event EVT-001  # 手动触发会话创建
   eaasp session chat EVT-001            # 在事件上下文中与 Agent 对话
   eaasp session stream EVT-001          # 流式查看 Agent 输出
   eaasp event close EVT-001             # 关闭事件，触发记忆沉淀
   eaasp event replay EVT-001            # 回放完整事件生命周期
```

**eaasp-cli 在开发阶段可完全替代 eaasp-web 完成 E2E 验证，让团队在没有 Web 前端的情况下跑通全链路。**

### 5.6 部署全景图

```
┌──────────────────────────────────────────────────────────┐
│                    部署全景                                │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─── Grid 个人工具链（不需要 server 进程）───┐           │
│  │  grid CLI / grid Studio                    │           │
│  │  直接嵌入 grid-engine，本地执行             │           │
│  │  无治理层                                  │           │
│  └───────────────────────────────────────────┘           │
│                                                          │
│  ┌─── EAASP 企业平台（三个服务 + CLI）──────┐             │
│  │                                          │             │
│  │  eaasp-cli (Python)        管理+验证工具  │             │
│  │      ↕ REST                              │             │
│  │  eaasp-web (TS/React)      L5 协作层      │             │
│  │      ↕ WebSocket/SSE + REST              │             │
│  │  eaasp-server (Python)     L3+L4         │             │
│  │      ↕ REST + gRPC                       │             │
│  │  eaasp-runtime-server (Rust) L1+L2       │             │
│  │    ├ L1: gRPC 运行时接口                  │             │
│  │    ├ L2: REST Skill/Memory/MCP API        │             │
│  │    └ L2: MCP Server (供L1 Agent连接)      │             │
│  │                                          │             │
│  │  M2+: L2 各模块可按需拆为独立服务         │             │
│  └─────────────────────────────────────────┘             │
│                                                          │
│  共享底座: grid-types, grid-engine, grid-sandbox,        │
│           grid-runtime, grid-hook-bridge                  │
└──────────────────────────────────────────────────────────┘
```

---

## 六、Gap 分析：现有 Codebase vs M1 目标

### 6.1 L1 执行层 — 成熟度 85%

| 已有 | M1 需补 | 工作量 |
|------|--------|--------|
| grid-runtime Rust T1 (13方法 RuntimeContract + GridHarness) | SessionPayload 新增 3 字段 (event_context, memory_refs, evidence_anchor_id) | 小 |
| claude-code-runtime Python T1 (39 tests) | 对应 Python 侧 SessionPayload 扩展 | 小 |
| hermes-runtime Python T2 (12 tests) | M1 不需要 T2，defer | — |
| grid-hook-bridge (11 tests) | M1 用 Claude Code 原生 hooks，bridge 不在热路径 | — |
| eaasp-certifier (6 tests) | 不变 | — |
| proto v1.3 (SessionPayload 已有 L2 字段) | 新增 event_context 和 memory_refs proto 字段 → proto v1.4 | 小 |
| **新增：Memory write channel** | L1→L2 MCP 通道，Agent 执行中写入证据锚点和记忆文件 | 中 |

**L1 总结：主要工作是 proto 扩展 + Memory MCP write channel。核心 AgentLoop 不变。**

### 6.2 L2 资产层 — 成熟度 40%

| 已有 | M1 需补 | 工作量 |
|------|--------|--------|
| grid-l2-skill-registry crate (REST + SQLite, 10 tests) | 完善，M1 用 2 阶段晋升(draft→production) | 小 |
| grid-l2-mcp-orchestrator crate (YAML + subprocess, 4 tests) | 增加 Memory Engine 和 Skill Registry 作为内建 MCP Server | 中 |
| **新增：Memory Engine** | 证据锚点库(append-only) + 文件化记忆(versioned) + 关键词索引(SQLite FTS5) + 6 MCP tools + REST API | **大** |

**L2 总结：Memory Engine 是 M1 最大的新建工作。**

### 6.3 L3 治理层 — 成熟度 60%

| 已有 | M1 需补 | 工作量 |
|------|--------|--------|
| 策略 DSL 编译器 + 四层合并器 (Rust) | 移植到 Python（eaasp-server 中） | 中 |
| 意图路由（多关键词权重匹配） | 从 L3 上移到 L4（概念调整，逻辑可复用） | 小 |
| 运行时池选择 | M1 只有 Claude Code 一个运行时，不变 | — |
| 遥测采集 + 审计事件 | 审计事件增加 evidence_anchor_ids 字段 | 小 |
| 会话控制（三方握手 + 消息代理） | 握手流程增加 L2 Memory 查询步骤 | 中 |
| **新增：advisory-mode PreToolUse hook** | 拦截所有 write 类工具调用 | 小 |
| **新增：证据链验证** | 检查 Agent 输出是否挂载证据锚点 | 中 |

**L3 总结：基础治理能力已有（Rust），需移植到 Python 并扩展。**

### 6.4 L4 编排层 — 成熟度 10%

| 已有 | M1 需补 | 工作量 |
|------|--------|--------|
| 基础会话管理（BH-MVP, Rust） | 概念复用，Python 重新实现 | — |
| **新增：事件引擎** | Webhook 接入 → 签名校验 → 去重(时间窗口) → 事件对象 → 状态机(5态) | **大** |
| **新增：EventRoom 数据模型** | 事件室：长生命周期，关联事件+会话+四卡数据 | 中 |
| **新增：3 个 L4 Hook 触发点** | EventReceived, PreSessionCreate, PostSessionEnd | 中 |
| **新增：上下文拼装** | 会话创建前查 L2 Memory + Skill → 组装 SessionPayload | 中 |
| **新增：会话编排器** | 1:N event:session 映射，状态管理 | 中 |

**L4 总结：事件引擎是 M1 第二大新建工作。全部用 Python 实现。**

### 6.5 L5 协作层 — 成熟度 5%

| 已有 | M1 需补 | 工作量 |
|------|--------|--------|
| web/ (React workbench) | 可复用 React 框架和组件库 | — |
| WebSocket 基础设施 | 扩展为 SSE/WebSocket 四卡推送 | 中 |
| **新增：eaasp-web 事件室页面** | 事件列表 + 事件室详情（四卡 + 对话流 + 时间线） | 大 |
| **新增：四卡组件** | EventCard + EvidencePack + ActionCard + StatusCard(M1) | 大 |
| **新增：Admin Console** | 策略编辑器 + Skill 管理 + Memory 浏览器 + 基础监控 | 大 |

**L5 总结：前端工作量最大，但 M1 开发阶段用 eaasp-cli 替代，Web Portal 可后置。**

### 6.6 全局总结

| 层级 | 成熟度 | M1 核心新建 | 规模 |
|------|--------|-----------|------|
| L1 | 85% | SessionPayload 扩展 + Memory MCP channel | 小 |
| L2 | 40% | **Memory Engine** (Rust crate) | 大 |
| L3 | 60% | 策略引擎 Python 移植 + advisory hook + 证据链验证 | 中 |
| L4 | 10% | **事件引擎 + EventRoom + 上下文拼装** (Python) | 大 |
| L5 | 5% | eaasp-cli 替代 + 四卡 API (Web Portal 后置) | 中 |

**三大件：Memory Engine (L2, Rust) + 事件引擎 (L4, Python) + eaasp-cli (替代 L5)**

---

## 七、M1 实施路径

### 7.1 总体策略

**夯实 L1/L2 底座 + 垂直验证通道。** 分四个阶段，每阶段有独立可验证的交付物。

- **M1 功能范围 = 最小集**（advisory mode，不做 A2A、不做审批链、不做 IM bot）
- **L1/L2 工程质量 = 生产级**（核心组件完备、层间通讯可靠、接口定义到位）
- **垂直通道 = 验证全链路可跑**（用服务器告警模拟场景 E2E 贯通）

### 7.2 垂直验证场景：服务器告警

选择理由：
1. 覆盖 M1 核心特性：告警聚类、证据锚点、记忆沉淀
2. Mock 数据易构造：Prometheus alertmanager webhook payload 几行 JSON
3. 贴近目标用户体验：运维/调度人员看告警是 M1 核心场景
4. Skill 不复杂：一个 workflow-skill 半天可写

### 7.3 阶段 0：脚手架与 proto 升级（1 周）

**目标：eaasp-server 项目骨架 + proto v1.4 + eaasp-cli 骨架**

```
交付物：
  1. eaasp-server Python 项目骨架
     - FastAPI + uvicorn
     - 模块结构: orchestration/, governance/, memory/, skills/, mcp/
     - 配置系统 (YAML + env)
     - 连接 eaasp-runtime-server 的 gRPC client stub

  2. proto v1.4 升级
     - SessionPayload 新增: event_context, memory_refs, evidence_anchor_id
     - 新增 event.proto: Event 对象、EventRoom、EventState
     - 新增 memory.proto: EvidenceAnchor、MemoryFile、MemorySearch
     - Python + Rust 双端 stub 编译

  3. eaasp-cli 骨架
     - Python CLI (click 或 typer)
     - 子命令结构: event, session, policies, skills, memory, audit, status
     - 连接 eaasp-server REST API

  4. eaasp-runtime-server 定位确认
     - 现有 grid-server 代码基础上重构或新建
     - L2 模块边界划分 (skills/, memory/, mcp/)

验证标准：
  eaasp-server 启动 → eaasp-cli status 返回连接成功
  proto v1.4 双端编译通过
```

### 7.4 阶段 1：L2 Memory Engine + L1 Memory 通道（2-3 周）

**目标：证据锚点 + 文件化记忆 + 关键词检索 + MCP 暴露，L1 可读写**

```
交付物：
  1. Memory Engine 核心 (eaasp-runtime-server Rust 模块)
     - 证据锚点库: append-only SQLite 表, CRUD REST API
     - 文件化记忆: versioned SQLite 表, CRUD REST API
     - 关键词检索: SQLite FTS5 索引, search REST API
     - 6 MCP tools: memory_search, memory_read, memory_write_anchor,
                    memory_write_file, memory_list, memory_archive

  2. L1 SessionPayload v1.8 扩展
     - grid-runtime: proto v1.4 字段支持
     - claude-code-runtime: 同步扩展
     - GridHarness: 新字段透传到 SystemPrompt

  3. L1 Memory write channel
     - Agent 执行中通过 MCP 写入证据锚点
     - PostToolUse hook 触发证据写入
     - PostSessionEnd 触发记忆沉淀

验证标准：
  eaasp-cli memory search "test" 返回结果
  L1 Agent 执行后 Memory 中出现证据锚点和记忆文件
  eaasp-cli 可浏览证据链: 结论 → 锚点 → 数据引用
```

### 7.5 阶段 2：L4 事件引擎 + L3 基础治理（2-3 周）

**目标：Webhook→事件→会话→Agent 执行的完整下行流**

```
交付物：
  1. L4 事件引擎 (eaasp-server Python 模块)
     - Webhook 接收端点: POST /api/v1/events/webhook
     - 签名校验 + 去重 (时间窗口, event_id/hash)
     - 事件对象创建 + SQLite 持久化
     - 事件状态机: received → triaging → active → closed → retrospective
     - EventRoom 数据模型

  2. L4 会话编排器 (eaasp-server)
     - 上下文拼装: 查 L2 Memory + 查 L2 Skill → 构建 SessionPayload
     - 三方握手: 调 L3 编译 hooks → 调 L1 gRPC Initialize
     - 会话状态管理 (1:N event:session 映射)

  3. L3 基础治理 (eaasp-server Python 模块)
     - 策略编译器: YAML → managed_hooks_json
     - Advisory-mode PreToolUse hook: 拦截 write 类工具
     - 审计服务: 接收 L1 遥测, 存储审计事件
     - 证据链验证: 检查 Agent 输出是否挂载证据锚点

  4. L4 Hook 触发点
     - EventReceived: 事件接入时过滤/优先级
     - PreSessionCreate: 配额/权限检查
     - PostSessionEnd: 记忆沉淀触发

验证标准：
  eaasp-cli event inject alert.json → 事件创建
  eaasp-cli session create --event EVT-001 → 三方握手 → Agent 执行
  eaasp-cli session chat S-001 → 流式对话
  审计服务记录完整执行轨迹
  Advisory mode 阻止 write 工具调用
```

### 7.6 阶段 3：垂直验证通道 E2E（1-2 周）

**目标：服务器告警场景全生命周期跑通**

```
交付物：
  1. 验证场景 Skill
     - server-alert-analysis.md (workflow-skill)
     - frontmatter hooks: 证据覆盖率检查、输出质量评估
     - Mock MCP 连接器: 模拟 Prometheus/Grafana 数据

  2. eaasp-cli 完整事件生命周期命令
     - event inject / show --cards / close
     - session create / chat / stream
     - memory search / evidence chain
     - event replay (回放完整生命周期)

  3. E2E 测试脚本
     - 自动化: inject → 等待事件创建 → create session →
       验证证据锚点 → 验证记忆沉淀 → close → 验证复盘记忆
     - 断言: 证据链完整、审计记录存在、advisory mode 生效

  4. 四卡数据 API (为 L5 Web 做准备)
     - GET /api/v1/events/{id}/cards → 返回四卡 JSON
     - 事件卡 + 证据包 + 行动卡 + 状态卡(M1)
     - eaasp-cli event show --cards 渲染文本版

验证标准：
  完整演示:
    Prometheus 告警 webhook → 事件聚合 → 事件卡生成
    → Agent 分析(挂载证据) → 结论输出 → 记忆沉淀
    → 下次同类事件自动注入历史记忆
  E2E 测试脚本全部通过
```

### 7.7 阶段总结

| 阶段 | 周期 | 核心交付 | 验证里程碑 |
|------|------|---------|-----------|
| **0** | 1周 | 骨架 + proto v1.4 | eaasp-cli status 连通 |
| **1** | 2-3周 | L2 Memory + L1 通道 | Agent 写入证据，CLI 可查 |
| **2** | 2-3周 | L4 事件引擎 + L3 治理 | Webhook→事件→会话→Agent 跑通 |
| **3** | 1-2周 | E2E 垂直验证 | 完整事件生命周期演示 |

**总计 6-9 周达到 M1 核心能力验证。**

L5 eaasp-web 在阶段 3 之后启动，有四卡 JSON API 作为基础。

---

## 八、开发阶段 E2E 验证流程（eaasp-cli 替代 L5）

```bash
# 1. 启动平台
$ eaasp-runtime-server start     # L1+L2 Rust 服务
$ eaasp-server start             # L3+L4 Python 服务

# 2. 部署策略
$ eaasp policies deploy advisory-mode.yaml

# 3. 提交 Skill
$ eaasp skills submit server-alert-analysis.md
$ eaasp skills promote SK-001

# 4. 注入模拟事件
$ eaasp event inject test-alert.json
  → 事件引擎接收 → 创建事件 EVT-001

# 5. 查看事件卡
$ eaasp event show EVT-001 --cards
  ┌─ 事件卡 ────────────────────────┐
  │ 服务器 X 资源耗尽               │
  │ 严重度: high  影响: 3 个服务     │
  │ 建议: [查看详情] [发起分析]      │
  └─────────────────────────────────┘

# 6. 创建分析会话
$ eaasp session create --event EVT-001 --skill server-alert-analysis
  → L4 查 L2 Memory → L3 三方握手 → L1 Initialize
  Session S-001 created

# 7. 与 Agent 对话
$ eaasp session chat S-001
  You: 分析这个告警的根因
  Agent: 根据监控数据（证据 ANC-001: CPU 使用率 12:00-12:10 窗口）...

# 8. 查看证据链
$ eaasp event show EVT-001 --evidence
  证据链:
    ANC-001: CPU 监控数据 (Prometheus://server-x/cpu/12:00-12:10) sha256:abc...
    ANC-002: 部署记录 (CMDB://deployments/v2.3.1) sha256:def...

# 9. 关闭事件，触发记忆沉淀
$ eaasp event close EVT-001
  → PostSessionEnd hook → 记忆沉淀
  → 事件状态: active → closed → retrospective

# 10. 验证记忆沉淀
$ eaasp memory search "服务器 X"
  [MEM-001] 经验: 服务器X内存泄漏与v2.3.1部署相关 (agent_suggested)
  [MEM-002] 校准: 服务器X正常CPU基线60%，告警阈值85%
```

---

## 九、关键设计决策记录

| # | 决策 | 理由 |
|---|------|------|
| KD-M1-1 | Grid 和 EAASP 是两个独立产品线 | 治理级别、用户规模、部署方式完全不同 |
| KD-M1-2 | grid-server 不参与 EAASP 架构 | EAASP 通信路径上不需要 grid-server |
| KD-M1-3 | eaasp-runtime-server (Rust) 承载 L1+L2 | L2 主要消费者是 L1（高频 MCP 读写），应同进程部署 |
| KD-M1-4 | eaasp-server (Python) 承载 L3+L4 | 业务编排需要灵活迭代，企业团队主要用 Python/TS/Java |
| KD-M1-5 | L3 和 L4 同进程，内部函数调用 | L3 是 L4 的内部依赖，M1 规模不需要拆分 |
| KD-M1-6 | Memory Engine 用 Rust，放在 eaasp-runtime-server | 高频读写需要性能，且离 L1 近减少网络跳数 |
| KD-M1-7 | L2 各子系统代码模块独立，可按需拆微服务 | 开发方便一体，生产按负载拆分 |
| KD-M1-8 | eaasp-cli 替代 L5 做开发阶段验证 | 全链路验证不依赖 Web 前端，加速底座验证 |
| KD-M1-9 | 垂直验证用服务器告警场景 | 覆盖事件聚类+证据锚点+记忆沉淀三大 M1 核心特性 |
| KD-M1-10 | 砍掉"创新平台"方向（归 Grid）和"多Agent决策"方向（M3+） | 前者与 EAASP 治理哲学矛盾，后者技术风险过高 |
| KD-M1-11 | 新增"培训与模拟"方向 | advisory mode + 事件室天然支持场景模拟训练 |
| KD-M1-12 | 应用 = Skill + Hook + MCP + 事件源 | 不需要写平台代码，配置即应用 |

---

## 十、历史资产处置与 Deferred 项清理

### 10.1 现有 tools/ 下五个 EAASP 工具的处置

v1.8 M1 蓝图确立了新的服务架构（eaasp-server + eaasp-runtime-server），现有 `tools/` 下的五个独立工具需要明确处置。

| 工具 | 位置 | 语言 | 端口 | 处置 | 说明 |
|------|------|------|------|------|------|
| **eaasp-skill-registry** | `tools/eaasp-skill-registry/` | Rust | 8081 | **保留，迁入 eaasp-runtime-server** | 已有完整 REST API (models/routes/store/git_backend)，10 tests。作为 eaasp-runtime-server 的 L2 Skill 模块，代码可直接复用 |
| **eaasp-mcp-orchestrator** | `tools/eaasp-mcp-orchestrator/` | Rust | 8082 | **保留，迁入 eaasp-runtime-server** | 已有 config/manager/routes，4 tests。作为 eaasp-runtime-server 的 L2 MCP 模块 |
| **eaasp-certifier** | `tools/eaasp-certifier/` | Rust | — | **保留不动** | 独立的认证/盲盒测试工具，不属于运行时服务。继续作为独立 CLI 工具存在 |
| **eaasp-governance** | `tools/eaasp-governance/` | Python | 8083 | **代码整合进 eaasp-server** | 已有完整的 L3 五契约 API (policy_deploy, intent_gateway, skill_lifecycle, telemetry_ingest, session_control)、compiler、merger、runtime_pool。直接作为 eaasp-server 的 governance/ 模块基础 |
| **eaasp-session-manager** | `tools/eaasp-session-manager/` | Python | 8084 | **代码整合进 eaasp-server** | 已有 L4 四平面架构 (experience, integration, control, persistence) + L3 client。v1.8 需要从"会话管理器"升级为"事件驱动编排器"，现有代码作为起点，大幅重构 |

**整合后 tools/ 目录变化：**
- `tools/eaasp-skill-registry/` → 代码迁入后可归档或保留为参考
- `tools/eaasp-mcp-orchestrator/` → 代码迁入后可归档或保留为参考
- `tools/eaasp-certifier/` → 不变
- `tools/eaasp-governance/` → 代码整合后归档
- `tools/eaasp-session-manager/` → 代码整合后归档

**注意：** 整合不是一次性完成。Phase 0 先建骨架，Phase 2 逐步将 governance 和 session-manager 的核心逻辑迁入 eaasp-server。原始 tools 在迁入完成前保持可用，之后归档。

### 10.2 历史 Phase Deferred 项处置

以下是 Phase BD 到 Phase BI 积累的所有 Deferred 项，在 v1.8 M1 新架构下的处置决定。

#### Phase BD (grid-runtime EAASP L1) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BD-D1 | hook-bridge crate | **CLOSED** | 已在 Phase BE W2 完成 |
| BD-D2 | RuntimeSelector 真实策略 | **DEFER to M2** | M1 只有 Claude Code 一个运行时，selector 返回固定值即可 |
| BD-D3 | 盲盒对比 | **DEFER to M2** | 需要多运行时才有意义，M1 不需要 |
| BD-D4 | managed-settings 完整实现 | **ABSORB into M1 Phase 2** | eaasp-server L3 governance 模块需要实现策略编译和 managed-settings 分发 |
| BD-D5 | SessionPayload L4 字段 | **CLOSED by M1 Phase 0** | proto v1.4 已定义 event_context, memory_refs, evidence_anchor_id |
| BD-D6 | payload 字段传递到 AgentLoop | **ABSORB into M1 Phase 1** | 新字段需要透传到 SystemPrompt，在 Memory 通道实现时一并处理 |
| BD-D7 | telemetry user_id | **ABSORB into M1 Phase 2** | L3 审计服务需要 user_id 关联，在治理模块实现时一并处理 |

#### Phase BE (协议层 + HookBridge + certifier) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BE-D1 | integration tests (L3→L1 全链路) | **ABSORB into M1 Phase 3** | E2E 垂直验证就是全链路集成测试 |
| BE-D2 | streaming tests (hook bidirectional) | **DEFER to M2** | M1 advisory mode 不需要复杂的流式 hook 交互 |
| BE-D3 | mock-l3 CLI | **SUPERSEDED by eaasp-cli** | eaasp-cli 取代了 mock-l3 的角色 |
| BE-D4 | hook.proto streaming tests | **DEFER to M2** | 同 BE-D2 |
| BE-D5 | certifier CI integration | **DEFER to M2** | M1 用手动验证 |
| BE-D6 | claude-code-runtime Dockerfile | **DEFER to M2** | M1 开发阶段不需要容器化 |
| BE-D7 | MCP real connect | **ABSORB into M1 Phase 1** | Memory Engine MCP Server 是真实连接 |
| BE-D8 | YAML hooks in claude-code-runtime | **DEFER to M2** | M1 用 managed_hooks_json 即可 |
| BE-D9 | session persist (claude-code-runtime) | **DEFER to M2** | M1 会话不需要跨重启持久化 |
| BE-D10 | BASE_URL e2e test | **ABSORB into M1 Phase 3** | E2E 验证覆盖 |

#### Phase BF (L2 统一资产层 + L1 抽象机制) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BF-D1 | Git 追溯 (Skill 版本) | **DEFER to M2** | M1 用 2 阶段晋升 (draft→production)，不需要 Git 追溯 |
| BF-D2 | PerSession/OnDemand MCP | **DEFER to M2** | M1 MCP 连接在会话创建时建立 |
| BF-D3 | RBAC (L2 Skill 访问控制) | **DEFER to M2** | M1 单组织单元，不需要 RBAC |
| BF-D4 | ELO 统计 (盲盒) | **DEFER to M2** | 需要多运行时 |
| BF-D5 | 成本排序 | **DEFER to M2** | M1 单运行时，无成本比较 |
| BF-D6 | Skill 依赖解析 | **DEFER to M2** | M1 Skill 独立使用 |
| BF-D7~D10 | 其他 L2 增强 | **DEFER to M2** | M1 聚焦 Memory Engine 新建 |

#### Phase BG (Enterprise SDK) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BG-D1 | Policy DSL | **ABSORB into M1 Phase 2** | eaasp-server L3 需要策略 DSL 编译，复用 SDK 的 Pydantic 模型 |
| BG-D2 | Playbook DSL | **DEFER to M2** | M1 不需要 Playbook |
| BG-D3 | TypeScript SDK | **DEFER to M3** | M1 平台全部 Python |
| BG-D4~D10 | SDK 其他增强 | **DEFER to M2/M3** | 按需推进 |

#### Phase BH-MVP (E2E 全流程验证) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BH-D1 | 审计事件过滤 | **ABSORB into M1 Phase 2** | L3 审计服务需要 |
| BH-D2 | 策略版本历史 UI | **DEFER to M2** | M1 用 eaasp-cli 管理 |
| BH-D3 | 审计事件过滤增强 | **ABSORB into M1 Phase 2** | 同 BH-D1 |
| BH-D4 | Webhook 事件接入 | **ABSORB into M1 Phase 2** | L4 事件引擎核心功能 |
| BH-D5 | 意图路由增强 | **ABSORB into M1 Phase 2** | L4 会话编排器 |
| BH-D10 | 策略版本回滚 | **DEFER to M2** | M1 先实现基础策略部署 |

#### Phase BI (hermes-runtime) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| HR-D1~D10 | StreamHooks, Skill/Memory/MCP L2 sync, CI, subagent governance, PerSession | **全部 DEFER to M2** | M1 只用 Claude Code (T1)，hermes (T2) 不在 M1 范围 |

#### Phase BC (TUI) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BC-D1~D3 | ToolFormatter trait theme, thinking muted palette, style_tokens cleanup | **保持 DEFER** | TUI 属于 Grid Studio 产品线，与 EAASP 无关 |

#### Phase BB (TUI 视觉升级) Deferred 项

| ID | 描述 | 处置 | 说明 |
|----|------|------|------|
| BB-D1~D4 | message spacing, statusbar responsive, welcome border, formatter migration | **保持 DEFER** | 同上，Grid Studio 范畴 |

### 10.3 Deferred 项处置汇总

| 处置类型 | 数量 | 说明 |
|---------|------|------|
| **CLOSED** | 2 | BD-D1 (已完成), BD-D5 (被 M1 Phase 0 替代) |
| **ABSORB into M1** | 10 | 被 M1 各阶段的任务自然覆盖 |
| **SUPERSEDED** | 1 | BE-D3 (被 eaasp-cli 替代) |
| **DEFER to M2** | 23 | M1 不需要，M2 时评估 |
| **DEFER to M3** | 2 | TypeScript SDK, 知识图谱等远期项 |
| **保持 DEFER (Grid)** | 7 | TUI/Studio 相关，不属于 EAASP |

### 10.4 EAASP_ROADMAP.md 的状态

`docs/design/Grid/EAASP_ROADMAP.md` 基于 v1.7 规范编写（基线 Phase BD W1+W2）。v1.8 M1 蓝图（本文档）已经替代其作为 EAASP 后续开发的权威指引。

**ROADMAP.md 处置：** 保留作为历史参考。在文档头部追加声明：

```
> ⚠️ 本路线图基于 EAASP v1.7 规范。v1.8 后续开发请参考：
> `docs/design/Grid/EAASP_v1.8_M1_IMPLEMENTATION_BLUEPRINT.md`
```

ROADMAP.md 中的已确定设计决策（KD-1~KD-5）在 v1.8 中仍然有效：
- KD-1 (运行时 Tier 定义) — 不变
- KD-2 (12 方法运行时接口) — 不变（proto v1.4 在 SessionPayload 追加字段，不改方法签名）
- KD-3 (L1/L3 通过 hooks 通信) — 不变
- KD-4 (容器化运行常态) — 不变
- KD-5 (SDK 与 Runtime 是两个东西) — 不变

### 10.5 grid-platform crate 的处置

`crates/grid-platform/` 当前是 Rust crate 空壳，原定位为"多租户多 Agent 平台服务"。

**处置：废弃。** v1.8 架构下，L3+L4 用 Python (eaasp-server) 实现，Rust 侧只承载 L1+L2。grid-platform 的多租户愿景由 eaasp-server 的未来版本 (M2/M3) 在 Python 中实现。

具体操作：
- 从 `Cargo.toml` workspace members 中移除 `crates/grid-platform`
- 在 crate 目录下放置 `DEPRECATED.md` 说明
- 不删除代码（历史参考），只是不再编译

---

## 十一、Managed Agent 研究引入的调整 (2026-04-10)

### 11.1 背景

2026-04-10 对三份外部参考资料做了深度阅读：

1. **Anthropic Engineering Blog** — `https://www.anthropic.com/engineering/managed-agents`
2. **Claude Platform Docs** — `https://platform.claude.com/docs/en/managed-agents/overview`
3. **《企业级 Managed Agent 深度研究报告》** — `docs/design/Grid/Managed Agent 企业私有化研究.docx`

报告系统性地论证了企业级私有化 Managed Agent 的 4 维模型（Agent/Environment/Session/Events）、七层控制平面、零信任安全三大基石（Agent Gateway + 硬件级沙箱 + 出站 DLP），以及 Build-vs-Buy 临界点（日 Token >1000万 或 月云支出 >$3-5K）。

### 11.2 对齐验证（EAASP v1.8 方向正确性）

| 外部主张 | EAASP v1.8 对应 | 结论 |
|---------|---------------|------|
| Harness/Environment/Session/Events 四维模型 | L1 Harness + SandboxProfile + SessionPayload + ResponseChunk | ✅ 逻辑同构 |
| 七层控制平面（意图/感知/记忆/推理/工具/编排/治理） | 五层架构 L1-L5 + 三条纵向管线 | ✅ 概念同构 |
| Level 3 受控自主（非 L4 全自主）是企业甜区 | M1 advisory mode + deny-always-wins | ✅ KD-M1-12 已对齐 |
| MCP 作为"AI 时代的 USB-C" | L2 MCP Orchestrator (BF W3 已完成) | ✅ 已上线 |
| 监督者指挥模式 (Orchestrator) | L4 SessionOrchestrator + EventRoom | ✅ 计划中 |
| 策略即代码 (Policy-as-Code) | L3 managed-settings.json + hook.proto | ✅ 已有 |
| 证据链可追溯（合规审计） | Evidence Anchor (append-only) + 5 种 AnchorType | ✅ proto v1.4 规划 |
| 会话独立于容器 (Cattle, not pets) | KD-4: L1 临时容器 + SessionPayload 写回 L4 | ✅ 已有 |
| 多时间维度记忆 (短期/长期) | L2 Memory Engine (证据锚点 + 文件化记忆 + FTS5) | ✅ M1 Phase 1 |

**结论**：七层控制平面里 **4.3 记忆 / 4.4 推理 / 4.5 工具 / 4.6 编排 / 4.7 观测** 都已布局到位。砍掉"创新平台"归 Grid 和"多 Agent 决策"转 M3+ 的决策得到验证。

### 11.3 识别的 P0/P1 安全缺口

外部研究揭示了 3 个 P0 级安全能力缺口和 1 个 P1 级权限缺口，这些能力 M1 可以 advisory mode 先上线，**但 M2 必须补齐**，否则 EAASP 无法承接金融/医疗/国防客户。

#### 缺口 1 (P0): AI Agent Gateway — 独立守门人

**原文**（第 5.1 节）：直接赋予智能体底层 API 凭证极其危险。所有工具调用请求应封装为统一的 JSON-RPC 格式通过网关。基于 OPA Rego 的策略引擎介入评估。

**现状**：L3 `evaluate_hook` 只在会话创建时三方握手一次；工具调用实时拦截依赖 L1 harness 本地 PreToolUse hook（在容器内执行）——"内贼看门"问题。没有独立 Gateway 进程作为守门人；凭据管理的 MCP Server secrets 仍可能被 Agent 生成代码读取。

**风险**：一旦 L1 容器被越权（prompt injection 逃逸），攻击者同时掌握决策和执行——违反"安全控制逻辑与业务逻辑必须彻底解耦"原则。

#### 缺口 2 (P0): 硬件级沙箱隔离

**原文**（第 5.2 节）：常规 Docker 容器与宿主机共享内核，难以防范底层内核逃逸漏洞。必须部署 Firecracker / Kata Containers / gVisor。

**现状**：`crates/grid-sandbox/` 只有三种 adapter — native subprocess / WASM (Wasmtime) / Docker (Bollard)。没有 microVM 支持。`SandboxProfile::Prod` 的 `network_mode="none"` 只是基础隔离，内核共享依然存在。

**风险**：对金融/医疗/国防客户，共享内核的 Docker 不满足合规。AI YOLO 模式（自主代码生成）下必须有硬件级隔离底线。

#### 缺口 3 (P0): 出站控制 + 数据防泄漏 (Egress/DLP)

**原文**（第 5.3 节）：MCP 编排下的智能体带来了"横向风险"——智能体读取财务季报后可能自主调用外网 API 寻求解析协助，合规数据悄无声息流出。必须部署域名过滤 + DNS 管控 + TLS Inspection。

**现状**：L1 Docker adapter 支持 `network_mode="bridge"/"none"`，但没有 egress proxy / 域名白名单机制；没有出站流量审查（TLS MITM）；L2 Memory Engine 写入时未做 PII 检测（SDK 里有 `aidefence_has_pii` 但未接入数据流）。

**风险**：Agent 调用一个外部"解析助手"MCP Server 就能绕过所有治理——数据已离开 VPC。这对 GDPR/NIS2 是致命违规。

#### 缺口 4 (P1): Agentic RAG 文档级权限 (ABAC/ReBAC)

**原文**（第 3.2 节）：传统 RBAC 粗放管理已无法满足需求，转向 ABAC/ReBAC（SpiceDB/OpenFGA）。每份文档摄取时其 ACL 被深度编码为向量元数据，查询时动态过滤。

**现状**：L2 Memory Engine 规划只有 `MemoryScope` 四级枚举（USER/TEAM/ORG_UNIT/EVENT_TYPE），没有 ABAC/ReBAC 细粒度权限；没有 ACL 绑定到向量元数据；FTS5 检索时只按 scope 过滤，不按用户权限上下文过滤。

**风险**：普通员工可能通过精心构造的 query 从 HR/财务记忆中检索高管薪酬等机密信息。

### 11.4 新增 Deferred Items (MA-D1 ~ MA-D7)

以下 Deferred 项补入 M1 蓝图，作为 M2 规划的输入。命名空间 `MA-` 表示 "Managed Agent research"。

| ID | 标题 | 描述 | 阶段 | 优先级 |
|----|------|------|------|--------|
| **MA-D1** | AI Agent Gateway | 新增独立 `eaasp-agent-gateway` 进程 (Python FastAPI 或 Rust Axum)，所有 L1→外部的 MCP 工具调用必须经过。集成 OPA Rego 策略引擎 (通过 `opa` Go binary sidecar 或 `rego-python` 库)。凭据由 Gateway 持有，L1 sandbox 只提交意图 (tool_name + args)。 | Phase 2 L3 治理之前 | **P0** |
| **MA-D2** | Firecracker microVM sandbox | `crates/grid-sandbox/` 新增 Firecracker adapter，启动 <200ms。`SandboxProfile` 新增 `ProdHighSec` 档位：强制 Firecracker/Kata，ephemeral pod，阅后即焚。评估 AWS `firecracker-containerd` 或 `kata-containers` 作为 runtime。 | Phase 1 与 L2 Memory 同步 | **P0** |
| **MA-D3** | Egress Proxy + DLP | L2 新增 `eaasp-egress-proxy` 组件 (评估 Squid / Envoy / 自研)：DNS 白名单（只允许 MCP 注册表声明域名）；HTTPS MITM（解密 + 关键字扫描 + PII 过滤）；命中即 `deny always wins`。L2 Memory Engine `memory_write_*` MCP tool 必须先过 PII scanner。 | Phase 2 L3 治理 | **P0** |
| **MA-D4** | Memory Engine ACL + OpenFGA ReBAC | L2 Memory Engine schema 新增 `acl_tags: Vec<String>`；查询时强制注入 `subject_attributes` (来自 L3 session context)。集成 OpenFGA (Go 实现，gRPC API) 作为授权引擎。Evidence Anchor 同样带 ACL。 | **Phase 1 强制** | **P1** |
| **MA-D5** | Environment Registry (L2 新子系统) | 把 Environment 提升为 L2 资产层的第四个一等公民 (与 Skill/MCP/Memory 同级)。新增 `proto/eaasp/environment/v1/environment.proto` 定义 `EnvironmentTemplate` (预装包、网络规则、挂载文件)。eaasp-cli 新增 `eaasp env create/list/use` 子命令。 | Phase 2 | P2 |
| **MA-D6** | AI 质量观测管道 | L3 `audit_service.py` 新增两条管道：`health_pipe` → Prometheus/OpenTelemetry (SLA 告警)；`quality_pipe` → 异步批处理 + 周期性 eval job (复用 grid-eval crate)。AI 质量指标：幻觉率、策略违规倾向、证据覆盖率、输出相关性。 | Phase 3 | P2 |
| **MA-D7** | ExecutionLifetime proto + ephemeral 强制语义 | runtime.proto SessionPayload 新增 `ExecutionLifetime lifetime = 16;` 字段，枚举 `LIFETIME_EPHEMERAL` (默认，session 结束立刻销毁 pod) / `LIFETIME_PERSISTENT` (仅 Dev Profile 允许)。显式化表达 KD-4 "容器是临时的"语义。 | proto v1.5 (M2) | P2 |

### 11.5 proto v1.4 的预留字段调整

**不影响 Phase 0 执行**，但在 Task 1 的 `SessionPayload` 扩展中，建议同时预留字段位号作为 M2 扩展点：

```protobuf
message SessionPayload {
  // ... existing fields 1-12 (v1.3) ...

  // v1.8 M1 fields:
  EventContext event_context = 13;           // Event context from L4 event engine
  repeated MemoryRef memory_refs = 14;       // Memory references from L2 query
  string evidence_anchor_id = 15;            // Evidence anchor ID for this session

  // Reserved for Managed Agent research alignment (M2):
  reserved 16 to 20;
  // 16 — MA-D7: ExecutionLifetime lifetime
  // 17 — MA-D5: string environment_template_id
  // 18 — MA-D1: string gateway_endpoint
  // 19 — MA-D4: map<string,string> subject_attributes
  // 20 — reserved for future
}
```

这样 Phase 0 的代码不需要改动，但为 M2 留好扩展空间，避免后续字段位号冲突。

### 11.6 新增关键设计决策 (KD-M1-13 ~ KD-M1-16)

补入第九节的设计决策表：

| # | 决策 | 理由 |
|---|------|------|
| **KD-M1-13** | EAASP 的"安全控制逻辑"和"业务逻辑"必须彻底解耦 | 对齐 Anthropic Managed Agents "credentials never reach the sandbox" 原则，防止 prompt injection 逃逸后内贼看门 |
| **KD-M1-14** | M1 不强制 AI Agent Gateway/Firecracker/Egress DLP，但 M2 必须补齐 | M1 以 advisory mode 覆盖基础合规场景；金融/医疗/国防客户必须等 M2 的 MA-D1~D3 上线 |
| **KD-M1-15** | Memory Engine ACL (MA-D4) 在 M1 Phase 1 强制实施 | 避免 Memory 数据先积累后补权限导致的权限倒灌问题 |
| **KD-M1-16** | proto v1.4 在 SessionPayload 中预留字段位号 16-20 | 为 M2 的 MA-D1/D4/D5/D7 提供扩展空间，避免未来字段位号冲突 |

### 11.7 Phase 0 的影响评估

| Task | 是否需要修改 | 说明 |
|------|------------|------|
| Task 1 (proto v1.4 SessionPayload) | **轻微修改** | 追加 `reserved 16 to 20;` 一行注释 |
| Task 2 (event.proto + memory.proto) | 不变 | 保持原有定义 |
| Task 3 (eaasp-server 骨架) | 不变 | 骨架不涉及 gateway/firecracker/egress |
| Task 4 (eaasp-cli 骨架) | 不变 | CLI 不涉及 |
| Task 5 (Python proto stubs) | 不变 | 编译脚本不变 |
| Task 6 (Makefile + 集成测试) | 不变 | — |
| Task 7 (claude-code-runtime sync) | 不变 | 新字段透传即可 |

**结论**：Phase 0 可以立即执行，只需在 Task 1 的 proto 扩展中追加一行 `reserved` 声明。

---

## 十二、参考文档

| 文档 | 位置 | 用途 |
|------|------|------|
| EAASP v1.8 完整设计规范 | `docs/design/Grid/EAASP_Design_Specification_v1_8.docx` | 权威架构定义 |
| M1 "Operator Edition" 规范 | `docs/design/Grid/EAASP_v1_8_Milestone_1_Operator_Edition.docx` | M1 功能范围和接口定义 |
| 三类元范式分析 | `docs/design/Grid/EAASP应用类型按"三类元范式"深度展开.pdf` | 应用方向和管线映射 |
| v1.8 架构蓝图 (md) | `docs/design/Grid/EAASP_ARCHITECTURE_v1.8.md` | 五层架构详细定义 |
| v1.8 架构构想 (md) | `docs/design/Grid/EAASP_v1.8_架构构想.md` | 各层职责和对象模型 |
| GPT Review 定版建议 | `docs/design/Grid/EAASP-Design-Specification-v1.8-update-review-by-gpt.md` | 业界对标和定版口径 |
| EAASP 演进路线图 | `docs/design/Grid/EAASP_ROADMAP.md` | 历史演进记录 |
| **Managed Agent 企业私有化研究** | `docs/design/Grid/Managed Agent 企业私有化研究.docx` | **第十一章补强来源** |
| Anthropic Engineering Blog | `https://www.anthropic.com/engineering/managed-agents` | Harness/Sandbox/Session 设计哲学 |
| Claude Platform Managed Agents | `https://platform.claude.com/docs/en/managed-agents/overview` | Agent/Environment/Session/Events 四维模型 |
