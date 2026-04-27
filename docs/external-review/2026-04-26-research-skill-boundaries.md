# Research: Skill 边界 — 工具 / 人 / 知识 / 已有系统 / 信创

**研究日期**: 2026-04-26
**目的**: 锁定 skill 与外部世界的边界,作为对外方案 v0.1 的"非孤岛"约束基础
**读者**: 调度业务专家(写 skill 的电力工程师)+ IT 集成商(部署平台的工程师)+ CTO / 调度处长
**站位**: 技术架构师写给企业 CTO + 调度处长 + 集成商联合阅读的 architectural 报告
**研究方式**: 结合公开标准文献(MCP、Anthropic Skills、IEC 61970、IEC 61968、IEC 61850、IEC 60870-5-104、CIM、GB/T 22239 等保 2.0、DL/T 2614-2023 电力行业等保、《关基条例》、《密码法》)+ 业界 LLM agent 工程实践(LangGraph interrupt、Anthropic Contextual Retrieval、Microsoft GraphRAG、Portkey/Maxim 错误处置 patterns)+ 国家电网调度系统公开资料(D5000、调控云、B 接口、达梦/麒麟信创实践)
**约束**: 凡引用具体国标 / 行标编号、试点准确率数字,**正式发布前需电力 / 安全专家校对**

> **Anchor**: 本报告 5 维 (§A 工具 / §B 人 / §C 知识 / §D 已有系统 / §E 信创) **互锁** —— 任一维 deficit 都会导致 skill 退化为"在调度环境里跑的玩具":
> 缺 §A → skill 让 LLM 自己写算法 → 输出与机理偏离;
> 缺 §B → 无 HITL → 监管不放行,出事无法分责;
> 缺 §C → 规程硬编码进 SKILL.md → 规程改一次就过期;
> 缺 §D → skill 不能调 D5000 → 无生产价值;
> 缺 §E → 信创不通过 → 一行代码也部署不了。

---

## §A. Skill ↔ 工具调用编排

> **核心原则(读者必须先内化)**: **skill 是 orchestration,tool 是 capability**。skill body 教 LLM "**怎么用** tool 完成任务",而不是"**做** tool 已经能做的事"。任何让 LLM 在 skill body 里重新发明工具能力(自己写 SQL、自己 implement N-1、自己模拟潮流)都是反模式。

### A.1 工具类型 taxonomy

调度场景下,skill 可调用的工具分 6 类。每类都有不同的引用契约、风险模型、错误处置策略:

| # | 类别 | 例子 | 调度对应 | 调用契约 | 风险等级 | 主要错误模式 |
|---|------|------|----------|----------|----------|--------------|
| 1 | **MCP tool(只读观测)** | scada.get_measurement / ems.get_topology / pms.get_equipment_info | SCADA 实时量测 / EMS 拓扑 / 台账查询 | MCP tools/call, JSON Schema 入参,JSON content 出参 | 低(读取不改状态) | 数据陈旧、字段缺失、超时 |
| 2 | **MCP tool(计算)** | ems.run_powerflow / ems.run_n_minus_1 / stab.run_transient | 潮流计算、N-1 校验、稳定计算 | MCP, 长任务 → MCP 2025-11-25 Async Tasks 扩展 | 中(只算不动) | 不收敛、模型不一致、超时 |
| 3 | **MCP tool(控制 / 写入)** | scada.send_setpoint / scada.execute_remote / agc.set_target | 遥控、改定值、AGC 指令 | MCP + 必须挂 x-grid-risk: irreversible annotation;必须挂幂等 key | **极高(动一次电网)** | 拒动、误动、抵达延迟、闭锁拒绝 |
| 4 | **CLI / Bash 脚本** | opticket-cli generate -t standard / report-gen -d 2026-04-26 | 操作票生成 CLI、日报生成、检修登记 | bash subprocess + stdin/stdout JSON 协议 | 中(看脚本本身) | 退出码污染、stderr 解析失败 |
| 5 | **Python module / 算法库** | psspy.run_pf() / 自有 OPF 求解器 / sklearn.cluster | BPA / PSD-BPA / PSASP 接口、自研最优潮流、负荷聚类 | 通过 MCP server 包成 tool 暴露;**禁止**让 skill 直接 exec(python_code) | 中 | 求解器 license 失效、版本不兼容 |
| 6 | **其他 skill(skill-as-tool)** | skill A 调用 skill B(检修预案 → 调用操作票生成 skill) | 多 skill 协作场景 | skill 引用 skill 走 host 层调度,**不在 skill body 内 import** | 同被调 skill | 依赖循环、版本兼容 |

**关键判断**:
- 类型 5(Python 算法库)**必须**经 MCP server 封装,不能让 skill body 让 LLM 写 exec(...) 调用 — 否则等于把 sandbox 关了
- 类型 6 的"skill 调用 skill"在业界(Anthropic / LangChain Skills)**目前没有形式标准**,需要本规范自定 contract(详见 §A.5 反模式 R6)

### A.2 skill 引用工具的具体写法 + 业界 reference

**Anthropic Agent Skills(SKILL.md)的 allowed-tools 字段是当前业界事实标准**。其格式(2025-12-18 agentskills.io spec):

```yaml
---
name: dispatch-realtime-monitor
description: 监控某省调直流系统实时运行状态,在量测异常时输出告警分析。Use when 调度员需要快速了解直流断面稳定裕度。
allowed-tools:
  - mcp:scada.get_measurement
  - mcp:scada.get_alarm_active
  - mcp:ems.get_topology
  - mcp:ems.run_powerflow
metadata:
  x-grid-risk-class: B           # A=控制 / B=分析 / C=查询
  x-grid-required-certifications: [dispatcher-l3]
  x-grid-affected-voltage-levels: [500kV, 220kV]
  x-grid-requires-two-person-approval: false
  x-grid-sla-response-ms: 3000
  x-grid-tool-error-policy: strict   # strict / degrade / advisory
---
```

要点:
1. **allowed-tools 是白名单(deny by default)** — host 在加载 skill 时校验,skill body 里 LLM 想调用未声明 tool 直接拒绝
2. **MCP tool URI 格式** mcp:server.tool — 业界正在收敛,目前各家略有差异
3. **x-grid-* 命名空间** 是 Subagent 2 已建议的扩展空间,本节只聚焦工具相关
4. **x-grid-tool-error-policy** 是 §A.4 错误处置的入口字段

**与 MCP tools/list 的协作**:
- skill 启动时 host 把 allowed-tools 与 MCP server 的 tools/list 求交集,缺失则拒绝加载
- skill body 引用 tool 时建议用 fenced markdown:"调用 scada.get_measurement(station_id, point_id) 取断面 P 值"。LLM 在 in-context learning 下会优先使用本 tool;若 LLM 走偏调用未声明 tool,host 拦截

**业界更细的 reference**:
- LangGraph 的 tool node —— graph 节点级声明,适合复杂 SOP,但**不要把 LangGraph 写到 L1 spec 里**(它是 client side 实现)
- Cursor / Cline 的 tool registry —— 声明 + 路由分离,与 SKILL.md allowed-tools 设计哲学一致
- OpenAI function calling —— 仅作 LLM 客户端内部投影格式参考,**不作 wire 层标准**(详见 Subagent 2 §1.2)

### A.3 LLM 推理 vs 命令脚本的决策树

调度 skill 的核心工程问题:**何时让 LLM 自己推理,何时直接调命令脚本/算法库**。错误的选择=两个极端事故:

- 极端 A:**全交给 LLM 推理** — LLM 自己"算"潮流、自己"判"N-1 → 物理基础不在,出 OOD 错
- 极端 B:**全用脚本** — skill 等于一个长 bash 脚本,LLM 完全没用,放弃 AI 价值

**决策树(自上而下,命中即停)**:

```
1. 该任务有没有「物理 / 数学硬约束」(基尔霍夫、热稳极限、N-1、AGC 调差)?
   ├─ YES → 必须调用 算法库 / EMS tool;LLM 不可代行 (类型 2/3/5)
   └─ NO → 进 2

2. 该任务的执行路径「是否事先固定」(每次都同样的 step 序列)?
   ├─ YES → 用 Chain Workflow (LLM 仅作参数填充,各 tool 顺序固定)
   └─ NO → 进 3

3. 该任务的输出「是否有标准答案」(可被规程 / 历史案例校验)?
   ├─ YES → 让 LLM 推理,但输出后必须经 verify tool 校验 (§A.4)
   └─ NO → 进 4

4. 该任务是「探索式 / 解释式」(故障复盘、负荷预测解读、规程应用解释)?
   ├─ YES → 让 LLM 自由推理,但必须挂 RAG (§C) 给出引用
   └─ NO → skill 设计有问题,重新拆解 SOP
```

**调度场景实例**:
- 「跑潮流」→ 1 命中 → 调 ems.run_powerflow,LLM 不算
- 「检修方案 N-1 校核」→ 1 命中 → 调 ems.run_n_minus_1 + 调用规程 RAG 解释结果
- 「写日报」→ 2 命中 → Chain: 取数 → 模板填充 → 输出
- 「故障复盘解读」→ 4 命中 → 自由推理 + 规程 RAG + 历史案例 RAG

### A.4 工具调用错误处置 + 降级

业界 2025-2026 已收敛的 LLM agent 错误处理 patterns(见 Portkey / Maxim 公开博客)需在调度场景做**风险加权**:

**错误分类(分两层)**:
| 层 | 类别 | 示例 | 处置 |
|----|------|------|------|
| L1 网络/系统 | Transient | 超时、429 限流、TCP RST | 指数退避重试,**最多 3 次**(调度场景再多就背离 SLA) |
| L1 网络/系统 | Persistent | 服务下线、认证失败 | 立刻 fail,告警值班,**不重试** |
| L2 业务 | Tool 返回业务错 | "拓扑不一致"、"量测过期" | 进入 §A.4 降级矩阵 |
| L2 业务 | Tool 返回但语义错 | 潮流不收敛、N-1 失败 | **不可隐藏**,作为 finding 显式输出给 HITL |

**降级矩阵(对应 SKILL.md x-grid-tool-error-policy)**:

| Policy | 含义 | 适用 |
|--------|------|------|
| strict | tool 失败 → skill 整体失败,不输出任何建议 | A 类(控制 skill)默认值 |
| degrade | tool 失败 → skill 输出"信息不全的建议",但**必须**显式声明哪个 tool 失败、缺什么数据 | B 类(分析 skill) |
| advisory | tool 失败 → skill 仍出建议,但不进入闭环;事后审计可见 | C 类(纯查询 skill) |

**禁止反模式 D1**: tool 失败,LLM "脑补"一个看起来合理的输出 → 这是调度场景的最高级 skill 红线。
**禁止反模式 D2**: 在 skill body 里写"如果 tool 失败,假设量测为典型工况" — 任何这类指令都必须删除。

**Circuit Breaker(电路熔断)— 调度特化**:
- 同一 MCP server 在 5 分钟内失败 ≥ 3 次,熔断 10 分钟,期间所有依赖该 server 的 skill 进入 degraded 模式
- 熔断事件 MUST 入 audit log + 告警通知运维
- 业界默认是模型级熔断(Sonnet → Haiku),**调度场景应是 tool 级熔断**(SCADA → 缓存 + 标记陈旧)

### A.5 反模式: skill 重新发明工具的常见错误

> 这是最高频的 skill 设计错误。L1 标准 SHOULD 显式列出禁忌,作为 lint 规则。

| ID | 反模式 | 例子 | 正确做法 |
|----|--------|------|----------|
| R1 | **LLM 自己写 SQL** | "请根据这条 SQL SELECT * FROM measurements WHERE ... 取断面值" | 包成 MCP tool scada.query_measurement(query_id, params) |
| R2 | **LLM 自己实现 N-1 校验** | "请遍历所有支路,假设每条断开后剩余支路潮流不超过限额" | 调用 ems.run_n_minus_1 |
| R3 | **LLM 自己模拟潮流** | "请用直流潮流公式估算各支路功率" | 调用 ems.run_powerflow |
| R4 | **LLM 自己生成操作票格式** | "请按以下 markdown 模板输出" | 调用 opticket.generate_template(scenario_id) 的 CLI/MCP 工具 |
| R5 | **LLM 直接 exec 代码** | "执行以下 Python 代码:import psspy; psspy.run_pf()" | 包成 MCP tool 暴露,不让 LLM 直接 exec |
| R6 | **skill 内 import 另一 skill 的 body** | "请按 skill-A 的步骤 1-5 执行..." | skill B 通过 host 调用 skill A,host 维护 skill 间依赖图 |
| R7 | **LLM 解释闭锁逻辑** | "请判断本操作是否触发某项防误闭锁" | 必须调用闭锁系统 tool interlock.check(operation_id),LLM 不可仿真 |
| R8 | **LLM 编造规程引用** | "根据《电网调度规程》§3.2.1..." | 必须调用 RAG (§C) 取真实条款 |
| R9 | **LLM 自评 tool 输出** | "请判断 N-1 校验结果是否安全"(LLM 自己当裁判) | 必须由独立 verify tool 给 verdict |

### A.6 调度场景 walkthrough(3 个完整例子)

#### A.6.1 实时监控 skill — dispatch-realtime-monitor

**目标**: 调度员说"看一下当前直流系统稳定状态",skill 给出量化分析。

**SKILL.md 关键字段**:
```yaml
allowed-tools:
  - mcp:scada.get_measurement      # 量测取数
  - mcp:scada.get_alarm_active     # 当前告警
  - mcp:ems.get_topology           # 拓扑
  - mcp:ems.run_powerflow          # 潮流复算
  - mcp:knowledge.search_proc      # 规程 RAG
metadata:
  x-grid-risk-class: B
  x-grid-tool-error-policy: degrade
```

**编排流程(LLM 在 skill body 中按指引执行)**:
```
Step 1: 取数据 (3 个 MCP tool 并行)
  ├─ scada.get_measurement(station_ids=[...])  → 实时 P/Q/V
  ├─ scada.get_alarm_active()                    → 当前告警
  └─ ems.get_topology(area="华东")               → 当前拓扑

Step 2: 复算 (1 tool)
  └─ ems.run_powerflow(topology, measurements)   → 潮流断面

Step 3: 比对 (LLM 推理 — 这一步才是 LLM 真正干活的地方)
  ├─ 计算断面裕度 = 限额 - 实测潮流
  ├─ 识别低裕度断面
  └─ 关联告警

Step 4: 引用 + 解释 (1 tool)
  └─ knowledge.search_proc(query="某断面低裕度处置")  → 规程引用

Step 5: 输出
  ├─ 结构化字段: 断面状态表 + 裕度排序 + 引用
  └─ 自然语言摘要: 给调度员快速读
```

**错误处置**:
- scada.get_measurement 失败 → degrade,标 [SCADA 失联,数据可能陈旧]
- ems.run_powerflow 不收敛 → degrade,标 [潮流不收敛,以下分析仅基于实测]
- knowledge.search_proc 失败 → degrade,但**仍输出**(纯规程引用属 advisory)

#### A.6.2 检修预案 skill — maintenance-plan-generator

**目标**: 输入"X 月 Y 日某 500kV 线路停电检修",skill 输出操作票草稿 + N-1 校验。

**SKILL.md 关键字段**:
```yaml
allowed-tools:
  - mcp:pms.get_equipment_info     # 设备台账
  - mcp:ems.get_topology
  - mcp:ems.run_n_minus_1          # 关键!
  - mcp:interlock.check            # 闭锁系统校验
  - mcp:opticket.generate          # 操作票生成 CLI 包装
  - mcp:knowledge.search_proc
metadata:
  x-grid-risk-class: A             # 控制类
  x-grid-tool-error-policy: strict # 失败必须 fail
  x-grid-requires-two-person-approval: true
  x-grid-required-certifications: [dispatcher-l4, safety-officer]
```

**编排流程**:
```
Step 1: 台账核实 (mcp:pms.get_equipment_info)
  → 设备 ID / 类型 / 限额 / 当前状态

Step 2: N-1 仿真 (mcp:ems.run_n_minus_1) — 必须走 EMS,LLM 不可仿
  → 检修期间任意元件再故障的影响

Step 3: 闭锁系统校验 (mcp:interlock.check) — 必须走闭锁系统,LLM 不可仿
  → 现行挂牌 / 紧急保电 / 操作冲突

Step 4: 操作票生成 (mcp:opticket.generate)
  → 标准格式操作票草稿

Step 5: 规程引用 (mcp:knowledge.search_proc)
  → 防误规程引用 + 安全措施清单

Step 6: 输出 → 提交 HITL (§B)
  → 调度员 + 安全员双签 → 执行
```

**错误处置(strict)**:
- 任何 tool 失败 → skill 整体不输出建议,告警值班 → 调度员转人工
- interlock.check 返回"冲突" → skill 输出"本预案触发闭锁 X,**不可执行**",**严禁** LLM 改写为"建议绕过"

**反模式陷阱**:
- ❌ 让 LLM 自己生成操作票文本 → 违反 R4
- ❌ 让 LLM 自己解释闭锁规则 → 违反 R7
- ❌ tool 失败时 fall back 到"经验类比" → 违反 D1

#### A.6.3 故障处置 skill — fault-response-advisor

**目标**: SCADA 报告某 220kV 线路故障跳闸,skill 给出处置建议(advisor 模式,人工执行)。

**SKILL.md 关键字段**:
```yaml
allowed-tools:
  - mcp:scada.get_measurement
  - mcp:scada.get_alarm_history    # 故障前后告警序列
  - mcp:ems.get_topology
  - mcp:ems.run_powerflow
  - mcp:stab.run_transient         # 暂态稳定 — BPA / PSASP 包装
  - mcp:knowledge.search_case      # 历史故障案例 RAG
  - mcp:knowledge.search_proc
  - mcp:agc.get_status             # AGC 状态(只读!)
metadata:
  x-grid-risk-class: B             # advisor 不直接控制
  x-grid-tool-error-policy: degrade
  x-grid-sla-response-ms: 5000     # 故障处置秒级
```

**编排流程**:
```
Step 1-3: 量测 + 告警序列 + 拓扑(并行)
Step 4: 潮流复算(看故障后的稳态)
Step 5: 暂态稳定校验(看是否有失稳风险) — 走 BPA/PSASP MCP wrap
Step 6: 历史案例匹配(RAG)
Step 7: LLM 推理:
  ├─ 故障性质判断
  ├─ 处置优先级排序(切负荷 / 调机组出力 / 拉路)
  └─ 每条建议引用规程条款 + 历史案例
Step 8: 输出给调度员(advisor — 人工选择执行)
```

**关键设计**:
- AGC 只读 — skill 看 AGC 状态但不发指令(这是 §B HITL 红线)
- 5s SLA — 真正控制级故障(秒级)skill 不可能 HITL,只能 advisor + 调度员现场决策
- 历史案例 RAG 必须返回 case ID + 时间戳,可回溯到原始故障复盘报告

---

## §B. Skill ↔ 人 (HITL)

> **核心原则**: 调度员是 skill 最终监督者。"AI 让我做的"在事故复盘时不被任何监管接受。**所有 A 类 skill MUST HITL,B 类 SHOULD HITL,C 类 MAY HITL**。

### B.1 HITL 模式分级

业界 IBM / IT Revolution 已将 HITL 形式化为四档(2025 主流 framing):

| 模式 | 名称 | 含义 | 适用 SLA |
|------|------|------|----------|
| **HITL** | Human-in-the-Loop | 人必须 approve,**未approve 不执行** | 分钟-小时级 |
| **HOTL** | Human-on-the-Loop | AI 执行,人监控,**人有 veto** | 秒-分钟级 |
| **HIC** | Human-in-Command | 人主导,AI 仅顾问;最终决定永远是人 | 任意 |
| **Advisor** | 仅建议 | AI 输出 → 进调度员台前显示 → 人独立判断 | 秒级 |

**调度业务对应**(电力行业事故责任 = 司法 + 监管双重压力,设计 SHOULD 偏保守):

| 调度操作类别 | 推荐模式 | 理由 |
|--------------|---------|------|
| 一级关键控制(发遥控、改保护定值、合分闸) | **HITL + 双签** | 出事人身责任,法规已强制双人审 |
| 二级常规调控(出力调整建议、AGC 调节、电压调整) | **HOTL** | 秒级 SLA,人有 veto 但 AI 可执行 |
| 三级分析咨询(故障复盘、检修预案分析、规程查询) | **Advisor / HIC** | 不动电网 |
| 四级查询类(台账问答、规程检索) | 自由模式 | 无生产影响 |

**关键边界(MUST 写入规范)**:
- A 类控制 skill **不可** 用 HOTL —— 一旦 LLM 误下指令,即使秒级 veto 也可能动了电网。HITL 是**必经**。
- 实时控制(秒级,如 AGC)**不适用** HITL —— 这类业务原则上 AI **不应该**进入控制环。如必须,只能 advisor 模式(给建议,人现场操作)。

### B.2 acceptance flow + 反馈闭环

**业界(LangGraph interrupt + Anthropic 工程实践)的标准 flow**:

```
skill 输出 → host 在 approval_node 中断 → 调度员 UI 显示
  ├─ 显示内容: skill 名 + 输入 + 输出 + 引用 (规程/历史案例) + tool call trace + 风险等级
  └─ 调度员决策:
       ├─ APPROVE → resume,进入执行
       ├─ REJECT  → 终止,记录 reject_reason 入 audit
       └─ MODIFY  → 调度员手改输出 → 再 approve → 执行
```

**MUST 字段(approval_event audit log)**:
- skill_id + version
- tool_calls (完整 trace,含入参出参)
- decision (approve / reject / modify)
- reject_reason 或 modify_diff(如有)
- approver_id + approver_cert_level
- approval_timestamp
- second_approver_id(双签场景)

**反馈闭环(skill 自学习,P1 SHOULD,P2 MUST)**:
- reject_reason / modify_diff 进入回流通道,作为 skill 改进素材
- 不应作为 LLM **自动**微调输入(避免污染) — 应进入人工评审 → skill 更新 PR
- 业界相关:LangChain "evaluation tracing" + Anthropic "agent transcripts"

### B.3 SKILL.md 中如何声明 HITL 要求

**建议的 frontmatter 字段**(使用 x-grid-* 命名空间避免与 Anthropic 核心字段冲突):

```yaml
metadata:
  x-grid-hitl-mode: required                   # required / suggested / advisor / none
  x-grid-hitl-tier: critical                   # critical / standard / advisory
  x-grid-requires-two-person-approval: true
  x-grid-approver-cert-level: dispatcher-l4
  x-grid-second-approver-cert-level: safety-officer
  x-grid-approval-timeout-seconds: 300         # 超时未审 → 默认 reject
  x-grid-modify-allowed: true                  # 允许 approver MODIFY 输出
  x-grid-fallback-on-timeout: reject           # reject / hold
```

**host 必须**:
- 启动时校验该字段,若缺则 x-grid-risk-class=A 默认 required
- A 类 skill 缺 x-grid-hitl-mode 字段 → lint 失败,拒绝注册

### B.4 HITL 中断点设计(skill 流程的哪个步骤插入)

**通用规则**: HITL gate 应在 **"决策已成型,执行尚未发生"** 的边界。

**调度 skill 标准 4 段式流程 + HITL 插入点**:

```
[Phase 1] 取数 (read tools)
  ↓ 无 HITL
[Phase 2] 计算/校验 (compute / verify tools)
  ↓ 无 HITL
[Phase 3] 推理/生成 (LLM reasoning) → 产出方案
  ↓ ★★★ HITL gate (调度员 approve / reject / modify)
[Phase 4] 执行 (write / control tools)
  ↓ 无 HITL,但每个 write tool MUST 入 audit log
```

**反模式**:
- ❌ HITL gate 放在 Phase 1 前(还没决策,人不知 approve 什么)
- ❌ HITL gate 放在 Phase 4 后(执行已发生,approve 没意义)
- ❌ Phase 4 内插 HITL(分布式事务,失败回滚极难)
- ❌ skill body 里 LLM 自己"判断要不要询问人" → 必须由 host 强制中断,不靠 LLM 自觉

**LangGraph reference**:
- interrupt() 函数 / interrupt_before 参数 — 标准 dynamic interrupt 模式
- 调度 skill **不应** 让 LLM 决定何时 interrupt;由 host 根据 x-grid-hitl-mode 强制

### B.5 HITL 与 SLA 的冲突 — 不可调和的设计选择

| SLA | HITL 可行性 | 设计选择 |
|-----|-------------|----------|
| 毫秒-秒级(AGC 调节) | 不可行 | AI **不进控制环**;若必须,advisor only |
| 秒-分钟级(电压紧急调整) | 困难 | HOTL + 自动执行 + 人随时 veto |
| 分钟-小时级(检修预案) | 可行 | HITL + 双签是标准 |
| 小时-天级(检修计划制定) | 可行 | HITL + 多级评审 |

**结论**: 调度场景的 HITL 设计第一步是**业务分类**,不是技术优化。秒级 SLA + HITL = 设计错误。这一条 MUST 显式写到对外方案,避免厂商把 HITL 当万能盾牌。

### B.6 调度员信任建立路径(skill 投运的 5 步)

新 skill 上线最大风险**不在技术** — 是调度员**不信任 / 不会用 / 用错**。建议路径:

1. **离线沙盘**(2-4 周): 历史数据回放,skill 输出 vs 调度员实操对比;不接生产
2. **影子模式**(2-4 周): 接生产数据 read-only,skill 输出旁路显示给调度员,**调度员不依据 skill 操作**;统计 skill 输出的"如果按此操作会怎样"
3. **建议模式**(4-8 周): skill 输出进入调度员台前,**advisor 模式**;调度员可参考可忽略;系统统计采纳率
4. **HITL 模式**(8-12 周): skill 输出进入 approval flow;调度员有 approve / reject 权;**至少跑过 N 次 reject 才说明 HITL 真起作用**
5. **正式投产**(若适用): A 类 skill 一般停留在 HITL 不再前进;B 类可进 HOTL

**关键机制**:
- 每个阶段必须有**退出门槛**(KPI:误报率、漏报率、采纳率、reject reason 分布)
- 调度员培训 MUST 包括**反例库**(skill 在 OOD 场景的失败案例)
- 一线调度员对 skill 的 reject reason 进入下一版本规约,形成闭环

---

## §C. Skill ↔ 知识 (RAG / KG)

> **核心原则**: 规程 / 台账 / 案例 / 实时 4 类知识源**适配不同模式**。把规程整段抄进 SKILL.md = 规程改一次 skill 就过期。

### C.1 四种模式对比

| 模式 | 实现 | 优点 | 缺点 | 适用 |
|------|------|------|------|------|
| **A. 硬编码** | 把规程文本直接写进 SKILL.md body | 零检索成本、确定性高 | 规程更新即过期、上下文占用大 | 极稳定 + 短小的核心规则(如"任何 500kV 操作必须双签") |
| **B. RAG** | skill 调用 knowledge.search_proc 工具检索规程库 | 灵活、可更新、不占 skill 上下文 | 检索质量风险、可能漏检关键条款 | 大量分散的规程条款、案例库 |
| **C. KG (Knowledge Graph)** | skill 引用 KG 节点 ID,host 解析关系 | 精准、可推理、关系可见 | KG 建设成本高、维护人力大 | 设备台账(节点结构清晰)、闭锁逻辑(关系明确) |
| **D. 混合** | 关键 invariant 硬编码 + 长尾走 RAG/KG | 兼顾确定性 + 覆盖度 | 设计复杂、需要边界规则 | 调度场景**默认推荐** |

**调度场景默认架构**:
```
SKILL.md body (硬编码)
  ├─ 极少量「永远成立的硬约束」(双签、闭锁)
  ├─ 编排流程指引
  └─ 引用规则:"调用 knowledge.search_proc 取具体条款"

外部知识层
  ├─ RAG (规程 / 案例) — 文本检索 + 重排
  ├─ KG (台账 / 闭锁 / 设备拓扑) — 图查询
  └─ MCP resource (实时量测) — 直接 tool 调用
```

### C.2 知识源分类 + 模式适配

| 知识源 | 类型 | 推荐模式 | 关键工具 |
|--------|------|----------|----------|
| **调度规程**(《调度规程》《安稳导则》《操作规程》《防误手册》) | 文本结构化、章节明确、变更频率中 | **B (RAG) 优先 + D 兜底** | knowledge.search_proc(query, regulation_type) |
| **设备台账**(变电站、线路、机组、定值清册) | 结构化数据、关系密集 | **C (KG)** 或 **MCP DB tool** | pms.get_equipment_info(id) / KG query |
| **运行限额表** | 结构化、与拓扑/温度相关 | **MCP tool** | ems.get_limit(branch_id, scenario) |
| **历史故障案例** | 半结构化、长文 + 元数据 | **B (RAG) + reranker** | knowledge.search_case(query, severity) |
| **典型事故复盘** | 长文报告 | **B (RAG)** | knowledge.search_postmortem(query) |
| **操作票模板** | 半结构化模板 | **CLI tool** 直接生成 | opticket.generate_template(scenario) |
| **SCADA 实时量测** | 时序数据 | **MCP tool** | scada.get_measurement(...) |
| **PMU 数据** | 时序数据 + 高频 | **MCP tool** + 缓存层 | pmu.get_phasor(...) |
| **告警事件** | 事件流 | **MCP tool 订阅** | scada.subscribe_alarm(...) |

### C.3 RAG 输出可溯源性(与 Subagent 1 NEW-M3 决策溯源链对应)

**MUST 要求**:
- RAG tool 返回必须含: chunk_id, source_document, chapter, section, version, last_updated, retrieval_score
- skill 输出引用规程时 MUST 带 source_document + chapter/section + version
- 不允许 LLM 自行编造引用(详见 §A.5 反模式 R8)
- audit log MUST 记录 RAG query + top-k chunks,事后可重放检索复现

**Anthropic Contextual Retrieval(2024-09 公开)** 的 2 个关键改进调度场景应继承:
1. **Contextual Embedding**: 每个 chunk 嵌入前补一段 chunk-specific 的上下文摘要 → 减少 35% 检索失败
2. **Contextual BM25**: 把 contextual 信息也加入 BM25 索引 → 与上一步合用减少 49% 失败
3. + reranker → 减少 67%

**调度场景特化**:
- 规程 chunk 的 contextual prefix 应包含: 章节路径 + 上级条款依赖 + 适用电压等级 + 最后修订时间
- 案例 chunk 应包含: 故障类型 + 发生时间 + 涉及电压等级 + 严重程度

### C.4 RAG 失败处置

| 失败类型 | 处置 |
|----------|------|
| 检索为空 | skill 输出"未找到相关条款"; **严禁** LLM 编造一个 |
| 检索结果矛盾(2 条规程冲突) | skill 输出多条引用 + 标记冲突; HITL 决断 |
| 检索结果与 skill 推理矛盾 | skill MUST 信任规程,不信任 LLM(规程引用单调胜出) |
| RAG 服务不可用 | C 类 skill: degrade,标"规程库不可用";A/B 类: strict,fail |
| 检索分数全部低于阈值 | 等同检索为空 |

### C.5 知识更新流水(规程变更后)

```
规程委员会发布新版 → IT 集成商 / 平台方
  ↓
1. 新版规程入库 (PDF / Word → 结构化 chunks)
  ↓
2. RAG 索引重建(增量 or 全量)
  ↓
3. 影响 skill 扫描:哪些 skill 引用了被改动的章节?
  ↓
4. 受影响 skill 进入 review:SKILL.md body 是否需要改
  ↓
5. 灰度发布:新索引 + 新 skill 先在影子环境跑 N 周
  ↓
6. 正式切换:全平台 skill 引用新索引
```

**MUST 字段**:
- 每条规程 chunk 必须有 version + effective_date
- skill audit log 必须记录"本次决策引用的规程版本"
- 规程版本切换 MUST 走变更评审,不可静默 push

**反模式**:
- ❌ 规程更新后直接 hot reload 索引,不通知任何 skill 维护者
- ❌ 旧版 skill 还在跑,但 RAG 已经返回新版引用 → 事故复盘时责任无法归因

---

## §D. Skill ↔ 已有调度系统

> **核心原则**: skill **不直接** 懂 D5000 / IEC 104 / IEC 61850 等私有 / 行业协议。中间需要一层 **L1.5 业务接口适配层**,把这些协议包装成 MCP tool。这层是**集成商工作的核心**。

### D.1 现有调度系统清单 + 接口现状

| 系统 | 全称 / 厂商 | 主要功能 | 主要协议 / 接口 | skill 接入方式 |
|------|-------------|----------|-----------------|----------------|
| **D5000** | 智能电网调度技术支持系统(国网/南瑞主导) | SCADA + EMS + OPS + SCS + OMS + DTS | 内部 API + IEC 61970(CIM) + IEC 61850 + 自定义 RPC | **L1.5 包成 MCP** |
| **OPEN3000** | D5000 前代 | 调度自动化基础平台 | IEC 61970 / 自定义 | 同上(逐步淘汰) |
| **调控云**(国网新一代) | 国网调度技术支持平台 | 跨级调度协同、模型 / 数据 / 业务三朵云 | RESTful + B 接口(国网行标) | RESTful → MCP |
| **AGC** | 自动发电控制 | 频率 / 联络线功率自动控制 | 内部于 EMS | **只读** MCP tool;skill 不发指令 |
| **AVC** | 自动电压控制 | 电压 / 无功自动控制 | 内部于 EMS | **只读** MCP tool |
| **EMS** | 能量管理系统 | 潮流 / N-1 / OPF / 状态估计 | 内部 API + CIM | **包成 MCP**(只读 + 计算) |
| **PMS** | 生产管理系统 | 设备台账 / 工单 / 检修 | RESTful / DB | **包成 MCP** |
| **OMS** | 运行管理系统 | 操作票 / 检修计划 / 日志 | RESTful / DB | **包成 MCP** |
| **闭锁系统** | 防误闭锁(主站 + 现场) | 操作合法性校验 | 厂家私有 | **包成 MCP**(只读 verify);skill 仅 client 不可 issuer |
| **变电站(SAS)** | 智能变电站自动化系统 | 设备控制 + 量测 | IEC 61850 (MMS / GOOSE / SV) | 一般**不直接**给 skill — 经 SCADA 中转 |
| **EMS-OS / OS2 / OS3** | 操作系统私有协议 | 内部消息总线 | 内部 | 由 D5000 IT 部统一包装 |

### D.2 L1.5 业务接口适配层概念

**架构层次(明确分清)**:

```
┌─────────────────────────────────────────┐
│ L0 协议层  MCP / JSON-RPC / OAuth        │ ← 业界协议(Subagent 2 §1)
├─────────────────────────────────────────┤
│ L1 通用 skill 层  SKILL.md / allowed-tools / hooks │ ← Anthropic Skills 开放标准
├─────────────────────────────────────────┤
│ L1.5 业务接口适配层  D5000-MCP / EMS-MCP / PMS-MCP │ ← **本节焦点 / 集成商核心工作**
├─────────────────────────────────────────┤
│ L2 调度 skill 约束层  风险分级 / 双签 / 闭锁绑定 / HITL │ ← 本规范
├─────────────────────────────────────────┤
│ L3 业务 skill 层  实际 skill 包(实时监控、检修预案、故障处置) │ ← 调度员 + 集成商共建
└─────────────────────────────────────────┘
```

**L1.5 的 7 个核心职责**:
1. **协议翻译**: IEC 104 / IEC 61850 MMS / 私有 RPC → MCP tools/call
2. **schema 投影**: CIM XML / E 文件 → JSON Schema 2020-12 + JSON-LD(可选)
3. **认证桥接**: 既有 PKI(网调 CA)→ MCP OAuth 2.1 token
4. **审计转发**: MCP 调用日志 → 调度审计平台
5. **限流 / 降级 / 熔断**: 保护既有系统不被 LLM agent 流量打爆
6. **风险标注**: 在 MCP annotations 注入 x-grid-risk / 幂等 key / 工单号绑定
7. **国密合规改造**: 替换 SSL/TLS 为国密 SSL,签名走 SM2(详见 §E)

**关键设计选择**:
- L1.5 **不是** 一个新系统,是一组 **MCP server**;每个调度子系统配套一个 L1.5 server
- 集成商按子系统分包负责(D5000-MCP 一队、PMS-MCP 一队);**避免**一个 mega-server
- L1.5 server 可以由**原厂商**提供(华为 / 南瑞 / 国电南自 等)— 比平台方写更现实

### D.3 CIM (IEC 61968 / 61970) 作为语义中介

CIM(Common Information Model)是 IEC 61968/61970/62325 系列定义的电力领域**公共信息模型**(UML 类层级),已是国网内部标准模型。CIM 在 skill 场景的价值:

- **MCP tool 入参 / 出参** 直接基于 CIM 类型做 JSON Schema(如 cim:ACLineSegment、cim:Breaker、cim:GeneratingUnit)
- 不同厂商系统(D5000 / 调控云 / 第三方 EMS)只要都映射到 CIM,skill 不需要**为每个厂商写一套**
- CIM RDF (IEC 61970-552) 可直接做 KG 数据源 → §C.2 KG 模式

**CIM 与 IEC 61850 的协调**:
- 公开论文(2022 HAL 学位论文等)指出: D5000 (CIM) ↔ 变电站 (IEC 61850) 的模型映射**仍是工程问题**
- skill **不应**直接对接 IEC 61850 — 由 SCADA / D5000 完成模型映射后,skill 只看 CIM 视图

**实际建议**:
- L1.5 server 内部用 CIM/E 文件解析既有数据,对外暴露 JSON Schema(基于 CIM 类型命名)
- skill 文档 / 培训 SHOULD 让调度员熟悉 CIM 对象类型,降低写 skill 门槛
- CIM RDF → JSON-LD 转换是已有工业实践,可在 L1.5 实现

### D.4 厂商 SDK / API 标准化暴露建议

**国网 + 各分公司在用的厂商**(均有 D5000 相关产品 / 接口):
- **南瑞**(NARI): D5000 主要供应商之一
- **国电南自**: 调度自动化、变电站自动化
- **华为**: 调控云硬件 + 一些 EMS 模块、Atlas 边缘 / 鲲鹏服务器
- **烽火**: 通信 + 部分调度模块
- **达梦数据库** + **金仓数据库**: 国产数据库,80% D5000 部署在用
- **麒麟 OS / 中标麒麟**: 国产操作系统

**标准化暴露建议**:
1. **国网调度公司层面**统一 L1.5 schema 规范(以 CIM + B 接口为基础),发行 Q/GDW 类企业标准
2. **厂商**按规范实现自己的 L1.5 MCP server,经认证测试
3. **skill 写作者**只看统一 schema,不感知厂商差异
4. 类似 **OPC UA** 在工业自动化领域起的作用(统一访问层,屏蔽厂商) — 调度场景缺这层中介,本规范是机会窗口

**可参考既有标准**:
- 国网 **B 接口**(企业间数据交换接口标准),已大规模部署 → 可作为 L1.5 schema 基础
- IEC 61968-100(系统集成框架)→ 接口设计哲学

### D.5 数据格式适配(E 文件 / IEC 61850 / CIM XML → MCP)

| 既有格式 | 目标 MCP 格式 | 转换工具状态 |
|----------|---------------|--------------|
| **E 文件**(国网内部数据交换格式) | JSON Schema 投影 | 国网内部已有解析器,需暴露为 MCP |
| **CIM XML / RDF**(IEC 61970-552) | JSON-LD(保留语义)或 plain JSON(降维) | RDF→JSON-LD 工具链成熟(rdflib + JSON-LD context) |
| **IEC 61850 MMS**(变电站) | 由 D5000 SCADA 网关转 → CIM JSON | **不直接** 暴露给 skill |
| **IEC 60870-5-104**(主子站通信) | 由 SCADA 转 → CIM JSON | **不直接** 暴露给 skill |
| **Modbus / DNP3**(场站) | 经协议网关 → IEC 104 → SCADA → CIM | **不直接** 暴露给 skill |
| **OPC UA**(部分新建场站) | 直接包成 MCP server(语义层接近) | OPC UA → MCP wrapper 已有开源探索 |
| **私有厂商 RPC** | 一对一包成 MCP | 厂商负责 |

**关键约束**:
- skill **不应** 出现 IEC 104 / IEC 61850 / Modbus 这类原始协议名 — 这些应在 L1.5 完全屏蔽
- 平台 + 集成商 SHOULD 维护"哪些 MCP tool 对应哪个底层系统"的映射表,作为运维诊断依据

### D.6 legacy 不可改的情况下的桥接 + 适配模式

**调度系统的现实**: D5000 部署在生产网,**不允许改**。任何 AI / skill 接入**只能**走外挂中间层。

**5 种桥接模式**:

| # | 模式 | 实现 | 优点 | 缺点 |
|---|------|------|------|------|
| 1 | **只读镜像 / 只读 API** | L1.5 从 D5000 只读取数据(SCADA 量测、拓扑) | 安全、生产无影响 | 只能做分析,不能控制 |
| 2 | **写入门控代理** | L1.5 接收 skill 输出 → 走调度员 approval → 经既有调度终端下发 | 安全、合规 | 必须 HITL,无法自动化 |
| 3 | **数据库直读 + 业务 API 写入** | L1.5 从备库取数据,经原系统业务 API 写入 | 性能好 | 数据陈旧 / 备库延迟 |
| 4 | **协议网关二次封装** | L1.5 直接对接 IEC 104 / 61850 网关 | 实时性好 | 网关被绕过会引发安全审计风险 |
| 5 | **影子部署 + 灰度切换** | 新建并行 D5000-MCP 集群,持续影子运行,逐步切换 | 风险可控 | 部署成本高 |

**强烈推荐**: 调度场景默认 **#1 + #2 组合** — 读用只读 API,写用 HITL 门控 + 既有调度终端。**避免** #4 直接对接底层协议(过分耦合,出问题难溯源)。

### D.7 集成测试不影响生产的策略

**3 层环境隔离(MUST)**:

| 环境 | 用途 | 数据 | 控制权 |
|------|------|------|--------|
| **离线沙盘**(SIM) | skill 开发 + 单元测试 | 历史回放 / 仿真 | skill 写作者 |
| **影子环境**(SHADOW) | 集成测试 + 准入测试 | 生产 read-only | 平台运维 + 调度处审核 |
| **生产环境**(PROD) | 实战 | 生产 read-write | 调度处 |

**关键控制**:
- skill 必须先在 SIM 通过单元测试(假数据 + mock tool)
- 进入 SHADOW 后**不可写**任何 D5000 / 闭锁系统
- 进入 PROD 必须经**正式准入流程**:覆盖率(测试用例)+ 性能(压测)+ 安全(等保渗透)+ 调度处签字

**跨环境唯一性**:
- skill 在 3 个环境**同一个 MD5**(SKILL.md + scripts 的哈希),只是 host 配置不同(MCP server 端点)
- 不允许"为 PROD 临时改 SKILL.md" — 改了必须重新走 SHADOW

---

## §E. Skill ↔ 监管 / 信创合规

> **核心原则**: 中国电力行业 = **关键信息基础设施(关基)+ 等保三级+** + **国密** + **数据不出境**。这些不是 nice-to-have,是**部署门槛**。一行不合规代码 = 系统不能上电。

### E.1 信创要求(国产 LLM / 国产基础设施)

**国产化矩阵(电力行业实际)**:

| 层 | 国产化要求 | 实际选型(国网范围) |
|----|------------|---------------------|
| **CPU** | MUST | 海光 / 鲲鹏 / 飞腾 |
| **OS** | MUST | 麒麟 (Kylin) / 统信 UOS / 中标麒麟 |
| **数据库** | MUST | 达梦 (Dameng,80% D5000 在用) / 金仓 (KingbaseES) / 神通 |
| **中间件** | MUST | 东方通 (TongWeb) / 金蝶 Apusic / 中创 |
| **LLM** | MUST(关基场景) | 文心 / 通义 / 智谱 GLM / DeepSeek / 百川 / Kimi(私有部署) |
| **嵌入模型** | SHOULD | bge / m3e / GTE(均国内) |
| **向量库** | MAY | Milvus / Faiss(开源,国内可控) |
| **容器** | MAY | k8s / Docker(开源,国内可用)+ 信创版加固 |

**Skill 视角的 MUST**:
- skill 调用的 MCP server 必须**完全部署在境内**
- skill 调用的 LLM endpoint 必须是**境内私有化部署**(不可调 Anthropic / OpenAI 的境外 API)
- skill SHOULD 在 SKILL.md 声明**最低模型能力**,host 校验当前模型是否达标
- skill SHOULD NOT 在 body 里硬编码模型名(claude-sonnet/gpt-4),应让 host 注入

**国产 LLM 在调度场景的 5 项关键评估维度**:

| 维度 | 含义 | 调度场景重要性 |
|------|------|----------------|
| Function calling 准确度 | tool 调用参数填写正确率 | **极高**(skill 全靠 tool) |
| 长上下文 | 32K+ 处理能力(规程多、引用长) | 高 |
| 中文专业语义 | 电力术语理解 | 高 |
| 私有化部署能力 | 是否提供境内部署版 + 国密支持 | **MUST** |
| 推理稳定性 | 多次调用一致性 | 高(避免随机性产生事故) |

**业界公开(2025-12)对比**:
- **智谱 GLM**: function calling 表现最好(中英文都准),华为昇腾上 SOTA 训练 — **A 类候选**
- **通义 Qwen 3.5+**: 长上下文 256K 适合规程类、tool calling 成熟 — **A 类候选**
- **文心**: 中文语义佳,但 function calling 文档有限 — B 类候选
- **DeepSeek**: 推理强但 function calling 稳定性需验证 — B 类候选
- **豆包**: 适合工具联动场景,但调度行业 reference 案例少 — C 类候选

**MUST**: 不应锁单一模型 — 平台 SHOULD 抽象 LLM provider,skill 不感知具体模型(避免供应商锁定 + 模型迭代风险)

### E.2 国密合规(SM2 / SM3 / SM4)

**适用范围**(《密码法》+ 等保 2.0):
- 关基系统 MUST 通过密评(密码应用安全性评估),每年 1 次
- 加密 / 签名 / 摘要 必须用国密算法替代国际算法

**算法替换对应**:

| 用途 | 国际算法 | 国密对应 | skill 场景 |
|------|----------|----------|------------|
| 非对称加密 / 签名 | RSA / ECDSA | **SM2** | OAuth token 签名、API 调用签名 |
| 哈希 | SHA-256 | **SM3** | audit log 摘要、调用幂等 key |
| 对称加密 | AES | **SM4** | tool 出参敏感数据加密 |
| 流密码 | 无 | SM7 | 不常用 |

**Skill / 平台层面的 MUST**:
- MCP server 与 host 之间 TLS 必须为**国密 SSL**(SM2 证书 / SM3 摘要 / SM4 套件)
- OAuth 2.1 token 签发使用 SM2(代替 RSA / ECDSA);JWT 改 GM-JWT(SM2 签名版)
- skill audit log 完整性签名 MUST SM2 + SM3
- 业界 OAuth 2.1 + JWT spec 不直接支持 SM2,需要本地化扩展(国密 GM/T 系列已发布相关技术规范)

**已有工业实践**:
- 国密 SSL 证书已有商业 CA 提供(沃通、CFCA 等)
- GmSSL(开源)+ 国密 OpenSSL 在工业场景成熟
- 信创版中间件(东方通、Apusic)已默认支持国密

**反模式**:
- ❌ skill 输出"用 RSA 加密用户数据后存储" — 设计上禁止,等保审查直接拒绝
- ❌ tool 实现里硬编码 import hashlib; hashlib.sha256(...) — 必须走 SM3 wrapper
- ❌ MCP wire 上还在用 ECDSA — 必须切 SM2

### E.3 数据不出境硬约束

**法规**: 《数据安全法》§31 / §40,《网络安全法》§37,《关基条例》§31。
**调度场景实际**: 调度数据是**电力调度运行数据 = 重要数据 + 关基数据**,境外传输需安全评估,**实操中等同禁止**。

**Skill 层面的 MUST**:
- skill 不可调用境外 LLM API(包括所谓"国内代理境外大模型"的灰色路径)
- skill 不可使用 SaaS 形态向量库 / RAG 服务(Pinecone / Weaviate Cloud)
- skill 不可使用境外 telemetry / observability(Datadog / Sentry SaaS)— 必须用境内方案(用友 / 蓝鲸 / 自建 OTel)
- skill audit log MUST 全境内存储,不可上云(指境外云)

**Host / 平台层面**:
- LLM provider 必须本地部署(参 §E.1)
- 模型权重不出境(微调 / RAG 的中间产物均算数据)
- 出境合规审计 SHOULD 集成到 skill lint 阶段(扫描 SKILL.md 是否引用境外服务)

### E.4 等保 + 关基要求

**电力调度二次系统 = 等保三级**(部分核心 = 关基):
- 标准: GB/T 22239-2019(等保 2.0) + DL/T 2614-2023(电力行业等保,2023-11-26 实施)
- 200MW+ 风电 / 光伏场站电力监控系统按等保三级
- 调度数据网内、外网分区,横向隔离装置 + 纵向加密认证网关

**Skill 层面落地要求**:
- skill 部署的服务器必须满足等保三级:边界防护、安全审计、入侵防范、恶意代码防范、可信验证
- skill 平台 SHOULD 通过**关基保护测评**(更严格,注重供应链安全)
- 高危 skill(A 类控制)的访问 MUST 走调度员台账身份认证(可能加密码 token + 工卡 + 生物识别)
- skill 调用频次 / 时段 SHOULD 进入安全态势感知系统(异常调用 = 入侵候选)

**电力行业新增重点**(DL/T 2614-2023 vs GB/T 22239-2019):
- 强化电力监控系统的**网络分区**(生产控制大区 I/II 区,管理信息大区 III 区)
- skill 平台**理论上**部署在 III 区(管理大区) → 经横向隔离装置访问 II 区(非控制类)→ I 区(实时控制)默认禁止
- A 类控制 skill 跨区调用 = 必须特批,等保测评单列项

### E.5 监管审计日志(与 §A.4 调用日志的关系)

**双轨**(都是 MUST):

**轨 1: skill 工程审计**(对内、技术诊断)
- 包含: tool call trace + 入参出参 + LLM 模型 + 温度 + seed + RAG retrieval + 错误处置
- 用途: skill 调试 / A/B 对比 / 性能优化
- 保留: 30 天滚动

**轨 2: 监管审计**(对外、法规)
- 包含: 决策溯源链(详见 Subagent 1 NEW-M3) + HITL approval 记录 + 闭锁系统验证记录 + 模型版本 + 操作员身份
- 用途: 事故复盘、监管检查、电力调度规程符合性证明
- 保留: 行业要求(电力行业可能要求 ≥ 6 个月,关基 ≥ 1 年)
- 加密: SM4 + SM2 完整性签名;**不可篡改**

**MUST 字段(轨 2)**:
- event_id (UUID + SM3 hash)
- skill_id + version + signature
- timestamp (NTP 同步,精度 ≥ ms)
- dispatcher_id + cert_level
- tool_calls (含 input + output digest, 不一定全文,看敏感性)
- decision_trace (LLM 推理摘要 + 引用规程 + 引用案例)
- approval_chain (单签 / 双签 / reject reason)
- affected_equipment (CIM 类型 + ID)
- signature (SM2 over event)

### E.6 业界标准的国情适配

| 业界标准 | 国情适配 |
|----------|----------|
| OAuth 2.1 + DCR | 框架可继承,token 签发改 SM2;DCR 注册需国内身份核验 |
| OIDC | 同上;ID Token 签名 SM2 |
| JWT | GM-JWT(SM2 签名)国密版,GM/T 已发布相关技术规范 |
| W3C Trace Context (traceparent header) | 数据格式可继承,trace 后端 MUST 境内 |
| OpenTelemetry GenAI semconv | 命名规范继承,但 collector 需信创版 |
| TLS 1.3 | TLS 国密版(GB/T 38636-2020 RFC 8998 草案) |
| X.509 证书 | SM2 证书,CA 走国密 CFCA / 沃通 |
| RBAC / ABAC | 框架不变,与国网内部统一身份认证(SSO)对接 |
| FIDO2 / WebAuthn | 国内对应 工卡 + 短信验证 + 生物识别;SHOULD 但不强制 |

### E.7 国产 LLM 选型对调度场景的影响

**Function calling 是 skill 命门** — 选错模型 = skill 全部退化。

**实测建议**(电力 / 调度场景特化):
1. **A 类控制 skill** 在选 LLM 前 MUST 跑**调度专项 benchmark**:
   - tool 选择正确率(选对 tool 的概率)
   - tool 参数正确率(参数 schema 合规率)
   - 多 tool 顺序正确率(SOP 顺序符合规程)
   - 拒绝调用未声明 tool 的能力
2. 模型升级 / 切换 SHOULD 经**回归测试**:历史 skill 输出 diff,差异 > 阈值则报警
3. **温度 / seed** 锁定:A 类 skill temperature=0,B 类 temperature ≤ 0.3
4. **多模型一致性**:关键 A 类 skill 可双模型并行 voting(如 GLM + Qwen),不一致 → HITL

**可参考的国产 LLM benchmark 工具**:
- OpenCompass(国内开源)
- C-Eval、CMMLU(中文知识)
- BFCL(Berkeley Function Calling Leaderboard,可作为底座但需补电力域)
- 调度专项 benchmark **目前不存在**,建议本规范配套定义(对外方案重要 takeaway)

---

## §F. 5 维之间的协作关系(mental model)

```
                           ┌──── §E 信创/合规 ─────┐
                           │ 国产 LLM / 国密 SSL    │
                           │ 等保三级 / 数据不出境 │
                           │ (整个体系的地基)      │
                           └─────────┬──────────┘
                                     │ 全栈合规
                                     ▼
        ┌──────────────────────────────────────────────────┐
        │              skill body (orchestration)          │
        │           ┌─────────────┐                        │
        │           │ LLM 推理    │ ← 决策树 §A.3          │
        │           └──┬──┬──┬───┘                        │
        │              │  │  │                           │
        └──────────────┼──┼──┼───────────────────────────┘
                       │  │  │
              ┌────────┘  │  └────────┐
              ▼           ▼           ▼
       ┌────────────┐ ┌────────────┐ ┌────────────┐
       │ §A 工具    │ │ §C 知识    │ │ §B 人HITL  │
       │ MCP tool   │ │ RAG / KG   │ │ approve    │
       │ CLI / Lib  │ │ 规程/案例  │ │ reject     │
       │ 错误处置   │ │ 实时量测   │ │ modify     │
       └─────┬──────┘ └─────┬──────┘ └─────┬──────┘
             │               │              │
             └───────────────┼──────────────┘
                             ▼
                   ┌──────────────────────┐
                   │ §D L1.5 适配层       │
                   │  D5000-MCP / EMS-MCP │
                   │  PMS-MCP / OMS-MCP   │
                   │  闭锁-MCP / SCADA-MCP│
                   │ (集成商核心工作)     │
                   └──────────┬───────────┘
                              │ 协议翻译 / schema 投影
                              ▼
                    ┌──────────────────────┐
                    │ 调度自动化既有系统   │
                    │ D5000 / 调控云 / AGC │
                    │ AVC / EMS / PMS / OMS│
                    │ (生产网,不可改)     │
                    └──────────────────────┘
```

**5 维各自承担的角色**:
- §A **工具** = skill 的"手脚"(执行)
- §B **人** = skill 的"刹车"(监督)
- §C **知识** = skill 的"教科书"(依据)
- §D **已有系统** = skill 接的"地"(生产)
- §E **信创** = skill 的"地基与红线"(合规)

**任一维 deficit 的失败模式**:
- 缺 §A → skill 让 LLM 自己写算法 → 输出与机理偏离 → A.5 反模式爆发
- 缺 §B → 无 HITL → 出事无法分责 → 监管不放行
- 缺 §C → 规程整段抄进 SKILL.md → 规程改一次就过期 → "AI 让我做的"成立
- 缺 §D → skill 不能调 D5000 → 无生产价值 → 退化成沙盘玩具
- 缺 §E → 信创 / 国密 / 数据出境 任一不通过 → 一行代码也部署不了

---

## §G. Open Issues + Top 5 Takeaway

### Open Issues(本研究 confidence 低 / 仍需深化)

| # | Issue | 当前认知 | 验证路径 |
|---|-------|----------|----------|
| O1 | **MCP 2025-11-25 Async Tasks 在调度长任务的实际成熟度** | 协议刚发布,生态适配进度不一 | 等 1-2 quarter 看 server / client 适配,再决定纳入 MUST |
| O2 | **国密 OAuth / GM-JWT 的具体技术规范成熟度** | GM/T 系列已发布若干文件,但工业实践案例公开不多 | 联系国家密码管理局或国网安全部门确认实际部署情况 |
| O3 | **CIM 在国网各分公司的覆盖度** | 总部 / 部分省调使用,地调 / 县调可能仍是 E 文件为主 | 调研国网内部信息化部 |
| O4 | **国产 LLM 的 function calling 调度域 benchmark** | 目前无公开调度域 benchmark | 建议本规范配套定义,作为后续标准化工作 |
| O5 | **闭锁系统的 MCP 化改造的厂商配合度** | 闭锁系统多为厂家私有黑盒,API 暴露程度不一 | 与南瑞 / 国电南自 / 烽火等厂商讨论 L1.5 接入义务 |
| O6 | **影子环境的"够真实"程度对 skill 上线信心的影响** | 业界普遍认为影子环境有 5-15% 行为漂移 | 经验数据收集,指导阶段 KPI 设定 |
| O7 | **"先调度员培训再 skill 上线"的人力成本** | 一个 skill 培训 100 名调度员 ≈ 200 小时人力 | 路径依赖,需国调统筹 |

### Top 5 Takeaway(对外方案 v0.1 必写入)

> 这 5 条是本研究最重要的工程结论。每一条都对应一个**避免事故 / 避免返工 / 避免信任崩塌**的硬约束。

#### Takeaway #1: skill 不重新发明工具 — 立 9 条反模式 lint 规则

skill body **永远不应** 让 LLM 自己写 SQL / 自己实现 N-1 / 自己模拟潮流 / 自己解释闭锁 / 自己编造规程 / 自己当裁判员。
**对外方案 MUST 列出** §A.5 的 9 条反模式(R1-R9),作为 skill 准入 lint 检查项。
否则 skill 会迅速退化为"在调度环境跑的 LLM 玩具"— 行业不会接受。

#### Takeaway #2: HITL 模式必须按业务分级,不能笼统"加个 approve 节点"

- A 类控制 skill **MUST** HITL + 双签
- B 类分析 skill **SHOULD** HOTL
- 秒级 SLA 控制(AGC)**不适用** HITL → AI **不进控制环**,只 advisor
- 业界 LangGraph 风格的 interrupt() 节点是技术参考,但**调度的关键约束**(双签、秒级、票号)**必须在 SKILL.md frontmatter 强制声明**,host 校验
- 配套调度员信任建立 5 阶段(沙盘 → 影子 → 建议 → HITL → 投产),每阶段明确退出门槛

#### Takeaway #3: L1.5 业务接口适配层是集成商的核心工作 — 必须显式列入对外方案

调度 skill **不直接** 懂 D5000 / IEC 104 / IEC 61850 / 闭锁系统私有协议。
中间需 **L1.5 MCP server**(D5000-MCP / EMS-MCP / PMS-MCP / 闭锁-MCP / SCADA-MCP 等)。
- 这层**最适合**由原厂商(南瑞 / 国电南自 / 华为)提供
- CIM(IEC 61968/61970)是 schema 标准选择
- B 接口可作为 schema 起点
- 这层的认证、限流、审计、风险标注都不可省

**没有 L1.5,skill 永远只能在沙盘里跑**。对外方案 MUST 显式定义这一层(Subagent 1/2/3 都没特别强调,本研究是 SSOT)。

#### Takeaway #4: RAG 是规程对接的默认模式;硬编码 + KG 是兜底

**默认架构**:
- SKILL.md body 只硬编码"永远成立的硬约束"(双签、闭锁、电压等级红线)
- 长尾规程走 RAG(knowledge.search_proc)
- 设备台账 / 闭锁逻辑走 KG(pms.get_* / KG query)
- 实时量测走 MCP tool

**Anthropic Contextual Retrieval(2024-09)+ reranker** 是当前业界 SOTA RAG,减少 67% 检索失败 — 调度场景应继承,**chunk contextual prefix 必须含规程章节路径 + 适用电压等级 + 修订时间**。

RAG 输出 **MUST 可溯源**(chunk_id + chapter + version + retrieval_score),与决策溯源链(Subagent 1 NEW-M3)对接。

#### Takeaway #5: 信创 + 国密 + 关基保护是部署门槛,不是 nice-to-have

**对外方案 MUST 把 §E 列为"非合规则不部署"的硬约束**:
- LLM 必须境内私有化(文心 / 通义 GLM / 智谱 / DeepSeek 候选,**MUST 跑调度 function calling benchmark**)
- 加密 / 签名 / 摘要 必须 SM2 / SM3 / SM4
- 数据 100% 境内,不允许任何境外 SaaS 形态
- 等保 2.0 三级 + 关基保护 + DL/T 2614-2023 电力行业等保
- 国密 OAuth(GM-JWT)+ 国密 TLS 是 wire 层强制

**集成商 SHOULD 把信创合规检查纳入 skill CI**:lint 阶段扫 SKILL.md 是否引用境外 endpoint、tool 是否走国密 wire、模型 endpoint 是否境内。
否则等保 / 密评一票否决,整个 skill 项目作废。

---

## References

### MCP & Skills 协议层
- Model Context Protocol Specification 2025-11-25: https://modelcontextprotocol.io/specification/2025-11-25
- Anthropic — Equipping agents for the real world with Agent Skills: https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills
- agentskills.io — Open standard for Agent Skills: https://agentskills.io/specification
- GitHub anthropics/skills — Public Agent Skills: https://github.com/anthropics/skills
- PulseMCP — OpenAI adopts Agent Skills, Anthropic donates MCP: https://www.pulsemcp.com/posts/openai-agent-skills-anthropic-donates-mcp-gpt-5-2-image-1-5

### HITL 模式
- LangChain — Interrupts in LangGraph: https://docs.langchain.com/oss/python/langgraph/interrupts
- IBM — What Is Human In The Loop (HITL)?: https://www.ibm.com/think/topics/human-in-the-loop
- IT Revolution — Human-in-the-Loop Is Non-Negotiable in Safety-Critical Systems: https://itrevolution.com/articles/human-in-the-loop-is-non-negotiable-leading-ai-adoption-in-safety-critical-systems/
- arXiv 2408.12548 — HITL ML for Safe and Ethical Autonomous Vehicles: https://arxiv.org/html/2408.12548v1

### RAG / GraphRAG
- Anthropic — Contextual Retrieval (2024-09): https://www.anthropic.com/news/contextual-retrieval
- Microsoft GraphRAG — GitHub: https://github.com/microsoft/graphrag
- Microsoft Research — GraphRAG: https://www.microsoft.com/en-us/research/blog/graphrag-unlocking-llm-discovery-on-narrative-private-data/
- DataCamp — Anthropic Contextual Retrieval Implementation Guide: https://www.datacamp.com/tutorial/contextual-retrieval-anthropic

### LLM Tool Error Handling
- Portkey — Retries, fallbacks, and circuit breakers in LLM apps: https://portkey.ai/blog/retries-fallbacks-and-circuit-breakers-in-llm-apps/
- Maxim — Retries, Fallbacks, and Circuit Breakers in LLM Apps: A Production Guide: https://www.getmaxim.ai/articles/retries-fallbacks-and-circuit-breakers-in-llm-apps-a-production-guide/
- GoCodeo — Error Recovery and Fallback Strategies in AI Agent Development: https://www.gocodeo.com/post/error-recovery-and-fallback-strategies-in-ai-agent-development

### Tool Composition / Multi-Step Workflow
- Spring AI — Building Effective Agents (Patterns): https://spring.io/blog/2025/01/21/spring-ai-agentic-patterns/
- Deepchecks — Multi-Step LLM Chains: Best Practices: https://deepchecks.com/orchestrating-multi-step-llm-chains-best-practices/
- arXiv 2601.22037 — Optimizing Agentic Workflows using Meta-tools: https://arxiv.org/html/2601.22037v2
- LangChain — Workflows and agents: https://docs.langchain.com/oss/python/langgraph/workflows-agents

### 电力调度系统(D5000 / 调控云 / CIM / IEC)
- IEC — Common Information Model (Wikipedia): https://en.wikipedia.org/wiki/Common_Information_Model_(electricity)
- Springer — IEC 61970/61968 Common Information Model: https://link.springer.com/chapter/10.1007/978-3-642-34916-4_6
- PNNL — A Power Application Developer's Guide to the CIM: https://www.pnnl.gov/main/publications/external/technical_reports/PNNL-34946.pdf
- ENTSO-E — Common Information Model for Grid Models Exchange: https://www.entsoe.eu/digital/common-information-model/cim-for-grid-models-exchange/
- HAL theses-03828420 — Using IEC 61850 and CIM (CIM-61850 harmonization): https://theses.hal.science/tel-03828420/file/DIOP_2022_archivage.pdf
- 北极星电力 — 探访新一代智能电网调控系统 D5000: https://m.bjx.com.cn/mnews/20130402/426358.shtml
- 达梦数据 — 联合国家电网打造智能电网调度技术支持系统: https://www.dameng.com/case_44.html
- 中国电力企业联合会 — 新一代电网调度自动化系统支撑平台关键技术: https://www.csee.org.cn/pic/u/cms/www/201912/04143723f7am.pdf
- Atlantis Press — Research and Application of Dispatching Misoperation Prevention: https://www.atlantis-press.com/article/25866784.pdf

### 信创 / 国密 / 等保
- 公安部 — 网络安全等级保护 2.0 标准解读: https://m.mps.gov.cn/n6935718/n6936584/c7369073/content.html
- 安全内参 — 电力行业主要标准与等保 2.0 标准的对比: https://www.secrss.com/articles/17224
- 安全内参 — 从等保 2.0 看密码应用: https://www.secrss.com/articles/11997
- 博客园 — 等保 2.0 各级别基本要求与安全设备清单: https://www.cnblogs.com/suntroop/articles/18816760
- DL/T 2614-2023 — 电力行业网络安全等级保护基本要求: https://zhuanlan.zhihu.com/p/668459820

### 国产 LLM
- 阿里云 — Qwen Function Calling 文档: https://help.aliyun.com/zh/model-studio/qwen-function-calling
- CSDN — 国产七大 AI 模型对比: https://blog.csdn.net/EnjoyEDU/article/details/148221701
- 53AI — 大模型在电力行业的应用案例: https://www.53ai.com/news/zhinengyingjian/2025050123865.html
- BetterYeah — 电力企业如何构建高效 AI 知识图谱: https://www.betteryeah.com/blog/how-to-build-a-high-efficiency-ai-knowledge-graph-for-power-enterprises

### 工业协议 / Bridge
- Bivocom — How to Convert Modbus to IEC104: https://www.bivocom.com/blog/how-to-covert-modbus-to-iec104
- PBS Control — SCADA Protocols Introduction: https://www.pbscontrol.com/pdf/SCADAProtocols.pdf
- Westermo — SCADA Protocol conversion to IEC 104: https://www.westermo.com/solutions/industrial-remote-access/Protocol-conversion/protocol-conversion-iec-104

### 已有内部研究(同一系列)
- docs/external-review/2026-04-26-research-mandatory-validation.md (Subagent 1 — MANDATORY 验证)
- docs/external-review/2026-04-26-research-skill-industry-standards.md (Subagent 2 — 业界标准 survey)
- (in progress) Subagent 3 — skill 编写方法论 + taxonomy + 库治理
