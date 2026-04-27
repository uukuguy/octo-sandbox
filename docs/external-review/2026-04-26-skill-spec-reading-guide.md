# 电力调度 AI 智能体 Skill 规范 v0.1 导读

**日期**: 2026-04-26
**配套文档**: 规范本体三个版本 — 紧凑版、标准版、完整版(均位于同目录)
**读者**: 国调中心调度业务专家、IT 集成商、标准化委员会成员、决策层

> 本文是规范的**主读材料**。它解释规范"为什么这么定、关键决策依据、读者应当如何使用"。
> 规范本体三个版本是参考手册,**不必从头读到尾** —— 看完导读后,带着具体问题查规范的对应章节即可。

---

## 引言:这份规范要解决什么问题

国调中心和上下游厂商面临一个具体困境:**写出来的 AI Skill 投不了产**。

- 规程能写、模型能调、prompt 能写,但落到生产时被一连串问题挡住:
  - 闭锁系统不放行
  - 安监不放行
  - 调度员不敢用
  - 出问题找不到责任人
- 业内试着抄通用 agent 模板,跑得起来但不放心 —— 因为电力调度有它的硬约束,不是任何"通用 skill"能直接套上的
- 各厂商各做各的,跨厂商不互通、跨调度中心不共享、想复用别人的 skill 没法直接接进自己的库里

这就是这份规范要回答的事:**给"什么样的 skill 能在调度场景投运"画一条清晰的线,让所有人(写 skill 的业务专家、做平台的厂商、审批的安监委员会、用 skill 的调度员)对同一个判断标准有共识**。

---

## 第一节:规范的定位与方法论

### 1.1 业界已经有相当成熟的 agent / skill 标准

要想得到一份能落地的规范,**最重要的事是先认清:不要从零开始**。业界的 agent / skill 工业标准这两年快速收敛,我们的规范应当**站在它们肩上**,只补电力调度独有的部分。

关键的几条业界标准:

- **MCP (Model Context Protocol)** —— Anthropic 发起、多厂商支持的开放协议,定义 host(用 LLM 的客户端)和 skill server 之间怎么通信。`tools/list` 列工具、`tools/call` 调用工具、JSON Schema 描述参数 —— 已经是事实工业标准。
- **Anthropic Agent Skills + agentskills.io** —— 2025 年 12 月开源的跨平台 skill 包格式;Codex / Cursor / Gemini CLI / OpenCode / Windsurf 都已对接。
- **OpenTelemetry GenAI Semantic Conventions + W3C Trace Context** —— 调度可追溯性合规硬要求(操作票回溯、事故分析、等保审计)。
- **JSON Schema、SemVer、JSON-RPC 2.0** —— 基础规范层,几乎不用想就该引。
- **OAuth 2.1 / OIDC** —— 远程 server 认证基线。在国内信创合规场景下需替换为国密体系(SM2 / SM3 / SM4),概念照搬即可。

这些标准都是公开可下载、有完整文档、有多家厂商实现的。我们的规范应当**直接引用 + 在前面加一层电力调度专属约束**,而不是重新发明。

### 1.2 这份规范的分层定位

基于上面这点,规范采用四层结构:

```
L0 wire 协议层  —— 直接用 JSON-RPC 2.0 / JSON Schema(基础,不动)
L1 skill 通用层 —— 直接引 MCP / Anthropic Skills / OpenTelemetry(业界标,不重写)
L1.5 业务接口适配层 —— 把 D5000 / 闭锁系统 / SCADA 等私有系统包成 MCP server
L2 调度行业约束层 —— 本规范主体,只规定"调度独有的硬约束"
L3 项目落地层 —— 各调度中心 / 厂商具体 skill 实现
```

读者最需要关心的是 **L1.5 和 L2** —— L0 和 L1 直接用业界标,不需要我们费力气;L3 是各家自己的事。

**L1.5 是这份规范的关键创新**(后文 §6.4 详述)。在业界通用的 L1(skill 协议)和我们要约束的 L2(电力调度行业要求)之间,需要一层"业务接口适配" —— 让 skill 能跟现有调度系统对接。这一层是"没有规范就根本对接不了"的关键缺位。

**L2 是真正的电力调度硬约束** —— 这是规范的主体,后文 §3 给出 7+3 条核心要求。

### 1.3 我们的方法论原则

在起草过程中,我们守住一个判断准则:**每条 MUST,如果违反,会出什么生产事故 / 监管不合规?** 答得出来就保留,答不出来就降级或删除。

这是规范"硬"的来源,也是它能落地的基础 —— 不是"看上去面面俱到",而是"每条都对应真实生产风险"。

---

## 第二节:Skill 的三类划分(后续所有 MUST 的前提)

读后文之前,必须先建立一个共识:**电力调度场景下的 AI Skill 不是同质的**。一个查规程的 skill 和一个发 AGC 指令的 skill,所需要的安全约束差别巨大。如果用同一套硬约束笼统覆盖所有 skill,要么过度工程,要么严重失重。

我们把 skill 按风险划分三类:

| 类别 | 具体例子 | 风险根源 |
|------|--------|--------|
| **A 类:控制类** | 发遥控指令、改定值、合分闸、AGC 指令、生成正式操作票 | 直接动一次电网 —— 误操作可能导致设备损毁、停电、人身伤害 |
| **B 类:分析类** | 潮流计算辅助、检修计划生成、负荷预测、故障诊断、运行方式分析 | 影响调度员判断,但有人复核兜底 —— 错了不会立即出事故,但日积月累可能误导决策 |
| **C 类:查询类** | 规程检索、设备台账问答、历史故障案例查询 | 与一般企业知识库相当 —— 错了用户能立即看出来,影响有限 |

**为什么强调这个划分?** 因为后文每一条 MUST 都标注 "适用 A 类 / B 类 / C 类"。

- 对 C 类查询 skill 强行要求"机理硬约束"是过度工程
- 对 A 类控制 skill 仅要求"可解释输出"远远不够

读规范时,先看这条 MUST 适用于哪类 skill,再决定要不要执行。

---

## 第三节:电力调度独有的硬约束

这一节给出 7 条核心 MUST + 3 条分阶段 MUST。前 7 条是"不满足拿不到投运批准"的硬底线;后 3 条是"投运后逐步强制"的目标。

每一条都标了适用类别(A/B/C),并且配一个反例(没有这条会出什么生产事故)。**对于行业熟悉的硬约束(机理优先 / 闭锁绑定),这里只点到核心**;对于行业不熟悉的新约束(数据质量 / 决策溯源 / 跨调度层级 / 模型版本治理 / 极端工况回放),这里展开多说。

### 3.1 核心 7 条 MUST

#### M1. 机理引擎单调胜出(适用 A、B 类)

电力系统是物理系统,有不可违反的硬约束(基尔霍夫定律、热稳极限、暂态稳定极限、N-1 准则)。LLM 是统计模型,在分布外场景可能给出"违反基尔霍夫"的方案而置信度看起来还高。**AI 输出不能直接覆盖机理结果**。

历史教训:2003 年美加大停电、2012 年印度大停电的根因都是局部决策违反系统级约束。AI 没有理由比人类调度员更"敢"。

**MUST 化措辞**:机理引擎 verdict 与 AI 输出冲突时,机理 verdict 单调胜出,且冲突事件 MUST 入审计日志。这条把"谁裁决冲突"说清楚,而不是"机理优先"这种模糊表述。

C 类查询 skill 豁免本条,因为它们不输出控制建议。

#### M2. 输入侧 + 输出侧两道校核闸(适用 A、B 类)

校核分两道不可省的关口:输入侧(权限 + 数据质量 + 任务边界)+ 输出侧(N-1、潮流、限额)。对 LLM 来说"过程"是黑盒,真正可校的只有输入和输出。

**反例**:AI 推荐"500kV 某线路开断",直接生成操作票。但该线路开断后另一回线潮流 105% 长期稳定极限,N-1 失效 → 后续任何故障都可能发展成系统瓦解。

**MUST 化措辞**:输出校核 MUST 调用机理引擎(潮流计算、N-1 扫描),**不可由 LLM 自评**(自评 = 让裁判员同时当运动员)。

#### M3. 闭锁系统作为外部权威(仅 A 类)

这条不仅是合规要求,也是**电力调度行业现行规程已经强制的**(操作票 + 双人复核 + 闭锁逻辑是 N 多年的运行实践)。AI 进入控制环不能架空既有闭锁机制。

**反例**:AI skill 输出"开断 500kV 某线",绕过常规挂牌闭锁直接下发 → 现场带电检修人员未撤离 → **人身事故**。这是行业最敏感的红线。

**MUST 化措辞**:**闭锁系统作为外部权威,AI skill 仅作 client(消费判决),不可作 issuer(创建闭锁条目)** —— AI 作为新增智能体,绝不能给自己开闭锁后门。

#### M4. 数据质量与时间戳一致性(适用 A、B 类)

> 这条是新约束(行业之前没遇到过 LLM 时代的相应问题),展开多说。

电力调度的输入数据是 SCADA 量测,但量测**天然带不可靠性**:

- 数据 stale(传感器掉线,值冻结但状态位未刷新)
- 数据 bad(校验位失败)
- 时间戳跳变(主备切换 jitter)
- PMU 不同步

**garbage in, garbage out**。这件事在调度场景比其他行业更危险:

- 输出不准还能被输出校核(M2)拦下
- 输入不可信连校核都校不出来 —— 因为校核器的输入也是同一份脏数据

人类调度员看到一条"潮流稳定但断路器状态显示离线"的告警会立刻起疑;LLM 没有这种"反常感",会按"数据是准的"做推理。

**反例**:某变电站 RTU 通信中断 30 分钟,SCADA 数据冻结但状态位未刷新。AI skill 看见"潮流稳定"就给出"维持现方式"建议。实际线路在断面已超载 5%。无 stale 检测 → 错失干预窗口。

**MUST 化措辞**:A/B 类 skill MUST 声明所依赖的量测点列表,平台在每次调用前 MUST 注入数据质量元数据(timestamp / quality_flag / staleness_seconds);任一关键量测 stale 超阈值或 quality bad,skill MUST 返回 `INPUT_QUALITY_FAILED` 错误,**不得继续推理**。

#### M5. 决策溯源链(适用所有类别)

> 这条要把两件事区分开:**可解释文案**(SHOULD)和**决策溯源链**(MUST)。

- **可解释文案** —— 面向人,目的是让调度员看懂"AI 为什么这么建议"
- **决策溯源链** —— 面向监管 + 事故复盘,目的是回答"输入数据是什么 + 调用了哪些工具 + 机理校核给了什么结果 + LLM 生成时模型版本/温度/seed 是什么"

**前者重要但是 SHOULD,后者必须是 MUST**。LLM 可以编造一个"看起来像规程条款"的引用("GB/T 31464-2015 §6.3" —— 这条号可能根本不存在),如果没有下游做引用真实性校验,可解释性等于装饰。但溯源链不一样,它是责任认定的物理基础。

**反例**:一次 AI 推荐造成调度员误操作。事故复盘时,要重现 AI 当时"看到了什么数据、做了什么校核",但日志只存了 prompt + reply,RAG 检索结果丢失,机理校核结果丢失 → 无法判断是 AI 错、数据错还是校核器错 → 责任认定僵局,监管处罚转嫁到调度员个人。

**MUST 化措辞**:每次 skill 调用 MUST 生成 evidence chain 存档,包含:input_snapshot_id(SCADA 量测 hash 引用)、tool_call_sequence(顺序+入参+返回)、mechanism_check_results(机理引擎结论)、llm_generation_params(model/temperature/seed)。chain 内任意环节缺失 → skill 调用 MUST 标记为 `evidence_incomplete` 不得入正式记录。

#### M6. 法定签名 + WORM 审计(仅 A 类)

电力调控指令在监管层面是**有法律效力的命令**,谁下发、谁复核、谁执行有法定追责链条。AI 进入这一链条必须给出**可法庭采信**的签名 —— 不是 LLM 自带的"我建议",而是带数字证书 + 时间戳 + 哈希链的不可抵赖记录。

**反例**:发生设备损毁事故,调查组要求复盘"是谁批准了这次操作"。AI skill 日志只有 "agent_id=xxx, prompt=yyy, reply=zzz" → 法律上无法认定 AI 提供商责任,也无法证明操作员"未充分复核",最终被迫按"无法定责"结案。

这一条是**电力监管入网评审必查项** —— 没有不可抵赖审计的 AI skill 根本拿不到投运批准。

**MUST 化措辞**:A 类 skill 每次输出 MUST 携带 `(agent_signature, model_version, prompt_hash, evidence_chain_hash, utc_timestamp)` 五元组,签名 MUST 使用电力调度专用 PKI(国密 SM2)证书;日志 MUST 进入 WORM(一次写入多次读取)存储,保留期 ≥ 国家电网调度日志法定保留年限(常见 10 年以上)。

#### M7. 跨调度层级权限边界(适用 A、B 类)

> 这是行业熟悉但 LLM 时代特别危险的约束 —— 多层级数据混入 LLM 上下文容易"越想越大",展开多说。

中国电力调度分国调 / 网调 / 省调 / 地调 / 县调五级,**每一级对电网的调管范围严格隔离**:省调不能直接遥控县调辖区设备,反之亦然。在传统调度自动化里,这层隔离由系统配置静态保证。但**到了 LLM 上下文里,多层级数据可能混入对话** —— LLM 完全可能在"想得更深"的过程中生成跨层级指令,这是行业旧机制覆盖不到的新风险。

**反例**:省调一个负荷预测 skill,在生成"明日运行方式"时引用了某个 220kV 站(地调辖区)的检修计划,结果建议"开断 110kV 某线" —— 这条 110kV 线属于地调辖区,省调无权指挥。指令下发现场会被拒,但更严重的是**这个建议本身泄露了越权倾向**,在监管检查时是重大不合规。

**MUST 化措辞**:A/B 类 skill MUST 声明 `dispatch_level: {national | regional | provincial | municipal | county}` + `controllable_assets_filter`(基于电网拓扑模型的可控资产列表)。任何输出涉及非声明范围资产 → MUST 标 `out_of_scope` 拒绝。跨层级数据访问 MUST 走调度数据网纵向加密通道,不得旁路。

### 3.2 分阶段 3 条 MUST

下面三条评级为"高优先级 MUST,但可分阶段强制"(给行业一个落地缓冲期)。

#### M8. LLM 延迟硬上限 + 秒级闭环禁区

> 这是规范容易被误解的一条,展开多说。

行业有"时效分级"的共识(秒级 / 分钟级 / 小时级 / 日级),但真正的硬约束不是"分级",而是**禁区**:**LLM 不能进入秒级闭环**。这不是"应该尽量快",而是**架构禁区**。

一次 LLM 调用 P99 延迟在秒级是常态(不是慢,是物理决定),而 AGC、AVC、紧急控制、稳控装置的控制环周期都在毫秒到秒级 —— 物理上不兼容。把秒级和分钟级并列叙述为"分级",会让厂商误以为"我做得快一点就能上 AGC",这是误导。

**反例**:LLM-driven skill 错部署在 AGC 二次调频闭环里,某一次模型推理超时 5 秒,导致 4 个 AGC 周期没有 setpoint 更新,联络线功率越限触发解列保护 → 区域电网解列。

**MUST 化措辞**:LLM-backed skill MUST 声明 `latency_class`:

- `closed_loop_seconds`(秒级闭环):**禁止使用 LLM 推理**,仅允许用规则引擎或强化学习策略
- `advisory_minutes`(分钟级建议):允许 LLM,P99 延迟 ≤ 10 秒
- `planning_hours_or_more`(小时级以上):允许 LLM,无延迟硬约束

平台 MUST 在调度入口校核 skill 声明的 latency_class 与挂载位置匹配,违反 → 调用拒绝。现阶段先以"禁止部署在 AGC / 紧急控制路径"宣示;延迟 SLA 细则后续完善。

#### M9. 模型版本锁定 + 漂移管控

> 这条是 LLM 时代特有的、行业完全没遇到过的新风险,展开多说。

LLM 提供商升级模型是常态,但**对调度场景,任何一次模型升级都等于换了一次"AI 调度员"**。新模型可能推理倾向不同、温度参数响应不同、对相同提示给出不同结果。如果不锁版本,昨天测试通过的 skill 今天可能就突然行为漂移。

更隐蔽的是:**API 提供商可能在不通知用户的情况下后端切换模型**(eg. 一个 `gpt-4o` 名字下的模型 weights 可能某周静默升级)。调度场景对这种"沉默漂移"零容忍 —— 它意味着调度系统正在用一个"自己不知道版本"的 AI 做决策。

**反例**:某 skill 在 model-X-2025-Q1 上验证通过投运。半年后 provider 静默升级到 model-X-2025-Q3,行为微调。某次工况下输出更激进 → 人工复核没发现 → 出现操作偏差。事后查日志,发现"是同一个 skill 但模型版本悄悄变了",这种事故责任认定极难。

**MUST 化措辞**:A/B 类 skill MUST 在元数据声明 `model_lock: {provider, model_id, version, fingerprint}`;每次调用 MUST 校验实际响应中的模型 fingerprint 与声明一致,不一致 → 拒绝输出 + 告警。模型升级 MUST 走"重新评测 + 回放历史 case + 调度专家复审"的流程,不能默认继承上一版本投运批准。

现阶段先要求 fingerprint 校验;升级评测流程后续标委会发指引。

#### M10. 投运前历史极端工况回放

任何 AI skill 进入调度生产环境之前,必须证明它在**电网历史发生过的极端工况**下不会出错。这不是"测试覆盖率"那种工程概念,而是行业特定的 —— 这条线路 N-1 这条断面失稳过、那个变电站全停过、这个新能源场站脱网过 —— skill 必须在这些 case 上回放并通过。

通用行业的测试覆盖率不能替代调度行业的"已知极端工况扫描"。8.14 美加大停电的根本原因之一就是"系统在某种极端组合工况下从未被测试过";AI 不能让这个教训重演。

**MUST 化措辞**:A/B 类 skill 投运 MUST 通过国调中心维护的**历史极端工况库回放测试**(具体覆盖项由标委会维护)。回放结果 MUST 作为投运批准前置材料归档。新增极端 case 入库后,既有 skill MUST 在合理周期内重新通过回放。

现阶段以"声明已回放"+ 抽查为主。

### 3.3 与"5 条独有要素"的关系

行业内已经有"机理 + AI 双引擎、三重校核、强可解释输出、闭锁绑定、时效分级"这五条独有要素的共识。本规范在此基础上做了三件事:

| 行业既有 5 条 | 本规范处理 |
|-------------|----------|
| 机理 + AI 双引擎 | 沉淀为 M1,明确"冲突时机理单调胜出 + 入审计" |
| 三重校核 | 沉淀为 M2(简化为输入 + 输出两道闸),且**输出校核必须由机理引擎完成,不可由 LLM 自评** |
| 强可解释输出 | 拆分:**M5 决策溯源链 升 MUST**,可解释文案降为 SHOULD |
| 闭锁绑定 | 沉淀为 M3,**强化 client/issuer 边界**(AI 不可创建闭锁条目,只能消费判决) |
| 时效分级 | 拆分:**M8 秒级闭环禁区 升 MUST**,分级提示降为 SHOULD |
| | 新增 M4 数据质量、M5 溯源(独立)、M6 法定签名、M7 跨层级、M9 模型锁定、M10 极端回放 |

**简单说**:行业既有 5 条都保留;5 条中有 2 条做了拆分(把面向人的可解释和面向监管的溯源拆开;把"分级"和"禁区"拆开);并补充了 6 条与 LLM 时代相关的新约束(数据质量、签名审计、跨调度层级、模型版本治理、极端工况回放)。

这 6 条新约束**不是行业之前没意识到**,而是 AI 智能体进入调度环境之前没遇到过的具体场景 —— LLM 输入 garbage 难以被发现、模型可能静默升级、跨调度层级在 LLM 上下文中容易被打破、监管追责需要法定签名 —— 这些是 LLM 时代特有的。

---

## 第四节:Skill 不是 prompt —— 写作方法论

### 4.1 Skill 和 prompt 是两类不同的工程产物

| 维度 | Prompt | Skill |
|------|--------|-------|
| 触发方式 | 用户每次手动输入 | LLM 根据 description 自主判断是否激活 |
| 生命周期 | 单次对话 | 持久化文件,跨会话 / 跨用户复用 |
| 结构 | 自由文本 | 结构化目录(SKILL.md + scripts/ + references/ + assets/) |
| 可观测性 | 跑一次过一次 | 可版本化、可 review、可 evaluate、可 deprecate |

写 skill 需要的能力是:**领域专业 + 模式抽象 + 文档工程 + 一定的 LLM 行为感知**。它不是 prompt 工程的"加长版",而是介于 prompt 工程、技术文档、API 设计、SOP 标准化作业书之间的**新文档类型**。

业务专家最容易绊倒的地方是**写作视角的迁移**。下面四条是关键。

### 4.2 Skill 写作的四个迁移

**第一,从"我现在告诉 AI 怎么做"→"未来一类任务都让 AI 这么做"**

prompt 是"这一次"的指令;skill 是"每一次符合此模式时"的程序。这要求作者有**模式抽象能力**:从一次具体调度操作里识别出通用步骤、留出可变参数、分离硬约束和启发式建议。

Anthropic 推荐的方法是 **"hands-on 倒推法"**:业务专家先用普通 prompt 跟 AI 跑一次真实任务,然后回过头让 AI 把"这次成功的步骤、纠正点、上下文"提炼成 skill。**不是先写 skill,而是先成功一次再固化**。这个方法对调度场景特别合适 —— 调度规程是 declarative 的(应当如何),而 skill 需要的是 procedural 的(具体怎么做),从规程直接演绎成步骤往往出问题,从一次成功操作倒推则自然。

**第二,从"自由发挥"→"degrees of freedom 校准"**

prompt 不需要担心 AI 自由度;skill 必须**明确**告诉 AI "这一步给你多少自由":

- **High freedom(高自由度)**:多种合理路径,给方向不给步骤 —— 适合检修方案分析
- **Medium freedom(中自由度)**:有偏好模板,允许参数调节 —— 适合操作票草稿、负荷预测
- **Low freedom(低自由度)**:必须 exact sequence,任何偏离即拒绝 —— 适合发遥控指令、改定值

调度场景里**控制类 skill 一定 low freedom,辅助分析 medium,信息查询 high**(对应 §2 的 A/B/C 三类划分)。

**第三,从"读者是 AI"→"读者是 AI + 未来的同事"**

skill 是会被另一位调度员读、review、改的工程产物。每行字得能解释"为什么这么写"。业界明确反对的反模式是 **"让没有领域知识的 LLM 直接生成 skill,结果是通用过程,毫无生产价值"** —— skill 必须由领域专家主导写作,LLM 是辅助。

**第四,从"想到啥说啥"→"省 token 预算"**

Skill 一旦激活,full body 进入 context window,跟对话历史、其他 skill metadata 抢空间。Anthropic 硬性建议:**SKILL.md body 控制在 500 行 / 5000 token 以内**。这跟 prompt 工程的"越详细越好"是相反的直觉 —— skill 要做的不是把所有事都说,而是说**最关键的、其他 skill 学不到的**。

### 4.3 Progressive disclosure(渐进披露)—— skill body 不是越长越好

Anthropic 把 skill 设计的核心原则定义为 **progressive disclosure**(渐进披露,3 级):

```
第 1 级:metadata (name + description) — 启动时全员加载,~50-100 token/skill
第 2 级:SKILL.md body — LLM 决定用此 skill 时才读,< 500 行
第 3 级:references/*.md, scripts/*.py, assets/* — body 引用时才按需读
```

调度 skill 的层级建议:

```
第 1 级 (description):
  "Generates 220kV substation switching operation tickets..."

第 2 级 (SKILL.md body, < 500 行):
  - Critical Rules (硬约束 — 闭锁条件、双人审批、机理校核 reject)
  - Workflow steps (查台账 → 排序 → 闭锁校核 → 输出 → 复核)
  - Out of scope (不做调度命令直接下发,不做带电作业方案)

第 3 级 (references/, 按需加载):
  - references/standard-templates.md (各电压等级操作票模板)
  - references/interlock-rules.md (五防闭锁规则全集)
  - references/troubleshooting-faq.md (常见 reject 原因 + 修复)
  - scripts/validate_ticket_format.py (格式校验,直接执行)
```

**关键反模式**:Anthropic 反复警告 **"deeply nested references"** —— 当 references 文件再引用 sub-references 时,LLM 经常用 `head -100` 部分读,导致信息丢失。**所有 reference 必须从 SKILL.md 直接 link,只 1 级深**。

### 4.4 SKILL.md body 必备的几个段

业界对 40+ 失败 skill 的复盘归纳出"必备 4 段"结构,加上调度场景的特化:

```markdown
# {Skill Name}

## Critical Rules    # 不可违反的硬约束 — 在最显眼处
- MUST [硬约束 1]
- MUST NOT [禁止 1]
- 任何 ambiguity → ask user, do not assume

## Core Workflow    # 主步骤,每步可勾选
1. [step 1, 含调用什么 tool]
2. [step 2, 含决策点]
3. [step 3, 含 validation]

## Examples    # 输入 → 输出对,>= 2 个,真实数据
### Example 1: 典型场景
### Example 2: edge case

## Boundaries    # 范围 + 不在范围
**In scope**: ✓ ...
**Out of scope**: ✗ ... → use {other-skill} instead
**When unclear**: ask 2-3 specific clarifying questions

## Mechanism Constraints (调度专属)    # 引规程条款 + 数值阈值
## Interlock Requirement (调度专属,A 类必有)
## Output Provenance (调度专属,M5 落地)
## Escalation (调度专属,A 类必有)
## Gotchas (调度专属创新点,详见 §4.5)
```

### 4.5 Gotchas 段 —— 调度行业最有价值的内容,本规范的创新点

> 这是规范的核心创新点之一,值得详细展开。

#### 4.5.1 Gotchas 是什么

Anthropic best practices 给出一个重要论断:**"highest-value content in many skills is a list of gotchas — environment-specific facts that defy reasonable assumptions"**(在很多 skill 里,价值最高的内容就是一组 gotcha —— 那些环境特有、违反常识假设的具体事实)。

这一条对调度行业特别重要 —— 因为调度行业的 gotcha **几乎都是已经存在的实物**:反措通报、安监通报、事故复盘报告。每条 gotcha 等于一次"差点出事"或"已经出事"的反直觉具体事实。**这些是规程做不到的** —— 规程是 declarative(应当如何),Gotcha 是反直觉具体事实(实际是这样)。

调度场景的 Gotchas 示例:

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

#### 4.5.2 Gotchas 对智能体执行 skill 的 4 类具体影响

为什么 Gotchas 段 MANDATORY?因为它在 skill 执行时**实质改变 LLM 行为**,不是装饰性内容。具体有 4 类影响:

**第一类:改变 LLM 的"默认假设"**

LLM 拿一个任务,会用训练数据里学到的"通用电网知识"补全空白。这些通用假设大多数时候是对的,但**反措通报记录的恰恰是"通用假设错的"那些场景**。Gotchas 把这些反直觉事实显式塞进 LLM 的 working context,让它不要套通用假设。

举例:看到"主变低压侧隔离开关分位",LLM 默认会按"OFF" / "0" 去查保护定值;有了上面第一条 Gotcha,它会改用 "DISCONNECTED" 查,**直接命中而不是返回"未找到"**。

**第二类:拒绝 LLM 的某些推理路径**

有些 Gotchas 不是补充信息,而是**反例**:"如果 AI 想这么做,停下"。这类 Gotcha 配合 SKILL.md 的 Critical Rules,会直接阻断错误推理。

举例:LLM 看到调控云图上的母联开关,长得像 BREAKER,会调 `mcp:ems.run_n_minus_1(type=BREAKER)` —— 类型传错,N-1 算错。有了第二条 Gotcha,它会主动改用 `type=COUPLER`,避开错误路径。

**第三类:触发额外的复核步骤(safety net)**

某些 Gotchas 会让 skill 在执行到特定步骤时**主动多走一步校核**,即使常规流程不要求。

举例:LLM 按常规 SOP 输出"远程分闸"作为操作票最后步;遇到第四条 Gotcha 标记的设备,它会**自动追加** "现场确认人 + 现场口令" 字段,并输出告警"本设备无远程紧急分闸能力,需现场配合" —— 把"现场配合"这个原本依赖人记忆的步骤,固化进 AI 行为。

**第四类:行业级反措 → AI 调度员的肌肉记忆(长期最大价值)**

电力调度行业每年产生大量反措通报、安监通报、事故复盘报告。**这些是行业最值钱的运行经验,但传统上靠人记忆** —— 老师傅口头传授,新人慢慢积累,经验是私人的、易流失的。

Gotchas 段配合本规范定义的强制更新流程(governance hook),把这些经验**机器可读化**:

```
反措通报发布
   ↓
标准化工作组提取 Gotcha 草稿
   ↓
评审(业务专家 + 安全员 sign-off)
   ↓
更新所有受影响的 SKILL.md Gotchas 段
   ↓
skill MAJOR 升级(因为 body 改变 → 重新过 lint + Quality Gate)
   ↓
新版本投运 → 所有调度员的 AI 助手立刻"知道"这次反措
```

对智能体执行 skill 的具体影响:
- 行业级反措一发布,**几天内**所有 skill 都能反映这个反措 —— 不再是单个调度中心慢慢内化
- 不会"老师傅退休带走经验" —— 经验已经凝固在 skill 库里
- 跨调度中心可以共享 Gotchas(脱敏后),减少"每家自己踩一遍"的浪费

**这一类是 Gotchas 段最大的长期价值** —— 把行业的"集体反措经验"工程化成 AI 调度员的"肌肉记忆"。

#### 4.5.3 Gotchas 的写作 + 治理约束

Gotchas 段 MANDATORY,但写不好会变成"注意安全"式的废话。规范定义以下约束:

- **数量 ≥ 3 条** —— 太少证明业务专家没下功夫挖掘
- **必须具体到设备 / 场景 / 字段名** —— "注意保护定值"是无效 Gotcha,"主变中性点接地刀闸 SCADA 与现场实际相差 180°"是有效 Gotcha
- **测试标准**: 每条 Gotcha 应该让 LLM **不知道时直接犯错,知道时不犯错** —— 这是判断 Gotcha 是否合格的标准
- **超长处理**:Gotchas 累积是常态,一旦 SKILL.md 体量超 5000 token,应按设备类别拆 `references/gotchas-by-equipment-type.md`,不能硬塞 SKILL.md
- **强制更新流程**:每次反措通报 / 安监通报 MUST 触发受影响 skill 的 Gotchas 段更新 → SKILL.md MAJOR 版本升级 → 重新过 Quality Gate

#### 4.5.4 Gotchas 与调度员信任建立的反馈循环

Gotchas 段还有一个间接价值:**加速 §6.2 的"调度员信任建立 5 阶段"**。

一旦 Gotchas 真正生效几次(eg. AI 因为某条 Gotcha 拦下了一个潜在的误操作),调度员对 AI 的信任会显著上升 —— 因为 **AI 不只是"通用工具",而是"知道我们这一片实际怎么回事"的工具**。这种"地方知识感"是其他维度的硬约束(机理校核 / 闭锁绑定)做不到的,因为那些是"防止出错",而 Gotchas 是"主动展现对环境的细节理解"。

### 4.6 Description 是 skill 触发的命门

业界对 40+ skill 失败的调研给出最大失效模式是 **"Skill 该激活时不激活"**。调度员说"把 110-X 线停了做检修",AI 不会触发"操作票生成"skill,因为 description 没出现"倒闸"或"操作票"等正式术语。

Description 写得好坏直接决定 skill 用不用得起来。Tessl × Snyk 给出量化评分基线:**description 质量分 ≥ 80(满分 100)、触发准确率 ≥ 85%**。

四条规则:

1. **包含触发场景** —— "Use when ...","Use even if user only describes goal without these exact terms"
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

# ✅ 修复
description: >
  Generates switching operation tickets (倒闸操作票) for substations.
  Use when the user wants to take a line/transformer/feeder out of service,
  put it back in service, isolate equipment for maintenance, or asks for
  操作票 / 倒闸顺序 / 检修隔离 / 送电步骤 / 停电步骤.
  Use even if user only describes the goal (e.g. "把 110-X 停了做检修")
  without using these exact terms.
```

### 4.7 Skill 与 system prompt 的边界

业务专家最容易把 skill 当 super-prompt 用 —— 把所有"应该这么做"的话都堆进 SKILL.md。这是上一代 prompt 工程的死胡同。

判断"放 system prompt 还是放 skill"的简单原则:

| 内容 | 放 system prompt | 放 skill |
|------|----------------|---------|
| 全局身份 / 立场 / 安全红线 | ✅ | ❌ |
| 跨任务通用约束(规程引用必填、双人审批) | ✅ | ❌ |
| 特定任务的步骤、模板、闭锁规则、Gotchas | ❌ | ✅ |
| 触发性强的"用户提到 X 才需要"知识 | ❌ | ✅ |

调度场景具体例子:

```
✓ system prompt 包含:
  - "你是一个电力调度辅助 AI,所有控制类输出 MUST 经过机理校核"
  - "MUST 引用规程条款编号 + 量化校核数据"
  - "禁止跨调度层级越权"

✓ skill 包含:
  - "220kV 倒闸操作的 13 步标准流程"(specific workflow)
  - "母线故障应急预案的步骤树"(specific decision tree)
  - "AGC 调节 skill 的限值表"(specific lookup data)
```

---

## 第五节:原子 Skill vs SOP Skill —— 调度场景的关键二分(规范创新点之二)

> 业界标准没有这个二分;本规范在调度场景下显式做出二分。

### 5.1 业界为什么没有这个二分

业界 agent / skill 工业标准在 "原子能力 vs 流程编排" 的命名上还没形成单一共识 —— 不同框架(Anthropic Skills、Microsoft Semantic Kernel、CrewAI、LangGraph、Microsoft Agent Framework)各有自己的叫法:

| 框架 | 原子(单一能力) | SOP(流程编排) |
|------|---------------|--------------|
| **Anthropic Agent Skills** | "Skill" — 一个 SKILL.md 目录 | 同样是 Skill,但 body 是 multi-step workflow |
| **Microsoft Semantic Kernel** | Plugin function | Process Framework / Workflow |
| **CrewAI** | Task | Crew |
| **LangGraph** | Tool / Node | Graph / StateGraph |
| **Microsoft Agent Framework** | Agent / Tool | GroupChat / Workflow |

agentskills.io 规范本身**不强制区分**,Anthropic best practices 也只给出定性提示("Design coherent units. Skills scoped too narrowly force multiple skills to load for a single task. Skills scoped too broadly become hard to activate precisely.")。这对企业用户在写第一个 skill 时不够具体 —— 他们需要一个**可操作的判断标准**,而不是一句"看情况"。

但业界正在快速收敛到一个共识:**纯 LLM 自主调度多步骤是不可靠的,必须有显式编排兜底**:

- Microsoft Semantic Kernel **弃用 Planner**,改用 function calling loop + Process Framework
- AutoGen 标 deprecated,迁移到 Microsoft Agent Framework 的 Workflow
- CrewAI 推 hierarchical Process(由 manager agent 显式分配)取代纯 sequential

这个共识对调度场景**强制必需** —— 电网不允许 AI 即兴发挥。所以本规范在调度场景下**显式做出原子 / SOP 二分**,给业务专家一个明确的判断标准。

### 5.2 二分定义

| | 原子 skill | SOP skill |
|---|-----------|-----------|
| 含义 | 单一可重用能力 | 业务流程级编排 |
| 例子 | 查 SCADA 量测、算 N-1、写操作票表头、校核五防 | 检修流程、故障处置预案、操作票编制全流程 |
| 调用其他 skill | 不允许 | 允许(MUST 经 host,不能在 body 里 import) |
| 状态 | 无状态 | 跨步骤状态 |
| 粒度准则 | 1 skill = 1 verb + 1 noun | 多步、跨工具 |
| 测试 | 给输入查输出,与其他 skill 解耦 | 端到端流程 + 边界状态 |

### 5.3 A 类控制 SOP MUST 用静态步骤的物理理由

> 这条是规范"硬"的来源之一,值得讲清楚。

A 类控制 SOP **MUST 用静态 numbered steps,不允许 LLM 动态决定流程顺序**。这条约束的物理理由是 **"不可回放复审"**:

如果 LLM 在 SOP 里动态决定 step 顺序("if 当前是大方式, do A; else do B"),那么:
- 投运前 review 时,reviewer **看不到"AI 实际会怎么走"** —— 因为这取决于运行时的输入数据
- 投运后事故复盘时,如果调度员说"AI 让我做的",**审计组没办法验证"如果重来一次,AI 还会做同样的决定吗"** —— LLM 输出有随机性,即使锁了 model_lock 和 temperature,推理路径仍可能不同
- 监管追责时,**没办法证明"这个 SOP 在这个场景下应该怎么走是预期的"** —— 因为根本没有"应该"的预期,只有 LLM 当时怎么走的

而静态 numbered steps 把这个不确定性消除:reviewer 看到 13 步就是 13 步,事故复盘时调度员实际走了哪几步在日志里写得明明白白,监管追责时"预期路径 vs 实际路径"清晰对比。

举一个对比例子:

```markdown
# ❌ 反模式 — 让 LLM 自由分支(A 类禁止)
## Workflow
For 220kV line outage:
- If line is in operation, ...
- If line is already de-energized, ...
- Decide based on current state...

# ✅ 修复 — 静态 numbered steps
## Workflow: Generate 220kV Switching Operation Ticket
1. Read source substation single-line diagram (call: scada_query_topology)
2. Identify switching boundary (operating equipment + auxiliary equipment)
3. For each isolation: validate against five-prevention rules
   (call: interlock_check, MUST exit if any rule fails)
4. Order operations: open breakers FIRST, then disconnectors, then earthing
5. Output formatted ticket per references/template-220kV.md
6. Append regulation references (cite exact 条款编号 from regulation DB,
   never invent)
```

注意:这不是说"A 类 SOP 不能有分支" —— 上面的步骤 3 仍然有分支(if any rule fails)。区别在于**分支条件是显式的、有限的、写死在 skill 里的,不是 LLM 在推理时自由产生的**。LLM 的角色是"按照写死的步骤执行 + 决定参数",不是"决定下一步该做什么"。

### 5.4 二分对业务专家的具体决策影响

业务专家拿到一个调度任务,**第一件事不是"开始写 skill",而是"判断这是原子还是 SOP"**。两种 skill 的设计成本、复用模式、出错代价完全不同:

| 决策维度 | 写原子 skill | 写 SOP skill |
|--------|------------|------------|
| 开发成本 | 低(几小时到几天) | 高(几天到几周) |
| 测试成本 | 低(unit test 即可) | 高(端到端 + 状态校核) |
| 复用价值 | 高(被多个 SOP 调用) | 中(场景特定) |
| 失败影响 | 局部(被调用 SOP 该处失败) | 全局(整个流程 abort) |
| 是否触发 §6.4 L1.5 | 几乎都触发(直接调 D5000-MCP / EMS-MCP) | 间接触发(通过原子 skill) |

**简单的判断流程**:

```
任务能用一句"verb + noun"描述吗?(eg. "查 SCADA 量测")
├─ 是 → 写原子 skill
└─ 否(任务包含多步、跨工具、有状态)
    ├─ 任务是 A 类控制吗?
    │  ├─ 是 → SOP skill,MUST 用静态 numbered steps
    │  └─ 否 → SOP skill,可以有适度自由分支
    └─ 写 SOP 之前先看:能不能拆成"几个原子 skill + 一段编排"?
       ├─ 能 → 优先拆,原子 skill 复用价值高
       └─ 不能 → 接受 SOP 形态
```

### 5.5 调度领域原子 skill 候选清单

业务专家可以参考下面这份清单做"原子 skill 库"的初版骨架:

```
信息获取类:
  - query-scada-realtime         查 SCADA 实时量测
  - query-scada-historical       查 SCADA 历史
  - query-equipment-ledger       查设备台账
  - query-protection-settings    查保护定值
  - query-regulation             查规程条款
  - query-maintenance-window     查检修计划

机理计算类:
  - calc-powerflow               潮流计算
  - calc-n-minus-1               N-1 校验
  - calc-stability-margin        稳定裕度计算

校核类:
  - verify-five-prevention       五防闭锁校核
  - verify-dispatch-authority    调度权限校核
  - verify-data-quality          数据质量校核

生成类:
  - generate-ticket-header       生成操作票表头
  - format-decision-explanation  格式化决策解释
```

**SOP skill 的实际例子**:输变电检修预案、故障处置预案、操作票编制全流程。这些 skill 内部会编排上述原子 skill。

---

## 第六节:Skill 不是孤岛 —— 五个外部边界

skill 写得再好,如果不能跟外部世界打交道,只能在沙盘里跑。这一节讲 skill 与 5 个外部维度的边界:工具、人、知识、已有调度系统、信创合规。

5 个边界**互锁**,任一缺位 skill 就退化为"在调度环境里跑的玩具":

- 缺工具 → skill 让 LLM 自己写算法 → 输出与机理偏离
- 缺 HITL → 监管不放行,出事无法分责
- 缺知识 → 规程硬编码进 SKILL.md → 规程改一次就过期
- 缺 L1.5 → skill 不能调 D5000 → 无生产价值
- 缺信创 → 一行代码也部署不了

### 6.1 Skill ↔ 工具(MCP / CLI / 算法库)

**核心原则:skill 是 orchestration,tool 是 capability**。skill body 教 LLM "**怎么用** tool 完成任务",而不是"**做** tool 已经能做的事"。

调度场景下 skill 可调用的工具分 6 类,每类有不同契约 / 风险 / 错误处置:

| 类别 | 例子 | 风险 | 主要错误模式 |
|------|------|------|-------------|
| MCP tool(只读观测) | scada.get_measurement | 低 | 数据陈旧、超时 |
| MCP tool(计算) | ems.run_powerflow / ems.run_n_minus_1 | 中 | 不收敛、模型不一致 |
| MCP tool(控制 / 写入) | scada.send_setpoint / agc.set_target | **极高** | 拒动、误动、闭锁拒绝 |
| CLI / Bash 脚本 | opticket-cli generate | 中 | 退出码污染、stderr 解析失败 |
| Python module / 算法库 | psspy.run_pf() / 自有 OPF 求解器 | 中 | license 失效、版本不兼容 |
| 其他 skill(skill-as-tool) | skill A 调用 skill B | 同被调 skill | 依赖循环、版本兼容 |

**关键约束**:
- 类型 5(Python 算法库)**MUST 经 MCP server 包装** —— 不能让 skill body 让 LLM 自己运行任意 Python 代码,等于把 sandbox 关了
- 类型 6 的"skill 调用 skill"**MUST 走 host 调度**,不在 skill body 内 import

#### Skill 重新发明工具的反模式(R1-R9)

业务专家最容易在 skill body 里"教 LLM 自己做工具的活"。规范定义 9 条 lint 规则:

| ID | 反模式 | 正确做法 |
|----|-------|---------|
| **R1** | LLM 自己写 SQL | 包成 MCP tool `scada.query_measurement` |
| **R2** | LLM 自己实现 N-1 校验 | 调用 `ems.run_n_minus_1` |
| **R3** | LLM 自己模拟潮流 | 调用 `ems.run_powerflow` |
| **R4** | LLM 自己生成操作票格式 | 调用 `opticket.generate_template` |
| **R5** | LLM 自评 tool 输出(自己当裁判员) | 必须由独立 verify tool 给 verdict |
| **R6** | skill 内 import 另一 skill body | skill 通过 host 调用 skill |
| **R7** | LLM 解释闭锁逻辑 | 必须调用 `interlock.check`,LLM 不可仿真 |
| **R8** | LLM 直接运行任意代码 | 包成 MCP tool 暴露 |
| **R9** | LLM 编造规程引用 | 必须调用 RAG 取真实条款 + 校验 |

**反模式 R5(LLM 自己当裁判员)是调度场景最危险的一条** —— AI 给出建议又自己评估"这个建议安全吗" 等于运动员自己当裁判。任何 output 校核 MUST 走机理引擎,不能 LLM 自评。

#### 工具调用错误处置(三档降级)

| Policy | 含义 | 适用 |
|--------|------|------|
| **strict** | tool 失败 → skill 整体失败,不输出建议 | A 类(控制 skill)默认值 |
| **degrade** | tool 失败 → skill 输出"信息不全的建议",**显式声明**哪个 tool 失败 | B 类(分析 skill) |
| **advisory** | tool 失败 → skill 仍出建议,但不进入闭环 | C 类(纯查询 skill) |

**禁止 D1**(最高级红线):tool 失败,LLM "脑补"一个看起来合理的输出。
**禁止 D2**:在 skill body 里写"如果 tool 失败,假设量测为典型工况"。

### 6.2 Skill ↔ 人(HITL,Human-in-the-loop)

调度员是 skill 最终监督者。"AI 让我做的"在事故复盘时不被任何监管接受。

| Skill 类 | HITL 模式 | 说明 |
|---------|----------|------|
| **A 控制类** | **MUST HITL + 双签** | 调度员 approve 后才执行;关键操作需双签 |
| **秒级控制(AGC / 紧急控制)** | **MUST 不进控制环** | LLM 仅 advisor,不进闭环(对应 M8) |
| **B 分析类** | **SHOULD HOTL**(human-on-loop) | 调度员事后复核 |
| **C 查询类** | **MAY 全自动** | |

**关键判断**:HITL 在秒级 SLA 场景**根本不适用** —— 不可能让人 5 秒内决策。这类场景 AI **必须只 advisor 不进环**,事后审计兜底。把"加个 approve 节点"敷衍秒级控制是工程上的失败。

**调度员信任建立 5 阶段**(每阶段必须有退出 KPI):

```
沙盘 → 影子 → 建议 → HITL → 投产
  ↓      ↓      ↓     ↓     ↓
模拟器  与生产  调度员  双签  全量
跑通    并跑    采纳率  通过  且抽审
```

| 阶段 | 退出 KPI(参考值) |
|------|---------------|
| 沙盘 | 模拟器跑 100 个典型 case 全过 + 至少 5 个反例正确拒绝 |
| 影子 | 与生产并跑 ≥ 7 天,日均无误报 / 无机理违反 |
| 建议 | 调度员采纳率 ≥ 50%(B 类)/ ≥ 30%(A 类),持续 14 天 |
| HITL | 双签通过率 ≥ 70%(B 类)/ ≥ 90%(A 类),持续 30 天 |
| 投产 | 标委会评审通过 + 至少 1 个调度中心安全员签字 |

具体 KPI 数值是行业基准空白,以上为推荐区间,具体待行业共识。

### 6.3 Skill ↔ 知识(规程 / RAG)

调度规程、安稳导则、操作规程是**有版本、有发布日期、有修订记录**的法定文档。

四种模式对比:

| 模式 | 说明 | 优 | 缺 |
|------|------|----|----|
| **A 硬编码** | 把规程整段抄进 SKILL.md | 快 | 规程更新就过期 |
| **B RAG** | skill 通过检索工具找规程库 | 灵活 | 检索质量风险 |
| **C 知识图谱** | skill 引用 KG 节点 ID | 精准 | KG 建设成本高 |
| **D 混合** | 关键条款硬编码 + RAG 补充 | 平衡 | 复杂 |

**MUST 规则**:
- 规程 / 导则 **MUST 走 RAG 引用**,不得硬编码进 SKILL.md
- 引用 MUST 携带 `(regulation_id, version, clause_id, library_snapshot_hash)`(对应 M9)
- RAG 知识库写入 MUST 经规程编辑工作流(双人审批 + 版本记录)
- 设备台账 / 实时量测走 MCP tool,不走 RAG
- 历史故障案例走 RAG,但 case_id MUST 可回溯到原始故障复盘报告

不同知识源对应不同模式:

| 知识源 | 推荐模式 | 理由 |
|-------|--------|------|
| 规程 / 导则 | RAG 受控 | 文本结构化但庞大,需要语义检索 |
| 设备台账 | KG / MCP tool | 强结构化,需要精确字段查询 |
| 历史案例 | RAG + rerank | 模糊匹配 + 时序权重 |
| 实时量测 / 告警 | MCP tool | 时效性强,不进 RAG 索引 |

### 6.4 Skill ↔ 已有调度系统:L1.5 业务接口适配层(规范创新点之三)

> 这是规范的核心创新点,值得详细展开。

#### 6.4.1 问题:skill 不是新建系统

skill 不是一个新建的孤立系统,它要跟已运行十几年的调度自动化系统协作 —— D5000 OPEN3000、调控云、AGC、AVC、EMS、PMS、OMS。这些系统都有自己的协议:

- IEC 61850 (变电站自动化)
- IEC 60870-5-104 (调度数据采集)
- E 文件(国调内部交换格式)
- 厂商私有 API(各家不同)

skill 不可能直接懂这些协议。**没有适配层,skill 只能在沙盘里跑**。

#### 6.4.2 业界标准为什么没有覆盖这一层

L1 通用层(MCP / Anthropic Skills / OpenTelemetry)解决"skill 跟 host 怎么对话";L2 行业约束层(本规范主体)解决"skill 在调度场景必须满足什么硬约束"。

中间这一层"skill 怎么跟 D5000 / SCADA 等私有系统对接"**是行业特定的,业界标准不会覆盖** —— 因为业界标准是通用的,不知道电力调度有哪些私有系统、走什么协议。

但缺了这一层,L1 和 L2 都用不上 —— 这就是为什么本规范要**显式定义 L1.5**。

#### 6.4.3 L1.5 的设计

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

#### 6.4.4 schema 基础:CIM 作为语义中介

L1.5 不只是"协议翻译",更关键的是**语义对齐**。同一个"500kV 线路"在 D5000 里叫 `Line_500kV_001`,在 EMS 里叫 `BRANCH_5001`,在 PMS 里叫 `资产 ID 12345` —— 没有语义中介,skill 无法跨系统协作(eg. 检修预案 SOP skill 需要从 PMS 取台账、从 EMS 算 N-1、最后从 D5000 下票,三个系统的命名不通)。

**CIM (IEC 61968 / IEC 61970)** 是国际电力公共信息模型,做的就是这件事:把电网设备、拓扑、运行数据用统一的语义标识。L1.5 MCP server 应当**用 CIM 作为对外暴露的语义层**,各调度系统的内部命名在 server 内部翻译成 CIM。

国网 B 接口已经在做类似事情(标准化数据交换接口),L1.5 可以基于 B 接口扩展,而不是另起炉灶。

#### 6.4.5 谁来写 L1.5:厂商 + 国调中心评审

谁来写这一层?**推荐由原厂商提供** —— 南瑞、国电南自、华为这些厂商最懂自家系统的私有协议,让他们把自家系统包成符合规范的 MCP server。

国调中心的角色:

- **评审 contract test** —— 跨厂商兼容性 MUST 通过 contract test 验证。比如"D5000-MCP server 不论是南瑞版还是国电南自版,都必须实现同一组 MCP tool 接口,返回同样格式的 CIM 数据"
- **维护 schema 标准** —— CIM + 国网 B 接口的延展、调度专属字段的命名空间(`x-grid-*`)
- **保留升级版本仲裁权** —— 当不同厂商的 L1.5 server 在某个 schema 细节上分歧,国调中心仲裁

#### 6.4.6 L1.5 的工程价值:跨厂商互通 + 跨调度中心复用

有了 L1.5,**skill 可以跨厂商复用**:

- 一个由南瑞写的"检修预案 SOP skill",如果只调用 L1.5 标准 tool,可以直接装到国电南自的平台跑 —— 因为底层 D5000-MCP / EMS-MCP 不论谁实现,接口一致
- 一个由 A 调度中心写的"故障处置 advisor skill",可以直接复用到 B 调度中心 —— 不用每个中心从头写

**没有 L1.5,这种复用不可能**:每家厂商都要给自己的 D5000 写一遍适配,跨厂商互通就是反复造轮子。

这是规范创新点的工程价值 —— **不是技术上的难点**(MCP server 的写法业界已经成熟),而是**协调上的难点**(把"各厂商各做各的"变成"国调中心牵头,跨厂商共享一组 contract test")。规范定义的 L1.5 是这个协调机制的载体。

### 6.5 Skill ↔ 信创合规

中国电力行业的红线,绕不过。

**信创要求**:
- LLM **MUST 国产模型**(文心 / 通义 / 智谱 / 百川 / DeepSeek 等);本地部署,数据不出境
- 数据库 / OS / 中间件 / 芯片 **MUST 信创全栈**(达梦 / OpenGauss / 麒麟 / 海光 / 鲲鹏 / 飞腾)

**国密合规**:
- 签名 / 加密 **MUST 国密 SM2 / SM3 / SM4**(替代 RSA / SHA-256 / AES);M6 法定签名走 SM2
- 数字证书走国密体系(替代 OAuth 2.1 / W3C 标准的 RSA / ECDSA)

**等保 + 关键信息基础设施**:
- 调度系统是关基,skill MUST 满足等保 2.0 三级 + 关基保护要求
- 调用日志 MUST 可被监管机构审计(对应 M5 / M6)

**国产 LLM 选型**对调度场景的影响是 open issue —— 不同国产模型在能力、价格、国密支持度上差异较大,需要专项评估,不在本规范范围。

---

## 第七节:技能库治理 —— skill 是企业最重要的 AI 资产(规范创新点之四)

> 技能库治理与 §3 MANDATORY、§4 写作方法、§5 原子-SOP、§6 外部边界**同等重要**。在调度行业,**它是 AI 投运后两年内必然要面对的工程问题** —— 第一天就要把治理框架搭好,而不是事后补救。

### 7.1 技能库为什么是企业资产

调度行业的"运行经验"过去几十年靠**老师傅口头传授 + 私人笔记**积累:谁更懂某站的接线特殊性、谁记得某反措的具体含义、谁清楚某历史事故的真因。这种知识有三个固有问题:

- **流失**: 老师傅退休带走;调度员调动带走;反措通报半年后被淡忘
- **不可复用**: A 调度中心总结的经验不能直接给 B 调度中心,只能"从头学"
- **不可审计**: 这种经验的"应用过程"无法回溯,只能事后说"靠的是经验"

技能库的本质,是把这三类经验**工程化、可机器读、可版本化、可跨人员跨调度中心共享**。一个治理良好的技能库使企业获得四个工程能力:

1. **经验沉淀** —— 反措通报、安监通报、事故复盘 自动转化为 skill 的 Gotchas 段(§4.5)+ 极端 case 库(M10)+ 投运批准前置材料
2. **跨调度中心复用** —— 国调中心维护核心 skill 库,各省调本地化 + 增量补丁;不再"每家从头建"
3. **跨厂商互通** —— skill 跟 L1.5 MCP server 解耦后,A 厂商写的 skill 可装到 B 厂商的平台跑(§6.4)
4. **可审计 / 可追责** —— 每次 skill 调用 emit 完整 evidence chain(M5),监管检查时直接调取

skill 数到 50+ 时,没有治理就变成"谁都不知道哪个 skill 是最新的、哪些 skill 跟哪些有冲突、哪些应该退役"的烂摊子。**这是规范要求第一天就把治理搭好的根本原因**。

### 7.2 Lifecycle:从草稿到退役

```
draft → review → shadow(影子运行) → canary(试点) → production
                                                       ↓
                                                    retired
```

每阶段必有 governance hook(谁审、看什么、退出 KPI、退回机制):

| 阶段 | 审核人 | 退出 KPI(参考) | 退回触发 |
|------|------|---------------|--------|
| review | 业务专家 + 安全员 | §6 lint + 写作规范 全过 + 双方 sign-off | lint 任一不过 → 退回 draft |
| shadow | 班长 + 业务专家 | 静默运行 7 天,日均无误报 / 无机理违反 | 任何机理违反场景被通过 → 退回 review |
| canary | 调度员 + 班长 | 30 天采纳率 ≥ 60%(B 类)/ ≥ 70%(A 类) | 调度员主动 disable ≥ 3 次 → 退回 review |
| production | 标委会 | 投运批准 + 极端 case 回放(M10)+ 安全员 sign-off | 任何投运后事故 → 立即退到 shadow,全网 freeze 同类 |
| retire | 任意责任人 | deprecated 提案 + 30 天通知期 + 替代 skill 已 production | — |

**3 个关键约束**:
- production 阶段仍然每月走 1 次"幽灵审计"(随机抽 5% 调用复审),不是上线就放手
- 任意阶段失败 MUST 入审计日志,不仅是状态变更
- retire 不等于"删除" —— 历史调用日志仍可审计(M5 / M6 不豁免);只是不再被新调用 invoke

KPI 数值是行业基准空白,具体待行业共识。

### 7.3 知识资产沉淀机制 —— 技能库与行业经验的连接

技能库治理的最大长期价值,是**把行业经验工程化**。具体的连接流水:

```
反措通报 / 安监通报 / 事故复盘
   ↓
标准化工作组提取(挑出"对哪些 skill 有影响 + 该加什么 Gotcha / 该改什么 Steps")
   ↓
受影响 skill 走 MAJOR 升级流水(§4.5 + §7.5)
   ↓
通过 review → shadow → canary 全流水
   ↓
新版本 production → 替代旧版本(旧版本进 retire)
   ↓
全调度中心的 AI 助手立即"知道"这次反措
```

**这条流水把"老师傅经验"转化成"机器可读 + 跨人员 + 跨调度中心可传承"的资产**。每条反措对应的 skill 升级,既加深了 skill 库的厚度,也产生了行业共享的价值 —— A 调度中心总结的反措 → A 调度中心 skill 升级 → 国调中心审核 → 跨调度中心同步 → B 调度中心也"立即学到"。

**等到 skill 数 50+ 才补这条流水,行业经验已经流失太多** —— 这是规范要求**第一天就建立**的原因。

### 7.4 多维分类索引

skill 库必须至少按以下 4 维分类索引,**任意一维都可独立查询 + 跨维交叉查询**:

| 维度 | 取值 |
|------|------|
| **业务域** | 实时监控 / 安稳校核 / AGC / AVC / 检修 / 新能源 / 故障处置 / 分析咨询 |
| **调度层级** | 国调 / 网调 / 省调 / 地调 / 县调 |
| **风险等级** | A / B / C(对应 §2) |
| **skill 类型** | atomic / sop / hybrid(对应 §5) |

典型查询场景:"查所有省调适用的 A 类控制 skill"(风险 + 调度层级)、"查所有跟 AGC 相关的 atomic skill"(业务域 + 类型)、"查跨调度层级共用的 skill"(controllable_assets_filter 字段索引)。

平台 SHOULD 暴露语义检索能力(基于 description 向量索引),让调度员用自然语言查 skill。

### 7.5 跨厂商互通 + 版本治理

**跨厂商互通**(对应 §6.4 L1.5):
- skill 包格式 MUST 遵循 agentskills.io 2025-12-18
- 厂商专属字段 MUST 在 `vendor.<vendor-name>.*` 命名空间
- skill 间 dependency **禁止跨厂商私有调用**,MUST 走 L1.5 MCP server
- L1.5 MCP server 跨厂商兼容性 MUST 通过 contract test —— 国调中心维护这组 contract test

跨厂商互通的反例:某 skill 直接 import 另一厂商的私有 Python 库 → 锁死在该厂商平台 → 不可移植。

**版本治理**:
- SemVer:MAJOR(破坏性,如 description / Gotchas 重大改动) / MINOR(加字段) / PATCH(仅修复)
- MAJOR 升级 MUST 重新过 lint + Quality Gate
- 引用其他 skill 时 MUST 锁版本范围(类似 npm semver range,eg. `^1.2.0`)
- **模型升级触发 skill 重评**:`model_lock` fingerprint 不一致时 skill 拒绝调用(对应 M9)

### 7.6 集中 + 本地化 架构

技能库**集中 + 本地化**架构:

| 层 | 维护方 | 内容 |
|----|------|------|
| **国调中心核心库** | 国调中心 + 标委会 | 跨调度通用的 skill(规程检索、N-1 校核、潮流计算 等);极端 case 库;contract test |
| **省调 / 网调 增量库** | 各省调 / 网调 | 本调度区域特有的 skill(本省新能源接入特性 / 本省稳定特性 等) |
| **地调 / 县调 本地化补丁** | 各地 / 县调 | 本辖区设备特有的 Gotchas 段 + 操作惯例 patches |

**集中 vs 本地化的边界**:
- 国调核心库用 CIM 标准命名,语义中立
- 省 / 地调增量库可以引入本地化 Gotchas(eg. 某站接线特殊性)
- 跨层级共享时**必须脱敏**(去除涉及国安的具体设备 ID / 容量 / 网架细节)

这层架构既保证了核心 skill 一致性(避免重复造轮子),又保留了本地化能力(本厂的反措沉淀不会被国调中心"标准化掉")。

### 7.7 业界主流 registry 模式

JFrog Agent Skills Registry、Tessl × Snyk、Alibaba Nacos 3.2、Anthropic Enterprise Skills 都给出了相似的模式,本规范沿用:

- **集中发布,分布消费**:国调统一 registry + 各省调本地缓存
- **签名 + 完整性校验**:国密 SM2 签名 + skill 包哈希(对应 M6);本地缓存调用前必须校验签名 + 哈希,签名失败 → 拒绝调用
- **依赖图 + 版本范围**:跨 skill dependency 显式声明,跨 skill MAJOR 升级时自动产生影响评估报告(类似 npm 的 audit)
- **审计 + 可观测**:每次 skill 调用 emit OpenTelemetry trace 到 registry,registry 维护跨调度中心的 skill 调用全景图(对应 M5)

**registry 部署模式 open issue**: 国调统一 registry 是数据不出境约束下的合规边界 —— 各省调本地缓存需要明确"哪些数据走中心、哪些只在本地"。这是政策决策,不在本规范范围,标委会专项评审(详见 §10)。

---

## 第八节:Skill 的失效模式 —— 写好之后还能怎么坏

业界对 40+ 失败 skill 的复盘、Snyk ToxicSkills 报告(3,984 个公开 skill 中 36% 含 prompt injection)归纳出 9 类失效:

| 序号 | 表现 | 调度场景灾难等级 |
|------|------|---------------|
| 1 | **Skill 该激活时不激活**(description 触发词不全) | 🔴 高 |
| 2 | **Skill 激活但产生 generic 输出**(缺 examples / Gotchas) | 🔴 高 |
| 3 | Over-trigger 抢其他 skill 的活 | 🟡 中 |
| 4 | Context rot — 长会话后 skill 越写越钝 | 🟡 中 |
| 5 | 越权 / 越能力(没 Boundaries 段) | 🔴 高 |
| 6 | **幻觉式补字段 / 编造规程条款** | 🔴 极高 |
| 7 | Prompt injection 通过 skill 注入 | 🔴 极高 |
| 8 | 指令冲突(skill 内 / skill 间) | 🟡 中 |
| 9 | Outgrowth/Regression(模型升级后行为漂移) | 🟡 中 |

**关键洞察**: 1、2、6 三个失效模式占调研中 **70%+ 的失败案例**。规范的 quality gate 必须显式覆盖这三类:

- 失效 1 → description 触发准确率 ≥ 85% 评分(§4.6)
- 失效 2 → Gotchas ≥ 3 条 + Examples ≥ 2 个(§4.5)
- 失效 6 → 规程引用强制带 `library_snapshot_hash` + 投运前 `verify_regulation_refs.py` 校验(M5)

---

## 第九节:这份规范不解决什么(反向边界)

为了避免规范越界,以下事项**明确不在本规范范围**:

- **具体调度业务流程**(各调度中心自定 SOP 内容)—— 规范定义 skill 的容器,不定义 skill 的内容
- **具体 LLM 模型选型** —— 由信创采购决定
- **L1.5 MCP server 的具体实现** —— 由各厂商负责
- **训练数据合规** —— 由数据治理规范覆盖
- **skill 商业模式 / 计费** —— 由商务规范覆盖
- **skill 内部业务算法** —— 算法属于业务逻辑,不属于 skill 协议

---

## 第十节:还有哪些尚未确定的事

以下 10 项是规范在起草阶段已识别、但需要电力专家、法务、政策评估方进一步确定的开放问题:

1. **国标条款号**(GB/T 31464、DL/T 510、GB 38755 等)正式发布前需电力专家 + 法务复核
2. **WORM 法定保留年限**(常见 5-10 年,具体年限待法务给定)
3. **"机理引擎单调胜出"在新能源高比例场景下的边界** —— 传统机理在新能源大规模接入后可能本身不准,待标委会评审特定场景例外
4. **国密 SM2 强制是否过度** —— 会挡非国产 LLM 服务,这是政策决策不是技术决策
5. **极端 case 库管理** —— 防止变成厂商准入黑名单
6. **国产 LLM 选型对调度场景能力差异** —— 需要专项评估
7. **multi-skill 编排 MANDATORY 化时机**(SHOULD → MUST 的窗口)
8. **跨厂商 MCP server contract test 是否标准化** —— 跨厂商互通的执行边界
9. **Skill registry 集中 vs 分布部署的合规边界**(数据出境约束)
10. **HITL 5 阶段每阶段 KPI 数值**(采纳率 / 试点天数)的行业基准

---

## 第十一节:给国调中心的具体建议

### 11.1 立即可做(不需要等规范定稿)

1. **任命 1 个标准化工作组牵头人** —— 牵头本规范的进委员会评审、标准号申报、跨厂商对齐
2. **挑选 1 个 C 类 skill 试点**(eg. 规程检索)—— C 类风险低,可以先跑通完整 lifecycle 5 阶段,作为流程验证的"标杆"
3. **建立反措通报 → Gotchas 段更新流程** —— 这是规范创新点的落地基础,流程上需要走通

### 11.2 在 v0.1 评审通过后

1. **L1.5 MCP server 厂商对齐会议** —— 拉齐南瑞、国电南自、华为等厂商,定 SCADA-MCP / EMS-MCP / 闭锁-MCP / PMS-MCP 各自的 schema 边界(对应 §6.4)
2. **规程 RAG 知识库 v1** —— 国调中心受控、国密签名、版本可回溯
3. **skill registry v1** —— 集中发布 + 分布消费,签名校验完整链路
4. **极端 case 库 v1**(M10) —— 国调中心牵头建,明确入库流程 + 厂商使用边界

### 11.3 6-12 个月路线图(可调)

| 时间 | 目标 |
|------|------|
| Month 1-3 | 基础设施搭建(L1.5 / 国密 / RAG / registry) |
| Month 4-6 | 首批 B 类 skill 投运(实时监控 / 负荷预测 / 历史案例查询) |
| Month 7-9 | A 类 skill 试点(检修预案 generator) |
| Month 10-12 | 规模化(skill 数 ≥ 10)+ 跨厂商互通验证 + 标委会评审 v1.0 标准 |

---

## 第十二节:三份规范本体的使用场景

读完导读后,如果你需要展开细节,以下三份规范本体按使用场景选用:

| 文件 | 篇幅 | 使用场景 |
|------|------|--------|
| `skill-spec-v0.1.md` 紧凑版 | ~3 页 | 内部对齐 / 高层快速参考 |
| `skill-spec-v0.1-standard.md` 标准版 | ~6-8 页 | 委员会评审 / 厂商对齐 |
| `skill-spec-v0.1-complete.md` 完整版 | ~12-15 页 | 国调正式发布候选 / 配套 implementation guide |

三份的逻辑结构相同(§1 适用范围、§2 核心理念、§3 L1 业界标、§4 L2 MANDATORY、§5 SKILL.md 编写规范、§6 外部边界、§7 准入 lint、§8 库治理),完整版多了实施路径图、5 阶段 KPI 数值、3 个调度场景 walkthrough、规程 → skill 4 步转写法等附录。

### 关于规范的格式风格

三份规范本体均采用 **RFC 2119 关键字风格 + 轻量化结构**,引 `MUST` / `SHOULD` / `MAY` 关键字保证强制力,但不堆 GB/T 国标的形式障碍(术语定义专章、正式 IPR 声明、规范性引用编号体系等)。

这是 **v0.1 评审稿的合适形态** —— 内容还在迭代,先用更轻量格式跑通业内共识。完整的 GB/T 国标形式留待 v0.2 内容稳定后,由专业标准化文员(国调中心标准化办公室或外聘)按 GB/T 1.1-2020《标准化工作导则》格式重排成 GB/T 草案。

升级路径:

```
v0.1 (现在,RFC 风格评审稿)
   ↓ 收集国调 / 电力专家 / 厂商反馈
v0.2 (内部稳定版,仍 RFC 风格)
   ↓ 国调中心立项 + 标委会牵头
GB/T 草案 (按 GB/T 1.1-2020 格式重排)
   ↓ 征求意见 / 审查 / 报批
GB/T XXXX-202X (正式国标)
```

v0.1 → v0.2 关注**内容硬度**(MUST 是否经得起业务专家挑战);v0.2 → GB/T 关注**形式规整**(术语定义、引用编号、章节顺序等)。两个阶段不要混。

---

## 结语

这份规范的目的不是"管住 AI",而是 **"让企业能信任地把 AI 接入调度环境"**。两者差别巨大:

- "管住 AI"的规范会变成"什么都不让做"的清单 → 厂商失去落地空间
- "让企业信任"的规范会回答"做什么、不做什么、出了事怎么追责" → 形成可投运的工程标准

我们在起草中始终守住一个判断准则:**每条 MUST,如果违反,会出什么生产事故 / 监管不合规?** 答得出来就保留,答不出来就降级或删除。这是规范"硬"的来源,也是它能落地的基础。

下一步是收集国调中心、电力专家、安监委员会、厂商集成商的反馈,迭代到 v0.2,再走标准化委员会评审程序。
