# 电力调度 AI 智能体 Skill 规范 v0.1 (紧凑版)

**版本**: v0.1 草稿
**日期**: 2026-04-26
**读者**: 国调中心调度业务专家 + IT 集成商
**关键字**: 本规范使用 RFC 2119 关键字: **MUST(必须)/ SHOULD(应当)/ MAY(可以)**

> **格式说明**: 本规范采用 RFC 2119 关键字 + 轻量化结构(评审稿合适形态),不按 GB/T 国标完整形式。完整 GB/T 形式留待 v0.2 内容稳定后,由标准化办公室按 GB/T 1.1-2020 重排。配套**导读文档** `2026-04-26-skill-spec-reading-guide.md` 是规范的主读材料 —— **建议先读导读再读本规范**。
>
> **本紧凑版**: 用于内部对齐 / 高层快速参考。只列要点 + 创新点核心,不展开例子。需要例子见标准版 / 完整版。

---

## §1. 适用范围

本规范定义电力调度 AI 智能体(agent)中 **skill** 的设计、编写、上线、治理。读者熟悉 MCP / Anthropic Skills / Tool calling 基础概念。

### 1.1 Skill 三类(分类决定 MANDATORY 强度)

| 类别 | 例子 | 风险 | 适用 MANDATORY |
|------|------|------|---------------|
| **A 控制类** | 发遥控指令 / 改定值 / 合分闸 / AGC 指令 / 操作票生成 | 极高(直接动电网) | 全套 M1-M10 |
| **B 分析类** | 潮流计算辅助 / 检修计划 / 负荷预测 / 故障诊断 | 中 | M1 / M2 / M4 / M5 / M7 |
| **C 查询类** | 规程检索 / 台账问答 / 历史故障案例 | 低 | M5(溯源)即可 |

> 后文每条 MANDATORY 都标注适用类别。对 C 类强行要求"机理硬约束"是过度工程,对 A 类只要"可解释输出"远远不够。

### 1.2 不属于本规范的事(反向边界)

- 具体调度业务流程(各调度中心自定 SOP 内容)
- 具体 LLM 模型选型(由信创采购决定)
- L1.5 MCP server 的具体实现(厂商负责)
- 训练数据合规(由数据治理规范覆盖)
- skill 商业模式 / 计费(由商务规范覆盖)

---

## §2. 核心理念

### 2.1 Skill 不是 prompt

| | Prompt | Skill |
|---|--------|-------|
| 触发 | 用户每次手动 | LLM 根据 description 自主激活 |
| 周期 | 单次对话 | 持久化文件,跨会话复用 |
| 结构 | 自由文本 | SKILL.md + scripts/ + references/ |
| 可观测 | 跑一次过一次 | 可版本化、可 review、可 evaluate |

写 skill 需要**领域专业 + 模式抽象 + 文档工程 + LLM 行为感知**。介于 SOP 标准化作业书与 API 设计之间的**新文档类型**。

### 2.2 Skill = orchestration,Tool = capability

> **核心原则**: skill 正文教 LLM **怎么用** tool 完成任务,**不是做** tool 已经能做的事。任何让 LLM 在正文里重新发明工具能力(自己写 SQL / 自己 N-1 / 自己模拟潮流)都是反模式 (§7 R1-R9)。

### 2.3 原子 skill vs SOP skill 显式二分(本规范创新点)

业界规范(agentskills.io)**未做此二分**;调度场景**必须显式二分**。

| | 原子 skill | SOP skill |
|---|-----------|-----------|
| 含义 | 单一可重用能力 | 业务流程级编排 |
| 例子 | 查 SCADA 量测 / 算 N-1 / 写操作票表头 | 检修流程 / 故障处置预案 / 操作票编制全流程 |
| 调度方式 | 由 SOP skill 或 LLM 直接调用 | 内部静态调度原子 skill |
| 调用其他 skill | 不允许 | 允许(必须经 host) |
| 状态 | 无状态 | 跨步骤状态 |
| 粒度准则 | 1 skill = 1 verb + 1 noun | 多步、跨工具 |

**业界趋势**: Microsoft Semantic Kernel 弃用 Planner,AutoGen 迁 Agent Framework 的 Workflow,CrewAI 推 hierarchical Process —— **共识是纯 LLM 自主调度多步骤不可靠,必须有显式编排兜底**。

**A 类 skill MUST 是 SOP 形态,内部用静态 numbered steps,不允许 LLM 动态决定流程顺序**。理由:**LLM 动态决策不可在控制路径上回放复审** —— 投运前 reviewer 看不到"AI 实际会怎么走"(取决于运行时输入),事故复盘时审计组无法验证"如果重来一次 AI 还会做同样决定吗",监管追责时没有"预期路径 vs 实际路径"的对比基础。静态 steps 把这个不确定性消除。

### 2.4 4 层架构

```
L0   wire 协议        —— JSON-RPC 2.0 / JSON Schema
L1   skill 通用层     —— MCP / Anthropic Skills / OpenTelemetry GenAI(引业界标)
L1.5 业务接口适配层   —— D5000-MCP / EMS-MCP / SCADA-MCP / 闭锁-MCP / PMS-MCP / AGC-MCP
L2   调度行业约束层   —— 本规范 §3-§7 全部内容
L3   项目落地层       —— 各调度中心 / 厂商具体 skill 实现
```

**L1.5 是本规范关键创新**(详见 §6.4):skill 不直接懂 D5000 / IEC 104 / IEC 61850 / 闭锁系统私有协议,中间需一组 MCP server 做"业务接口适配"。**没有 L1.5,任何 skill 都没法跟现有调度系统对接,只能在沙盘里跑**。

---

## §3. L1 通用层 — 引业界标准(MUST)

| 标准 | 引用级别 | 用途 |
|------|---------|------|
| **MCP 2025-11-25**(Model Context Protocol) | MUST | host ↔ skill 唯一现实 wire 协议 |
| **Anthropic Agent Skills + agentskills.io 2025-12-18** | MUST | 唯一公开 skill 包格式 + 元数据 + 渐进披露(progressive disclosure) |
| **JSON Schema draft 2020-12** | MUST | 入参 / 出参结构定义 |
| **OpenTelemetry GenAI Semantic Conventions + W3C Trace Context** | MUST | 调度可追溯性合规硬要求(操作票回溯 / 事故分析 / 等保审计) |
| **SemVer** | MUST | skill 版本演化 |
| **JSON-RPC 2.0** | MUST | 错误模型(MCP 内嵌) |
| **OAuth 2.1 + Dynamic Client Registration (RFC 7591)** | SHOULD | 远程 server 认证基线;**国密替代见 §6.4** |

**自定义命名空间**: 调度专属字段 MUST 用 `x-grid-*` 前缀(eg. `x-grid-risk-class`),不污染 MCP / Skills 标准命名空间。

---

## §4. L2 调度行业 MANDATORY 清单

> **7 条核心 + 3 条分阶段**。前 7 条是"不满足拿不到投运批准"的硬底线。

### 4.1 核心 7 条

| ID | 名称 | 一句话定义 | 适用 |
|----|------|-----------|------|
| **M1** | 机理引擎单调胜出 | 机理与 AI 输出冲突时,机理 verdict 单调胜出,冲突 MUST 入审计日志 | A / B |
| **M2** | 输入侧 + 输出侧两道校核闸 | 输入校权限 + 数据质量;输出校物理(N-1 / 潮流 / 限额),禁止 LLM 自评 | A / B |
| **M3** | 闭锁系统作为外部权威 | 控制类 skill 仅作闭锁系统 client,不可作 issuer;无 token 即拒绝 | A only |
| **M4** | 数据质量与时间戳一致性 | 量测 stale / bad / 时间戳跳变时 skill MUST 拒绝推理,不得"硬着头皮算" | A / B |
| **M5** | 决策溯源链(evidence chain) | input_snapshot + tool_calls + mechanism_check + llm_params + output 全链留痕,缺一项标 incomplete | All |
| **M6** | 法定签名 + WORM 审计 | 输出带国密签名 + 时间戳 + 哈希,WORM 存储 ≥ 法定保留年限 | A only |
| **M7** | 跨调度层级权限边界 | skill 声明 `dispatch_level` + `controllable_assets_filter`,越界拒绝 | A / B |

### 4.2 分阶段 3 条(投运后逐步强制)

| ID | 名称 | 现阶段做法 |
|----|------|----------|
| **M8** | LLM 延迟硬上限 + **秒级闭环禁区** | **禁止部署在 AGC / 紧急控制路径**;LLM 不进秒级控制环 |
| **M9** | 模型版本锁定 + 漂移管控 | `model_lock: {provider, model_id, version, fingerprint}` 校验;模型升级走专家复审 |
| **M10** | 投运前历史极端工况回放 | 国调中心维护极端 case 库;A / B 类 skill 投运前 MUST 通过回放 |

### 4.3 与"5 条独有要素"的关系

行业内已经有"机理 + AI 双引擎、三重校核、强可解释输出、闭锁绑定、时效分级"5 条独有要素的共识。本规范在此基础上做三件事:

| 行业既有 5 条 | 本规范处理 |
|-------------|----------|
| 机理 + AI 双引擎 | 沉淀为 M1,明确"冲突时机理单调胜出 + 入审计" |
| 三重校核 | 沉淀为 M2(简化为输入 + 输出两道闸),输出校核必须由机理引擎完成 |
| 强可解释输出 | **拆分**: M5 决策溯源链 升 MUST,可解释文案 降 SHOULD |
| 闭锁绑定 | 沉淀为 M3,**强化 client / issuer 边界**(AI 不可创建闭锁条目) |
| 时效分级 | **拆分**: M8 秒级闭环禁区 升 MUST,分级提示 降 SHOULD |
| | 新增 M4 数据质量 / M5 溯源 / M6 法定签名 / M7 跨层级 / M9 模型锁定 / M10 极端回放 |

新增的 6 条不是行业之前没意识到,而是 AI 智能体进入调度环境之前没遇到过的具体场景(LLM 输入 garbage 难以被发现、模型可能静默升级、跨调度层级在 LLM 上下文中容易被打破)。

---

## §5. SKILL.md 编写规范

### 5.1 frontmatter 必需字段

```yaml
---
name: dispatch-fault-handle-220kv-line-trip
description: <一句话描述何时触发 + 做什么。Use when 220kV 线路单跳重合不成功>
version: 1.0.0
skill_type: atomic | sop | hybrid           # MUST(本规范创新)
allowed-tools:                              # 引用工具,L1.5 MCP server 提供
  - mcp:scada.get_measurement
  - mcp:ems.run_n_minus_1
metadata:
  x-grid-risk-class: A                      # A / B / C
  x-grid-dispatch-level: provincial         # national / regional / provincial / municipal / county
  x-grid-required-certifications: [dispatcher-l3]
  x-grid-requires-two-person-approval: true # A 类必须 = true(秒级场景例外)
  x-grid-tool-error-policy: strict          # strict / degrade / advisory
  x-grid-latency-class: advisory_minutes    # closed_loop_seconds / advisory_minutes / planning_hours
  x-grid-model-lock: {provider, id, version, fingerprint}
---
```

### 5.2 SKILL.md 正文必需段

| 段名 | 必需 | 说明 |
|------|------|------|
| `## Purpose` | MUST | 这个 skill 解决什么具体调度问题 |
| `## Steps` | MUST(SOP)/MAY(原子) | 控制类 SOP **必须用 numbered steps**,不允许 if / else 让 LLM 自由分支 |
| `## Mechanism Constraints` | MUST(A / B 类) | 引规程 ID + 条款 + 数值阈值;机理硬约束的可声明表达 |
| `## Gotchas` | MUST(数量 ≥3,A / B 类) | 见 §5.3 — **本规范创新点** |
| `## Examples` | SHOULD | 至少 1 正例 + 1 反例(A 类 MUST 含反例) |
| `## References` | MUST | 引用规程 ID + version + clause + library_snapshot_hash |
| `## Boundaries / Out of Scope` | MUST | 防止 over-trigger,告诉 LLM 何时该让位给其他 skill |
| `## Escalation` | MUST(A 类) | 何时升级到人工 / 高级工程师 / 安监 |

### 5.3 Gotchas 段(本规范创新点)

Anthropic best practices 论断:**"很多 skill 里价值最高的内容,是一组 gotcha —— 那些环境特有、违反常识假设的具体事实"**。

调度行业的 gotcha 几乎都是已经存在的实物 —— 反措通报、安监通报、事故复盘报告。每条 Gotcha = 一次"差点出事"或"已经出事"的反直觉具体事实。**这是规程做不到的** —— 规程是声明式的(应当如何),Gotcha 是反直觉具体事实(实际是这样)。

**Gotchas 对 LLM 执行 skill 的 4 类具体影响**:

1. **改变默认假设** —— 把"LLM 默认会按通用电网常识办"覆盖成"按本厂实际办"(eg. SCADA 显示 "0/OFF" → 保护逻辑用 "DISCONNECTED")
2. **拒绝错误推理路径** —— 反例式 gotcha 阻断 LLM 的错误类比(eg. 母联开关在五防里 type=COUPLER 不是 BREAKER)
3. **触发额外的 safety check** —— 遇到特定设备 / 场景时主动多走一步校核(eg. 无远程分闸能力 → 操作票加现场确认字段)
4. **行业级反措 → AI 调度员的肌肉记忆** —— 反措通报 → 受影响 skill 的 Gotchas 段更新流水(governance hook),让"老师傅经验"工程化、跨人员、跨调度中心传承

**写法约束**:
- **数量 ≥ 3 条**(太少证明业务专家没下功夫挖掘)
- **必须具体到设备 / 场景 / 字段名** —— "注意保护定值"是无效 gotcha,"主变中性点接地刀闸 SCADA 与现场实际相差 180°"是有效 gotcha
- **测试标准**: 每条 gotcha 应该让 LLM **不知道时直接犯错,知道时不犯错**
- **超长处理**: 累积超 5000 token 时,按设备类别拆 `references/gotchas-by-equipment-type.md`,不能硬塞 SKILL.md

**强制更新流水**: 每次反措通报 / 安监通报 MUST 触发受影响 skill 的 Gotchas 段更新 → SKILL.md MAJOR 版本升级 → 重新过 §7 Quality Gate。这是 §10 演进机制的核心环节。

### 5.4 体量上限

- SKILL.md 正文 **MUST ≤ 500 行 / 5000 token**(Anthropic 硬性建议;超出走渐进披露子文件)
- description **MUST ≤ 200 字符**;触发准确率 ≥ 85%(Tessl 风格评分 ≥ 80)

### 5.5 自由度校准(degrees of freedom)

每个 skill 必须明确告诉 LLM"这一步给你多少自由":

- A 类(低自由度):必须严格按序,任何偏离即拒绝
- B 类(中自由度):有偏好模板,允许参数调节
- C 类(高自由度):多种合理路径,给方向不给步骤

---

## §6. Skill 与外部边界(MUST)

> 5 个边界**互锁**:任一缺位 skill 就退化为"在调度环境里跑的玩具"。

### 6.1 工具边界

skill 调用工具分 6 类:MCP tool(只读 / 计算 / 控制)、CLI 脚本、Python 算法库、其他 skill。**核心约束**:
- Python 算法库 MUST 经 MCP server 包装(R8) —— skill 正文不允许让 LLM 任意运行代码
- "skill 调用 skill" MUST 走 host 调度(R6) —— 不在正文 import 别的 skill
- 错误处置三档(strict / degrade / advisory),按 A / B / C 类映射

### 6.2 人(HITL)边界

| Skill 类 | HITL 模式 | 说明 |
|---------|----------|------|
| A 控制类 | **必须 HITL + 双签** | 调度员 approve 后才执行 |
| 秒级控制(AGC / 紧控) | **必须不进控制环** | LLM 仅 advisor,不进闭环(M8) |
| B 分析类 | 应当事后复核(HOTL) | 调度员事后复核 |
| C 查询类 | 可以全自动 | |

**调度员信任建立 5 阶段**(应当): 沙盘 → 影子 → 建议 → HITL → 投产。每阶段必须有退出 KPI。

### 6.3 知识边界(规程 / RAG)

- 规程 / 安稳导则 / 操作规程 **必须走 RAG 引用**,不得硬编码进 SKILL.md(规程更新就过期)
- 引用必须携带 `(regulation_id, version, clause_id, library_snapshot_hash)`(M9 关联)
- RAG 知识库写入必须经规程编辑流水(双人审批 + 版本记录)
- 设备台账 / 实时量测走 L1.5 MCP tool,不走 RAG

### 6.4 已有调度系统边界:L1.5 业务接口适配层(本规范关键创新点)

#### 问题

skill 不是新建系统,它要跟已运行十几年的调度自动化系统协作 —— D5000 OPEN3000、调控云、AGC、AVC、EMS、PMS、OMS。这些系统都有自己的协议(IEC 61850、IEC 60870-5-104、E 文件、厂商私有 API),skill 不可能直接懂这些协议。

#### 业界为什么没覆盖这一层

L1 通用层(MCP / Anthropic Skills)解决"skill 跟 host 怎么对话";L2 行业约束层解决"skill 在调度场景必须满足什么硬约束"。中间这一层"skill 怎么跟 D5000 / SCADA 等私有系统对接"**是行业特定的,业界标准不会覆盖**。

#### L1.5 设计

```
应用层(skill)
   ↓ 用 MCP 调
L1.5 业务接口适配层
   ├── D5000-MCP server      ← 把 D5000 私有 API 包成 MCP tools
   ├── EMS-MCP server         ← 包潮流、N-1、稳定计算
   ├── SCADA-MCP server       ← 包实时量测、告警、状态
   ├── 闭锁-MCP server        ← 包五防、挂牌、操作票闭锁查询
   ├── PMS-MCP server         ← 包设备台账、检修计划
   └── AGC-MCP server         ← 只读!skill 不能从这里发指令
   ↓ 用各种私有协议调
现有调度系统
```

#### Schema 基础:CIM 作语义中介

L1.5 不只是协议翻译,更是**语义对齐**。同一个"500kV 线路"在 D5000 / EMS / PMS 里命名不同。**CIM (IEC 61968 / IEC 61970)** 是国际电力公共信息模型 —— L1.5 MCP server 应当用 CIM 作为对外暴露的语义层,各调度系统的内部命名在 server 内部翻译成 CIM。**国网 B 接口**已经在做类似事情,L1.5 可基于 B 接口扩展。

#### 谁来写 L1.5

推荐由原厂商(南瑞、国电南自、华为等)提供 —— 他们最懂自家系统的私有协议。**国调中心**评审 contract test(跨厂商兼容性必须通过 contract test 验证)、维护 schema 标准、保留升级版本仲裁权。

#### L1.5 的工程价值

有了 L1.5,**skill 可以跨厂商复用**:一个由南瑞写的 skill 如果只调用 L1.5 标准 tool,可以装到国电南自的平台跑。**没有 L1.5**,每家厂商都要给自己的 D5000 写一遍适配,跨厂商互通就是反复造轮子 —— 这是规范定义 L1.5 的真正工程价值。

### 6.5 信创合规边界(MUST)

- LLM:**必须国产模型**(文心 / 通义 / 智谱 / 百川 / DeepSeek 等);本地部署,数据不出境
- 签名 / 加密:**必须国密 SM2 / SM3 / SM4**(替代 RSA / SHA-256 / AES);M6 法定签名走 SM2
- 等保:整体架构必须符合等保 2.0 三级 + 关键信息基础设施保护要求
- 数据库 / OS / 中间件 / 芯片:**必须信创全栈**(达梦 / OpenGauss / 麒麟 / 海光 / 鲲鹏 / 飞腾)

### 6.6 多 skill 编排冲突(SHOULD)

多 skill 协同部署平台应当维护 skill 优先级与互斥矩阵;冲突建议 MUST 升级到调度员人工裁决;当国调首批 ≥ 5 skill 并发部署 6 月后,本条升 MANDATORY。

---

## §7. Skill 准入 lint(必须,反模式硬底线)

| ID | 反模式 | 正确做法 |
|----|-------|--------|
| **R1** | LLM 自己写 SQL 查实时量测 | 必须调 `mcp:scada.get_measurement` |
| **R2** | LLM 自己实现 N-1 校验 | 必须调 `mcp:ems.run_n_minus_1` |
| **R3** | LLM 自己模拟潮流 | 必须调 `mcp:ems.run_powerflow` |
| **R4** | LLM 自己生成操作票格式 | 必须调 `mcp:opticket.generate` |
| **R5** | LLM 自评 tool 输出(自己当裁判员) | 必须由独立 verify tool 给 verdict(M2) |
| **R6** | skill 内 import 另一 skill 正文 | 必须经 host 调用 |
| **R7** | LLM 解释闭锁逻辑 | 必须调 `mcp:interlock.check`,LLM 不可仿真 |
| **R8** | LLM 在 skill 正文里运行任意代码 | 必须经 MCP server 包装 |
| **R9** | LLM 编造规程引用 | 必须通过 RAG + library_snapshot_hash 校验 |

**R5(LLM 自评)是调度场景最危险的一条** —— 等于运动员自己当裁判。任何 output 校核必须走机理引擎,不能 LLM 自评。

**额外质量门**(必须):
- description 触发准确率 ≥ 85%(Tessl × Snyk 评分基线)
- prompt 注入安全扫描 0 critical(Snyk ToxicSkills:36% 公开 skill 含 prompt 注入)
- skill 正文 ≤ 500 行 / 5000 token
- A 类 skill 至少含 1 反例(机理违反场景)

---

## §8. 技能库治理(本规范创新点)

> 技能库是**企业最重要的 AI 资产**(详细论述见导读 §7;详细 governance hook + 集中本地化架构见完整版 §F)。本节列必备要求。

### 8.1 Lifecycle (5 阶段 + retire)

```
draft → review → shadow → canary → production → retired
```

每阶段必须有 governance hook(谁审、看什么、退出 KPI、退回机制)。production 阶段每月必须做 1 次"幽灵审计"(随机抽 5% 调用复审)。retire 不等于删除 —— 历史调用日志仍可审计(M5 / M6 不豁免)。

### 8.2 多维分类索引(必须 4 维)

skill 库必须至少按以下 4 维分类索引,**任意一维独立查询 + 跨维交叉查询**:

- 业务域: 实时监控 / 安稳校核 / AGC / AVC / 检修 / 新能源 / 故障处置 / 分析咨询
- 调度层级: 国调 / 网调 / 省调 / 地调 / 县调
- 风险等级: A / B / C(对应 §1.1)
- skill 类型: atomic / sop / hybrid(对应 §2.3)

平台 SHOULD 暴露语义检索能力(基于 description 向量索引)。

### 8.3 跨厂商互通

- skill 包格式必须遵循 agentskills.io 2025-12-18(对应 §3 L1)
- 厂商专属字段必须在 `vendor.<vendor-name>.*` 命名空间
- skill 间 dependency **禁止跨厂商私有调用**,必须走 L1.5 MCP server(对应 §6.4)
- L1.5 MCP server 跨厂商兼容性必须通过 contract test(国调中心维护)

### 8.4 版本治理 + 模型版本联动

- SemVer:MAJOR 破坏性 / MINOR 加字段 / PATCH 仅修复
- MAJOR 升级必须重新过 §7 Quality Gate
- 引用其他 skill 时必须锁版本范围(类似 npm semver range)
- 模型升级触发 skill 重评:`model_lock` fingerprint 不一致时 skill 拒绝调用(对应 M9)

### 8.5 知识资产沉淀机制(连接反措 → skill 升级)

每次反措通报 / 安监通报 / 事故复盘 **必须触发**受影响 skill 的 Gotchas 段更新流水(对应 §5.3):

```
反措通报 → 受影响 skill MAJOR 升级 → review → shadow → canary → production
```

这条流水把"老师傅经验"转化成"机器可读 + 跨人员 + 跨调度中心可传承"的资产 —— 这是技能库**作为企业资产的核心机制**。

### 8.6 集中 + 本地化 架构

技能库分三层维护:**国调中心核心库**(跨调度通用 skill)+ **省 / 网调增量库**(区域特有)+ **地 / 县调本地化补丁**(本辖区 Gotchas + 操作惯例)。跨层级共享必须**脱敏**(去除涉及国安的具体设备 ID / 容量 / 网架细节)。

详细架构见完整版 §F.6。

### 8.7 Skill registry(沿用业界主流模式)

集中发布 + 分布消费 / 国密 SM2 签名 + 哈希校验 / 依赖图 + 版本范围 / OpenTelemetry trace 审计。**国调统一 registry vs 各省调本地缓存的合规边界**是 open issue(标委会专项评审)。

---

## §9. 不属于本规范的事

(同 §1.2,此处不重复)

---

## §10. 演进机制

- 本规范以 ADR-style 记录变更,每条 MANDATORY 调整必须有理由 + 反对意见 + 投票记录
- 标委会 N+1 评审通过 N 票生效
- 引用业界标准(MCP / Skills / OpenTelemetry / 国密 GM/T 等)版本变更触发本规范 review
- 国调中心维护**极端 case 库**(M10),新增 case 后既有 skill 必须在合理周期内重新通过回放
- 每次反措通报 / 安监通报必须触发既有 skill Gotchas 段更新流水(§5.3)

---

## §A. 附录:Open Issues 待领域专家校对

1. 国标条款号(GB/T 31464 / DL/T 510 / GB 38755 等)正式发布前需电力专家 + 法务复核
2. WORM 法定保留年限(常见 5-10 年,具体年限待法务给定)
3. "机理引擎单调胜出"在新能源高比例场景下的边界(传统机理可能本身不准),待标委会评审特定场景例外
4. 国密 SM2 强制是否过度(会挡非国产 LLM 服务)—— 政策决策
5. 极端 case 库管理(防止变成厂商准入黑名单)
6. 信创 LLM 选型对调度场景能力差异
7. multi-skill 编排 MANDATORY 化时机(SHOULD → MUST)

---

*v0.1 起草自 4 份独立 research 报告(共 ~3,400 LOC)。所有 5 维(MANDATORY / 业界标准 / 写作方法论 / skill 边界 / 库治理)均有专项 research 支撑,详见 `docs/external-review/` 同目录其他 `research-*.md` 文件(内部参考)。*
