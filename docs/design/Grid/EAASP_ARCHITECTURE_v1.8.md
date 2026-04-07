# EAASP 架构分层设计 v1.8

> **版本**: v1.8 (Architecture Blueprint)
> **创建日期**: 2026-04-07
> **基线**: EAASP v1.7 规范 + AgentOS 前期研究 + 2026 业界调研
> **状态**: 设计蓝图，待逐步实施

---

## 一、设计背景

### 1.1 从 v1.7 到 v1.8 的驱动力

EAASP v1.7 规范定义了 L1-L4 四层架构，Phase BH-MVP 验证了 L4→L3→L2→L1 全链路可行性（58 tests）。但 MVP 完成后暴露了根本性问题：

**v1.7 的 L4 本质上是一个"任务派发机"** — 用户给指令 → Agent 完成 → 结束。这将企业 AI 弱化成对传统应用菜单项的 Agent 接管，而非真正的"AI 工作伙伴"。

### 1.2 业界 2026 趋势

| 平台 | 核心转变 | 关键能力 |
|------|--------|--------|
| **Anthropic Claude Cowork** | 从对话助手到持久协作空间 | 持久化线程、Computer Use、记忆 |
| **Microsoft Copilot Wave 3** | 从起草助手到治理执行层 | Agent 365、审批闸门、自主触发 |
| **Salesforce Agentforce** | 从 CRM 到结果架构平台 | Atlas 推理引擎、A2A 协议、混合推理 |
| **Google Agentspace** | 从搜索到 Agent 工作空间 | Memory Bank、Agent Gallery、A2A |

共同趋势：**从被动响应 → 事件驱动主动服务，从无状态 → 持久化记忆，从单 Agent → 多 Agent 并行协作**。

### 1.3 AgentOS 前期研究的核心洞察

来自 `docs/design/AgentOS/` 的前期思考，三大设计假设：

1. **IM is the new Desktop** — 企业 AI 主战场从独立 App 迁移到 IM 入口
2. **Skills 是新时代的 Apps** — 能力交付单元从"功能模块"变为 Skill
3. **Memory as a File System** — 记忆是"文件化资产 + 混合检索"，不是向量数据库

以及五件套系统能力：事件化引擎、证据索引层、A2A 并行互审、确定性校核层、审批与回滚机制。

---

## 二、五层架构总览

```
L5  协作层  Cowork Layer         人与 Agent 的协作空间
L4  编排层  Orchestration Layer  事件驱动 + 会话编排 + A2A 协调
L3  治理层  Governance Layer     策略 + 审批 + 审计 + 校核
L2  资产层  Asset Layer          Skill + MCP + Memory Engine
L1  执行层  Execution Layer      Agent Runtime（LLM推理 + 工具 + 沙箱）
```

### 全局架构图

```
         L5 协作层          L4 编排层          L3 治理层          L2 资产层          L1 执行层
        ┌─────────┐       ┌─────────┐       ┌─────────┐       ┌─────────┐       ┌─────────┐
        │ IM/Web  │       │ 事件引擎 │       │ 策略引擎 │       │ Skill   │       │ Agent   │
        │ CLI     │       │ 会话编排 │       │ 审批闸门 │       │ Registry│       │ Loop    │
        │ Bot     │       │ A2A路由  │       │ 审计引擎 │       │ MCP     │       │ 工具    │
        │ 四卡渲染 │       │ 触发器   │       │ 校核器   │       │ Memory  │       │ 沙箱    │
        └────┬────┘       └────┬────┘       └────┬────┘       └────┬────┘       └────┬────┘
             │                 │                 │                 │                 │
  ═══════════╪═════════════════╪═════════════════╪═════════════════╪═════════════════╪═══════
  Hook 管线   │  L4: EventReceived / PreSessionCreate / PostSessionEnd              │
             │                 │  L3: PrePolicyDeploy / PreApproval                │
             │                 │                 │  L1: 9种生命周期事件              │
  ═══════════╪═════════════════╪═════════════════╪═════════════════╪═════════════════╪═══════
  数据流管线  │  下行: 事件/消息 → 上下文拼装 → 策略编译 → Skill注入 → AgentLoop     │
             │  上行: ResponseChunk → 遥测 → 记忆写入 → 卡片渲染 → 用户推送        │
  ═══════════╪═════════════════╪═════════════════╪═════════════════╪═════════════════╪═══════
  会话控制管线 │  EventRoom(L5) → Event+Sessions(L4) → 治理(L3) → 资产(L2) → Loop(L1)│
             │                 │                 │                 │                 │
```

---

## 三、各层职责定义

### 3.1 L1 执行层 Execution Layer — "手和脚"

```
输入：SessionPayload（用户上下文 + managed_hooks + skill + event_context）
输出：ResponseChunk 流 + TelemetryEvent + MemoryWrite

职责：
  ✅ LLM 推理循环（AgentLoop — 自主多步执行）
  ✅ 工具调用与沙箱执行
  ✅ Hook 评估（InProcess HookExecutor / gRPC HookBridge）
  ✅ 遥测上报（工具调用、Hook触发、token用量）
  ✅ 记忆写入请求（结论/偏好/复盘 → L2 Memory Engine）

不做：
  ❌ 不决定用哪个 Skill（L4 编排层决定）
  ❌ 不决定策略合规（L3 治理层决定）
  ❌ 不持久化记忆（L2 Memory Engine 负责）
  ❌ 不直接面向用户（L5 协作层负责）

Runtime 类型：
  T1 Harness: Grid Runtime (Rust), Claude Code Runtime (Python) — 原生 Hook
  T2 Aligned: 对齐协议但需 HookBridge 的第三方 Runtime
  T3 Framework: 仅实现基础 gRPC 接口的轻量 Runtime
```

### 3.2 L2 资产层 Asset Layer — "工具箱和记忆"

三大子系统：

```
1. Skill Registry（已实现）
   - CRUD、版本管理、搜索推荐
   - Skill = 新时代的 App — 可发现、可调用、可审计的能力单元
   - REST API: /api/v1/skills/*

2. MCP Orchestrator（骨架已实现）
   - 外部工具发现与连接池管理
   - Skill 可声明依赖的 MCP Server
   - REST API: /api/v1/mcp/*

3. Memory Engine（新增 ★）
   三层存储：
   ┌─────────────────────────────────────────────────┐
   │ 证据锚点库 Evidence Anchor Store                  │
   │   每个 Agent 结论必须挂载数据快照 ID              │
   │   拓扑版本、量测窗口、规则条款 → 可追溯           │
   ├─────────────────────────────────────────────────┤
   │ 文件化记忆 Memory Files                          │
   │   偏好/阈值/复盘经验沉淀为结构化文件              │
   │   类比 Claude CLAUDE.md — Agent 的长期记忆        │
   ├─────────────────────────────────────────────────┤
   │ 索引库 Memory Index                              │
   │   混合检索：关键词 + 语义 + 时间衰减              │
   │   跨 Agent 共享：多 Agent 读写同一用户/事件记忆    │
   └─────────────────────────────────────────────────┘

   REST API: /api/v1/memory/*
   
   为什么放 L2：
     记忆是一种"资产"，和 Skill 同级
     L1 执行时按需读写，L4 编排时按需查询
     独立服务，所有层可调用（对标 Google Memory Bank）
```

### 3.3 L3 治理层 Governance Layer — "规则和安全"

```
已有（BH-MVP 实现）：
  ✅ 策略 DSL 编译器（YAML → managed_hooks_json）
  ✅ 四层合并器（managed > skill > project > user, deny-always-wins）
  ✅ 策略版本管理 + 回滚（BH-D10）
  ✅ 运行时池选择
  ✅ 遥测采集 + 审计查询（含审计事件过滤 BH-D3）
  ✅ 会话控制（三方握手 + 消息代理 + 终止）
  ✅ 意图路由（多关键词权重匹配 BH-D5）

新增：
  ★ 审批闸门（控制链）
    Plan（只读计划）→ Check（确定性校核）→ Draft（草稿生成）
    → Approve（人工审批）→ Execute（执行）
    高风险操作默认只到 Draft，不自动执行

  ★ 确定性校核器
    高风险判断不依赖 LLM，交给规则引擎/仿真工具
    例：五防校核、越限检查、断面稳定边界
    校核结果挂载到证据锚点

  ★ 证据链管理
    每个 Agent 输出关联证据锚点 ID
    审计不只是"记录发生了什么"，而是"为什么这样判断"
    全链路可追溯：结论 → 证据包 → 原始数据快照
```

### 3.4 L4 编排层 Orchestration Layer — "大脑"

```
从 v1.7 的"会话管理器"升级为"事件驱动编排引擎"

三大引擎：

1. 事件引擎 Event Engine ★
   ┌─────────────────────────────────────────────┐
   │ 事件源接入                                    │
   │   Webhook / Kafka / CDC / 定时任务 / 用户消息 │
   │                                              │
   │ 事件处理管线                                   │
   │   接入 → 去重 → 聚类 → 关联分析 → 升级为"事件" │
   │   （海量告警不是 N 条消息，是一个事件对象）      │
   │                                              │
   │ 事件生命周期                                   │
   │   received → triaging → active →              │
   │   pending_approval → executing →              │
   │   monitoring → closed → retrospective         │
   └─────────────────────────────────────────────┘
   
   这是 AgentOS 最核心的差异化 — Agent 不等用户指令，
   而是事件驱动自主启动

2. 会话编排器 Session Orchestrator
   ┌─────────────────────────────────────────────┐
   │ 1:N 映射                                     │
   │   一个 Event Room 可关联多个 L3 Session        │
   │   意图变化 → 创建新 Session（保留旧上下文）     │
   │                                              │
   │ 状态机                                        │
   │   event_status × session_status × approval    │
   │                                              │
   │ 上下文传递                                    │
   │   Session 1 输出 → L2 Memory → Session 2 输入 │
   │   不传对话历史，传结构化记忆                     │
   │                                              │
   │ 自适应协作节奏                                 │
   │   简单操作 → 自主完成                          │
   │   需要判断 → 请求人类输入                      │
   │   敏感操作 → 走审批闸门                        │
   └─────────────────────────────────────────────┘

3. A2A 路由 Agent-to-Agent ★
   ┌─────────────────────────────────────────────┐
   │ 并行互审调度                                   │
   │   一件事拆分给多专长 Agent 并行校核             │
   │   继保审 + 方式审 + 安全审 → 并行运行          │
   │   结果以卡片形式汇聚呈现                       │
   │                                              │
   │ A2A 通信协议                                  │
   │   对齐 Google/Salesforce A2A 标准              │
   │   Agent 发布能力清单 → 按需发现调用             │
   │                                              │
   │ 结果汇聚                                      │
   │   多 Agent 结论合并为统一行动建议               │
   │   置信度加权 → 冲突检测 → 最终建议              │
   └─────────────────────────────────────────────┘
```

### 3.5 L5 协作层 Cowork Layer — "人与 Agent 的协作空间"

```
L5 是用户直接接触的层，可以是企业 IM 插件、Web Portal、CLI 等多种前端。
L5 本身不含业务逻辑，所有数据来自 L4（事件+状态）和 L3（策略+审计）。

核心概念：

1. 事件室 Event Room
   一事一室：每个业务事件自动生成独立协作空间
   包含：对话流 + 四卡置顶 + 关联文档 + 时间线
   类比：GitHub Issue + Slack Thread + Dashboard 的合体
   长生命周期 — 不随单个 Session 结束而关闭

2. 四卡置顶 Pinned Cards（AgentOS 核心交互范式）
   ┌─────────────────────────────────────────────┐
   │ ① 事件卡 Event Card                          │
   │   一屏看懂"发生了什么、风险多大、下一步建议"    │
   │   一句话结论 + 影响评估 + 快捷按钮             │
   ├─────────────────────────────────────────────┤
   │ ② 证据包 Evidence Pack（默认折叠）            │
   │   一键追溯"凭什么这样判断"                     │
   │   告警聚类摘要 + 量测窗口 + 拓扑版本 + 历史相似 │
   ├─────────────────────────────────────────────┤
   │ ③ 行动卡 Action Card                         │
   │   步骤树 + 分支条件 + 最小下一步               │
   │   一键动作：拉起事件室 / @相关专业 / 生成草稿   │
   ├─────────────────────────────────────────────┤
   │ ④ 审批卡 Approval Card                       │
   │   所有写入/派单/正式建议的唯一出口              │
   │   30秒原则：动作范围 + 理由摘要 + 回滚路径     │
   └─────────────────────────────────────────────┘
   
   不同场景复用同一套四卡，只是卡片内容不同。

3. 推送通道
   SSE / WebSocket（实时流式）
   IM Bot（企业微信/钉钉/飞书/Slack）
   邮件（审批通知）

4. 多端适配
   Web Portal — 完整四卡 + 事件室
   IM 插件 — 卡片化消息 + 按钮交互
   CLI — 结构化文本输出
   移动端 — 简化卡片 + 推送通知
```

---

## 四、纵向机制 A：Hook 管线

Hook 是 EAASP 最核心的治理手段，贯穿全部五层。

### 4.1 Hook 生命周期

```
创作(L5) → 编译(L3) → 部署(L3→L1) → 评估(L1) → 审计(L3)
```

### 4.2 各层 Hook 触发点

```
L4 编排层 Hook：
  • EventReceived    — 事件接入时（过滤/优先级调整）
  • PreSessionCreate — 创建会话前（配额/权限检查）
  • PostSessionEnd   — 会话结束后（记忆沉淀触发）

L3 治理层 Hook：
  • PrePolicyDeploy  — 策略部署前（格式/冲突检查）
  • PreApproval      — 审批前（自动校核）

L1 执行层 Hook（9 种生命周期事件）：
  • SessionStart     — Agent 初始化
  • PreToolUse       — 工具调用前（PII检查、权限、bash禁止）
  • PostToolUse      — 工具调用后（审计记录、结果校验）
  • PreEdit          — 文件修改前
  • PostEdit         — 文件修改后
  • PreCommand       — 命令执行前
  • PostCommand      — 命令执行后
  • Stop             — Agent 准备结束（清单检查、强制继续）
  • SessionEnd       — 会话结束（遥测flush、记忆沉淀）
```

### 4.3 策略合并与评估

```
四层合并（deny-always-wins）：
  managed (enterprise) — 最高优先级，不可覆盖
    > skill-scoped (L2 frontmatter hooks)
      > project
        > user — 最低优先级

评估结果：
  allow              — 放行
  deny(reason)       — 拦截，返回原因
  modify(input)      — 修改输入后放行

裁决原则：
  任一 rule 返回 deny → 最终结果为 deny（§10.8）

审计：
  每次评估无论结果如何 → 生成审计记录 → L3 审计引擎
```

---

## 五、纵向机制 B：数据流管线

### 5.1 下行流（用户/事件 → Agent 执行）

```
L5  用户消息 / 外部事件
     │
L4  事件引擎聚合 → 意图路由 → 上下文拼装（从 L2 Memory 拉取）
     │  输出：EventContext { event_id, skill_id, user_context, memory_refs[] }
     │
L3  三方握手：
     │  1. GET L2/skills/{id} → SkillContent
     │  2. 编译策略 → managed_hooks_json
     │  3. 选运行时 → runtime_endpoint
     │  4. gRPC L1.Initialize(SessionPayload)
     │
     │  SessionPayload {
     │    user_id, org_unit, managed_hooks_json,
     │    skill_ids, skill_registry_url,
     │    event_context,        ← v1.8 新增
     │    memory_refs,          ← v1.8 新增
     │    evidence_anchor_id    ← v1.8 新增
     │  }
     │
L2  Skill 内容 + 记忆上下文注入 SessionPayload
     │
L1  AgentLoop 自主执行：
       构建 SystemPrompt（Skill prose + 上下文 + 记忆）
       LLM 推理 → 工具调用 → Hook 评估 → 循环
```

### 5.2 上行流（Agent 输出 → 用户/系统）

```
L1  三类上行数据：
     ├─ ① ResponseChunk（流式）— text_delta / tool_start / tool_result / done
     ├─ ② TelemetryEvent（异步）— tool_call / hook_fired / hook_deny / token_usage
     └─ ③ MemoryWrite（按需）— 结论/偏好/复盘 → L2 Memory Engine
     │
L2  接收 ③：写入证据锚点库 / 文件化记忆 / 更新索引
     │
L3  接收 ②：遥测持久化 → 审计查询 → 合规报告
     │  审批闸门：Agent 产出草稿 → 暂停等待人工确认
     │
L4  接收 ① + ② 关键事件：
     │  更新事件状态机
     │  汇聚 A2A 多 Agent 结论
     │  构建四卡数据
     │
L5  接收渲染数据：SSE/WebSocket → 卡片更新 / 对话流
```

---

## 六、纵向机制 C：会话控制管线

### 6.1 会话模型

```
Event Room (L5, 长生命周期)
  └─ Event (L4, 一个业务事件)
       ├─ Session 1 (L3→L1, 绑定 skill_id + runtime_id)
       ├─ Session 2 (L3→L1, 可能不同 Skill)
       └─ Session 3 (L3→L1, A2A 派发的子任务)
```

- **Event Room**：面向用户，长生命周期，不随单个 Session 结束
- **Event**：一个业务事件对象，有状态机
- **Session**：一次 Agent 任务执行，绑定具体 Skill 和 Runtime

### 6.2 事件状态机

```
received → triaging → active → pending_approval → executing
  → monitoring → closed → retrospective
```

### 6.3 Session 编排规则

- 意图变化 → 创建新 Session（保留旧 Session 上下文通过 Memory）
- Agent 请求子任务 → A2A 派发新 Session
- 审批闸门触发 → Session 暂停，等待 L5 人工确认
- 所有 Session 完成 → Event 进入 monitoring/closed
- Event 关闭后 → 触发记忆沉淀（复盘经验写入 L2 Memory）

### 6.4 完整一轮时序

```
外部事件          L5         L4           L3          L2         L1
   │              │          │            │           │          │
   │─告警─────→│          │            │           │          │
   │              │──推送───→│            │           │          │
   │              │          │ 聚合为事件   │           │          │
   │              │          │ 创建EventRoom│          │          │
   │              │          │──请求Skill──────────→│           │
   │              │          │──三方握手──→│           │          │
   │              │          │            │──Initialize──────→│
   │              │←─事件卡──│            │           │          │
   │         用户点击        │            │           │          │
   │         [发起会审]      │            │           │          │
   │              │──────→│ A2A 派发     │           │          │
   │              │          │──────Send─────────────────→│
   │              │          │            │           │  AgentLoop│
   │              │          │←──stream──────────────────────│
   │              │←─更新卡片─│            │←─遥测──────────────│
   │              │          │            │           │←─记忆写入│
   │              │          │ 汇聚结论    │           │          │
   │              │←─行动卡──│            │           │          │
   │              │←─审批卡──│            │           │          │
   │         用户[审批]      │            │           │          │
   │              │──────→│──审批───→│           │          │
   │              │          │            │ 校核+放行  │          │
   │              │          │            │──Execute─────────→│
   │              │          │ Session完成 │           │          │
   │              │          │ 记忆沉淀────────────→│ 写入      │
   │              │←─复盘卡──│            │           │          │
```

---

## 七、各层通信协议

| 路径 | 协议 | 方向 | 内容 |
|------|------|------|------|
| L5 ↔ L4 | SSE / WebSocket | 双向 | 卡片数据下推、用户操作上送 |
| L4 → L3 | HTTP REST | 请求 | 三方握手、策略查询、审批提交 |
| L3 → L1 | gRPC (proto) | 双向流 | Initialize/Send/Terminate/OnToolCall |
| L3 → L2 | HTTP REST | 请求 | Skill 内容获取 |
| L4 → L2 | HTTP REST | 请求 | 上下文拼装（Memory 查询） |
| L1 → L2 | HTTP REST | 请求 | 运行时 Skill 搜索、Memory 写入 |
| L1 → L3 | gRPC / HTTP | 上报 | 遥测事件、Hook 评估结果 |
| L4 ↔ L4 | A2A (HTTP) | 双向 | Agent 间通信（并行互审） |
| 外部 → L4 | Webhook / Kafka | 单向 | 事件源接入 |

---

## 八、v1.7 → v1.8 变更总结

| 维度 | v1.7 | v1.8 |
|------|------|------|
| **层数** | L1-L4 四层 | L1-L5 五层（新增协作层） |
| **L5 协作层** | 不存在 | 事件室 + 四卡置顶 + 多端推送 |
| **L4 职责** | 会话管理器 | 编排引擎（事件+A2A+状态机） |
| **L2 记忆** | 不存在 | Memory Engine（证据锚点+文件化记忆+索引） |
| **驱动模型** | 请求驱动 | 事件驱动 + 请求驱动混合 |
| **交互范式** | REST 请求-响应 | 事件室 + 四卡置顶 |
| **安全控制链** | Hook deny/allow | Plan→Check→Draft→Approve→Execute |
| **多 Agent** | 单 Agent per Session | A2A 并行互审 |
| **主动性** | 被动等指令 | 事件触发 + 定时任务 + 主动推送 |
| **会话模型** | Conversation:Session = 1:1 | EventRoom:Session = 1:N |
| **纵向机制** | 未明确 | Hook管线 + 数据流管线 + 会话控制管线 |

---

## 九、与已实现代码的兼容性

```
BH-MVP 已有代码        v1.8 中的位置         变更程度
──────────────────────────────────────────────────
策略 DSL + 编译器    →  L3 策略引擎           ✅ 不变
5 API 契约          →  L3 治理层 API          ⚠️ 扩展（加审批闸门+证据链）
意图路由            →  L4 编排层              ⚠️ 从 L3 上移到 L4
RuntimePool         →  L3 运行时池            ✅ 不变
L4 四平面           →  L4+L5 拆分             🔄 持久化→L2, 体验→L5, 编排保留
PersistencePlane    →  L2 Memory + L4 事件存储 🔄 拆分
SDK eaasp run       →  L5 CLI 入口            ✅ 不变
E2E 测试            →  仍然有效               ✅ 不变
HR 示例             →  L5 场景模板            ✅ 不变
Mock L1/L2 客户端   →  保留用于测试            ✅ 不变
```

**核心结论**：L1/L2(Skill+MCP)/L3 的变更较小。最大变更在 L4（从会话管理到事件编排）和新增 L5（协作层独立）以及 L2 新增 Memory Engine。已写的代码不浪费。

---

## 十、后续实施路径（建议）

```
Phase 1 ✅ 已完成（BH-MVP）
  L3 策略引擎 + L4 基础会话 + E2E 全链路验证

Phase 2: 事件引擎 + 事件室基础
  L4 事件引擎（接入 → 聚合 → 事件对象 → 状态机）
  L5 事件室数据模型 + 事件卡 API
  验证：告警洪泛 → 聚合为事件 → 推送事件卡

Phase 3: Memory Engine + 证据索引
  L2 Memory Engine（证据锚点 + 文件化记忆 + 混合检索）
  L1→L2 记忆写入通道
  L4→L2 上下文拼装通道

Phase 4: 审批闸门 + 确定性校核
  L3 控制链（Plan→Check→Draft→Approve→Execute）
  L5 审批卡交互
  确定性校核器集成

Phase 5: A2A 并行互审
  L4 A2A 路由协议
  多 Agent 并行执行 + 结果汇聚
  L5 会审卡片展示

Phase 6: 完整四卡 + IM 集成
  L5 四卡渲染引擎
  IM Bot 插件（企业微信/钉钉/飞书）
  Web Portal
```
