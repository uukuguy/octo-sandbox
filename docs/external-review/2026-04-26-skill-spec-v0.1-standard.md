# 电力调度 AI 智能体 Skill 规范 v0.1 (标准版)

**版本**: v0.1 标准版
**日期**: 2026-04-26
**读者**: 国调中心调度业务专家 + IT 集成商 + 标准化委员会成员
**关键字**: 本规范使用 RFC 2119 关键字: **MUST(必须)/ SHOULD(应当)/ MAY(可以)**
**用途**: 委员会评审、厂商对齐、调度处长 + CTO 联合阅读

> **格式说明**: 本规范采用 RFC 2119 关键字 + 轻量化结构(评审稿合适形态),不按 GB/T 国标完整形式。完整 GB/T 形式留待 v0.2 内容稳定后,由标准化办公室按 GB/T 1.1-2020 重排。配套**导读文档** `2026-04-26-skill-spec-reading-guide.md` 是规范的主读材料 —— **建议先读导读再读本规范**。
>
> **本标准版**: 在紧凑版基础上,**每条 MANDATORY 配反例 + 论述强化**;创新点(原子-SOP / Gotchas / L1.5 / 技能库治理)展开 1 个调度场景例子。用于委员会评审 / 厂商对齐。需要完整 walkthrough / 转写法 / 实施路径见完整版。

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

#### A 类 SOP MUST 用静态步骤 —— 物理理由:不可回放复审

A 类 skill 必须是 SOP 形态,内部用静态 numbered steps,**不允许 LLM 动态决定流程顺序**。这条约束的物理理由是:

如果 LLM 在 SOP 里动态决定 step 顺序("if 当前是大方式, do A; else do B"),那么:
- 投运前 review 时,reviewer **看不到"AI 实际会怎么走"** —— 因为这取决于运行时的输入数据
- 投运后事故复盘时,如果调度员说"AI 让我做的",**审计组没办法验证"如果重来一次,AI 还会做同样的决定吗"** —— LLM 输出有随机性
- 监管追责时,**没办法证明"这个 SOP 在这个场景下应该怎么走是预期的"** —— 因为根本没有"应该"的预期,只有 LLM 当时怎么走的

而静态 numbered steps 把这个不确定性消除:reviewer 看到 13 步就是 13 步,事故复盘时调度员实际走了哪几步在日志里写得明明白白。

注意:这不是说"A 类 SOP 不能有分支"。**显式分支(写在 skill 里、有限的、可枚举的)允许;LLM 自由分支不允许**。LLM 的角色是"按写死的步骤执行 + 决定参数",不是"决定下一步该做什么"。

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
| **OAuth 2.1 + Dynamic Client Registration (RFC 7591)** | SHOULD | 远程 server 认证基线;**国密替代见 §6.5** |

**自定义命名空间**: 调度专属字段 MUST 用 `x-grid-*` 前缀(eg. `x-grid-risk-class`),不污染 MCP / Skills 标准命名空间。

---

## §4. L2 调度行业 MANDATORY 清单

> **7 条核心 + 3 条分阶段**。前 7 条是"不满足拿不到投运批准"的硬底线。

### 4.1 核心 7 条

#### M1. 机理引擎单调胜出(适用 A、B 类)

电力系统是物理系统,有不可违反的硬约束(基尔霍夫定律、热稳极限、暂态稳定极限、N-1 准则)。LLM 是统计模型,在分布外场景可能给出"违反基尔霍夫"的方案而置信度看起来还高。

**反例**: 大负荷低电压情境下,LLM 基于历史相似日推荐"投入电容器组 C1+C2",但 C2 当时正在检修。机理引擎(电网拓扑实时模型)知道 C2 不可用,如果 AI 输出能压机理输出,操作票就会下发到一台冷备设备 → 现场拒动 → 调度员被迫人工切负荷 → 经济损失级事故。

历史教训:2003 美加大停电 / 2012 印度大停电 根因都是局部决策违反系统级约束。

**MUST 化措辞**: 机理引擎 verdict 与 AI 输出冲突时,机理 verdict **单调胜出**,且冲突事件 MUST 入审计日志。C 类查询 skill 豁免本条。

#### M2. 输入侧 + 输出侧两道校核闸(适用 A、B 类)

校核分两道不可省的关口:输入侧(权限 + 数据质量 + 任务边界)+ 输出侧(N-1 / 潮流 / 限额)。对 LLM 来说"过程"是黑盒,真正可校的只有输入和输出。

**反例**: AI 推荐"500kV 某线路开断",直接生成操作票。但该线路开断后另一回线潮流 105% 长期稳定极限,N-1 失效 → 后续任何故障都可能发展成系统瓦解。

**MUST 化措辞**: 输出校核 MUST 调用机理引擎(潮流计算、N-1 扫描),**不可由 LLM 自评**(自评 = 让裁判员同时当运动员 → 见 R5)。

#### M3. 闭锁系统作为外部权威(仅 A 类)

这条不仅是合规要求,也是行业现行规程已经强制的(操作票 + 双人复核 + 闭锁逻辑)。AI 进入控制环不能架空既有闭锁。

**反例**: AI skill 输出"开断 500kV 某线",绕过常规挂牌闭锁直接下发 → 现场带电检修人员未撤离 → **人身事故**。这是行业最敏感的红线。

**MUST 化措辞**: **AI skill 仅作闭锁系统 client(消费判决),不可作 issuer(创建闭锁条目)** —— AI 作为新增智能体绝不能给自己开闭锁后门。无 token 即拒绝。

#### M4. 数据质量与时间戳一致性(适用 A、B 类)

电力调度的输入数据是 SCADA 量测,但量测**天然带不可靠性**:数据 stale(传感器掉线)、数据 bad(校验位失败)、时间戳跳变(主备切换 jitter)、PMU 不同步。

**输入不可信比输出不准更危险**:输出不准还能被 M2 拦下,输入不可信连校核都校不出来 —— 因为校核器的输入也是同一份脏数据。

**反例**: 某变电站 RTU 通信中断 30 分钟,SCADA 数据冻结但状态位未刷新。AI skill 看见"潮流稳定"就给出"维持现方式"建议。实际线路在断面已超载 5%。无 stale 检测 → 错失干预窗口。

**MUST 化措辞**: A/B 类 skill MUST 声明所依赖的量测点列表,平台在每次调用前 MUST 注入数据质量元数据(`timestamp` / `quality_flag` / `staleness_seconds`);任一关键量测 stale 超阈值或 quality bad,skill MUST 返回 `INPUT_QUALITY_FAILED` 错误,**不得继续推理**。

#### M5. 决策溯源链(适用所有类别)

> 这条是把"可解释文案"和"决策溯源链"显式分开。

- **可解释文案** —— 面向人,目的是让调度员看懂"AI 为什么这么建议"(SHOULD)
- **决策溯源链** —— 面向监管 + 事故复盘,目的是回答"输入数据是什么 + 调用了哪些工具 + 机理校核给了什么结果 + LLM 生成时模型版本/温度/seed 是什么"(MUST)

**前者重要但 SHOULD,后者必须 MUST**。LLM 可以编造一个"看起来像规程条款"的引用("GB/T 31464-2015 §6.3" —— 这条号可能根本不存在),没有溯源链就无法做下游真实性校验。

**反例**: 一次 AI 推荐造成调度员误操作。事故复盘时,要重现 AI 当时"看到了什么数据、做了什么校核",但日志只存了 prompt + reply,RAG 检索结果丢失,机理校核结果丢失 → 无法判断是 AI 错、数据错还是校核器错 → 责任认定僵局,监管处罚转嫁到调度员个人。

**MUST 化措辞**: 每次 skill 调用 MUST 生成 evidence chain 存档,包含:`input_snapshot_id`(SCADA 量测 hash)+ `tool_call_sequence`(顺序+入参+返回)+ `mechanism_check_results`(机理引擎结论)+ `llm_generation_params`(model/temperature/seed)。chain 内任意环节缺失 → 标 `evidence_incomplete` 不得入正式记录。

#### M6. 法定签名 + WORM 审计(仅 A 类)

电力调控指令在监管层面是**有法律效力的命令**。AI skill 进入这一链条必须给出**可法庭采信**的签名。

访问控制管"谁可以做",签名审计管"做了之后是谁做的、改不了"。两者不是一回事。

**反例**: 发生设备损毁事故,调查组要求复盘"是谁批准了这次操作"。AI skill 日志只有 `agent_id=xxx, prompt=yyy, reply=zzz` → 法律上无法认定 AI 提供商责任,也无法证明操作员"未充分复核",最终被迫按"无法定责"结案。

**MUST 化措辞**: A 类 skill 每次输出 MUST 携带 `(agent_signature, model_version, prompt_hash, evidence_chain_hash, utc_timestamp)` 五元组,签名 MUST 使用国密 SM2 证书;日志 MUST 进入 WORM 存储,保留期 ≥ 国家电网调度日志法定保留年限(常见 10 年以上)。

> 这是**电力监管入网评审必查项** —— 没有不可抵赖审计的 AI skill 根本拿不到投运批准。

#### M7. 跨调度层级权限边界(适用 A、B 类)

中国电力调度分国调 / 网调 / 省调 / 地调 / 县调五级,每一级对电网的调管范围严格隔离。

**LLM 时代的新风险**: 在传统调度自动化里,这层隔离由系统配置静态保证。但**到了 LLM 上下文里,多层级数据可能混入对话** —— LLM 完全可能在"想得更深"的过程中生成跨层级指令,这是行业旧机制覆盖不到的新风险。

**反例**: 省调一个负荷预测 skill,在生成"明日运行方式"时引用了某个 220kV 站(地调辖区)的检修计划,结果建议"开断 110kV 某线" —— 这条 110kV 线属于地调辖区,省调无权指挥。指令下发现场会被拒,但更严重的是**这个建议本身泄露了越权倾向**,在监管检查时是重大不合规。

**MUST 化措辞**: A/B 类 skill MUST 声明 `dispatch_level: {national | regional | provincial | municipal | county}` + `controllable_assets_filter`(基于电网拓扑模型的可控资产列表)。任何输出涉及非声明范围资产 → MUST 标 `out_of_scope` 拒绝。跨层级数据访问 MUST 走调度数据网纵向加密通道,不得旁路。

### 4.2 分阶段 3 条(投运后逐步强制)

#### M8. LLM 延迟硬上限 + 秒级闭环禁区

真正的硬约束不是"分级"而是**禁区**:LLM 不能进入秒级闭环。一次 LLM 调用 P99 延迟在秒级是常态(物理决定),而 AGC、AVC、紧急控制、稳控装置的控制环周期都在毫秒到秒级 —— 物理上不兼容。

**反例**: LLM-driven skill 错部署在 AGC 二次调频闭环里,某一次模型推理超时 5 秒 → 4 个 AGC 周期没有 setpoint 更新 → 联络线功率越限触发解列保护 → 区域电网解列。

把秒级和分钟级并列叙述为"分级",会让厂商误以为"我做得快一点就能上 AGC" —— 这是误导。**这是架构禁区,不是性能调优**。

**MUST 化措辞**: LLM-backed skill MUST 声明 `latency_class`:
- `closed_loop_seconds`(秒级闭环):**禁止使用 LLM 推理**,仅允许用规则引擎或强化学习策略
- `advisory_minutes`(分钟级建议):允许 LLM,P99 延迟 ≤ 10 秒
- `planning_hours_or_more`(小时级以上):允许 LLM,无延迟硬约束

平台 MUST 在调度入口校核 skill 声明的 latency_class 与挂载位置匹配,违反 → 调用拒绝。现阶段先以"禁止部署在 AGC / 紧急控制路径"宣示。

#### M9. 模型版本锁定 + 漂移管控

LLM 提供商升级模型是常态,但**对调度场景,任何一次模型升级都等于换了一次"AI 调度员"**。新模型可能推理倾向不同、温度参数响应不同、对相同提示给出不同结果。

更隐蔽的是 **API 提供商可能在不通知用户的情况下后端切换模型**(eg. 一个 `gpt-4o` 名字下的模型 weights 可能某周静默升级)。调度场景对这种"沉默漂移"零容忍 —— 它意味着调度系统正在用一个"自己不知道版本"的 AI 做决策。

**反例**: 某 skill 在 model-X-2025-Q1 上验证通过投运。半年后 provider 静默升级到 model-X-2025-Q3,行为微调。某次工况下输出更激进 → 人工复核没发现 → 操作偏差。事后查日志发现"是同一个 skill 但模型版本悄悄变了",事故责任认定极难。

**MUST 化措辞**: A/B 类 skill MUST 在元数据声明 `model_lock: {provider, model_id, version, fingerprint}`;每次调用 MUST 校验实际响应中的模型 fingerprint 与声明一致,不一致 → 拒绝输出 + 告警。模型升级 MUST 走"重新评测 + 回放历史 case + 调度专家复审"流水。

#### M10. 投运前历史极端工况回放

任何 AI skill 进入调度生产环境之前,必须证明它在**电网历史发生过的极端工况**下不会出错。这是行业特定的:这条线路 N-1 这条断面失稳过、那个变电站全停过、这个新能源场站脱网过 —— skill 必须在这些 case 上回放并通过。

**反例**: 某新建 skill 在常规工况下表现优秀,但从未在"夏季用电高峰 + 极热天气 + 新能源大幅波动"复合工况下测试过。第一次遇到这种工况,skill 给出明显失误的建议。

历史教训:8.14 美加大停电的根本原因之一就是"系统在某种极端组合工况下从未被测试过"。AI 不能让这个教训重演。

**MUST 化措辞**: A/B 类 skill 投运 MUST 通过国调中心维护的**历史极端工况库回放测试**(具体覆盖项由标委会维护)。回放结果 MUST 作为投运批准前置材料归档。新增极端 case 入库后,既有 skill MUST 在合理周期内重新通过回放。

### 4.3 与"5 条独有要素"的关系

行业内已经有"机理 + AI 双引擎、三重校核、强可解释输出、闭锁绑定、时效分级"5 条独有要素的共识。本规范在此基础上做三件事:

| 行业既有 5 条 | 本规范处理 |
|-------------|----------|
| 机理 + AI 双引擎 | 沉淀为 M1,明确"冲突时机理单调胜出 + 入审计" |
| 三重校核 | 沉淀为 M2(简化为输入 + 输出两道闸),输出校核必须由机理引擎完成 |
| 强可解释输出 | **拆分**: M5 决策溯源链 升 MUST,可解释文案 降 SHOULD |
| 闭锁绑定 | 沉淀为 M3,**强化 client / issuer 边界** |
| 时效分级 | **拆分**: M8 秒级闭环禁区 升 MUST,分级提示 降 SHOULD |
| | 新增 M4 数据质量 / M5 溯源 / M6 法定签名 / M7 跨层级 / M9 模型锁定 / M10 极端回放 |

新增的 6 条不是行业之前没意识到,而是 AI 智能体进入调度环境之前没遇到过的具体场景(LLM 输入 garbage 难以被发现、模型可能静默升级、跨调度层级在 LLM 上下文中容易被打破)。

---

## §5. SKILL.md 编写规范

### 5.1 frontmatter 必需字段

```yaml
---
name: dispatch-fault-handle-220kv-line-trip
description: >
  Handles 220kV line trip with 重合不成功. Use when SCADA reports 220kV line
  trip and 重合闸 fails (or user asks "X 线跳了/重合不成功怎么办").
  Outputs: fault assessment + recovery options + regulation references.
version: 1.0.0
skill_type: atomic | sop | hybrid           # MUST(本规范创新)
allowed-tools:                              # 引用工具,L1.5 MCP server 提供
  - mcp:scada.get_measurement
  - mcp:ems.run_n_minus_1
  - mcp:knowledge.search_proc
metadata:
  x-grid-risk-class: A                      # A / B / C
  x-grid-dispatch-level: provincial         # national / regional / provincial / municipal / county
  x-grid-controllable-assets-filter: "voltage_level <= 500kV AND area = '华东'"
  x-grid-required-certifications: [dispatcher-l3]
  x-grid-requires-two-person-approval: true # A 类 MUST = true(秒级场景例外)
  x-grid-tool-error-policy: strict          # strict / degrade / advisory
  x-grid-latency-class: advisory_minutes    # closed_loop_seconds / advisory_minutes / planning_hours
  x-grid-model-lock:
    provider: anthropic
    model_id: claude-sonnet-4-6
    version: '20251210'
    fingerprint: 'sha256:...'
---
```

#### description 写法 4 条规则

业界对 40+ 失败 skill 的复盘给出最大失效模式: **"Skill 该激活时不激活"** —— 调度员说"把 110-X 线停了做检修",AI 不会触发"操作票生成"skill,因为 description 没出现"倒闸"或"操作票"等正式术语。

Description 写得好坏直接决定 skill 用不用得起来。Tessl × Snyk 给出量化基线:**description 质量分 ≥ 80(满分 100)、触发准确率 ≥ 85%**。

四条规则:
1. **包含触发场景** —— "Use when ..." / "Use even if user only describes goal without these exact terms"
2. **覆盖业务俗称 + 正式术语** —— 用户说"把 110-X 停了做检修"也要触发,不能只用"倒闸操作票"等正式词
3. **声明 out-of-scope** —— 防止 over-trigger 抢其他 skill 的活
4. **第三人称描述** —— Anthropic 明确反对"I can help you..."

反例对照:

```yaml
# ❌ 反模式 — 太宽,会 over-trigger
description: Helps with electrical operations.

# ❌ 反模式 — 暴露内部机制,LLM 不知道何时该选
description: Wraps the D5000 RPC interface for telecontrol commands.

# ❌ 反模式 — 第一人称
description: I can help you generate operating tickets for switching operations.
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

#### Gotchas 对 LLM 执行 skill 的 4 类具体影响

为什么 Gotchas MANDATORY?它在 skill 执行时**实质改变 LLM 行为**,不是装饰性内容:

1. **改变默认假设** —— 把"LLM 默认按通用电网常识办"覆盖成"按本厂实际办"(eg. SCADA 显示 "0/OFF" → 保护逻辑用 "DISCONNECTED")
2. **拒绝错误推理路径** —— 反例式 gotcha 阻断 LLM 错误类比(eg. 母联开关在五防里 type=COUPLER 不是 BREAKER)
3. **触发额外 safety check** —— 遇到特定设备 / 场景时主动多走一步校核(eg. 无远程分闸能力 → 操作票加现场确认字段)
4. **行业级反措 → AI 调度员的肌肉记忆** —— 反措通报 → 受影响 skill 的 Gotchas 段更新流水(governance hook,§8.5),让"老师傅经验"工程化、跨人员、跨调度中心传承

#### Gotchas 写法约束

- **数量 ≥ 3 条**(太少证明业务专家没下功夫挖掘)
- **必须具体到设备 / 场景 / 字段名** —— "注意保护定值"是无效 gotcha,"主变中性点接地刀闸 SCADA 与现场实际相差 180°"是有效 gotcha
- **测试标准**: 每条 gotcha 应该让 LLM **不知道时直接犯错,知道时不犯错** —— 这是判断 gotcha 是否合格的标准
- **超长处理**: 累积超 5000 token 时,按设备类别拆 `references/gotchas-by-equipment-type.md`

#### 调度场景 Gotchas 例子

```markdown
## Gotchas

- 110kV 隔离开关在主变低压侧"分位"信号,在 SCADA 上显示为 "0/OFF",
  但在保护逻辑里对应 "DISCONNECTED"。涉及保护定值时使用后者命名,
  不是 "0/OFF"。

- 双母线接线的"母联开关"在五防闭锁里属于 type=COUPLER 不是 type=BREAKER,
  虽然在调控云图上画法相同。否则 N-1 校核会把它当线路处理导致漏算。

- "调度员命名" 不等于设备双重编号:"#1 主变" 在 A 厂可能对应"#2 主变" 在 B 厂的
  实际编号,跨厂操作必须用双重编号(站名+设备编号)。

- 本断路器无法远程紧急分闸(机构改造未到位),操作票最后步必须现场确认。
```

#### 强制更新流水

每次反措通报 / 安监通报 MUST 触发受影响 skill 的 Gotchas 段更新 → SKILL.md MAJOR 版本升级 → 重新过 §7 Quality Gate。这是 §8.5 知识资产沉淀机制的核心。

### 5.4 体量上限

- SKILL.md 正文 MUST ≤ 500 行 / 5000 token(超出走渐进披露子文件,所有 reference 必须从 SKILL.md 直接 link,**只 1 级深** —— 避免 deeply nested references 导致 LLM 信息丢失)
- description MUST ≤ 200 字符;触发准确率 ≥ 85%

### 5.5 自由度校准(degrees of freedom)

每个 skill 必须明确告诉 LLM"这一步给你多少自由":
- A 类(低自由度):必须严格按序,任何偏离即拒绝
- B 类(中自由度):有偏好模板,允许参数调节
- C 类(高自由度):多种合理路径,给方向不给步骤

### 5.6 Skill 与 system prompt 的边界

业务专家最容易把 skill 当 super-prompt 用 —— 把所有"应该这么做"的话都堆进 SKILL.md。这是上一代 prompt 工程的死胡同(LLM 上下文一旦被超长文挤满,反而会 lost in the middle 漏掉关键信息)。

判断"放 system prompt 还是放 skill"的简单原则:

| 内容 | 放 system prompt | 放 skill |
|------|----------------|---------|
| 全局身份 / 立场 / 安全红线 | ✅ | ❌ |
| 跨任务通用约束(规程引用必填、双人审批) | ✅ | ❌ |
| 特定任务的步骤、模板、闭锁规则、Gotchas | ❌ | ✅ |
| 触发性强的"用户提到 X 才需要"知识 | ❌ | ✅ |

---

## §6. Skill 与外部边界(MUST)

> 5 个边界**互锁**:任一缺位 skill 就退化为"在调度环境里跑的玩具"。

### 6.1 工具边界

skill 调用工具分 6 类:MCP tool(只读 / 计算 / 控制)、CLI 脚本、Python 算法库、其他 skill。错误处置三档(strict / degrade / advisory),按 A / B / C 类映射。

| Policy | 含义 | 适用 |
|--------|------|------|
| strict | tool 失败 → skill 整体失败,不输出建议 | A 类默认 |
| degrade | tool 失败 → skill 输出"信息不全的建议",**显式声明**哪个 tool 失败 | B 类 |
| advisory | tool 失败 → skill 仍出建议,不进闭环 | C 类 |

**关键约束**:
- Python 算法库 MUST 经 MCP server 包装(R8) —— skill 正文不允许让 LLM 任意运行代码
- "skill 调用 skill" MUST 走 host 调度(R6) —— 不在正文 import 别的 skill

**禁止 D1**(最高级红线): tool 失败,LLM "脑补"一个看起来合理的输出。
**禁止 D2**: 在 skill 正文里写"如果 tool 失败,假设量测为典型工况"。

### 6.2 人(HITL)边界

| Skill 类 | HITL 模式 | 说明 |
|---------|----------|------|
| A 控制类 | **必须 HITL + 双签** | 调度员 approve 后才执行 |
| 秒级控制(AGC / 紧控) | **必须不进控制环** | LLM 仅 advisor,不进闭环(M8) |
| B 分析类 | 应当事后复核(HOTL) | 调度员事后复核 |
| C 查询类 | 可以全自动 | |

**关键判断**: HITL 在秒级 SLA 场景**根本不适用** —— 不可能让人 5 秒内决策。这类场景 AI 必须只 advisor 不进环,事后审计兜底。

**调度员信任建立 5 阶段**(应当): 沙盘 → 影子 → 建议 → HITL → 投产。每阶段必须有退出 KPI(详见 §8.1 lifecycle 表)。

### 6.3 知识边界(规程 / RAG)

- 规程 / 安稳导则 / 操作规程 **必须走 RAG 引用**,不得硬编码进 SKILL.md(规程更新就过期)
- 引用必须携带 `(regulation_id, version, clause_id, library_snapshot_hash)`(M9 关联)
- RAG 知识库写入必须经规程编辑流水(双人审批 + 版本记录)
- 设备台账 / 实时量测走 L1.5 MCP tool,不走 RAG
- 历史故障案例走 RAG,但 case_id 必须可回溯到原始故障复盘报告

**4 种知识源对应不同模式**:

| 知识源 | 推荐模式 | 理由 |
|-------|--------|------|
| 规程 / 导则 | RAG 受控 | 文本结构化但庞大,需要语义检索 |
| 设备台账 | KG / MCP tool | 强结构化,需要精确字段查询 |
| 历史案例 | RAG + rerank | 模糊匹配 + 时序权重 |
| 实时量测 / 告警 | MCP tool | 时效性强,不进 RAG 索引 |

### 6.4 已有调度系统边界:L1.5 业务接口适配层(本规范关键创新点)

#### 问题

skill 不是新建系统,它要跟已运行十几年的调度自动化系统协作 —— D5000 OPEN3000、调控云、AGC、AVC、EMS、PMS、OMS。这些系统都有自己的协议(IEC 61850、IEC 60870-5-104、E 文件、厂商私有 API),skill 不可能直接懂这些协议。

**没有适配层,skill 只能在沙盘里跑**。

#### 业界标准为什么没覆盖这一层

L1 通用层(MCP / Anthropic Skills)解决"skill 跟 host 怎么对话";L2 行业约束层(本规范主体)解决"skill 在调度场景必须满足什么硬约束"。中间这一层"skill 怎么跟 D5000 / SCADA 等私有系统对接"**是行业特定的,业界标准不会覆盖**。

但缺了这一层,L1 和 L2 都用不上 —— 这就是本规范要显式定义 L1.5 的原因。

#### L1.5 的设计

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
现有调度系统(D5000 / 调控云 / AGC / EMS ...)
```

#### Schema 基础: CIM 作语义中介

L1.5 不只是协议翻译,更是**语义对齐**。同一个"500kV 线路"在 D5000 / EMS / PMS 里命名不同(D5000: `Line_500kV_001` / EMS: `BRANCH_5001` / PMS: `资产 ID 12345`)。没有语义中介,跨系统协作的 skill 无法工作。

**CIM (IEC 61968 / IEC 61970)** 是国际电力公共信息模型,做的就是这件事。L1.5 MCP server 应当**用 CIM 作为对外暴露的语义层**,各调度系统的内部命名在 server 内部翻译成 CIM。**国网 B 接口**已经在做类似事情,L1.5 可基于 B 接口扩展。

#### 谁来写 L1.5

**推荐由原厂商(南瑞 / 国电南自 / 华为 等)提供** —— 他们最懂自家系统的私有协议。**国调中心**:评审 contract test(跨厂商兼容性必须通过 contract test 验证)、维护 schema 标准、保留升级版本仲裁权。

#### L1.5 的工程价值: 跨厂商互通

有了 L1.5,**skill 可以跨厂商复用**:一个由南瑞写的"检修预案 SOP skill",如果只调用 L1.5 标准 tool,可以直接装到国电南自的平台跑。**没有 L1.5**,每家厂商都要给自己的 D5000 写一遍适配,跨厂商互通就是反复造轮子 —— 这是规范创新点的真正工程价值。

### 6.5 信创合规边界(MUST)

- LLM:**必须国产模型**(文心 / 通义 / 智谱 / 百川 / DeepSeek 等);本地部署,数据不出境
- 签名 / 加密:**必须国密 SM2 / SM3 / SM4**(替代 RSA / SHA-256 / AES);M6 法定签名走 SM2
- 等保:整体架构必须符合等保 2.0 三级 + 关键信息基础设施保护要求
- 数据库 / OS / 中间件 / 芯片:**必须信创全栈**(达梦 / OpenGauss / 麒麟 / 海光 / 鲲鹏 / 飞腾)

### 6.6 多 skill 编排冲突(SHOULD,后期升 MUST)

多 skill 协同部署平台应当维护 skill 优先级与互斥矩阵;冲突建议必须升级到调度员人工裁决;当国调首批 ≥ 5 skill 并发部署 6 月后,本条升 MANDATORY。

---

## §7. Skill 准入 lint(必须,反模式硬底线)

| ID | 反模式 | 正确做法 | 调度灾难等级 |
|----|-------|--------|------------|
| **R1** | LLM 自己写 SQL 查实时量测 | 必须调 `mcp:scada.get_measurement` | 🟡 中 |
| **R2** | LLM 自己实现 N-1 校验 | 必须调 `mcp:ems.run_n_minus_1` | 🔴 高 |
| **R3** | LLM 自己模拟潮流 | 必须调 `mcp:ems.run_powerflow` | 🔴 高 |
| **R4** | LLM 自己生成操作票格式 | 必须调 `mcp:opticket.generate` | 🟡 中 |
| **R5** | LLM 自评 tool 输出(自己当裁判员) | 必须由独立 verify tool 给 verdict(M2) | 🔴 极高 |
| **R6** | skill 内 import 另一 skill 正文 | 必须经 host 调用 | 🟡 中 |
| **R7** | LLM 解释闭锁逻辑 | 必须调 `mcp:interlock.check`,LLM 不可仿真 | 🔴 极高 |
| **R8** | LLM 在 skill 正文里运行任意代码 | 必须经 MCP server 包装 | 🔴 高 |
| **R9** | LLM 编造规程引用 | 必须通过 RAG + library_snapshot_hash 校验 | 🔴 极高 |

**R5(LLM 自评)是调度场景最危险的一条** —— AI 给出建议又自己评估"这个建议安全吗",等于运动员自己当裁判。任何 output 校核必须走机理引擎,不能 LLM 自评。

**额外质量门**(必须):
- description 触发准确率 ≥ 85%(Tessl × Snyk 评分基线)
- prompt 注入安全扫描 0 critical(Snyk ToxicSkills:36% 公开 skill 含 prompt 注入)
- skill 正文 ≤ 500 行 / 5000 token
- 至少 3 条 Gotchas
- A 类 skill 必含 ≥ 1 反例(机理违反场景)
- 所有引用规程条款必须通过 `verify_regulation_refs.py` 一致性检查

### 7.1 Skill 失效 9 类(对应失败模式)

| # | 表现 | 调度灾难等级 |
|---|------|------------|
| 1 | Skill 该激活时不激活(description 触发词不全) | 🔴 高 |
| 2 | Skill 激活但产生 generic 输出(缺 examples / Gotchas) | 🔴 高 |
| 3 | Over-trigger 抢其他 skill | 🟡 中 |
| 4 | Context rot — 长会话后 skill 越写越钝 | 🟡 中 |
| 5 | 越权 / 越能力(没 Boundaries 段) | 🔴 高 |
| 6 | 幻觉式补字段 / 编造规程条款 | 🔴 极高 |
| 7 | Prompt injection 通过 skill 注入 | 🔴 极高 |
| 8 | 指令冲突(skill 内 / skill 间) | 🟡 中 |
| 9 | Outgrowth/Regression(模型升级后行为漂移) | 🟡 中 |

> 1 / 2 / 6 占调研中 70%+ 的失败案例。Quality Gate 必须显式覆盖这三类。

---

## §8. 技能库治理(本规范创新点)

> 技能库是**企业最重要的 AI 资产**(详细论述见导读 §7;详细 governance hook + 集中本地化架构见完整版 §F)。本节列必备要求。

### 8.1 Lifecycle (5 阶段 + retire) + governance hook

```
draft → review → shadow(影子运行) → canary(试点) → production → retired
```

每阶段必须有 governance hook(谁审、看什么、退出 KPI、退回机制):

| 阶段 | 审核人 | 退出 KPI(参考) | 退回触发 |
|------|------|---------------|--------|
| review | 业务专家 + 安全员 | §7 lint 全过 + 双方 sign-off | lint 任一不过 → 退回 draft |
| shadow | 班长 + 业务专家 | 静默运行 ≥ 7 天,日均无误报 / 无机理违反 | 任何机理违反场景被通过 → 退回 review |
| canary | 调度员 + 班长 | 30 天采纳率 ≥ 60%(B 类)/ ≥ 70%(A 类),持续 14 天 | 调度员主动 disable ≥ 3 次 → 退回 review |
| production | 标委会 | 投运批准 + M10 极端 case 回放 + 安全员 sign-off | 任何投运后事故 → 立即退到 shadow,全网 freeze 同类 |
| retired | 任意责任人 | 30 天通知期 + 替代 skill 已 production | — |

production 阶段必须每月做 1 次"幽灵审计"(随机抽 5% 调用复审)。retire 不等于删除 —— 历史调用日志仍可审计(M5 / M6 不豁免)。KPI 数值是行业基准空白,具体待行业共识。

### 8.2 多维分类索引

skill 库必须至少按以下 4 维分类索引,**任意一维独立查询 + 跨维交叉查询**:

- 业务域: 实时监控 / 安稳校核 / AGC / AVC / 检修 / 新能源 / 故障处置 / 分析咨询
- 调度层级: 国调 / 网调 / 省调 / 地调 / 县调
- 风险等级: A / B / C(对应 §1.1)
- skill 类型: atomic / sop / hybrid(对应 §2.3)

平台 SHOULD 暴露语义检索能力(基于 description 向量索引)。

### 8.3 跨厂商互通

- skill 包格式必须遵循 agentskills.io 2025-12-18(对应 §3 L1)
- 厂商专属字段必须在 `vendor.<vendor-name>.*` 命名空间(eg. `vendor.nari.scada_ext.*`)
- skill 间 dependency **禁止跨厂商私有调用**,必须走 L1.5 MCP server(对应 §6.4)
- L1.5 MCP server 跨厂商兼容性必须通过 contract test(国调中心维护这组 contract test)

**反例**: 一个 skill 直接 import 另一厂商的私有 Python 库 → 锁死在该厂商平台 → 不可移植。

### 8.4 版本治理 + 模型版本联动

**SemVer**:
- MAJOR —— 破坏性变更(rename / 删字段 / 改 description 触发条件 / 改 allowed-tools 列表 / Gotchas 段更新)
- MINOR —— 加字段、扩展 description、加 Gotchas 单条
- PATCH —— 仅修复

**MAJOR 升级必须重新过 §7 Quality Gate** —— 不能"小改一下"就投运。引用其他 skill 时必须锁版本范围(类似 npm semver range)。

**模型版本联动**: skill 元数据声明 `model_lock`(M9)。每次调用 host 校验 fingerprint,**不一致 → 拒绝调用 + 告警**。模型升级触发 skill 重评流水(标委会专家复审 + 历史 case 回放)。

### 8.5 知识资产沉淀机制(连接反措 → skill 升级)

每次反措通报 / 安监通报 / 事故复盘 **必须触发**受影响 skill 的 Gotchas 段更新流水(对应 §5.3):

```
反措通报 → 受影响 skill MAJOR 升级 → review → shadow → canary → production
                                                               ↓
                                                  替代旧版本 → 旧版本进 retire
```

这条流水把"老师傅经验"转化成"机器可读 + 跨人员 + 跨调度中心可传承"的资产 —— **这是技能库作为企业资产的核心机制**。**等到 skill 数 50+ 才补这条流水,行业经验已经流失太多** —— 第一天就建立。

### 8.6 集中 + 本地化 架构

技能库分三层维护:

| 层 | 维护方 | 内容 |
|----|------|------|
| 国调中心核心库 | 国调中心 + 标委会 | 跨调度通用 skill(规程检索、N-1 校核、潮流计算 等);极端 case 库;contract test |
| 省调 / 网调 增量库 | 各省调 / 网调 | 本调度区域特有的 skill |
| 地调 / 县调 本地化补丁 | 各地 / 县调 | 本辖区设备特有的 Gotchas + 操作惯例 patches |

**集中 vs 本地化的边界**: 国调核心库用 CIM 标准命名,语义中立;省 / 地调增量库可引入本地化 Gotchas;跨层级共享时**必须脱敏**(去除涉及国安的具体设备 ID / 容量 / 网架细节)。

### 8.7 Skill registry 模式

业界主流模式(JFrog / Tessl × Snyk / Alibaba Nacos / Anthropic Enterprise Skills),本规范沿用:

1. **集中发布,分布消费** —— 国调统一 registry + 各省调本地缓存(脱敏 + 受控同步)
2. **签名 + 完整性校验** —— 国密 SM2 签名(M6)+ skill 包哈希;签名失败 → 拒绝调用
3. **依赖图 + 版本范围** —— skill 间 dependency 显式声明,跨 skill MAJOR 升级时自动产生影响评估报告
4. **审计 + 可观测** —— 每次 skill 调用 emit OpenTelemetry trace 到 registry(对应 M5)

**registry 部署模式 open issue**: 国调统一 registry vs 各省调本地缓存的合规边界(数据出境约束)是政策决策,标委会专项评审。

---

## §9. 不属于本规范的事

- 具体调度业务流程(各调度中心自定 SOP 内容)
- 具体 LLM 模型选型(由信创采购决定)
- L1.5 MCP server 的具体实现(厂商负责)
- 训练数据合规(由数据治理规范覆盖)
- skill 商业模式 / 计费(由商务规范覆盖)
- skill 内部业务算法

---

## §10. 演进机制

- 本规范以 ADR-style 记录变更,每条 MANDATORY 调整必须有理由 + 反对意见 + 投票记录
- 标委会 N+1 评审通过 N 票生效
- 引用业界标准(MCP / Skills / OpenTelemetry / 国密 GM/T 等)版本变更触发本规范 review
- 国调中心维护**极端 case 库**(M10),新增 case 后既有 skill 必须在合理周期内重新通过回放
- 每次反措通报 / 安监通报必须触发既有 skill Gotchas 段更新流水(§5.3 + §8.5)

---

## §A. 附录:Open Issues 待领域专家校对

1. 国标条款号(GB/T 31464 / DL/T 510 / GB 38755 等)正式发布前需电力专家 + 法务复核
2. WORM 法定保留年限(常见 5-10 年,具体年限待法务给定)
3. "机理引擎单调胜出"在新能源高比例场景下的边界(传统机理可能本身不准),待标委会评审特定场景例外
4. 国密 SM2 强制是否过度(会挡非国产 LLM 服务)—— 政策决策
5. 极端 case 库管理(防止变成厂商准入黑名单)
6. 信创 LLM 选型对调度场景能力差异
7. multi-skill 编排 MANDATORY 化时机(SHOULD → MUST)
8. 跨厂商 MCP server contract test 是否标准化
9. Skill registry 集中 vs 分布部署的合规边界(数据出境约束)
10. HITL 5 阶段每阶段 KPI 数值的行业基准

---

*v0.1 标准版起草自 4 份独立 research 报告(共 ~3,400 LOC)。详细 walkthrough / 规程 → skill 4 步转写法 / 实施路径图 / lifecycle hooks 详见完整版。*
