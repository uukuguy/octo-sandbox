# EAASP v1.8 架构构想

> **版本**: v1.8 Draft
> **基线**: EAASP v1.7 设计规范 + 2026 业界趋势调研
> **性质**: 架构构想完整说明，供进一步讨论与迭代
> **适用范围**: EAASP 平台规范，不限定特定实现

---

## 第一部分：设计背景与核心转变

### 1. 从 v1.7 到 v1.8 的驱动力

#### 1.1 v1.7 的成就

EAASP v1.7 规范定义了一个清晰的四层架构（L1 执行层、L2 资产层、L3 治理层、L4 人机协作层），配套三项跨层机制（Hooks、Skills、MCP），解决了六大核心挑战：

- **运行时无关的执行**：L1 内部抽象机制（运行时选择器、适配器注册表、Hook 桥接器、遥测采集器）使任何实现了 12 方法运行时接口契约的智能体都可加入运行时池。从 T1 Harness（EAASP Runtime、Claude Code）到 T2 Aligned（Aider、Goose）再到 T3 Framework（LangGraph、CrewAI），通过三层适配器厚度适配。
- **统一的智能体治理**：L3 将企业策略编译为受管 hooks，通过 managed-settings 部署到所有运行时。"拒绝优先"（Deny always wins）保证企业策略的确定性强制执行。
- **智能工作流**：L2 中的 workflow-skills 将业务流程编码为智能作战手册，Skill 在 YAML frontmatter 中携带作用域 hooks，智能与保障在同一份资产中共存。
- **技能资产管理**：L2 提供六个运行时服务（Skill 仓库、晋升引擎、访问控制、依赖解析器、版本管理器、使用分析），通过四阶段晋升流水线（draft → tested → reviewed → production）进行治理。
- **人机协作**：L4 提供四个内部平面（体验、集成、控制、持久化），五个 L3/L4 API 契约定义了完整的跨层通信。
- **渐进式演进**：五个阶段逐步扩展既有运行时能力。

#### 1.2 v1.7 的局限性

MVP 验证完成后暴露了根本性问题：

**v1.7 的 L4 本质上是一个"任务派发机"** — 用户给指令 → 会话管理器分配运行时 → Agent 执行 → 返回结果 → 结束。这将企业 AI 弱化成对传统应用菜单项的 Agent 接管，而非真正的"AI 工作伙伴"。

具体表现为五个结构性不足：

1. **被动等指令**：L4 不监听外部事件，只在用户主动发起时才创建会话。海量告警、审批超时、数据异常等事件无法触发 Agent 主动服务。
2. **无持久化记忆**：L1 的会话记忆是短暂态（会话结束即消失），L4 仅保存会话存储用于续接。Agent 无法积累经验、无法跨会话引用历史结论、无法建立证据链。
3. **单 Agent 孤岛**：每个会话绑定一个运行时实例，没有多 Agent 并行互审的机制。复杂场景（如电力系统故障处理需要继保审、方式审、安全审同时进行）无法支撑。
4. **结论不可追溯**：Agent 输出结论但不挂载证据。审计只记录"发生了什么"（遥测），不记录"为什么这样判断"（证据锚点）。
5. **缺少协作节奏控制**：简单操作和高风险操作走同一条路径，没有"草稿→校核→审批→执行"的控制链。

#### 1.3 2026 业界趋势

| 平台 | 核心转变 | 关键能力 |
|------|--------|--------|
| **Anthropic（Claude Code / Agent SDK / Context Engineering）** | 从对话助手到持久协作空间 | 持久化线程、Computer Use、工作记忆、多 Agent 实践 |
| **Microsoft Copilot Wave 3** | 从起草助手到治理执行层 | Agent 365、审批闸门、自主触发 |
| **Salesforce Agentforce** | 从 CRM 到结果架构平台 | Atlas 推理引擎、A2A 协议、混合推理 |
| **Google Agentspace** | 从搜索到 Agent 工作空间 | Memory Bank、Agent Gallery、A2A |

共同趋势：**从被动响应 → 事件驱动主动服务，从无状态 → 持久化记忆，从单 Agent → 多 Agent 并行协作**。

EAASP v1.7 在治理能力（Hooks + Deny always wins）和运行时抽象（T1/T2/T3 三级适配）上领先业界，但在事件驱动、持久化记忆和多 Agent 协作三个维度上存在结构性缺口。v1.8 的目标是补齐这三个维度，同时保持 v1.7 已有的治理优势。

---

### 2. 核心设计理念：企业 AI 协作伙伴

v1.8 的设计基于三大假设和一个核心范式转变。

#### 2.1 三大设计假设

**假设一：IM is the new Desktop**

企业 AI 的主战场从独立 App 迁移到 IM 入口（企业微信、钉钉、飞书、Slack/Teams）。这意味着：

- 交互形态是**推送式**的（Agent 主动通知），而非**拉取式**的（用户打开应用查看）
- 信息载体是**卡片**，而非**页面**。每张卡片必须在 30 秒内让用户做出决策
- 对话是**异步长期**的（事件可能持续数天），而非**同步短暂**的（一轮问答结束）
- 多端一致（IM、Web、CLI、移动端）是基本要求

**假设二：Skills 是新时代的 Apps**

能力交付单元从"功能模块"变为 Skill。传统 App 的安装→打开→操作流程被替代为 Skill 的发现→调用→执行流程。这意味着：

- Skill 必须是**可发现**的（搜索、推荐、标签分类）
- Skill 必须是**可组合**的（workflow-skill 编排多个 domain-skill）
- Skill 必须是**可治理**的（版本控制、晋升流水线、访问控制、使用分析）
- Skill 必须是**运行时无关**的（同一个 Skill 可在不同运行时上执行）

这一假设在 v1.7 中已经实现。v1.8 保持不变。

**假设三：Memory as a File System**

记忆不是向量数据库，而是"文件化资产 + 混合检索"。这意味着：

- 记忆是**结构化文件**（类比 Claude 的 CLAUDE.md），而非嵌入向量的黑盒
- 记忆可以被**人类审阅和编辑**（偏好/阈值/口径可以手动修正）
- 记忆支持**混合检索**（关键词 + 语义 + 时间衰减），而非仅向量搜索
- 记忆可以**跨 Agent 共享**（多个 Agent 服务同一个用户/事件时共享记忆）
- Agent 的每一个结论都必须**挂载证据锚点**（原始数据快照 ID），保证可追溯

这一假设是 v1.8 最大的新增，对应 L2 Memory Engine 子系统。

#### 2.2 范式转变：从"执行器"到"协作伙伴"

| 维度 | v1.7 "执行器"模式 | v1.8 "协作伙伴"模式 |
|------|---------------|----------------|
| **触发** | 用户主动发起 | 事件驱动 + 用户发起混合 |
| **生命周期** | 单次会话（分钟级） | 事件室（小时到天级） |
| **交互** | 对话流（消息列表） | 四卡置顶 + 对话流 |
| **记忆** | 会话内暂态 | 持久化记忆 + 证据链 |
| **协作** | 单 Agent | 多 Agent 并行互审 |
| **控制** | Hook deny/allow | Plan→Check→Draft→Approve→Execute |
| **主动性** | 被动 | 主动推送 + 自适应协作节奏 |

#### 2.3 五件套系统能力

v1.8 引入五个系统级能力，贯穿所有业务场景：

1. **事件化引擎** — 海量告警/日志/变更事件不以逐条消息形式呈现，而是聚类为"业务事件"对象。100 条告警可能只是一个"配电线路过载事件"。事件引擎将原始事件流转化为结构化的业务事件，并驱动后续的 Agent 协作。

2. **证据索引层** — 所有 Agent 结论必须挂载证据锚点（拓扑快照 ID、量测数据窗口、规则条款编号）。审计不只是"记录发生了什么"，而是"凭什么这样判断"。证据锚点存储在 L2 Memory Engine 中，全链路可追溯。

3. **A2A 并行互审** — 复杂场景拆分给多个专长 Agent 并行校核。继保审 + 方式审 + 安全审同时运行，结果以卡片形式汇聚呈现。克服人类交互带宽的限制（人一次只能看一个对话，但可以同时看多个审核结论卡片）。

4. **确定性校核层** — 高风险判断不依赖 LLM 的概率性推理，而是交给规则引擎或仿真工具。例如：电力五防校核、财务越限检查、合规规则匹配。校核结果挂载到证据锚点，成为证据链的一部分。

5. **审批与回滚机制** — 所有写入操作（修改配置、派发工单、发布文档）必须通过审批闸门。审批卡提供 30 秒决策所需的全部信息：动作范围 + 理由摘要 + 回滚路径。Agent 只生成草稿，不越权执行。

#### 2.4 四卡交互范式

四卡置顶是 v1.8 的核心交互创新。每个事件室顶部强制悬浮四张动态卡片，替代传统的消息列表式交互：

**① 事件卡 Event Card**
- 定位：一屏看懂"发生了什么、风险多大、下一步最小可行动作"
- 内容：一句话结论 + 影响评估（范围/等级/趋势）+ 快捷动作按钮
- 更新频率：随事件状态实时更新
- 设计原则：30 秒内让用户决定"是否需要关注"

**② 证据包 Evidence Pack**（默认折叠）
- 定位：一键追溯"凭什么这样判断"
- 内容：告警聚类摘要 + 量测数据窗口 + 拓扑版本快照 + 历史相似事件
- 每条证据挂载锚点 ID，可追溯到原始数据
- 设计原则：让审计和复盘有据可查

**③ 行动卡 Action Card**
- 定位：将 Agent 结论组织为"步骤树 + 分支条件 + 最小下一步"
- 内容：行动步骤列表，每步标注前置条件和预期结果
- 快捷动作：拉起子事件室 / @相关专业 Agent / 生成草稿
- 设计原则：不是给一个长文答案，而是给一个可执行的步骤树

**④ 审批卡 Approval Card**
- 定位：所有写入/派单/正式建议的唯一出口
- 内容：动作范围 + 理由摘要 + 回滚路径
- 操作：批准 / 拒绝 / 要求补充信息
- 设计原则：30 秒原则 — 审批人在 30 秒内能做出决策

**为什么是四张而不是更多？**

四张卡片覆盖了决策的四个认知阶段：认知（发生了什么）→ 验证（凭什么）→ 规划（怎么做）→ 授权（是否执行）。更多卡片会增加认知负担；更少卡片会遗漏关键环节。不同业务场景复用同一套四卡结构，只是卡片内容不同。

---

### 3. 一个事件的完整生命周期

以下用一个运维告警事件的完整处理过程来说明 v1.8 各层如何协作。这个叙事覆盖了从外部事件到记忆沉淀的全部环节。

#### 阶段 1：事件接入与聚合（L4 事件引擎）

```
外部系统（监控平台、SCADA、告警中心）通过 Webhook/Kafka 推送原始告警：
  - 12:01 告警：10kV 馈线 A 电流过载 (85%)
  - 12:02 告警：10kV 馈线 A 电流过载 (92%)
  - 12:03 告警：配变 T-042 温度异常 (78°C)
  - 12:04 告警：10kV 馈线 A 电流过载 (96%)
  - 12:05 告警：配变 T-042 过载保护动作

L4 事件引擎处理管线：
  接入 → 去重（同源同类 3 分钟内合并）
       → 聚类（空间相关性：同一馈线/配变）
       → 关联分析（电流过载 + 温度异常 + 保护动作 = 配变过载事件）
       → 生成事件对象：
         {
           event_id: "EVT-20260407-001",
           type: "equipment_overload",
           severity: "high",
           scope: "10kV 馈线 A / 配变 T-042",
           raw_alerts: [5 条原始告警],
           created_at: "2026-04-07T12:05:00Z"
         }
```

#### 阶段 2：事件室创建与事件卡推送（L5 + L4）

```
L4 创建事件室（EventRoom），关联事件对象。
L4 构建事件卡数据 → 通过 SSE/WebSocket 推送到 L5。
L5 渲染事件卡，推送到用户的 IM/Web/移动端：

事件卡内容：
  标题：配变 T-042 过载事件
  摘要：10kV 馈线 A 持续过载（96%），配变保护已动作
  风险等级：高
  影响范围：下游 3 个台区、约 200 户
  建议动作：[查看详情] [发起会审] [转派值班员]
```

#### 阶段 3：用户介入与会话创建（L5 → L4 → L3 → L1）

```
用户点击 [发起会审]。

L4 会话编排器：
  1. 解析事件类型 → 匹配 skill: "equipment-overload-analysis"
  2. 决定会审策略：继保审 + 方式审 + 安全审 并行
  3. 请求 L3 创建 3 个并行会话

L3 三方握手（每个会话）：
  1. 解析用户角色 + 组织单元 → 编译受管 hooks
  2. 从 L2 获取 Skill 内容
  3. 选择运行时 → L1 分配实例
  4. 构建 SessionPayload:
     {
       user_id, org_unit, managed_hooks_json,
       skill_id: "equipment-overload-analysis",
       event_context: { event_id, severity, scope },
       memory_refs: [该配变历史事件记忆引用],
       evidence_anchor_id: "EVT-20260407-001"
     }
  5. gRPC L1.Initialize(SessionPayload)

L1 AgentLoop 启动：
  构建 SystemPrompt（Skill 指令 + 事件上下文 + 历史记忆）
  → LLM 推理 → 工具调用（查询 SCADA 数据、拓扑分析）
  → Hook 评估（PreToolUse: 权限检查 / PostToolUse: 审计）
  → 循环直到形成结论
```

#### 阶段 4：Agent 分析与证据挂载（L1 + L2）

```
每个 Agent 在分析过程中：
  1. 调用工具获取数据 → L1 通过 MCP 访问外部系统
  2. 每次关键数据获取 → 向 L2 Memory Engine 写入证据锚点：
     {
       anchor_id: "ANC-001",
       type: "measurement_window",
       data_ref: "SCADA://feeder-A/current/12:00-12:10",
       snapshot_hash: "sha256:abc123..."
     }
  3. 形成结论时挂载证据：
     结论: "馈线 A 过载原因为下游负荷突增（工业用户 X 启动大功率设备）"
     证据: [ANC-001 电流曲线, ANC-002 负荷分布, ANC-003 用户档案]
```

#### 阶段 5：A2A 并行互审与结果汇聚（L4）

```
三个 Agent 并行完成后，L4 A2A 路由汇聚结论：

  继保审结论：保护正确动作，不需要调整定值
    置信度: 0.95，证据: [ANC-004 保护动作记录, ANC-005 定值校验]
    
  方式审结论：建议转移部分负荷到馈线 B
    置信度: 0.82，证据: [ANC-006 馈线 B 容量, ANC-007 转供路径]
    
  安全审结论：转供方案安全，无 N-1 风险
    置信度: 0.90，证据: [ANC-008 N-1 分析, ANC-009 断面检查]

L4 汇聚策略：
  - 冲突检测：三方结论一致（无冲突）
  - 置信度加权：综合置信度 0.89
  - 生成统一行动建议
```

#### 阶段 6：确定性校核与审批（L3 + L5）

```
L3 确定性校核器：
  转供方案 → 五防校核引擎（规则引擎，非 LLM）
  结果：通过，无安全约束违反
  校核结果写入证据锚点 ANC-010

L5 更新卡片：
  行动卡：
    步骤 1: 断开配变 T-042 与馈线 A 的联络开关
    步骤 2: 闭合馈线 B 联络开关
    步骤 3: 确认负荷转移完成
    步骤 4: 监控馈线 B 电流 30 分钟

  审批卡：
    动作范围：10kV 馈线 A/B 联络操作
    理由摘要：配变过载，转供至馈线 B（容量裕度 40%）
    回滚路径：反向操作恢复原供电方式
    校核结果：五防校核通过
    [批准] [拒绝] [要求补充]
```

#### 阶段 7：执行与监控（L4 → L3 → L1）

```
用户点击 [批准]。
L4 更新事件状态：pending_approval → executing
L3 清除审批闸门 → L1 执行操作（通过 MCP 调用 SCADA 系统）
L4 更新事件状态：executing → monitoring
L5 推送监控卡片（实时电流/温度数据）
```

#### 阶段 8：复盘与记忆沉淀（L4 → L2）

```
监控正常 30 分钟后，用户确认关闭事件。
L4 更新事件状态：monitoring → closed → retrospective

触发记忆沉淀（PostSessionEnd hook）：
  L2 Memory Engine 写入：
    1. 证据锚点索引（本次事件的完整证据链）
    2. 文件化记忆：
       - 配变 T-042 过载阈值偏好：85% 告警 / 92% 预警 / 96% 动作
       - 馈线 A→B 转供操作经验：30 分钟监控足够
    3. 复盘经验：
       - 工业用户 X 大功率设备启动是常见诱因
       - 下次可提前在用户用电计划变更时预警

这些记忆在下次类似事件发生时，由 L4 在上下文拼装阶段注入到 Agent 的 SessionPayload 中。
```

---

## 第二部分：五层架构定义

### 4. 架构总览

#### 4.1 五层模型

```
L5  协作层  Cowork Layer         人与 Agent 的协作空间
L4  编排层  Orchestration Layer  事件驱动 + 会话编排 + A2A 协调
L3  治理层  Governance Layer     策略 + 审批 + 审计 + 校核
L2  资产层  Asset Layer          Skill + MCP + Memory Engine + Ontology
L1  执行层  Execution Layer      Agent Runtime（LLM 推理 + 工具 + 沙箱）
```

**重要声明：五层是逻辑架构，不等于五个必须独立部署的物理系统。** 在早期阶段，L5 与 L4 可合并部署，L2 的各子系统可共享进程。逻辑分层的价值在于明确职责边界和变更隔离，而非强制物理拆分。

| 层级 | 职责 | 关键组件 | 状态归属 | 强制机制 |
|------|------|--------|--------|--------|
| L5 协作层 | 人与 Agent 的协作空间：事件室、四卡渲染、多端适配 | 事件室引擎、卡片渲染器、推送通道、IM Bot | 无（纯展示，数据来自 L4/L3） | 四卡强制悬浮 |
| L4 编排层 | 事件驱动编排：事件接入、会话编排、A2A 路由、触发器 | 事件引擎、会话编排器、A2A 路由、触发器管理 | 事件状态机、会话路由表 | 事件状态机约束 |
| L3 治理层 | 策略强制执行、审批闸门、审计、校核 | 策略引擎、审批闸门、审计服务、校核器、MCP 注册表、意图网关 | 策略配置、RBAC、hooks、managed-settings | 受管 hooks，"拒绝优先" |
| L2 资产层 | Skill 资产管理、MCP 编排、持久化记忆、业务本体 | Skill 仓库、MCP Orchestrator、Memory Engine、Ontology Service | 版本化：Skill 内容、元数据、记忆文件、本体模型 | Skill frontmatter hooks |
| L1 执行层 | LLM 推理→工具→Hook 评估循环 | 运行时选择器、适配器注册表、Hook 桥接器、遥测采集器 | 短暂态：对话、内核状态、文件 | 原生 hooks 或桥接 hooks |

#### 4.2 三条纵向管线

三条管线贯穿五层，保证跨层的一致性和可观测性：

**管线 A：Hook 管线**（确定性强制执行）
```
创作(L5 管理控制台) → 编译(L3 策略编译器) → 部署(L3 managed-settings)
  → 桥接(L1 Hook Bridge) → 评估(L1 内联执行) → 审计(L3 审计服务)
```
Hook 是 EAASP 最核心的治理手段。v1.8 在 v1.7 的 9 个 L1 生命周期事件基础上，新增 L4 编排层和 L5 协作层的 Hook 触发点。

**管线 B：数据流管线**（下行 + 上行）
```
下行：用户/事件(L5) → 事件聚合+意图路由+上下文拼装(L4)
  → 三方握手+策略编译(L3) → Skill+记忆注入(L2) → AgentLoop(L1)

上行：ResponseChunk流(L1) → 遥测(L1→L3) → 记忆写入(L1→L2)
  → 事件状态更新+卡片数据(L4) → 卡片渲染+推送(L5)
```

**管线 C：会话控制管线**（生命周期编排）
```
EventRoom(L5, 长生命周期)
  └─ Event(L4, 事件状态机)
       ├─ Session 1(L3→L1, 绑定 skill_id + runtime_id)
       ├─ Session 2(L3→L1, 可能不同 Skill)
       └─ Session 3(L3→L1, A2A 派发的子任务)
```
Event Room 不随单个 Session 结束而关闭。事件可以包含多个 Session，Session 之间通过 L2 Memory（而非对话历史）传递上下文。

#### 4.2.1 协作闭环（v1.8 核心叙事主线）

三条管线共同支撑一个完整的协作闭环，这是 v1.8 的核心运行模式：

```
Event → Context → Plan → Check → Draft → Approve → Execute → Observe → Retrospect → Memory
  ↑                                                                                      │
  └──────────────────── 记忆反馈下一次事件处理 ──────────────────────────────────────────────┘
```

| 阶段 | 负责层 | 产出 |
|------|--------|------|
| Event | L4 事件引擎 | 结构化事件对象 |
| Context | L4 编排器 + L2 Memory/Ontology | 完整 SessionPayload |
| Plan | L1 Agent | 操作计划（步骤树） |
| Check | L3 确定性校核器 | 校核结果 + 证据锚点 |
| Draft | L1 Agent | 可执行草稿 |
| Approve | L3 审批闸门 + L5 审批卡 | 人工决定 |
| Execute | L1 Agent | 执行结果 |
| Observe | L4 + L3 审计 | 遥测 + 证据链 |
| Retrospect | L4 编排器 | 复盘报告 |
| Memory | L2 Memory Engine | 偏好/经验/知识写入 |

**核心原则：Agent 负责形成方案，系统负责校核、授权、执行与留痕。**

#### 4.3 设计原则

**继承自 v1.7（5 条）：**

- **治理是统一的**：同一套 L3 受管 hooks 适用于 L1 中的每一种智能体
- **智能通过 Skills 流动**：L2 中的 Skills 是运行时无关的知识资产
- **Hooks 提供保障，智能体提供判断**：确定性由 hooks，推理由智能体
- **扩展，而非重建**：每个阶段增量添加组件
- **拒绝优先**：任一作用域的 hook 拒绝即阻止（Deny always wins）

**v1.8 新增（4 条）：**

- **事件驱动**：Agent 不等用户指令，由事件触发主动服务。事件引擎将原始事件流转化为结构化业务事件
- **证据链完整**：每个 Agent 结论必须挂载证据锚点，全链路可追溯（结论 → 证据包 → 原始数据快照）
- **草稿优先**：Agent 只生成草稿，不越权执行。所有写入操作必须通过审批闸门（Plan → Check → Draft → Approve → Execute）
- **记忆是资产**：记忆与 Skill 同级，是 L2 管理的持久化资产。Agent 的经验积累不应随会话消失

---

### 5. L1 执行层 Execution Layer — "手和脚"

L1 是直接与 LLM 和工具交互的层级。v1.8 对 L1 的变更较小，主要是 SessionPayload 新增字段以支持事件上下文和记忆引用。

#### 5.1 L1 内部抽象机制（继承 v1.7）

L1 的内部抽象机制实现了与智能体运行时无关的运行。L4、L3 和 L2 通过该机制与 L1 交互，而不是与具体智能体直接耦合。

四个组成部分：

- **运行时选择器 (Runtime Selector)**：从运行时池中选择运行时。支持五类选择策略：任务匹配（capability matching）、随机盲盒（blind-box）、用户偏好、A/B 测试、成本优化。
- **适配器注册表 (Adapter Registry)**：每种运行时对应一个适配器。T1 Harness（薄适配器）、T2 Aligned（中等适配器）、T3 Framework（厚适配器）。
- **Hook 桥接器 (Hook Bridge)**：确保 L3 治理对所有运行时"同样生效"。原生支持 hooks 的运行时直接加载；非原生运行时通过外部 sidecar 拦截。
- **遥测采集器 (Telemetry Collector)**：统一归一化遥测数据，推送到 L3 审计服务。

#### 5.2 运行时分层（继承 v1.7）

**Tier 1: Harness 智能体（原生兼容）**

构建在 Anthropic 基础设施之上。原生 hooks（全部生命周期事件，四类 handler）、原生 MCP、原生 skills、managed-settings 分层体系。适配器很薄。

- **EAASP Runtime**：平台参考运行时。面向企业个人助理场景，包含模式路由、意图解析器、资源协调器、执行循环。
- **Claude Code**：参考级 harness。完整 managed-settings、带 frontmatter hooks 的 skills。
- **Claude Agent SDK**：用于构建自定义 harness 智能体。

**Tier 2: Aligned 智能体（对齐型，中等适配器）**

原生无界面的 CLI 智能体。部分兼容 MCP，但不原生支持 hooks。通过 Hook Bridge 桥接。

- Aider、Goose、Roo Code CLI、Cline CLI、OpenCode

**Tier 3: Framework 智能体（框架型，厚适配器）**

基于框架构建的智能体。适配器需要把 skills 翻译为框架内部形式。

- LangGraph、CrewAI、Pydantic AI、Microsoft Agent Framework、Google ADK

#### 5.3 运行时接口契约（继承 v1.7）

任何运行时加入运行时池，都必须（通过其适配器）实现以下 12 个方法：

| 方法 | 级别 | 描述 |
|------|------|------|
| initialize(payload) | MUST | 接收会话初始化载荷，返回会话句柄 |
| send(message) | MUST | 接收用户消息或结构化意图，返回流式响应 |
| loadSkill(content) | MUST | 加载 workflow-skill 内容，激活作用域 hooks |
| onToolCall / onToolResult | MUST | 发出 PreToolUse/PostToolUse 事件 |
| onStop() | MUST | 发出 Stop 事件，支持 exit-2 阻断机制 |
| getState / restoreState | MUST/SHOULD | 序列化/恢复会话状态 |
| connectMCP(servers) | MUST | 连接 MCP 服务器 |
| emitTelemetry() | MUST | 发出标准化遥测事件 |
| getCapabilities() | MUST | 返回能力清单 |
| terminate() | MUST | 清理资源，发出 SessionEnd |

#### 5.4 单实例内部组成（继承 v1.7）

每个运行时实例包含五个内部子系统：

- **会话引导平面**：上下文注入器、hook 加载器、系统提示词组装器
- **推理核心**：LLM 或框架引擎，维护对话上下文与内核状态
- **Hook 拦截层**：PreToolUse → 执行 → PostToolUse → Stop
- **工具执行平面**：MCP 客户端、计算沙箱、Web 获取、输出渲染器
- **记忆与上下文平面**：对话记忆、内核状态、工作文件系统

#### 5.5 v1.8 变更：SessionPayload 扩展

v1.8 对 L1 的唯一结构性变更是 SessionPayload 新增三个字段：

```
SessionPayload {
  // --- v1.7 已有 ---
  user_id, org_unit, managed_hooks_json,
  skill_ids, skill_registry_url,
  
  // --- v1.8 新增 ---
  event_context: {          // 事件上下文（由 L4 事件引擎提供）
    event_id,               // 关联的事件 ID
    event_type,             // 事件类型
    severity,               // 严重程度
    scope,                  // 影响范围
    raw_summary             // 原始告警/事件摘要
  },
  memory_refs: [            // 记忆引用（由 L4 从 L2 Memory 查询后注入）
    { memory_id, type, relevance_score }
  ],
  evidence_anchor_id: str   // 证据锚点 ID（Agent 结论将挂载于此）
}
```

L1 运行时不需要理解这些字段的业务含义——它们会被注入到 SystemPrompt 中，由 LLM 在推理过程中使用。

#### 5.6 v1.8 变更：记忆写入通道

v1.8 新增 L1 → L2 的记忆写入通道。Agent 在执行过程中可以向 L2 Memory Engine 写入三类数据：

- **证据锚点**：工具调用获取的关键数据快照
- **结论记忆**：Agent 形成的结论及其证据引用
- **偏好/经验**：用户偏好、操作经验等长期记忆

写入通过 HTTP REST 调用 L2 Memory Engine API，由 PostToolUse hooks 或 Stop hooks 触发。

---

### 6. L2 资产层 Asset Layer — "工具箱、记忆和知识"

L2 在 v1.7 基础上新增 Memory Engine 和 Ontology Service 两个子系统，从"技能资产层"扩展为"技能、记忆与知识资产层"。L2 包含四个子系统：Skill Registry（能力资产）、MCP Orchestrator（工具连接）、Memory Engine（经验与证据）、Ontology Service（业务知识模型）。

#### 6.1 Skill Registry（继承 v1.7）

Skill 资产的全生命周期管理：存储、发现、校验、晋升、访问控制、依赖解析、版本管理、使用分析。

六个运营服务：
- **Skill 仓库 (MCP server)**：七个 MCP 工具（search/read/versions/submit_draft/promote/dependencies/usage）
- **晋升引擎**：四阶段流水线（draft → tested → reviewed → production），受 L3 角色治理
- **访问控制服务**：组织范围 RBAC，与 L3 协同
- **依赖解析器**：Skill 间依赖追踪与兼容性校验
- **版本管理器**：语义化版本、分支、回滚
- **使用分析引擎**：调用频次、成功率、质量指标

四类 Skill（继承 v1.7）：
- **Workflow skills**：业务流程编码为智能作战手册
- **Production skills**：输出类型的最佳实践
- **Domain skills**：专业领域知识
- **Meta skills**：关于 Skill 本身的 Skill

Skill 内容结构（继承 v1.7）：
- YAML frontmatter（作用域 hooks、运行时亲和性、访问范围、依赖、元数据）
- 文字说明（Prose instructions，面向智能体的自然语言指令）
- 运行时亲和性声明（preferred_runtime / compatible_runtimes / 无声明）

#### 6.2 MCP Orchestrator（继承 v1.7 + 扩展）

MCP（Model Context Protocol）是 EAASP 的通用集成协议，使所有智能体以一致方式访问外部服务和内部平台能力。

**继承 v1.7 的核心职责：**
- 外部工具发现与连接池管理（Gmail、Calendar、Drive、ERP、CRM 等）
- Skill 可声明依赖的 MCP Server，由 MCP Orchestrator 在会话创建时预连接
- 连接器生命周期管理：注册、健康检查、凭据轮换、故障降级
- L3 MCP 注册表协同：连接器的启用/禁用受 L3 策略治理

**v1.8 扩展：**
- Memory Engine 自身作为 MCP Server 暴露，使 L1 运行时可通过标准 MCP 协议读写记忆
- Ontology Service 自身作为 MCP Server 暴露，使 Agent 可查询业务本体
- 事件引擎的事件源接入可通过 MCP 连接器实现（如企业监控系统的 MCP 适配器）
- MCP 连接器健康状态纳入事件引擎的事件源可用性监控

#### 6.3 Memory Engine（★ v1.8 新增核心子系统）

Memory Engine 是 v1.8 最大的新增子系统。它管理 Agent 的持久化记忆，使 Agent 能积累经验、追溯证据、跨会话引用。

**为什么放在 L2：** 记忆是一种"资产"，和 Skill 同级。L1 执行时按需读写，L4 编排时按需查询。作为独立服务，所有层可调用。对标 Google Vertex AI Memory Bank 的定位。

**核心设计约束：文件是治理对象，索引是检索对象，二者同源，不可分裂。** 每条记忆同时存在文件化视图（人可审阅、编辑、晋升、归档、做权限隔离）和索引化视图（关键词 + 语义 + 时间衰减混合检索）。两个视图基于同一份源数据，不允许出现"索引中存在但文件中找不到"的状态。

**三层存储架构：**

**第一层：证据锚点库 Evidence Anchor Store**

```
每个 Agent 结论必须挂载数据快照 ID，保证可追溯。

证据锚点对象：
{
  anchor_id: string,        // 全局唯一标识
  event_id: string,         // 关联的事件
  session_id: string,       // 产生该锚点的会话
  type: enum {              // 锚点类型
    measurement_window,     // 量测数据窗口
    topology_snapshot,      // 拓扑版本快照
    rule_reference,         // 规则条款引用
    document_excerpt,       // 文档摘要
    computation_result      // 计算/仿真结果
  },
  data_ref: string,         // 原始数据引用（URI）
  snapshot_hash: string,    // 数据完整性哈希
  source_system: string,    // 来源系统标识
  tool_version: string,     // 调用工具版本（保证可复现）
  model_version: string,    // LLM 模型版本（如适用）
  rule_version: string,     // 规则版本（确定性校核时）
  created_at: timestamp,
  created_by: string,       // Agent/Session ID
  metadata: object          // 扩展元数据
}

API:
  POST /api/v1/memory/anchors          — 创建证据锚点
  GET  /api/v1/memory/anchors/{id}     — 获取锚点详情
  GET  /api/v1/memory/anchors?event_id= — 按事件查询锚点链
```

**第二层：文件化记忆 Memory Files**

```
偏好/阈值/复盘经验沉淀为结构化文件。
类比 Claude 的 CLAUDE.md — Agent 的长期记忆。

记忆文件对象：
{
  memory_id: string,
  scope: enum {             // 记忆归属范围
    user,                   // 用户级（个人偏好）
    team,                   // 团队级（团队约定）
    org_unit,               // 组织单元级
    event_type              // 事件类型级（同类事件经验）
  },
  category: enum {
    preference,             // 偏好（阈值、格式、风格）
    experience,             // 经验（操作复盘、教训）
    knowledge,              // 知识（领域规则、惯例）
    calibration             // 校准（口径、标准、约定）
  },
  content: string,          // 结构化文本内容
  evidence_refs: [string],  // 关联的证据锚点 ID
  version: int,             // 版本号（可编辑）
  last_updated: timestamp,
  updated_by: string        // 人工或 Agent
}

设计要点：
  - 记忆文件可以被人类审阅和编辑（管理控制台或 API）
  - 每次修改产生新版本，支持回滚
  - Agent 写入的记忆标注为"Agent 建议"，人工确认后标注为"已确认"

API:
  POST   /api/v1/memory/files           — 创建记忆文件
  GET    /api/v1/memory/files/{id}      — 读取记忆
  PUT    /api/v1/memory/files/{id}      — 更新记忆（人工或 Agent）
  GET    /api/v1/memory/files?scope=&q= — 搜索记忆
  DELETE /api/v1/memory/files/{id}      — 归档记忆
```

**第三层：索引库 Memory Index**

```
混合检索引擎，支持三种检索模式的组合：
  - 关键词检索（精确匹配设备编号、事件类型等）
  - 语义检索（向量相似度，用于模糊匹配"类似事件"）
  - 时间衰减（近期记忆权重更高）

检索 API:
  POST /api/v1/memory/search
  {
    query: string,              // 自然语言查询
    scope: [user/team/org],     // 范围过滤
    category: [preference/...], // 类别过滤
    time_range: { from, to },   // 时间范围
    limit: int,
    search_mode: enum {
      keyword,                  // 纯关键词
      semantic,                 // 纯语义
      hybrid                    // 混合（默认）
    }
  }
  返回：按相关性排序的记忆列表，每条附带 relevance_score

跨 Agent 共享规则：
  - 同一 event_id 下的所有 Session 可读写同一组记忆
  - scope=user 的记忆对该用户的所有 Agent 可见
  - scope=team 的记忆对该团队的所有成员可见
  - 写入冲突：后写优先 + 版本号乐观锁
```

#### 6.4 Ontology Service（★ v1.8 新增）

Ontology Service 是 L2 的第四个子系统，负责**企业业务本体**与 EAASP 平台的集成。EAASP 不自建本体，而是定义与外部本体服务的集成契约，确保 Agent 在执行过程中能查询和利用企业的结构化业务知识。

**设计理念：** 企业业务本体（Ontology）借鉴 Palantir 的本体论思想，将企业的核心业务对象（设备、人员、流程、组织、规则）及其关系建模为结构化的知识图谱。本体由专门的本体服务方构建和维护，EAASP 通过标准化接口消费。

**EAASP 平台应提供的能力：**

```
1. 本体查询接口（MCP Server 形式暴露给 L1 运行时）
   - ontology_query: 按类型、属性、关系查询业务对象
   - ontology_traverse: 沿关系路径遍历（如：设备→所属馈线→所属变电站）
   - ontology_context: 为指定业务对象返回上下文包（相关联的对象集合）

2. 上下文拼装集成
   - L4 事件引擎在上下文拼装阶段，根据事件涉及的业务对象，
     从 Ontology 查询关联上下文注入 SessionPayload
   - 例：配变过载事件 → 查询该配变的上下游拓扑、历史事件、关联用户

3. 证据锚点关联
   - 证据锚点可引用本体对象 ID 作为 data_ref
   - 例：anchor.data_ref = "ontology://transformer/T-042/topology/v3"

4. Skill 本体依赖声明
   - Skill 可在 frontmatter 中声明依赖的本体类型
   - 例：equipment-overload-analysis skill 依赖 "power_grid" 本体
```

**本体服务方必须提供的能力（集成契约）：**

```
1. 标准查询 API（REST 或 MCP 协议）
   - 按对象类型查询（GET /ontology/objects?type=transformer）
   - 按关系遍历（GET /ontology/objects/{id}/relations?type=feeds）
   - 全文搜索（POST /ontology/search）

2. 模式发布（Schema Publication）
   - 发布本体的类型定义（对象类型、属性、关系类型）
   - EAASP 据此校验 Skill 的本体依赖是否可满足

3. 版本管理
   - 本体模型变更时提供版本标识
   - 支持按版本查询（保证证据锚点引用的一致性）

4. 访问控制对接
   - 与 EAASP L3 的 RBAC 协同，按用户角色控制本体查询范围
   - 某些敏感业务对象（如人员薪资）需要额外权限
```

**为什么放在 L2：** 本体是一种"知识资产"，与 Skill（能力资产）和 Memory（经验资产）同级。Agent 在执行时按需查询本体获取业务上下文，L4 在编排时查询本体进行事件关联分析。本体服务的治理（访问控制、审计）由 L3 负责。

#### 6.5 四大子系统的协作关系

```
Skill Registry ←→ Memory Engine：
  Skill 可声明依赖的记忆类型（如 domain-skill 依赖特定领域的知识记忆）
  Skill 执行结束后的记忆沉淀由 PostSessionEnd hook 触发

Skill Registry ←→ Ontology Service：
  Skill 可声明依赖的本体类型（如 equipment-analysis skill 依赖 power_grid 本体）
  Ontology 提供 Skill 执行所需的业务上下文

MCP Orchestrator ←→ Memory Engine：
  Memory Engine 作为 MCP Server 暴露给 L1 运行时
  Agent 在执行过程中通过 MCP 协议读写记忆

MCP Orchestrator ←→ Ontology Service：
  Ontology Service 作为 MCP Server 暴露给 L1 运行时
  Agent 在执行过程中通过 MCP 协议查询业务本体

Memory Engine ←→ Ontology Service：
  证据锚点可引用本体对象 ID（data_ref 指向本体实体）
  记忆文件中的知识类记忆可与本体类型关联

Skill Registry ←→ MCP Orchestrator：
  继承 v1.7，Skill 可声明依赖的 MCP Server
```

---

### 7. L3 治理层 Governance Layer — "规则和安全"

L3 在 v1.7 基础上新增审批闸门控制链、确定性校核器和证据链管理。

#### 7.1 策略引擎（继承 v1.7）

将组织策略翻译为受管 hooks。支持分层策略作用域（企业 → 事业部 → 部门 → 团队），较低层级可以收紧但不能放松较高层级的策略。

#### 7.2 策略编译器（继承 v1.7）

将人类可读的策略翻译为可执行的 hook JSON 配置 + 强制执行工件。四类 handler 输出：command（shell 脚本）、http（端点 URL）、prompt（提示词）、agent（子智能体配置）。

#### 7.3 Hook 部署服务（继承 v1.7）

通过策略部署 API 以原子方式分发 managed-settings.json。版本追踪、回滚、差异管理。

#### 7.4 审计服务（继承 + 扩展）

继承 v1.7 的遥测采集和结构化事件存储。

v1.8 扩展：
- 审计事件新增 evidence_anchor_ids 字段，关联证据锚点
- 审计查询支持"证据链追溯"：从结论反查所有关联证据
- 审计事件过滤和持久化增强

#### 7.5 MCP 注册表 + 意图网关（继承 v1.7）

MCP 注册表管理连接器生命周期。意图网关验证来源、策略授权、Skill 解析。

#### 7.5.1 MCP 安全分级（★ v1.8 新增）

MCP 连接器必须按安全等级分级管理，尤其在高风险企业场景（如电网、金融）中：

```
安全分级：
  - 只读连接器（无副作用）：查询类工具，可自动授权
  - 写入连接器（有副作用）：修改类工具，需策略授权或审批
  - 关键连接器（高风险副作用）：操作设备/资金/权限类，必须走审批闸门

治理要求：
  - 工具必须声明副作用类型（side-effect classification）
  - 连接器凭据应尽量短时化（按会话颁发、会话结束即失效）
  - 最小权限原则：Agent 只获得当前 Skill 所需的连接器权限
  - 高风险场景保留三重约束：出站网络白名单 + 工具白名单 + 操作级审批
```

#### 7.6 审批闸门控制链（★ v1.8 新增）

v1.8 引入五阶控制链，替代 v1.7 简单的 deny/allow 二元决策：

```
Plan（只读计划）
  → Check（确定性校核）
    → Draft（草稿生成）
      → Approve（人工审批）
        → Execute（执行）
```

**阶段定义：**

| 阶段 | 触发条件 | 产出 | 出口条件 |
|------|---------|------|---------|
| Plan | Agent 识别需要执行操作 | 操作计划（步骤树） | 自动进入 Check |
| Check | Plan 产出后 | 确定性校核结果 | 校核通过→Draft；失败→回退 Plan |
| Draft | Check 通过 | 可执行的草稿（配置变更/工单/文档） | Agent 自动产出 |
| Approve | Draft 产出后（高风险操作） | 审批决定 | 人工批准→Execute；拒绝→回退 |
| Execute | Approve 通过 | 实际执行结果 | 执行完成或回滚 |

**风险分级策略：**
- 低风险操作（只读查询、信息汇总）：Agent 自主完成，不进入控制链
- 中风险操作（生成报告、建议修改）：到 Draft 阶段，推送通知但不强制审批
- 高风险操作（修改配置、派发工单、操作设备）：必须走完整控制链

风险等级由 L3 策略引擎根据操作类型、影响范围和组织策略自动判定。

#### 7.7 确定性校核器（★ v1.8 新增）

高风险判断不依赖 LLM 的概率性推理，而是交给规则引擎或仿真工具：

```
校核器接口：
  POST /api/v1/governance/verify
  {
    check_type: string,      // 校核类型（如 "five_prevention", "limit_check"）
    plan: object,            // 待校核的操作计划
    context: object          // 校核所需的上下文数据
  }
  返回：
  {
    result: "pass" | "fail" | "warning",
    details: string,         // 详细说明
    evidence_anchor_id: str, // 校核结果作为证据锚点
    rules_applied: [string]  // 命中的规则列表
  }

校核器类型（可扩展）：
  - 规则引擎校核：业务规则匹配（如五防校核、越限检查）
  - 仿真校核：通过仿真工具验证方案可行性
  - 合规校核：法规/标准合规性检查
  - 数值校核：计算结果交叉验证

关键设计决策：
  校核器是确定性的 — 相同输入永远产生相同输出。
  校核结果自动写入证据锚点，成为证据链的一部分。
  校核不通过则操作计划无法进入 Draft 阶段。
```

#### 7.8 证据链管理（★ v1.8 新增）

每个 Agent 输出必须关联证据锚点 ID。审计不只是"记录发生了什么"，而是"为什么这样判断"。

```
证据链追溯路径：
  Agent 结论 → 证据包(多个锚点) → 原始数据快照

L3 证据链管理职责：
  1. 验证：检查 Agent 输出是否挂载了必要的证据锚点
  2. 完整性：校验证据锚点引用的数据快照是否存在且完整
  3. 审计查询：支持从结论反查完整证据链
  4. 合规报告：生成按证据链组织的合规审计报告

与 L2 Memory Engine 的关系：
  证据锚点的存储和检索由 L2 负责。
  证据链的验证、完整性校验和审计查询由 L3 负责。
  L3 定义规则（"什么输出必须挂载证据"），L2 提供存储。
```

---

### 8. L4 编排层 Orchestration Layer — "大脑"

L4 是 v1.8 变更最大的层级。从 v1.7 的"会话管理器"升级为"事件驱动编排引擎"。

#### 为什么拆分 v1.7 的 L4

v1.7 的 L4（人机协作层）承载了过多职责——它同时是用户界面（体验平面）、事件接入（集成平面）、会话调度（控制平面）和状态存储（持久化平面）。这种设计在 v1.7 的"请求→响应"模式下可行，但当 v1.8 引入事件驱动和长生命周期事件室后，"用户界面渲染"和"事件编排调度"的关注点差异变得不可调和：

- **生命周期不同**：事件室（天级）vs 会话（分钟级）vs 卡片渲染（秒级）
- **扩展维度不同**：渲染按并发用户数扩展，编排按事件吞吐量扩展
- **变更频率不同**：前端交互频繁迭代，编排逻辑相对稳定

因此 v1.8 将 v1.7 的 L4 拆分为两层：

| v1.7 L4 平面 | v1.8 归属 | 拆分理由 |
|-------------|----------|---------|
| 体验平面（门户、流程设计器、管理控制台） | → **L5 协作层**（事件室+四卡渲染+多端适配） | 用户交互关注点独立 |
| 集成平面（事件总线、API 网关、连接器织网） | → **L4 编排层**（事件引擎+触发器） | 升级为事件驱动引擎 |
| 控制平面（会话管理器、可观测性枢纽） | → **L4 编排层**（会话编排器+A2A 路由） | 升级为 1:N 编排 |
| 持久化平面（用户DB、执行日志、会话存储、成本台账） | → **L4**（事件/会话状态）+ **L2**（记忆类数据） | 记忆是资产，归 L2 |
| 安全架构（SSO/MFA、密钥库、提示注入防御） | → 继承，分布在 L4(认证)+L3(策略)+L1(防御) | 职责不变 |
| 成本治理（配额、归因、预算告警） | → 继承在 L4 编排层 | 见下文 8.4 |
| 多租户与组织拓扑 | → 继承在 L4 编排层 | 见下文 8.5 |

#### 8.1 事件引擎 Event Engine（★ v1.8 新增核心组件）

事件引擎是 v1.8 最核心的差异化组件。它使 Agent 从"被动等指令"变为"事件驱动主动服务"。

**事件源接入：**

```
支持的事件源类型：
  - Webhook：来自监控平台、CRM、HRIS、工单系统等
  - 消息队列：Kafka/NATS 消费（高吞吐场景）
  - CDC：数据库变更事件（数据库触发器、流式处理）
  - 定时任务：cron 表达式触发的周期性检查
  - 用户消息：用户通过 L5 主动发起的请求
  - A2A 调用：其他 Agent 发起的协作请求

每个事件源接入需要：
  - 签名校验（Webhook 签名、消息队列认证）
  - 去重（事件 ID 或内容哈希防止重复处理）
  - 格式化为结构化意图（统一事件对象格式）
```

**事件处理管线：**

```
接入 → 去重 → 聚类 → 关联分析 → 生成事件对象

去重规则：
  - 同源同类事件在可配置时间窗口内（默认 3 分钟）合并
  - 使用事件 ID 或内容哈希判定重复

聚类规则：
  - 空间关联：同一设备/区域/系统的事件聚为一组
  - 时间关联：时间窗口内的相关事件聚为一组
  - 因果关联：已知因果关系链上的事件聚为一组
  - 聚类算法可配置（规则引擎 或 ML 模型）

关联分析：
  - 跨源关联：监控告警 + 工单 + 用户反馈 = 同一事件
  - 升级判定：多个低优先级事件聚合后可能升级为高优先级事件
```

**事件对象模型：**

```
Event {
  event_id: string,            // 全局唯一标识
  type: string,                // 事件类型（可枚举 + 自定义）
  severity: enum { low, medium, high, critical },
  scope: string,               // 影响范围描述
  source_events: [{            // 原始事件引用
    source: string,            // 事件源
    raw_id: string,            // 原始事件 ID
    timestamp: datetime,
    payload: object            // 原始载荷
  }],
  status: enum {               // 事件状态机（见下文）
    received, triaging, active,
    pending_approval, executing,
    monitoring, closed, retrospective
  },
  event_room_id: string,       // 关联的事件室 ID
  sessions: [string],          // 关联的会话 ID 列表
  evidence_anchors: [string],  // 关联的证据锚点
  created_at: datetime,
  updated_at: datetime,
  closed_at: datetime | null,
  metadata: object
}
```

**事件状态机：**

```
received → triaging → active → pending_approval → executing
                                                       ↓
                                            monitoring → closed → retrospective

状态转换条件：
  received → triaging:     事件引擎完成聚类和关联分析
  triaging → active:       事件卡已推送，用户或 Agent 开始处理
  active → pending_approval: Agent 产出行动计划，进入审批闸门
  pending_approval → executing: 人工审批通过
  pending_approval → active:    人工拒绝，要求修改
  executing → monitoring:  操作执行完成，进入监控期
  monitoring → closed:     监控期正常，用户确认关闭
  closed → retrospective:  触发复盘和记忆沉淀
```

#### 8.2 会话编排器 Session Orchestrator

从 v1.7 的会话管理器升级。核心变化：从 1:1（用户消息:会话）变为 1:N（事件:会话）。

```
编排规则：
  - 一个 Event Room 可关联多个 L3 Session
  - 意图变化 → 创建新 Session（保留旧 Session 上下文通过 L2 Memory）
  - Agent 请求子任务 → A2A 派发新 Session
  - 审批闸门触发 → Session 暂停，等待 L5 人工确认
  - 所有 Session 完成 → Event 进入 monitoring/closed

上下文传递策略：
  Session 1 输出 → L2 Memory Engine 写入记忆
  Session 2 启动 → L4 从 L2 查询相关记忆注入 SessionPayload
  不传对话历史，传结构化记忆

自适应协作节奏：
  简单操作（查询、汇总） → Agent 自主完成，推送结果卡片
  需要判断（选方案、做预测） → Agent 产出行动卡，请求人类输入
  敏感操作（修改、派单、操作设备） → 走审批闸门控制链
  
  协作节奏由 L3 策略引擎根据操作类型和组织策略决定。
```

#### 8.3 A2A 路由 Agent-to-Agent（★ v1.8 新增）

多 Agent 并行互审的核心组件。

**关键约束：单 Agent 优先，A2A 升级触发。** EAASP v1.8 的默认执行模式是单 Agent 完成任务。只有满足以下条件之一时，L4 编排层才将任务升级为 A2A 并行互审：

- 需要**专业分工互审**（如继保审 + 方式审 + 安全审各需不同领域专长）
- 需要**并行取证降低时延**（多个数据源同时查询）
- 需要**不同信任域或工具权限隔离**（某些工具只有特定 Agent 有权访问）
- 需要**人类看到分歧**而非只看到一个结论（风险决策场景）

A2A 升级条件由 L3 策略定义，L4 编排层执行判定。

**L4 编排层的四个一级对象：**

```
Event:      事件对象（事件引擎产出）
Room:       协作空间对象（挂载多 Session、多结论、多审批节点）
Session:    一次 Agent 执行对象（绑定 Skill + Runtime）
ReviewSet:  多 Agent 互审对象（管理并行 Session 集合 + 结论汇聚）
```

ReviewSet 是 A2A 场景的核心编排单元：它由 L4 创建，包含一组并行 Session、每个 Session 的结论、以及汇聚后的统一建议。

```
并行互审调度：
  一件事可拆分给多个专长 Agent 并行校核。
  例：设备故障 → 继保审 + 方式审 + 安全审 并行运行
  
  调度策略：
    1. 事件类型 → 匹配必需的审核维度（由 L3 策略定义）
    2. 每个维度 → 选择对应的 Skill + 运行时
    3. 创建 ReviewSet → 并行创建 Session → 等待全部完成 → 汇聚结果

A2A 通信协议：
  对齐 Google/Salesforce 的 Agent-to-Agent 标准方向。
  
  Agent 能力发布：
    每个可用的 Agent（Skill + Runtime 组合）发布能力描述：
    {
      agent_id: string,
      capabilities: [string],    // 能力标签
      skill_id: string,
      runtime_tier: T1/T2/T3,
      avg_latency: duration,
      quality_score: float
    }
  
  协作请求：
    POST /api/v1/orchestration/a2a/dispatch
    {
      event_id: string,
      required_capabilities: [string],  // 需要的审核维度
      parallel: boolean,                // 是否并行
      timeout: duration,
      context: object                   // 共享上下文
    }

结果汇聚：
  多 Agent 结论合并为统一行动建议：
  
  汇聚策略：
    1. 冲突检测：多个 Agent 结论是否矛盾
    2. 置信度加权：每个结论的置信度加权合并
    3. 最终建议：
       - 一致 → 合并为统一行动卡
       - 冲突 → 标注分歧，请求人类裁决
       - 部分完成 → 标注已完成和待完成部分
```

#### 8.4 成本治理（继承 v1.7 + 扩展）

v1.7 的成本治理机制在 v1.8 中完整继承，且在事件驱动模型下更为重要——Agent 主动触发意味着成本更不可控，需要更精细的管控。

```
继承 v1.7：
  - 配额管理：按用户/团队/部门的限制，由 SessionStart hooks 检查并强制执行
  - 成本归因：遥测按组织单元聚合，支持按部门分摊计费
  - 预算告警：基于阈值的通知，由可观测性服务触发

v1.8 扩展：
  - 事件触发的成本控制：EventReceived hook 可检查事件处理预算
  - A2A 并行互审的成本预估：调度前评估多 Agent 并行的总成本
  - 记忆存储成本：Memory Engine 的存储用量纳入成本台账
```

#### 8.5 多租户与组织拓扑（继承 v1.7）

v1.7 的组织层级建模（企业 → 事业部 → 部门 → 团队）在 v1.8 中完整继承。策略可在任一层级定义，较低层级只能收紧不能放松。v1.8 的事件引擎在事件路由时同样遵循组织拓扑：事件根据影响范围路由到对应组织单元的值班人员。

#### 8.6 安全架构（继承 v1.7）

v1.7 定义的安全机制在 v1.8 中完整继承，分布在各层：
- **L4/L5 认证**：企业 SSO + MFA（人类用户），OAuth 客户端凭据（系统触发）
- **L3 密钥库**：HashiCorp Vault / AWS Secrets Manager 管理所有凭据
- **L1 提示注入防御**：所有外部输入在进入 Agent 前经过清洗
- **审计完整性**：L4 管理操作审计 + L3 执行审计，v1.8 新增证据链审计

---

### 9. L5 协作层 Cowork Layer — "人与 Agent 的协作空间"

L5 是 v1.8 全新的层级。它是用户直接接触的层，可以是企业 IM 插件、Web Portal、CLI 等多种前端。

**关键设计决策：L5 本身不含业务逻辑。** 所有数据来自 L4（事件 + 状态）和 L3（策略 + 审计）。L5 只负责渲染和用户交互。

#### 9.1 事件室 Event Room

```
核心概念：
  一事一室 — 每个业务事件自动生成独立协作空间。
  
  事件室包含：
    - 对话流（Agent 和用户的交互历史）
    - 四卡置顶（实时更新的状态卡片）
    - 关联文档（相关 Skill 产出物、历史报告）
    - 事件时间线（从接入到关闭的完整时间轴）
    - 参与者列表（涉及的用户和 Agent）
  
  类比：GitHub Issue + Slack Thread + Dashboard 的合体
  
  生命周期：
    事件室随 L4 事件创建时自动创建。
    不随单个 Session 结束而关闭。
    跟随事件状态机：received → ... → retrospective。
    复盘完成后归档（可查询但不可修改）。
```

#### 9.2 四卡置顶渲染

**关键约束：四卡是衍生视图（投影），不是事实源。** L5 不持有任何业务真状态，所有卡片数据来自下层：

| 卡片 | 数据源 | 源层 |
|------|--------|------|
| 事件卡 | Event 状态机 + 事件摘要 | L4 事件引擎 |
| 证据包 | Evidence Anchor 集合 | L2 Memory Engine |
| 行动卡 | Plan 对象 + Check 校核结果 | L4 编排 + L3 校核器 |
| 审批卡 | Approval 对象 + 回滚路径 | L3 审批闸门 |

这一约束保证多端渲染（Web/IM/CLI/移动端）时数据一致性——所有端读取同一份源数据，只是渲染方式不同。

```
卡片数据结构：

EventCard {
  title: string,
  summary: string,           // 一句话结论
  severity: enum,
  impact: {
    scope: string,           // 影响范围
    level: string,           // 影响程度
    trend: "rising" | "stable" | "declining"
  },
  quick_actions: [{          // 快捷动作按钮
    label: string,
    action_type: enum { view_detail, start_review, delegate, acknowledge }
  }],
  updated_at: datetime
}

EvidencePack {
  collapsed: boolean,        // 默认折叠
  anchors: [{
    anchor_id: string,
    type: string,
    summary: string,
    data_preview: object     // 可视化预览数据
  }],
  total_count: int
}

ActionCard {
  steps: [{
    step_id: string,
    description: string,
    precondition: string | null,
    expected_result: string,
    status: "pending" | "in_progress" | "completed" | "blocked",
    sub_steps: [...]         // 递归步骤树
  }],
  quick_actions: [{
    label: string,
    action_type: enum { create_sub_room, mention_agent, generate_draft }
  }]
}

ApprovalCard {
  approval_id: string,
  action_scope: string,      // 动作范围描述
  rationale: string,         // 理由摘要
  rollback_path: string,     // 回滚路径
  verification_result: {     // 确定性校核结果
    status: "pass" | "fail" | "warning",
    details: string
  },
  actions: ["approve", "reject", "request_info"],
  deadline: datetime | null  // 审批截止时间
}
```

#### 9.3 推送通道

```
实时推送：
  - SSE / WebSocket：Web Portal、移动端的实时流式更新
  - 卡片更新推送：事件卡/行动卡/审批卡状态变化

异步推送：
  - IM Bot：企业微信/钉钉/飞书/Slack/Teams 消息推送
    卡片化消息 + 按钮交互（IM 原生能力）
  - 邮件：审批通知、每日事件摘要

推送策略：
  - 紧急（critical）：全通道立即推送
  - 重要（high）：IM + 应用内推送
  - 一般（medium）：应用内推送，IM 汇总推送
  - 低优先级（low）：仅应用内，不主动打扰
  
  用户可配置静默时段和渠道偏好。
```

#### 9.4 多端适配

```
Web Portal：完整四卡 + 事件室 + 对话流 + 时间线
IM 插件：卡片化消息 + 按钮交互（简化版四卡）
CLI：结构化文本输出 + 命令行交互
移动端：简化卡片 + 推送通知 + 快速审批

L5 提供适配器接口，不限定具体前端技术。
每种前端实现相同的数据模型，渲染方式不同。
```

---

## 第三部分：跨层机制

### 10. Hook 管线（纵向机制 A）

Hook 是 EAASP 最核心的治理手段，贯穿全部五层。v1.8 继承 v1.7 的完整 Hook 架构，新增 L4 和 L5 的 Hook 触发点。

#### 10.1 Hook 生命周期（继承 v1.7）

```
创作(L5 管理控制台) → 编译(L3 策略编译器) → 部署(L3 managed-settings)
  → 桥接(L1 Hook Bridge) → 评估(L1 内联执行) → 审计(L3 审计服务)
```

#### 10.2 各层 Hook 触发点

**L1 执行层（9 个生命周期事件，继承 v1.7）：**
- SessionStart / UserPromptSubmit / PreToolUse / PostToolUse
- PostToolUseFailure / PermissionRequest / Stop / SubagentStop / PreCompact

**L3 治理层（v1.8 新增 2 个）：**
- PrePolicyDeploy — 策略部署前（格式/冲突检查）
- PreApproval — 审批前（自动校核触发）

**L4 编排层（★ v1.8 新增 3 个）：**
- EventReceived — 事件接入时（过滤/优先级调整/路由修改）
- PreSessionCreate — 创建会话前（配额检查/权限验证/运行时选择策略覆盖）
- PostSessionEnd — 会话结束后（记忆沉淀触发/遥测汇总/事件状态更新）

#### 10.3 Hook 作用域层级（继承 v1.7）

四个作用域层级，deny always wins：
1. **受管 hooks**（企业级）— 最高优先级，不可覆盖
2. **Skill 作用域 hooks**（frontmatter）— Skill 执行期间生效
3. **项目 hooks** — 团队共享
4. **用户 hooks** — 个人偏好

#### 10.4 四类 Handler 类型（继承 v1.7）

| 类型 | 机制 | 时延 |
|------|------|------|
| command | Shell 脚本，从 stdin 接收 JSON | 毫秒级 |
| http | POST 到 HTTP 端点 | ~100-500ms |
| prompt | 向 LLM 发送提示词评估 | ~1-3 秒 |
| agent | 拉起子智能体做深度验证 | ~5-30 秒 |

#### 10.5 Hooks 保证：拒绝优先（继承 v1.7）

任一作用域的 hook 拒绝了某个动作，无论其他 hooks 如何决定，该动作都会被阻止。这是确定性强制执行的根本保证。

---

### 11. 数据流管线（纵向机制 B）

#### 11.1 下行流（用户/事件 → Agent 执行）

```
L5  用户消息 / 外部事件
     │
L4  事件引擎聚合 → 意图路由 → 上下文拼装（从 L2 Memory 拉取相关记忆）
     │  输出：EventContext { event_id, skill_id, user_context, memory_refs[] }
     │
L3  三方握手：
     │  1. GET L2/skills/{id} → SkillContent
     │  2. 编译策略 → managed_hooks_json
     │  3. 选运行时 → runtime_endpoint
     │  4. gRPC L1.Initialize(SessionPayload)
     │
L2  Skill 内容 + 记忆上下文 注入 SessionPayload
     │
L1  AgentLoop 自主执行：
       构建 SystemPrompt（Skill prose + 事件上下文 + 记忆）
       LLM 推理 → 工具调用 → Hook 评估 → 循环
```

#### 11.2 上行流（Agent 输出 → 用户/系统）

```
L1  三类上行数据：
     ├─ ① ResponseChunk（流式）— text_delta / tool_start / tool_result / done
     ├─ ② TelemetryEvent（异步）— tool_call / hook_fired / hook_deny / token_usage
     └─ ③ MemoryWrite（按需）— 证据锚点 / 结论记忆 / 偏好经验
     │
L2  接收 ③：写入证据锚点库 / 文件化记忆 / 更新索引
     │
L3  接收 ②：遥测持久化 → 证据链验证 → 审计查询 → 合规报告
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

### 12. 会话控制管线（纵向机制 C）

#### 12.1 会话模型

```
Event Room (L5, 长生命周期)
  └─ Event (L4, 一个业务事件，有状态机)
       ├─ Session 1 (L3→L1, 绑定 skill_id + runtime_id)
       ├─ Session 2 (L3→L1, 意图变化创建的新会话)
       └─ Session 3 (L3→L1, A2A 派发的并行校核)
```

- **Event Room**：面向用户的协作空间，长生命周期，不随 Session 结束
- **Event**：一个业务事件对象，有完整的状态机
- **Session**：一次 Agent 任务执行，绑定具体 Skill 和 Runtime

#### 12.2 Session 编排规则

- 意图变化 → 创建新 Session（保留旧 Session 上下文通过 Memory）
- Agent 请求子任务 → A2A 派发新 Session
- 审批闸门触发 → Session 暂停，等待 L5 人工确认
- 所有 Session 完成 → Event 进入 monitoring/closed
- Event 关闭后 → 触发记忆沉淀（复盘经验写入 L2 Memory）

#### 12.3 完整一轮时序

```
外部事件          L5         L4           L3          L2         L1
   │              │          │            │           │          │
   │─告警─────→│          │            │           │          │
   │              │──推送───→│            │           │          │
   │              │          │ 聚合为事件   │           │          │
   │              │          │ 创建EventRoom│          │          │
   │              │          │─查询记忆─────────────→│           │
   │              │          │←记忆引用─────────────│           │
   │              │          │──请求Skill──────────→│           │
   │              │          │──三方握手──→│           │          │
   │              │          │            │──Initialize──────→│
   │              │←─事件卡──│            │           │          │
   │         用户点击        │            │           │          │
   │         [发起会审]      │            │           │          │
   │              │──────→│ A2A 派发     │           │          │
   │              │          │──────创建多个Session───────→│
   │              │          │            │           │  AgentLoop│
   │              │          │            │           │←─记忆读取│
   │              │          │            │           │←─证据写入│
   │              │          │←──stream──────────────────────│
   │              │←─更新卡片─│            │←─遥测──────────────│
   │              │          │ 汇聚结论    │           │          │
   │              │          │──校核请求──→│           │          │
   │              │          │←─校核结果──│           │          │
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

## 第四部分：接口与部署

### 13. 跨层接口契约

#### 13.1 L3/L4 REST 契约（继承 v1.7 + 扩展）

五个 REST API 契约定义了 L3 与 L4 之间的全部通信（继承 v1.7）：

**契约 1：策略部署**
- PUT /v1/policies/managed-hooks — 原子部署
- POST /v1/policies/test — 沙箱测试
- GET /v1/policies/versions — 版本历史
- POST /v1/policies/rollback — 版本回滚

**契约 2：意图网关**
- POST /v1/intents/dispatch — 分发意图
- GET /v1/intents/{id}/status — 查询状态
- POST /v1/intents/{id}/approve — 审批放行
- DELETE /v1/intents/{id}/cancel — 取消意图

**契约 3：技能生命周期**
- POST /v1/skills/submit — 提交草稿
- PUT /v1/skills/{id}/promote — 晋升
- GET /v1/skills/{id}/versions — 版本历史
- GET /v1/skills/search — 搜索

**契约 4：遥测采集**
- POST /v1/telemetry/events — 接收遥测
- GET /v1/telemetry/query — 结构化查询
- GET /v1/telemetry/aggregate — 聚合分析

**契约 5：会话控制**
- POST /v1/sessions/create — 三方握手
- GET /v1/sessions/{id} — 查询状态
- POST /v1/sessions/{id}/message — 发送消息
- DELETE /v1/sessions/{id} — 销毁会话

#### 13.2 L5↔L4 实时通信契约（★ v1.8 新增）

```
下行推送（L4 → L5）：SSE / WebSocket
  事件类型：
    event_card_update    — 事件卡数据更新
    evidence_pack_update — 证据包更新
    action_card_update   — 行动卡更新
    approval_card_update — 审批卡更新
    stream_chunk         — Agent 输出流式数据
    session_status       — 会话状态变化
    event_status         — 事件状态变化

上行操作（L5 → L4）：HTTP REST
  POST /v1/events/{id}/actions    — 用户操作（发起会审、转派等）
  POST /v1/approvals/{id}/decide  — 审批决定
  POST /v1/rooms/{id}/message     — 用户消息
```

#### 13.3 L4↔L2 Memory API（★ v1.8 新增）

```
L4 在上下文拼装阶段查询记忆：
  POST /api/v1/memory/search — 混合检索相关记忆
  GET  /api/v1/memory/anchors?event_id= — 获取事件证据链

L1 在执行过程中读写记忆：
  POST /api/v1/memory/anchors — 创建证据锚点
  POST /api/v1/memory/files — 写入记忆文件
  POST /api/v1/memory/search — 检索记忆
```

#### 13.4 L4↔L4 A2A 协议（★ v1.8 新增）

```
Agent 能力注册：
  POST /api/v1/a2a/register — 注册 Agent 能力
  GET  /api/v1/a2a/discover — 发现可用 Agent

协作调度：
  POST /api/v1/a2a/dispatch — 发起并行互审
  GET  /api/v1/a2a/tasks/{id}/status — 查询任务状态
  POST /api/v1/a2a/tasks/{id}/result — 提交结果

结果汇聚：
  GET  /api/v1/a2a/tasks/{id}/aggregate — 获取汇聚结论
```

#### 13.5 L1/L3 接口（继承 v1.7）

L1/L3 通过 hooks 进行通信（非 REST API）。在会话创建时，L3 将受管 hooks 注入 L1，此后每次工具调用都不需要额外的 L3 API 调用。

#### 13.6 三方握手（更新）

v1.8 的三方握手在 v1.7 基础上增加记忆查询步骤：

```
(1) L4 事件引擎接收事件 或 用户发起请求
(2) L4 从 L2 Memory Engine 查询相关记忆（memory_refs）
(3) L3 对用户/来源进行认证，解析组织角色与组织单元
(4) L3 调用 POST /v1/sessions/create，携带用户上下文、任务画像、
    运行时偏好、event_context、memory_refs
(5) L3 按策略校验用户角色，编译分层受管 hooks 集
(6) L1 运行时选择器基于任务画像、Skill 亲和性与选择策略选择运行时
(7) L1 分配运行时实例，会话引导平面启动，触发 SessionStart hook
(8) L3 将会话句柄返回给 L4
(9) L4 将事件室连接到会话并开始流式传输
```

---

### 14. 部署拓扑

平台部署跨越六个基础设施分区，每个分区可独立扩展。面向 Kubernetes 容器编排设计，但也适配虚拟机部署。

#### 14.1 边缘 / CDN 分区

托管静态门户资源（Web 应用、移动端 PWA）、API 限流与 DDoS 防护。

#### 14.2 L5 协作层集群（★ v1.8 新增）

```
事件室服务（2+ 副本）：事件室生命周期管理
卡片渲染服务（2+ 副本）：四卡数据组装与推送
推送网关（2+ 副本）：SSE/WebSocket 连接管理
IM Bot 服务（按需）：企业微信/钉钉/飞书/Slack 适配器
```

#### 14.3 L4 编排层集群

```
事件引擎（2+ 副本）：事件接入、聚类、状态机
会话编排器（3+ 副本）：三方握手协调
A2A 路由（2+ 副本）：并行互审调度与结果汇聚
触发器管理（2+ 副本）：定时任务、Webhook/CDC 触发
API 网关（3+ 副本）：认证、限流、路由
```

#### 14.4 L3 治理层集群

```
策略引擎 + 编译器（2+ 副本）：策略管理与 hook 编译
审批闸门（2+ 副本）：控制链状态管理
审计服务（3+ 副本）：高写入遥测接收
校核器（2+ 副本）：确定性校核服务
MCP 注册表（2+ 副本）：连接器健康检查
意图网关（2+ 副本）：事件驱动的工作流分发
Hook 部署服务：managed-settings 原子分发
```

#### 14.5 L2 资产层集群

```
Skill 仓库 MCP server（2+ 副本）
Memory Engine（2+ 副本）：证据锚点 + 文件化记忆 + 索引
Ontology Service 网关（2+ 副本）：本体查询代理 + MCP 暴露
晋升引擎 + 分析引擎（2+ 副本）
访问控制 + 依赖解析 + 版本管理
```

#### 14.6 L1 执行区域（容器池）

```
按会话创建的临时容器 pod，每个会话一个 pod。
T1 pod（EAASP Runtime、Claude Code）：原生加载 hooks
T2/T3 pod（Aider、Goose、LangGraph 等）：配套 Hook Bridge sidecar
按活跃会话数自动伸缩。
出站网络仅允许访问：MCP servers、L2 Skill 仓库、L3 审计服务。
```

#### 14.7 数据层

```
PostgreSQL：用户数据库(L4)、策略配置存储(L3)、Skill 元数据(L2)、
           Memory Engine 元数据(L2)
TimescaleDB / ClickHouse：执行日志(L4)、审计事件存储(L3)
Redis：会话存储(L4)、事件状态缓存(L4)
S3/GCS/Azure Blob：Skill 内容(L2)、证据快照(L2)、记忆文件(L2)
密钥库（HashiCorp Vault / AWS Secrets Manager）：凭据管理
```

#### 14.8 网络架构

```
公网区：CDN + API 网关
内网区：L5/L4/L3/L2 服务，mTLS 通信
运行时区：L1 容器，受限出站
```

#### 14.9 高可用与容灾

所有 L5/L4/L3/L2 服务均以跨可用区的 2+ 副本运行。数据库主从切换自动化。会话存储跨可用区复制。L1 容器无状态且可恢复。

RPO：活跃会话为零（实时复制），执行日志 15 分钟（异步批量写入）。
RTO：L5/L4/L3/L2 服务故障切换小于 5 分钟，L1 智能体替换小于 30 秒。

---

### 15. 七个后台系统技能

v1.8 定义七个系统级技能，作为所有业务场景复用的基础能力。这些技能不是具体的 Skill 文件，而是平台内建的系统能力，由各层协作实现。

| # | 系统技能 | 输入 | 输出 | 主要实现层 | 触发条件 |
|---|--------|------|------|---------|---------|
| 1 | **事件化** | 原始告警/日志/变更流 | 结构化业务事件对象 | L4 事件引擎 | 事件源推送 |
| 2 | **上下文拼装** | 事件对象 + 用户角色 | 完整的 SessionPayload | L4 编排器 + L2 Memory | 会话创建前 |
| 3 | **证据压缩与索引** | Agent 执行过程中的数据快照 | 证据锚点 + 索引条目 | L2 Memory Engine | 工具调用后(PostToolUse) |
| 4 | **A2A 会审调度** | 事件 + 所需审核维度 | 多 Agent 并行执行 + 结论汇聚 | L4 A2A 路由 | 复杂事件的会审请求 |
| 5 | **行动树生成** | Agent 结论 + 证据包 | 步骤树（分支条件 + 最小动作） | L1 Agent + L4 汇聚 | Agent 完成分析后 |
| 6 | **草稿化写入** | 行动树 + 审批要求 | 草稿文档（不越权执行） | L3 审批闸门 | 行动需要执行时 |
| 7 | **记忆沉淀** | 会话产出 + 证据链 | 偏好/经验/知识写入 Memory | L2 Memory Engine | 事件关闭后(PostSessionEnd) |

---

## 第五部分：演进与决策

### 16. 平台演进路线

EAASP v1.8 通过六个阶段逐步建设。每个阶段都能独立交付有价值的能力，不需要返工前一阶段的产出。

#### 阶段 0：MVP 验证（基线）

**目标**：验证 L4→L3→L2→L1 全链路可行性。

**交付能力**：
- L3 策略编译器（YAML 策略 → managed hooks）
- L3 五个 API 契约（策略部署、意图网关、技能生命周期、遥测采集、会话控制）
- L3 会话控制（三方握手 + 消息代理 + 终止）
- L2 Skill 注入到 L1 运行时
- L1 Hook 强制执行（受管 hooks + 作用域 hooks）
- 端到端：用户发起请求 → 策略编译 → 运行时选择 → Agent 执行 → 结果返回

**验证标准**：一个完整的 workflow-skill（如 HR 入职审批）可在受管 hooks 治理下端到端执行。

#### 阶段 1：事件驱动基础

**目标**：从被动请求→事件驱动。

**交付能力**：
- L4 事件引擎（事件源接入 → 去重 → 聚类 → 事件对象 → 状态机）
- L5 事件室数据模型 + 事件卡 API
- L4 EventReceived / PreSessionCreate / PostSessionEnd hooks

**前置依赖**：阶段 0 完成。

**验证标准**：外部告警系统推送 Webhook → 事件引擎聚合为事件 → 自动创建事件室 → 推送事件卡到用户。

**独立价值**：即使没有后续阶段，平台已能自动聚合告警并通知相关人员，减少告警疲劳。

#### 阶段 2：记忆与证据

**目标**：从无状态→持久化记忆+可追溯。

**交付能力**：
- L2 Memory Engine（证据锚点库 + 文件化记忆 + 混合检索索引）
- L1→L2 记忆写入通道（SessionPayload 扩展）
- L4→L2 上下文拼装通道（会话创建前查询相关记忆）

**前置依赖**：阶段 0 完成（阶段 1 不是前置条件，可与阶段 1 并行）。

**验证标准**：Agent 执行过程中写入证据锚点 + 结论记忆；下次类似任务时 L4 自动注入相关记忆到 SessionPayload。

**独立价值**：Agent 可积累经验，重复任务质量递增。证据链使审计从"记录"升级为"可追溯"。

#### 阶段 3：审批与校核

**目标**：从放任→受控执行。

**交付能力**：
- L3 审批闸门控制链（Plan → Check → Draft → Approve → Execute）
- L3 确定性校核器接口 + 规则引擎校核
- L5 审批卡交互

**前置依赖**：阶段 0 完成。阶段 2 完成时效果更佳（校核结果可写入证据链）。

**验证标准**：高风险操作自动进入控制链 → 确定性校核 → 生成草稿 → 推送审批卡 → 人工审批 → 执行。

**独立价值**：高风险操作有了系统性的控制机制，而非依赖 Agent 的自觉。

#### 阶段 4：多 Agent 协作

**目标**：从单 Agent→并行互审。

**交付能力**：
- L4 A2A 路由（Agent 能力发布 + 协作调度 + 结果汇聚）
- 多 Agent 并行执行
- L5 会审结论展示

**前置依赖**：阶段 1（事件引擎提供事件对象）、阶段 0（会话控制支持多 Session）。

**验证标准**：复杂事件 → A2A 派发给多个专长 Agent → 并行校核 → 结论汇聚 → 冲突检测。

**独立价值**：复杂决策有了多维度校核，克服单 Agent 认知局限。

#### 阶段 5：完整协作空间

**目标**：完整四卡 + IM 集成 + 复盘闭环。

**交付能力**：
- L5 四卡渲染引擎（事件卡 + 证据包 + 行动卡 + 审批卡完整实现）
- IM Bot 插件（企业微信/钉钉/飞书/Slack）
- 复盘与记忆沉淀闭环（事件关闭 → 自动复盘 → 经验写入记忆）

**前置依赖**：阶段 1-4 全部完成。

**验证标准**：从外部事件到复盘记忆沉淀的完整生命周期可在 IM 中以卡片交互形式完成。

#### 阶段 6：生态开放

**目标**：第三方运行时接入 + 技能市场 + 多租户。

**交付能力**：
- 运行时认证流水线（自动化接口契约测试 + 盲盒质量基准测试 + 安全扫描）
- L2 Skill 市场能力（跨组织 Skill 共享与访问控制）
- 多租户支持（企业 → 事业部 → 部门 → 团队完整组织层级）
- 平台 API SDK

**前置依赖**：阶段 0-5 全部完成。

**验证标准**：第三方运行时通过认证流水线加入运行时池；跨组织 Skill 发布到市场并受访问控制。

---

### 17. 设计原则与反模式

#### 17.1 设计原则（9 条）

**继承自 v1.7（5 条）**：
1. 治理是统一的
2. 智能通过 Skills 流动
3. Hooks 提供保障，智能体提供判断
4. 扩展，而非重建
5. 拒绝优先（Deny always wins）

**v1.8 新增（4 条）**：
6. 事件驱动
7. 证据链完整
8. 草稿优先
9. 记忆是资产

#### 17.2 设计反模式

**继承自 v1.7（9 条）**：
- 在需要 hooks 的地方用 prompts
- 过度使用高成本 hooks
- 跳过模式路由器
- 没有受管 hooks 的治理
- 单体化的 hook 脚本
- 没有作用域钩子的技能
- 绕过 Skill 仓库
- 把文件系统当作持久化
- 忽视无界面运行时的能力
- 双重抽象

**v1.8 新增（5 条）**：

11. **把每条告警当作独立事件**：100 条相关告警应该聚合为一个事件对象，而不是创建 100 个事件室。事件引擎的聚类能力是核心。

12. **没有证据的结论**：Agent 输出结论但不挂载证据锚点。审计时无法追溯"为什么这样判断"。所有关键结论必须引用证据。

13. **用 LLM 做确定性校核**："五防校核是否通过"不应该让 LLM 判断。规则引擎、仿真工具等确定性工具做校核，LLM 做需要推理的部分。

14. **Agent 越权执行**：Agent 应该只生成草稿，不直接执行高风险操作。"草稿优先"原则确保人类始终保有最终决策权。

15. **记忆当作向量数据库**：记忆文件应该是人类可读可编辑的结构化文本，而不是只有向量检索入口的黑盒。混合检索（关键词+语义+时间衰减）优于纯向量搜索。

#### 17.3 AgentOps / EvalOps（跨层运营机制，后续专题细化）

规模化 Agent 平台需要系统化的运营与评测机制。EAASP v1.8 在 v1.7 可观测性枢纽的基础上，定义以下运营指标框架：

```
离线评测（EvalOps）：
  - Skill 质量基准测试（benchmark / regression set）
  - 运行时质量对比（盲盒评分基准线）
  - 新运行时接入认证测试

在线运营（AgentOps）：
  - Session 执行追踪（成功率、时延、token 消耗）
  - 证据覆盖率（结论挂载证据的比例）
  - 审批响应时延（从推送审批卡到人工决定的时间）
  - 行动成功率（执行后是否达成预期）
  - 回滚率（执行后需要回滚的比例）
  - 记忆利用率（记忆被后续事件引用的比例）
```

具体的指标定义、采集方式和仪表盘设计将在后续专题中细化。

---

### 18. 完整设计决策轨迹

该表追溯了 EAASP 设计演化中每一个关键架构问题。

| 问题 | v1.1-v1.2 | v1.3 (hooks) | v1.4 (运行时池) | v1.7 最终版 | v1.8 |
|------|-----------|-------------|-------------|-----------|------|
| L1 是什么？ | 仅单一运行时 | 运行时 + 内核 + hooks | L1 内部抽象 + 运行时池 | 3 层适配器 + 12 方法契约 | 不变，新增记忆写入通道 |
| 工作流怎么做？ | YAML → skills | Skills + hooks | 亲和性 + hooks | workflow-skills（智能+确定性合一） | 不变 |
| 治理怎么做？ | 未定义 → policy | 受管 hooks | + hook bridge | 策略从 L3 管理控台编译部署；deny always wins | + 审批闸门五阶控制链 + 确定性校核 |
| 最高层是什么？ | 两层 (L2+L1) | 三层 + hooks | 四层 + hooks | L4 四平面 + 5 API 契约 | L5 协作层(事件室+四卡) + L4 编排层(事件引擎) |
| 运行时如何选择？ | N/A（单一） | N/A | 5 类策略 | 能力清单 + 生命周期管理 | 不变 |
| 选哪些智能体？ | 仅单一运行时 | 仅单一运行时 | 映射 4 个 tier | T1/T2/T3 + T4 暂缓 | 不变 |
| 状态模型？ | 未定义 | 短暂态 | L3/L2/L1 分层 | L4 持久化；L2 配置；L1 短暂态 | + L2 Memory Engine(持久化记忆) |
| 分几层？ | 两层 | 三层 + hooks | 四层 + hooks | 四层 + 3 跨层机制 | **五层 + 3 纵向管线** |
| 驱动模型？ | 请求驱动 | 请求驱动 | 请求驱动 | 请求驱动 | **事件驱动 + 请求驱动混合** |
| 多 Agent？ | N/A | N/A | N/A | 单 Agent per Session | **A2A 并行互审** |
| 记忆？ | 无 | 无 | 无 | L1 会话内暂态 | **L2 Memory Engine（三层持久化）** |
| 交互范式？ | REST API | REST API | REST API | L4 门户 + 对话流 | **L5 事件室 + 四卡置顶** |
| 业务本体？ | 无 | 无 | 无 | 无 | **L2 Ontology Service（外部本体集成契约）** |
| 安全/成本？ | 未定义 | 未定义 | 未定义 | L4 安全架构 + 成本治理 | 继承 + 事件驱动成本控制扩展 |

---

### 19. 团队分工建议

EAASP v1.8 按功能域组织团队，每个域对应一个或多个架构层级的核心能力。

#### 19.1 功能域划分

| 功能域 | 核心职责 | 涉及层级 | 建议规模 |
|--------|---------|---------|---------|
| **协作与体验** | 事件室前端、四卡渲染引擎、多端适配、IM Bot 集成 | L5 | 2-3 人 |
| **编排与事件** | 事件引擎、会话编排器、A2A 路由、触发器管理、API 网关 | L4 | 3-4 人 |
| **治理与安全** | 策略引擎、策略编译器、审批闸门、确定性校核器、审计服务 | L3 | 2-3 人 |
| **资产与记忆** | Skill 仓库运营（晋升/访问/版本/分析）、Memory Engine（三层存储+索引）、MCP 编排 | L2 | 3-4 人 |
| **执行与适配** | EAASP Runtime 维护、运行时适配器开发（T1/T2/T3）、Hook Bridge、遥测采集 | L1 | 2-3 人 |
| **平台基础设施** | 部署编排（K8s/Helm）、数据层运维、监控告警、CI/CD、安全基线 | 全层 | 1-2 人 |

**核心团队**：13-19 人。

#### 19.2 分工原则

- **一名开发者，一个域**：开发者应主要在单一功能域内工作，以保持清晰的架构边界。跨域工作会引入耦合。
- **按阶段对齐人员投入**：阶段 0-1 仅需"编排与事件"+"治理与安全"+"执行与适配"三个域的核心人员。"协作与体验"在阶段 5 才需要全员投入。
- **适配器语言与智能体一致**：每个智能体适配器必须使用该智能体的原生语言编写。
- **Memory Engine 与 Skill 仓库共享数据基础设施**：两者都是 L2 服务，共享元数据库 Schema 与对象存储客户端。

#### 19.3 按阶段人员投入

| 阶段 | 所需域 | 最小团队 |
|------|--------|---------|
| 阶段 0（MVP） | 治理+执行+资产（Skill 部分） | 5-6 人 |
| 阶段 1（事件驱动） | +编排 | +2 人 |
| 阶段 2（记忆） | +资产（Memory 部分） | +2 人 |
| 阶段 3（审批校核） | 治理域扩展 | +1 人 |
| 阶段 4（A2A） | 编排域扩展 | +1 人 |
| 阶段 5（协作空间） | +协作与体验 | +2-3 人 |
| 阶段 6（生态） | +基础设施完善 | +1-2 人 |

---

## 附录：v1.7→v1.8 变更总结

| 维度 | v1.7 | v1.8 |
|------|------|------|
| **层数** | L1-L4 四层 | L1-L5 五层（新增协作层） |
| **L5 协作层** | 不存在 | 事件室 + 四卡置顶 + 多端推送 |
| **L4 职责** | 会话管理器（四平面） | 编排引擎（事件+A2A+状态机） |
| **L3 新增** | — | 审批闸门控制链 + 确定性校核器 + 证据链管理 |
| **L2 新增** | — | Memory Engine（证据锚点+文件化记忆+索引）+ Ontology Service（业务本体集成）+ MCP 扩展 |
| **L1 变更** | — | SessionPayload 新增字段 + 记忆写入通道 |
| **驱动模型** | 请求驱动 | 事件驱动 + 请求驱动混合 |
| **交互范式** | REST 请求-响应 + 门户 | 事件室 + 四卡置顶 |
| **安全控制链** | Hook deny/allow | Plan→Check→Draft→Approve→Execute |
| **多 Agent** | 单 Agent per Session | A2A 并行互审 |
| **主动性** | 被动等指令 | 事件触发 + 定时任务 + 主动推送 |
| **会话模型** | Conversation:Session = 1:1 | EventRoom:Session = 1:N |
| **记忆** | L1 会话内暂态 | L2 Memory Engine（三层持久化） |
| **证据链** | 无 | 结论→证据包→原始数据快照 |
| **纵向机制** | 三项跨层机制 | 三条纵向管线（Hook+数据流+会话控制） |

---

> 本文档为 EAASP v1.8 架构构想的完整说明。设计细节将在后续与相关方讨论后进一步迭代和确认。
