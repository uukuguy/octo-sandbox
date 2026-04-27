# Research: Skill 编写方法论 + 原子/SOP 分类 + 技能库治理

**研究日期**: 2026-04-26
**目的**: 解决"企业用户如何写出合格 skill 并组织成可维护的库" — 调度业务专家不会写 skill,IT 集成商不会建库
**作用域**: 跟 Subagent 1(MANDATORY 验证)和 Subagent 2(协议 / schema 标准)互不重叠;本文专攻**人(写 skill 的人)+ 库(组织 skill 的人)**
**读者假设**: 已熟悉 MCP、SKILL.md frontmatter、tools/list 协议;不再解释这些
**口吻**: 写给企业架构师 + 业务专家联合阅读的方法论指南,**有可执行步骤,不写抽象口号**

---

## §0. 研究方法说明

- 三大来源:
  1. **Anthropic 官方** (`platform.claude.com/docs/.../agent-skills/best-practices`、`anthropic.com/engineering/equipping-agents-...`、Anthropic 32 页 The Complete Guide to Building Skills for Claude PDF)
  2. **agentskills.io 开放规范** (是 Anthropic 12 月开源的跨平台规范,Codex/Cursor/Gemini CLI/OpenCode/Windsurf 都已对接 — 是事实社区标准)
  3. **企业治理实践**(JFrog Agent Skills Registry、Tessl Skills Registry × Snyk、Alibaba Nacos 3.2 Skill Registry、Anthropic Enterprise Skills doc)
- 框架对比来源:Microsoft Semantic Kernel(`learn.microsoft.com/.../semantic-kernel`)、CrewAI(`docs.crewai.com`)、LangGraph(`docs.langchain.com`)、AutoGen / Microsoft Agent Framework
- 失效模式数据:Snyk ToxicSkills 报告(3,984 skills 抽样,36% 含 prompt injection)、cashandcache 40 skills 失败分析、MindStudio context-rot 报告
- 电力调度业务侧:GB/T 33590《智能电网调度控制系统技术规范》、电力行业 SOP 实践文献(标准化倒闸操作、故障处置预案、操作票审查专家系统)
- 凡未独立 verify 的电力行业具体条款号,文中明确标注"业务专家校对"

---

## §A. Skill 内容编写方法论

### A.1 Skill vs Prompt 核心差异(从写作视角)

业界最有共识的 4 条差异(Anthropic + agentskills.io + JFrog + cashandcache):

| 维度 | Prompt | Skill |
|------|--------|-------|
| **触发方式** | 用户每次手动输入 | LLM 根据 description 自主判断是否激活 |
| **生命周期** | 单次对话 | 持久化文件,跨会话 / 跨用户复用 |
| **结构** | 自由文本 | 结构化目录(SKILL.md + scripts/ + references/ + assets/) |
| **可观测性** | 跑一次过一次 | 可版本化、可 review、可 evaluate、可 deprecate |

**写作视角的关键迁移** — 这是业务专家最容易绊倒的地方:

1. **从"我现在告诉 AI 怎么做"→"未来一类任务都让 AI 这么做"**
   prompt 是"这一次"的指令;skill 是"每一次符合此模式时"的程序。这要求作者有**模式抽象能力**:从一次具体调度操作里识别出通用步骤、留出可变参数、分离硬约束和启发式建议。
   *Anthropic 给的方法*:Extract from a hands-on task — 业务专家先用普通 prompt 跟 AI 跑一次真实任务,然后回过头让 AI 把"这次成功的步骤、纠正点、上下文"提炼成 skill。**不是先写 skill,而是先成功一次再固化**。

2. **从"自由发挥"→"degrees of freedom 校准"**
   prompt 不需要担心 AI 自由度;skill 必须**明确**告诉 AI "这一步给你多少自由":
   - **High freedom**(代码 review、检修方案分析):多种合理路径,给方向不给步骤
   - **Medium freedom**(操作票草稿):有偏好模板,允许参数调节
   - **Low freedom**(发遥控指令、改定值):必须 exact sequence,任何偏离即拒绝
   Anthropic 用"机器人在悬崖窄桥 vs 开阔田野"打比方 — 调度场景里**控制类 skill 一定 low freedom,辅助分析 medium,信息查询 high**(对应 Subagent 1 §0 的三类划分)。

3. **从"读者是 AI"→"读者是 AI + 未来的同事"**
   skill 是会被另一位调度员读、review、改的工程产物。每行字得能解释"为什么这么写"。**原文 anti-pattern**: agentskills.io 警告"don't generate a skill by asking an LLM with no domain context — the result is generic procedures like 'handle errors appropriately'"。这正是华为方案被怀疑 AI 生成的根因。

4. **从"想到啥说啥"→"省 token 预算"**
   Skill 一旦激活,full body 进入 context window,跟对话历史、其他 skill metadata 抢空间。Anthropic 硬性建议:**SKILL.md body 控制在 500 行 / 5000 token 以内**;agentskills.io 同上限。这跟 prompt 工程的"越详细越好"是相反的直觉。

> **核心 takeaway**:Skill 编写不是 prompt 编写的"加长版",而是介于 prompt 工程、技术文档、API 设计、SOP 标准化作业书之间的**新文档类型**。写 skill 的人应当具备:领域专业 + 模式抽象 + 文档工程 + 一定的 LLM 行为感知。

---

### A.2 写好 SKILL.md 的实践模式

#### A.2.1 description 字段写法(skill 能不能被触发的命门)

agentskills.io 整篇 `Optimizing skill descriptions` 都在讲这个。综合 Anthropic + agentskills.io + Tessl 实战(开源 skill 评分从 22% 提升到 100%):

**5 条硬规则**:

1. **第三人称、命令式** — "Use this skill when..." 而非 "I can help" 或 "This skill does"。
   *Anthropic 原文*: "Always write in third person. The description is injected into the system prompt, and inconsistent point-of-view can cause discovery problems."

2. **同时回答 What + When**:做什么 + 什么场景下用。
   - ❌ "Helps with PDFs."(模糊)
   - ✅ "Extracts text and tables from PDF files, fills PDF forms, and merges multiple PDFs. Use when working with PDF documents or when the user mentions PDFs, forms, or document extraction."

3. **包含触发关键词,且覆盖"用户没明说但你应该接"的场景**
   agentskills.io 原文:"Err on the side of being pushy. Explicitly list contexts where the skill applies, including cases where the user doesn't name the domain directly: 'even if they don't explicitly mention CSV or analysis.'"
   电力调度示例:
   - ✅ "Use when user asks for operating ticket, switching sequence, isolation procedure, or 操作票/倒闸顺序/检修步骤,even if they only describe the goal like 'shut down line 110-X for maintenance'."

4. **聚焦用户意图,不暴露内部机制**
   - ❌ "Calls SCADA WebService APIs and queries Oracle PI historian"(LLM 不知道用户什么时候想要 SCADA;用户也不会用这个词)
   - ✅ "Retrieves real-time and historical telemetry for substations, lines, and units. Use when checking active power, voltage, frequency, breaker status, or 'is X breaker open?'"

5. **1024 字符硬上限,但不能为了短而牺牲触发覆盖**
   Tessl × Snyk 的实战数据(`snyk.io/blog/snyk-tessl-partnership`):"oauth skill 描述从 22% 评分提升到 100%, nodejs-core 从 45% 到 100%, fastify 从 48% 到 100%" — 改的就是描述的 trigger term 覆盖度 + 具体使用示例。**对企业 skill 库的启示是:description 评分应当作为 quality gate 之一**(见 §C.7)。

**调度领域 description 模板**(可直接套用):

```yaml
description: >
  [What] Performs [specific capability] for [substation/line/unit/scope].
  [When] Use when user asks about [keyword 1, keyword 2, ... including
  Chinese variants 调度术语], OR when the task involves
  [specific operational context], even if [domain term] is not mentioned.
  [Out of scope] Does NOT [what other skill handles].
```

**调度场景反例**(不应该这么写):

```yaml
# 反例 1: 太宽,会 over-trigger 抢其他 skill 的活
description: Helps with electrical operations.

# 反例 2: 内部机制,LLM 不知道何时该选
description: Wraps the D5000 RPC interface for telecontrol commands.

# 反例 3: 第一人称,Anthropic 明确反对
description: I can help you generate operating tickets for switching operations.
```

#### A.2.2 progressive disclosure 应用 — body 不是越长越好,而是分级

Anthropic 把 skill 设计的核心原则定义为 **progressive disclosure**(渐进披露,3 级):

```
第 1 级:metadata (name + description) — 启动时全员加载 (~50-100 token/skill)
第 2 级:SKILL.md body — LLM 决定用此 skill 时才读 (<500 行)
第 3 级:references/*.md, scripts/*.py, assets/* — body 引用时才按需读
```

**电力调度 skill 的层级建议**:

```
第 1 级 (description):
  "Generates 220kV substation switching operation tickets..."

第 2 级 (SKILL.md body, <500 行):
  - Critical Rules (硬约束 — 闭锁条件、双人审批、机理校核 reject)
  - Workflow steps (查台账 → 排序 → 闭锁校核 → 输出 → 复核)
  - Out of scope (不做调度命令直接下发,不做带电作业方案)

第 3 级 (references/, 按需加载):
  - references/standard-templates.md (各电压等级操作票模板)
  - references/interlock-rules.md (五防闭锁规则全集)
  - references/troubleshooting-faq.md (常见 reject 原因 + 修复)
  - scripts/validate_ticket_format.py (格式校验,直接执行)
```

**关键反模式**:Anthropic + Anthropic 32 页 PDF + cashandcache 40 skills 调研都重点警告 **"deeply nested references"** — 当 references 文件再引用 sub-references 时,LLM 经常用 `head -100` 部分读,导致信息丢失。**所有 reference 必须从 SKILL.md 直接 link,只 1 级深**。

#### A.2.3 步骤 / 约束 / 反例的混合(SKILL.md body 内部结构)

cashandcache 分析 40+ skills 失败后给出的"必备 4 段"结构:

```markdown
# {Skill Name}

## Critical Rules    # 不可违反的硬约束 — 在最显眼处
- MUST [硬约束 1]
- MUST NOT [禁止 1]
- 任何 ambiguity → ask user, do not assume

## Core Workflow    # 主步骤,每步可勾选
1. [step 1, 含调用什么 tool]
2. [step 2, 含决策点]
   - if X: do A
   - else: do B
3. [step 3, 含 validation]

## Examples    # 输入 → 输出对,>= 2 个
### Example 1: 典型场景
**Input**: ...
**Output**: ...
### Example 2: edge case
**Input**: ...
**Output**: ...

## Boundaries    # 范围 + 不在范围
**In scope**: ✓ ...
**Out of scope**: ✗ ... → use {other-skill} instead
**When unclear**: ask 2-3 specific clarifying questions
```

**Anthropic 关键发现** — `Gotchas sections` 是高价值内容:
> "The highest-value content in many skills is a list of gotchas — environment-specific facts that defy reasonable assumptions. These aren't general advice but concrete corrections to mistakes the agent will make without being told otherwise."

调度场景的 Gotchas 示例(必须以这种**反直觉具体事实**形式写):

```markdown
## Gotchas

- 110kV 隔离开关在主变低压侧"分位"信号,在 SCADA 上显示为 "0/OFF",但在保护逻辑里
  对应 "DISCONNECTED"。涉及保护定值时使用后者命名,不是 "0/OFF"。
- 双母线接线的"母联开关"在五防闭锁里属于 type=COUPLER 不是 type=BREAKER,虽然
  在调控云图上画法相同。否则 N-1 校核会把它当线路处理导致漏算。
- "调度员命名" 不等于设备双重编号:"#1 主变" 在 A 厂可能对应"#2 主变" 在 B 厂的实际编号,
  跨厂操作必须用双重编号(站名+设备编号)。
```

#### A.2.4 何时贴 example,何时不

cashandcache 40 skills 调研第二大失效模式:**skills 激活了但产生 generic / 无用 output** — 根因 80% 是 example 不足或 example 不真实。

**贴 example 的 4 条规则**:

1. **示例长度 ≥ 规则长度**(cashandcache 强约束) — 业务专家本能"列规则",AI 学不到模式;LLM 是 pattern matcher,**show, don't tell**。
2. **真实输入 + 真实输出**,不要修整。包括用户原始问法(口语化、错别字、不完整)+ 完整响应(完整格式、完整长度)。
3. **2-3 个示例覆盖常见 + edge case**:
   - Example 1: typical case
   - Example 2: edge case(残缺输入、多 valid answer、需要 ask back 的场景)
   - Example 3 (optional): high-stakes case
4. **何时不贴示例**:
   - 信息查询类 skill(纯 RAG,LLM 自己能产生格式合理的答案)
   - 通用太广无法用 1-2 个 example 代表的 skill(此时该拆 skill,不是堆 example)
   - 规则极简洁、机理校核直接 reject 即可的 skill(留点 token 空间)

#### A.2.5 与 system prompt 的边界(企业最容易混淆)

JFrog 原文(`jfrog.com/blog/agent-skills-new-ai-packages`):
> "skills allow organizations to embed the company's operational DNA directly into the instructions that guide the agent's behavior."

但**操作 DNA** 不应该全装在一个超大 system prompt 里 — 那是上一代 prompt 工程的死胡同(MindStudio context-rot 报告:200K context, 12K skill file = 6% 不读任务前已被消耗)。

**判断"放 system prompt 还是放 skill"的 3 条原则**:

| 内容 | 放 system prompt | 放 skill |
|------|----------------|---------|
| 全局身份 / 立场 / 安全红线 | ✅(每条对话都生效) | ❌ |
| 跨任务通用约束(规程引用必填、双人审批) | ✅(平台级,非任务级) | ❌ |
| 特定任务的步骤、模板、闭锁规则、Gotchas | ❌ | ✅ |
| 触发性强的"用户提到 X 才需要"知识 | ❌(永远在 ctx) | ✅(按需加载) |
| 跨厂商互通的"操作 DNA" | ❌(平台特异) | ✅(可移植) |

**调度场景具体例子**:

```
✓ system prompt 应包含:
  - "你是一个电力调度辅助 AI,所有控制类输出 MUST 经过机理校核"
  - "MUST 引用规程条款编号 + 量化校核数据"
  - "禁止跨调度层级越权(地调不发省调指令)"

✓ skill 应包含:
  - "220kV 倒闸操作的 13 步标准流程"(specific workflow)
  - "母线故障应急预案的步骤树"(specific decision tree)
  - "AGC 调节 skill 的限值表"(specific lookup data)

✗ 不该塞 system prompt 的:
  - 任何具体设备参数表(占 token,大多数对话用不到)
  - 任何 specific workflow 步骤(用得上时才该装载)

✗ 不该塞 skill 的:
  - 全局合规要求(应该是平台 system prompt + adr-guard hook)
  - "永远不许越权"这种通用红线(全局)
```

---

### A.3 Skill 出错的典型表现 + 反模式

汇总 4 个一手来源:Anthropic best practices anti-patterns、cashandcache 40 skills、MindStudio context-rot、Snyk ToxicSkills(3,984 skills 36% 有 prompt injection)。

#### 失效模式 9 类(优先级排序)

| # | 表现 | 根因 | 调度场景灾难等级 |
|---|------|------|---------------|
| **1** | **Skill 该激活时不激活** | description 触发词不全 / 没有 imperative phrasing / 用户用词与 description 词法不匹配 | 🔴 高 — 操作员以为 AI 用了 SOP skill,实际走默认 prompt,无机理校核 |
| **2** | **Skill 激活但产生 generic / 不用项目知识** | body 缺 examples / 缺 gotchas / "the agent already knew this" 类废话太多 / 缺 project-specific 上下文 | 🔴 高 — AI 给一条"通用电力操作建议",看似合理但不引用本厂双重编号 |
| **3** | **Over-trigger / 抢其他 skill** | description 太宽 / 没列 out-of-scope | 🟡 中 — "操作票 skill" 抢了"故障处置 skill" 的活,逻辑断裂 |
| **4** | **Context rot — 长会话后 skill 越写越钝** | SKILL.md body 持续增长 / 矛盾累积 / 中段信息被"lost in the middle" | 🟡 中 — 调度运行 8 小时后 AI 开始遗漏闭锁校核 |
| **5** | **越权 / 越能力** | 没有 Boundaries 段 / 没有 escape hatch("when unclear ask back") / scripts 调用范围未限 | 🔴 高 — "查台账 skill" 被 LLM 误用执行修改台账 |
| **6** | **幻觉式补字段 / 编造规程条款** | skill 鼓励 LLM 输出格式化结果但没有强制溯源约束 / 缺 validation loop | 🔴 极高 — AI 编一个不存在的"GB/T 31464-2015 §5.3.7" 让操作员以为有依据 |
| **7** | **Prompt injection 通过 skill 注入** | skill 来自不信任源 / SKILL.md 含恶意指令 / scripts 含外联 | 🔴 极高 — Snyk 报告 36% 公开 skill 有此问题,在调度内网相当于"内鬼" |
| **8** | **指令冲突(skill 内 / skill 间)** | 早期版本约束 vs 后加约束 / 多 skill 同时激活产生矛盾 | 🟡 中 — 检修 SOP skill 说"先停信号", 故障处置 skill 说"先发信号", 都触发时 AI 蒙圈 |
| **9** | **Outgrowth / Regression** | 模型升级后 skill 该有的引导被基模型自带能力盖住 / 模型行为变化导致原 skill workflow 失效 | 🟡 中 — Claude 4.7 升 5.0 后某些 skill 行为漂移 |

**关键洞察**: 1, 2, 6 三个失效模式占 cashandcache 40 skills 调研中 70%+ 的失败案例。**调度规范的 quality gate(§C.7)必须显式覆盖这三类**。

#### 反模式具体例子(对照)

```markdown
# ❌ 反模式 1: 触发词只覆盖正式术语
description: Generates 倒闸操作票 for 变电站.

# 用户问 "把 110-X 线停了做检修", AI 不会触发(没出现 "倒闸"/"操作票")

# ✅ 修复:
description: >
  Generates switching operation tickets (倒闸操作票) for substations.
  Use when the user wants to take a line/transformer/feeder out of service,
  put it back in service, isolate equipment for maintenance, or asks for
  操作票/倒闸顺序/检修隔离/送电步骤/停电步骤. Use even if user only describes
  the goal (e.g. "把 110-X 停了做检修") without using these exact terms.

---

# ❌ 反模式 2: 全是 declarative,不教 procedure
## Generate Operation Ticket
The ticket should be safe and follow regulations.

# ✅ 修复:procedural workflow
## Workflow: Generate 220kV Switching Operation Ticket
1. Read source substation single-line diagram (call: scada_query_topology)
2. Identify switching boundary (operating equipment + auxiliary equipment)
3. For each isolation: validate against five-prevention rules
   (call: interlock_check, MUST exit if any rule fails)
4. Order operations: open breakers FIRST, then disconnectors, then earthing
5. Output formatted ticket per references/template-220kV.md
6. Append regulation references (cite exact 条款 编号 from regulation DB,
   never invent)

---

# ❌ 反模式 3: 没有 out-of-scope
## Operation Ticket Skill
[generates tickets for everything]

# ✅ 修复:
## Boundaries
**In scope**:
  ✓ 110kV - 500kV 倒闸操作票(常规检修 / 送电 / 隔离)
  ✓ 操作票草稿生成,送审前阶段
**Out of scope**:
  ✗ 660kV / 750kV / 1000kV 特高压 → use special-voltage-ticket skill
  ✗ 带电作业方案 → use live-work-skill
  ✗ 故障处置预案 → use fault-handling skill
  ✗ 实时下发指令(本 skill 只产生草稿,不下发)
**When unclear**: ask user about voltage level, equipment scope, and
  operational reason before generating.

---

# ❌ 反模式 6: 鼓励 AI 编规程条款
## Output Format
Each step should cite a regulation reference.

# ✅ 修复:强制溯源 + reject 编造
## Output Format
Each step MUST cite a regulation reference using format:
  [规程编号]§[条款号] (e.g. "DL/T 5429-2009 §6.3.2")
**MUST NOT** invent regulation numbers. If no regulation applies,
write "无规程条款依据" — never fabricate.
Validation: run scripts/verify_regulation_refs.py before output;
unknown references → ABORT with error message listing unverified refs.
```

---

### A.4 调度场景特化建议

#### A.4.1 把电力调度规程写成 skill body 的方法

业务专家最常踩的坑:把规程条款照抄进 SKILL.md。这是失败的开始 — 规程是法律语言(完整、形式化、覆盖一切),skill 是操作语言(可执行、有顺序、有 escape hatch)。

**4 步转写法**:

1. **从规程提取硬约束**(放 Critical Rules / Gotchas)
   - 规程语言:"操作票必须经值班负责人审核"
   - skill 语言:`MUST: ticket draft must include reviewer_id field; if missing, reject with INTERLOCK_REQUIRED`

2. **从历史操作票提取 procedural patterns**(放 Workflow)
   - 不要从规程导出步骤(规程是 declarative);从过去 100 张已审通过的操作票里**统计共性顺序** → 写成 numbered workflow

3. **从历史误操作 / 反措通报提取 Gotchas**(放 Gotchas)
   - 这是最高价值的内容。每条 Gotcha = 一次"差点出事"或"已经出事"的具体反直觉事实
   - 例:"#1 主变中性点接地刀闸 SCADA 显示与现场实际相差 180° (历史接线问题),N-1 校核 MUST 用现场图,不用 SCADA 状态"

4. **从规程例外条款提取 Boundaries**(放 Out of Scope)
   - 规程"不适用于"的场景 → skill out-of-scope
   - 例:"500kV 系统大方式调整不适用本规程" → skill out-of-scope: "Does NOT cover 500kV system grand mode adjustments → use grand-mode-skill"

#### A.4.2 调度业务专家写 skill 的"Anthropic A/B 法"本地化版

Anthropic 推荐的"Claude A 帮你写 skill, Claude B 用 skill 跑活,你观察"在调度场景需要适配:

```
角色 A (skill 作者助手):    业务专家 + AI 协作 → 写 SKILL.md
角色 B (skill 使用者):      调度员 + AI 协作 → 用 skill 跑真实任务
观察者:                    业务专家 / 班长 / 标准化组

工作循环:
  1. 业务专家与 A 用普通对话完成一次真实操作票(不用 skill)
  2. 业务专家:"把这次成功的过程提炼成 skill"  → A 给出 SKILL.md 草稿
  3. 调度员用此 skill 在 B 处跑下一份操作票  → 观察:触发了吗?用对了吗?
  4. 业务专家观察异常 → 回到 A:"它漏了 X 闭锁"  → A 改进 skill
  5. 重复 3-5 次 → skill 进入试运行
```

**关键差异 vs 通用 Anthropic 流程**:
- B 端的"调度员"是**真业务用户**,不是测试工程师 — 他们的操作模式带真实噪声(口语化指令、不完整输入)
- 观察周期至少跨 3 个调度班(早班 / 中班 / 夜班 / 周末班),否则覆盖不全
- "失败"的定义不是 LLM 报错,而是"调度员重写了多少 / 复核员改了多少 / 是否被五防系统拒"

#### A.4.3 调度 skill 必备的 7 个标准段(在通用模板上加的)

通用 4 段(Critical Rules / Workflow / Examples / Boundaries)之外,调度 skill 应加:

```markdown
## 5. Mechanism Constraints (机理硬约束)    # 对应 Subagent 1 §3.1
- 引用规程条款 + 数值阈值
- AI 输出违反 → reject + MECHANISM_VIOLATION error
- 必须有至少 1 条"AI 推荐方案违反约束"的反例

## 6. Time Class (时效分级)                # 对应 Subagent 1 §3.5
time_class: minutes_powerflow  # 或 seconds_AGC / hours_dayplan / days_outage

## 7. Interlock Requirement (闭锁要求)     # 对应 Subagent 1 §3.4
interlock_required: true | false
required_token_format: ticket-{YYYYMMDD}-{seq}
no_token_behavior: ABORT_WITH_INTERLOCK_REQUIRED

## 8. Output Provenance (输出可溯源)       # 对应 Subagent 1 §3.3
required_fields:
  - regulation_refs: [{regulation, clause, version}]
  - quantitative_checks: [{quantity, threshold, margin}]
  - impact_assessment: {load_loss_kw, stability_margin_change}

## 9. Escalation (升级路径)
when_to_escalate:
  - any mechanism_violation
  - confidence < 0.85
  - cross-jurisdiction (e.g. 涉及邻省调)
escalate_to: human_dispatcher | senior_engineer | safety_officer
```

---

## §B. 原子 Skill vs SOP Skill 分类

### B.1 业界类似 taxonomy 概览

不同框架对"原子能力 vs 流程编排"的命名不同,但概念高度对应。下表是 5 大主流 agent 框架的对照:

| 框架 | 原子(单一能力) | SOP(流程编排) | 关键差异 / 备注 |
|------|---------------|--------------|----------------|
| **Anthropic Agent Skills** | "Skill" — 一个 SKILL.md 目录,聚焦单一 coherent unit | 同样是 Skill,但 body 是 multi-step workflow + checklist + 调用其他 skill / scripts | **官方不显式区分**;agentskills.io 的 "design coherent units" 隐含这是设计者责任。这是当前规范的**真空地带** — 也是华为方案要补的缝隙 |
| **Microsoft Semantic Kernel** | **Plugin function** — 一个 `[KernelFunction]` 标注的 native 方法 | **Planner**(已弃用,已被 function calling loop + Process Framework 替代);现在主流是**Process Framework / Workflow** | 历史上 Plugin = 多个 function 的组合;新趋势 Plugin 内部就是原子级,流程交给 Workflow 层 |
| **CrewAI** | **Task** — 单个 Agent 完成的具体任务 | **Crew** — 多个 Agent + Task 的组合,有 Process(sequential / hierarchical) | Task 自带 description + expected_output + agent;Crew 用 Process 决定串行还是分配 |
| **LangGraph** | **Tool**(单个 callable)+ **Node**(图中一个状态变换) | **Graph / StateGraph** — 显式状态机 + 边 + 条件路由 | 强调 stateful, 适合长任务 / 需要 retry / human-in-loop。Microsoft Agent Framework 的 Workflow 思想类似 |
| **AutoGen / Microsoft Agent Framework** | **Agent**(单一职责) + **Tool** | **GroupChat**(自主协商)+ **Workflow / Handoff**(显式编排) | GroupChat 是 LLM 自主路由(灵活但不可预测);Workflow 是显式 graph(可控但需设计) |

#### 关键观察 1:业界正在从"LLM 自主决策"向"显式编排"回流

- Semantic Kernel 弃用 Planner,改用 function calling loop + Process Framework
- AutoGen 标注 deprecated,迁移到 Microsoft Agent Framework 的 Workflow
- CrewAI 推 hierarchical Process(由 manager agent 显式分配)取代纯 sequential
- **业界共识**:**纯 LLM 自主调度多步骤是不可靠的,必须有 explicit graph / SOP 兜底** — 这对调度场景 mandatory(电网不允许 AI 即兴发挥)

#### 关键观察 2:agentskills.io 的"原子 vs SOP"命名空白

agentskills.io 规范不强制区分。Anthropic best practices 唯一接近的提示是:
> "Design coherent units. Skills scoped too narrowly force multiple skills to load for a single task. Skills scoped too broadly become hard to activate precisely."

**这意味着调度规范应当显式补上这个 taxonomy**(本文 §B.2-B.4)。

#### 关键观察 3:dependency graph 是新兴的 skill 治理工具

Graph of Skills 论文(arXiv 2604.05333)、Tessl 的 skill-architect、`skill-router`(LobeHub)都在做同一件事:**当 skill 数 >100 时,纯 description 触发不够,需要显式 dependency / composition graph**。Skill Instruction File Dependency Resolution(tdcommons.org)甚至给出版本范围解析算法(类似 npm)。

> **这对调度规范的启示**: 一旦 skill 数突破 ~50,必须引入"原子-SOP"二级 taxonomy + dependency 显式声明,否则触发不稳。

---

### B.2 原子 skill 定义 + 设计原则

#### B.2.1 定义(给业务专家用的判断标准)

**原子 skill = 满足以下 4 个条件的 skill**:

1. **单一可重用能力**:做一件事,做完返回。例如"查 SCADA 量测"、"算潮流"、"校核 N-1"
2. **不调用其他 skill**(只能调用 MCP tools / scripts)
3. **不维持跨 turn 状态**(无状态)
4. **可独立测试**(给它输入,检查输出,与其他 skill 解耦)

#### B.2.2 4 条设计原则

1. **粒度准则:1 skill = 1 verb + 1 noun**
   ```
   ✅ "查询 SCADA 量测"             (query + telemetry)
   ✅ "计算 N-1 潮流"                (calculate + N-1 flow)
   ✅ "生成操作票表头"               (generate + ticket-header)
   ✅ "校核五防闭锁"                 (verify + interlock)
   ❌ "查询并算 N-1"                (违反单一职责,该拆 2 skill)
   ❌ "完成检修流程"                (太广,这是 SOP)
   ```

2. **可组合性:输出格式标准化,便于被 SOP 串接**
   - 每个原子 skill 输出 schema 应是**可被 LLM 或下游 skill 直接消费的结构化数据**(不是散落 markdown)
   - 例:"查 SCADA" 输出 `{telemetry: [...], timestamp: ..., quality: ...}`,而非"电压 110.2kV, 电流 250A 左右"

3. **机理硬约束本地化**:每个原子 skill 的 reject 规则不依赖外部 SOP
   - "校核 N-1" skill 内置 N-1 violation reject,不能"依赖调用方查"
   - 这保证即使 SOP 写错,原子 skill 仍然守住底线

4. **幂等性 + retryable**
   - 多次调用同样输入 → 同样输出 + 不产生副作用
   - 对应 MCP `idempotentHint: true` annotation
   - 这是 SOP 内部 retry 时的前提

#### B.2.3 调度领域原子 skill 候选清单(粗粒度,业务专家校对)

```
信息获取类:
  - query-scada-realtime         查 SCADA 实时量测
  - query-scada-historical       查 SCADA 历史
  - query-equipment-ledger       查设备台账
  - query-protection-settings    查保护定值
  - query-regulation             查规程条款
  - query-maintenance-window     查检修计划

机理计算类:
  - calculate-power-flow         潮流计算
  - calculate-n-1                N-1 校核
  - calculate-stability-margin   稳定裕度
  - calculate-short-circuit      短路电流
  - calculate-load-shedding      减负荷量

校核类:
  - verify-five-prevention       五防闭锁校核
  - verify-thermal-limit         热稳极限校核
  - verify-voltage-limit         电压限值校核
  - verify-regulation-compliance 规程合规校核

格式 / 模板类:
  - format-operating-ticket      格式化操作票
  - format-fault-report          格式化故障报告
  - format-dispatch-log          格式化调度日志

控制 / 命令类(高风险,interlock_required):
  - issue-telecontrol-cmd        发遥控指令
  - modify-protection-setting    改保护定值
  - update-equipment-status      更新设备状态
```

---

### B.3 SOP skill 定义 + 设计原则

#### B.3.1 定义

**SOP skill = 满足以下条件**:

1. **业务流程级编排**:解决一类业务场景(检修流程 / 故障处置 / 黑启动 / 操作票全流程)
2. **调用 ≥2 个原子 skill**(可能也调 MCP tool / scripts)
3. **可能维持跨 turn 状态**(例:操作票编制流程横跨多次确认)
4. **包含决策点 / 条件分支 / 循环**

#### B.3.2 4 条设计原则

1. **显式 workflow,不是"让 LLM 自己想"**
   SOP body 必须写出 numbered steps + conditional branches。仅靠 description 让 LLM 推断流程是 cashandcache 失败模式 #2 的根因。
   ```markdown
   ## SOP Workflow: 220kV Line Maintenance Isolation
   1. Get scope (call: query-equipment-ledger with {line_id})
   2. Check active maintenance window (call: query-maintenance-window)
      - if no window: REJECT with NO_MAINTENANCE_WINDOW
      - if window mismatch: ASK USER to confirm
   3. Calculate N-1 impact (call: calculate-n-1 with {line_id} pre-disconnection)
      - if violation: REJECT with N1_VIOLATION + recommendations
   4. Generate switching sequence:
      - call: format-operating-ticket
      - apply: references/220kV-template.md
   5. Verify five-prevention (call: verify-five-prevention)
   6. Output draft + escalate for human approval
   ```

2. **静态调度 vs LLM 动态决策的权衡**

   | 风险等级 | 决策方式 |
   |---------|---------|
   | 控制 / 决策类(发指令、合分闸) | **静态调度** — 步骤、分支、reject 条件全部 hardcode 在 SOP |
   | 辅助分析类(检修建议) | **半静态** — 主流程 hardcode,某些分支让 LLM 决策 |
   | 信息查询类 | **动态** — LLM 决定调用哪些原子 skill 满足查询 |

   **调度规范 MANDATORY**:控制类 SOP **必须** 静态调度。这与 §B.1 关键观察 1 一致(业界整体回流到 explicit workflow)。

3. **每个分支都得有 escape hatch**
   ```
   - if {condition}: do A
   - else if {condition}: do B
   - else: ASK USER + log AMBIGUOUS_BRANCH
   ```
   永远不要让 LLM 在 SOP 内部 silently choose 分支。

4. **SOP 必须复用原子 skill,不重复实现**
   反模式:SOP body 里把"查 SCADA"的步骤再写一遍,而不是 `call: query-scada-realtime`。这导致:
   - 同一逻辑两套维护
   - 原子 skill 升级后 SOP 不跟进
   - 测试覆盖不全

#### B.3.3 SOP skill 内部"调用原子 skill"的 3 种风格

| 风格 | 描述 | 适用场景 |
|------|------|---------|
| **Static call** | SKILL.md 显式写 `call: skill-x` | 高风险流程,顺序固定 |
| **Conditional call** | "if X: call A, else call B" | 中风险,有少数分支 |
| **LLM-routed call** | "based on user's specific question, choose appropriate query skill from: [list]" | 低风险,复用查询 |

控制类 SOP:**只允许 Static + Conditional**。LLM-routed 用在查询类。

---

### B.4 粒度选择决策树

业务专家拿到一个业务场景,判断该写原子还是 SOP 的实操树:

```
Q1: 这个场景能用一个 verb + 一个 noun 概括吗?
  Yes → 倾向原子。继续 Q2。
  No  → 倾向 SOP。跳 Q4。

Q2: 输出能被未来其他 skill 复用吗?
  Yes → 写原子。
  No  → 可能是个一次性查询,不该写 skill,放进 prompt 即可。

Q3: 是否需要机理硬约束 reject?
  Yes → 必写原子,reject 规则内嵌(让任何调用方都不能绕)。
  No  → 写原子(信息查询 / 格式化)。

Q4: 流程跨 ≥2 个原子能力吗?
  Yes → 继续 Q5。
  No  → 回 Q1,可能粒度判断错了。

Q5: 流程是否包含决策点 / 分支?
  Yes → 写 SOP,显式编排。
  No  → 是简单串接,可考虑做 macro skill(原子的薄包装),也可让上层 LLM 直接串。

Q6: 流程是否高风险(控制类、可能产生事故)?
  Yes → SOP 必须静态调度 + interlock + 双人审批 + 全溯源。
  No  → SOP 可以 conditional + LLM-routed mix。
```

#### 粒度反例(常见错误)

| 反例 | 问题 | 修复 |
|------|------|------|
| 把"查台账 + 查定值 + 查检修计划"合成一个 skill | 太粗,无法独立复用 | 拆成 3 原子 + 1 个 SOP 串它们 |
| 把"操作票生成"拆成 50 个 skill,每个一行字段 | 太细,触发频繁、上下文爆 | 合并为 1 个原子(format-ticket) |
| 把"故障处置预案"做成原子 | 跨多步骤、有分支,实质 SOP | 改写为 SOP,内部调用原子 |
| 控制类 skill 让 LLM 自己决定调用顺序 | 不可重现、不可审计 | 改为 SOP 静态调度 |
| 把"机理校核"放在 SOP 里而不是原子 | 任何 SOP 错都可能绕过校核 | 校核必须在原子内,SOP 调用 |

---

### B.5 调度场景实例(3 个完整 walkthrough)

#### B.5.1 输变电检修 SOP skill

**业务背景**:某省调要把 220kV-X 线由运行转检修(常规年检)。流程涉及:核对检修计划 → 出操作票草稿 → 五防闭锁校核 → N-1 校核 → 调度审核 → 现场执行 → 检修完成 → 复电流程。

**SOP 设计**:

```yaml
---
name: line-maintenance-sop
description: >
  Standard Operating Procedure for taking a 110kV-220kV line out of service
  for scheduled maintenance, including draft ticket generation, five-prevention
  check, N-1 verification, and approval routing. Use when user asks for
  线路检修流程/送检步骤/转检修, even when only the line ID is mentioned.
  Does NOT cover live-line work, fault handling, or special voltage levels.
---

## Critical Rules
- MUST verify maintenance window in pre-scheduled plan; no window → ABORT
- MUST run N-1 check before generating ticket; violation → ABORT with recommendations
- MUST get human approval before any control command issuance
- MUST cite GB/T XX.XX clause for each step (use query-regulation skill)

## Workflow
1. Validate maintenance window
   - call: query-maintenance-window (line_id, target_date)
   - if no window: ABORT (NO_WINDOW)
2. Pre-disconnection N-1 check
   - call: calculate-n-1 (current_topology, simulate_disconnect=line_id)
   - if violation: ABORT + report stress lines
3. Generate switching sequence
   - call: query-equipment-ledger (line_id)  → get terminals
   - call: format-operating-ticket (terminals, voltage_level=220)
4. Five-prevention validation
   - call: verify-five-prevention (ticket_draft)
   - if any rule fails: REJECT + return failed rules
5. Output draft for human review
6. (After human approval, NOT in this SOP) — issue commands via separate
   issue-telecontrol-cmd with double-person token
7. Post-operation
   - call: update-equipment-status (line_id, status=MAINTENANCE)
   - log: audit-trail with skill_id + version

## Atomic Skills Used
- query-maintenance-window
- calculate-n-1
- query-equipment-ledger
- format-operating-ticket
- verify-five-prevention
- update-equipment-status

## Time Class: hours_dayplan
## Interlock Required: true (for actual command issuance step)

## Examples
[Example 1: 正常流程; Example 2: N-1 violation reject; Example 3: 检修计划缺失 ABORT]

## Boundaries
In scope: 110kV-220kV scheduled line maintenance (常规年检 / 月检)
Out of scope:
  - 500kV+ → use ehv-line-maintenance-sop
  - 带电作业 → use live-work-sop
  - 故障抢修 → use fault-isolation-sop
```

**关键设计决策**:
- 调用 6 个原子 skill,本 SOP 不做任何机理计算 / 数据查询(全部委托原子)
- 第 6 步显式分离到独立 skill — **下发指令是单独的 control SOP,不混入本 SOP**(降低误下发风险)
- 静态调度,无 LLM-routed 步骤(因为是控制类相关)

#### B.5.2 故障处置预案 SOP skill (220kV 线路单跳 / 重合不成功)

**业务背景**:220kV-Y 线发生故障,主保护动作跳闸,重合闸不成功。调度员需要:核对故障信息 → 隔离故障点 → 评估系统稳定 → 转移负荷 → 上报。

**SOP 设计**:

```yaml
---
name: fault-handling-220kv-line-trip
description: >
  Fault response SOP for 220kV transmission line trip with failed auto-reclose.
  Use when user reports 线路跳闸/主保护动作/重合不成功/事故跳闸 for 110kV-220kV lines.
  Does NOT cover bus faults, transformer trips, or 500kV+ events.
---

## Critical Rules
- MUST acknowledge within 30s; time_class: seconds_AGC ⇒ minutes_powerflow
- MUST identify fault by reading SCADA event sequence within first turn
- MUST verify isolation BEFORE recommending reclose attempt
- ANY uncertainty → escalate to human, do NOT proceed autonomously

## Workflow
1. Acknowledge + read fault info
   - call: query-scada-realtime (line_id, last_30s)
   - call: query-scada-event-sequence (line_id, last_30s)
2. Classify fault
   - if relay_action == "MAIN_PROTECTION" AND reclose_status == "FAILED":
     → continue Step 3
   - else: ABORT (out of scope for this SOP) → suggest other SOP
3. Verify isolation status
   - call: query-equipment-status (line breakers at both ends)
   - if not isolated: ABORT + alert (CRITICAL)
4. Stability assessment
   - call: calculate-stability-margin (current_topology)
   - if margin < threshold: continue to Step 5 with HIGH_RISK flag
5. Load transfer recommendation
   - call: calculate-load-transfer-options (lost_capacity)
   - rank options by: stability_impact, switching_count, reliability_index
6. Output recommendation report
   - format: fault-report-template
   - include: incident timeline, isolation status, stability assessment,
     transfer options, regulation refs, **explicit "human dispatcher
     approval required before execution"**
7. Wait for human dispatcher decision
   - SOP terminates here; execution via separate control SOPs

## Atomic Skills Used
- query-scada-realtime
- query-scada-event-sequence
- query-equipment-status
- calculate-stability-margin
- calculate-load-transfer-options

## Decision Branches
- isolation failed → escalate CRITICAL
- stability margin < 5% → flag HIGH_RISK + double-check with senior
- multiple equally-good transfer options → present all + ask user

## Time Class: seconds_AGC + minutes_powerflow (mixed)
## Interlock Required: false (this SOP only recommends, does not execute)

## Boundaries
In scope: 220kV line single-end trip + reclose failure
Out of scope:
  - bus fault → use bus-fault-sop
  - transformer trip → use transformer-fault-sop
  - cascading failure (≥2 elements) → use grid-emergency-sop
```

**关键设计决策**:
- **本 SOP 只到 recommendation 为止,不执行任何控制** — 严格遵守"AI 辅助决策不替代调度员"。下发动作交给独立 control SOP,带 interlock + 双人 token
- 第 5 步**保留 LLM-routed 决策**(选择最佳 transfer option),因为这是辅助分析类决策 — 但同时要求"present all + ask user"逃生口
- 整体 time_class 是 mixed(初期秒级,后期分钟级),反映调度员实际感受

#### B.5.3 操作票编制 SOP skill

**业务背景**:操作票是电力调度的法定文档,涉及:查台账 → 查定值 → 查闭锁 → 排序步骤 → 模板填充 → 复核。这是 §A.4.1 规程转写的最典型应用。

**SOP 设计**:

```yaml
---
name: operating-ticket-composer
description: >
  Composes draft 倒闸操作票 (switching operation ticket) for 110kV-220kV
  substations, including equipment lookup, settings retrieval, interlock
  verification, step ordering, and template filling. Use when user requests
  操作票/倒闸顺序/检修隔离/送电步骤, OR describes a switching goal like
  "把 #1 主变停了". Does NOT execute commands; produces draft for human review.
---

## Critical Rules
- MUST use 双重编号 (站名 + 设备编号) consistently throughout ticket
- MUST cite regulation clauses for each operation step (no fabrication)
- MUST verify five-prevention rules; any violation → REJECT
- MUST output reviewer_required=true (human review mandatory)

## Workflow
1. Parse user goal → extract scope
   - identify: source_substation, equipment_list, operation_type
   - if any field missing: ASK clarifying questions (max 3)
2. Lookup equipment ledger
   - call: query-equipment-ledger (substation, equipment_list)
   - retrieve: 双重编号, voltage_level, interlock_group, operation_class
3. Lookup protection settings (if operation involves trip/restore)
   - call: query-protection-settings (equipment_list)
4. Order operations per template
   - apply: references/220kV-switching-template.md
   - rules: 先停信号 → 拉开关 → 拉刀闸 → 验电 → 装接地线
   - reverse for restoration
5. Verify five-prevention
   - call: verify-five-prevention (ordered_steps)
   - any failure: REJECT + return failed rule + suggested fix
6. Lookup regulations
   - for each step: call: query-regulation
   - attach: regulation_id + clause + version
7. Assemble draft
   - format: ticket-template
   - fields: header, steps[], regulation_refs[], reviewer_id (empty),
     time_class, escalation_path
8. Validation pass
   - run: scripts/validate_ticket_format.py
9. Output draft + flag for human review

## Atomic Skills Used
- query-equipment-ledger
- query-protection-settings
- query-regulation
- verify-five-prevention

## Time Class: hours_dayplan (typical) | seconds_AGC (emergency variant via different SOP)
## Interlock Required: false (this SOP only drafts; execution is separate)

## Examples
[Example 1: routine maintenance; Example 2: ambiguous goal → ask back; Example 3: five-prevention violation → reject with fix suggestion]

## Boundaries
In scope:
  ✓ 110kV-220kV 倒闸操作票草稿(常规检修 / 送电 / 隔离)
Out of scope:
  ✗ 500kV+ 操作票
  ✗ 带电作业方案
  ✗ 实际下发(本 SOP 仅产生草稿)
  ✗ 故障处置(用 fault-handling SOPs)
```

**关键设计决策**:
- 4 个原子 skill,每个负责一个独立查询 / 校核职责
- 步骤 8 用 deterministic script 做格式验证(对应 Anthropic best practice "solve, don't punt")
- 步骤 9 的"reviewer_required=true"是**默认 fail-safe** — 即使 SOP 满分通过,也必须人复核

---

## §C. 技能库治理

### C.1 Skill Registry 模式

业界 4 种 skill registry 实现对比(基于 2026-04 在线公开材料):

| Registry | 定位 | 治理重点 | 调度可参考度 |
|----------|------|---------|------------|
| **Anthropic Skills marketplace** + Skills API | 个人 / 组织 skill 分发 | basic upload + sharing | 🟡 适合参考"分发模型",治理弱 |
| **Tessl Registry** + Snyk 安全扫描 | npm 风格 package manager | quality score + impact rating + version history + 安全扫描 | ✅ **最贴近企业级**,推荐参考 |
| **JFrog Agent Skills Registry** (NVIDIA AI-Q reference) | 企业内部 SSoT | 加签、Provenance evidence、跨工具统一(不被锁在单一 IDE) | ✅ 推荐参考"跨厂商互通"思路 |
| **Alibaba Nacos 3.2 Skill Registry** | 私有化部署、企业级 | 全生命周期 (Draft→Review→Gray→Formal→Offline) + RBAC + 命名空间隔离 + 全审计 | ✅ **最贴近调度场景**,生命周期清晰 |

**业界共识 4 条**(综合 4 家):

1. **GitHub-only 不够用**:能开发但无法治理(JFrog 原文:"works for quick experimentation, but it becomes nearly impossible to govern at scale")
2. **proprietary marketplace 是 walled garden**:跨工具 / 跨 vendor 不通,组织绑死(JFrog)
3. **必须把 skill 当作 first-class software asset 管**(不是 markdown 文件)
4. **publishing 必须 scan + sign + provenance evidence**(JFrog `jf skills publish --signing-key`)

#### 调度场景的 registry 模式建议

**建议架构**:
```
                    ┌──────────────────────┐
                    │  国调中心总目录      │
                    │  (master registry)   │
                    └──────────┬───────────┘
                               │
            ┌──────────────────┼──────────────────┐
            │                  │                  │
       ┌────┴────┐         ┌───┴────┐         ┌───┴────┐
       │ 网调    │         │ 省调   │         │ 厂家    │
       │ namespace│         │namespace│        │namespace│
       └────┬────┘         └───┬────┘         └────┬───┘
            │                  │                    │
       [私有 skill]        [私有 skill]         [厂家 skill]
                                                   ↓
                                              [安全扫描通过后]
                                                   ↓
                                              [可被调度引用]
```

**采纳特征**:
- Nacos 的 namespace + RBAC + 生命周期(强制做)
- Tessl 的 quality score + impact rating(投产门槛)
- JFrog 的 sign + provenance(防供应链攻击)
- Anthropic 的 description-driven discovery(查询机制)

---

### C.2 Skill Lifecycle(从草稿到退役)

综合 Nacos 3.2 (Draft/Review/Gray/Formal/Offline 5 阶段) + Anthropic enterprise(vet/evaluate/deploy/govern) + JFrog(publish/scan/sign):

```
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│ Draft    │ →   │ Review   │ →   │ Pilot    │ →   │ Production│ →   │Deprecated│ →   │ Retired  │
│ (草稿)   │     │ (评审)   │     │ (灰度)   │     │ (投产)   │     │ (停用)   │     │ (归档)   │
└──────────┘     └──────────┘     └──────────┘     └──────────┘     └──────────┘     └──────────┘
   作者写         同行评审         单调度组         全调度可见       不再推荐         不可调用
   单元测试       安全扫描         真实任务         双跑监控         迁移指引        历史检索保留
```

**每个阶段的 governance hooks**:

| 阶段 | 必经检查(自动) | 必经检查(人工) | 退出条件 |
|------|---------------|---------------|---------|
| Draft → Review | description 非空、SKILL.md 存在、frontmatter 合法 | 作者自检 | 提交评审申请 |
| Review → Pilot | quality score(Tessl 风格 ≥80)、Snyk 风格安全扫描 0 critical、单元测试 PASS | 业务专家 + 安全官签字 | 进入 pilot |
| Pilot → Production | 真实任务覆盖率 ≥X、误触发率 ≤Y%、无 critical incident | 调度组长签字 + 标准化办公室 备案 | 进入 production |
| Production → Deprecated | 监控告警(version 漂移 / 调用率骤降 / 模型升级影响) | 维护方决定 | 公告 N 个月 |
| Deprecated → Retired | 调用率 < 阈值持续 N 月 | 标准化办公室确认 | 进入归档 |

**关键 governance hooks(自动化)**:
- pre-publish: 静态扫描 + frontmatter lint + skill_id 唯一性
- pre-pilot: 沙箱跑 10-50 测试用例 + 触发率统计
- production: 实时监控 invocation rate / error rate / latency P95
- regression: 模型升级时 re-run pilot suite + flag drift

---

### C.3 版本治理

业界共识(LangChain 1.x release policy + Tessl pin-to-commit + Nacos immutable versions):

#### C.3.1 SemVer 是基线,但不够

skill 的版本含义比代码包多:
- 行为变更(workflow 步骤改了)
- description 变更(触发不同了)
- references 变更(知识更新了)
- 依赖原子 skill 版本变更
- 适配模型变更(Claude 4.7 → 5.0 行为漂移)

**MAJOR.MINOR.PATCH** 之外,推荐补 **compatibility tags**:
```yaml
metadata:
  version: "2.1.0"
  compatibility:
    models: ["claude-4.7", "claude-5.0", "qwen-72b"]
    runtime: ">=1.0"
  depends_on:
    - {skill: "calculate-n-1", version: ">=1.5,<2.0"}
    - {skill: "verify-five-prevention", version: ">=2.0"}
```

#### C.3.2 版本不可变(immutable versions)

Nacos 强约束:**published 后不可修改,只能发新版本**。这对调度场景必须采纳 — 生产投运的 skill 被偷偷改一行,所有引用它的 SOP 行为变了,无法追溯。

#### C.3.3 跨版本兼容承诺

参照 LangChain 1.x:
- 主版本(N → N+1 MAJOR)允许 breaking change
- 同主版本内(1.x.y),deprecated 行为保留至少 6 个月
- 调度场景建议加严:**主版本 breaking change 必须配迁移 SOP + 调度员培训通告**

#### C.3.4 厂家分支

企业实践不可避免厂家定制(华为版本 / 南瑞版本 / 国电南自版本)。3 种处理方式:

| 方式 | 描述 | 优劣 |
|------|------|------|
| **Pure namespace fork** | 厂家完整 fork 一份(`huawei.line-maintenance-sop`) | 简单但分裂、无法享受 upstream 改进 |
| **Inherit + override** | base skill + 厂家 patch(MCP server URL / 设备命名规则) | 平衡,推荐 |
| **Adapter layer** | 调度规范定 base SOP,厂家提供 adapter skill 适配本厂数据源 | 最干净但前期工作量大 |

**推荐**:Inherit + override(为现有调度规范的兼容性最佳)

---

### C.4 检索 / 发现机制

调度 skill 库当 N>50 时,纯 description 触发会失效(Subagent 2 §B.1 关键观察 3)。需多层发现机制:

#### C.4.1 4 层发现策略

```
Layer 1: 启动加载 metadata (name + description)
         → 触发 70-80% 标准请求
Layer 2: 关键词索引(标签 / 业务域 / 调度层级)
         → 调度员手动列举 "检修类 skill"
Layer 3: 语义向量检索(description embedding)
         → "我要做 220kV 线路停电..." → top-K 候选 skill
Layer 4: 显式 dependency graph
         → SOP A 依赖 atom B,激活 A 自动加载 B 的 metadata
```

#### C.4.2 多维分类(标签体系)

参考 Anthropic 32 页 PDF 的 taxonomy + Nacos namespace 实践 + 调度业务场景:

```yaml
metadata:
  taxonomy:
    domain: realtime_monitoring  # 实时监控/安稳校核/AGC/AVC/检修/新能源/计划
    skill_type: atomic | sop | hybrid
    risk_class: A | B | C   # 控制 / 辅助 / 查询
    voltage_level: ["110kV", "220kV"]
    dispatch_layer: provincial   # 国调/网调/省调/地调/县调
    time_class: minutes_powerflow
  tags:
    - 倒闸操作
    - 检修隔离
    - 五防闭锁
```

**好处**:
- 标准化办公室可按 risk_class=A 全量 audit 高风险 skill
- 调度员可按 voltage_level + dispatch_layer 找本省调用得上的
- 模型升级影响评估可按 skill_type=sop 优先 re-test

---

### C.5 多维分类体系建议(调度特化)

#### 按 5 维交叉分类

| 维度 | 取值 | 用途 |
|------|------|------|
| **业务域** | 实时监控 / 安稳校核 / AGC / AVC / 检修 / 新能源 / 计划 / 故障处置 / 黑启动 | 业务侧导航 |
| **调度层级** | 国调 / 网调 / 省调 / 地调 / 县调 | 权限 + 适用范围 |
| **技能类型** | atomic / sop / hybrid | 治理粒度 |
| **风险等级** | A 控制类 / B 辅助类 / C 查询类 | MANDATORY 强度差异 |
| **时效** | seconds_AGC / minutes_powerflow / hours_dayplan / days_outage | 编排优先级 |

#### 多调度层级的库分级与共享建议

```
┌───────────────────────────────────────────┐
│  master:registry/                         │
│    common/    (跨层级共享 — 计算 / 校核 / 查规程) │
│    atomic/    (大部分原子 skill 在此)       │
│    sop-template/ (SOP 模板 + base)         │
└───────────────────────────────────────────┘
              ↓ 继承 / 覆盖
┌───────────────────────────────────────────┐
│  national:registry/  (国调 namespace)     │
│    sop/  (国调专属 SOP — 跨网调编排)        │
│    custom/  (国调特化原子)                 │
└───────────────────────────────────────────┘
              ↓
┌───────────────────────────────────────────┐
│  regional:registry/  (网调)               │
│    sop/                                    │
└───────────────────────────────────────────┘
              ↓
┌───────────────────────────────────────────┐
│  provincial:registry/  (省调)             │
│    sop/                                    │
│    vendor-skills/  (华为/南瑞/南自 skill)  │
└───────────────────────────────────────────┘
```

**共享 / 私有分级原则**:
- **完全共享**:计算类、校核类、规程查询类(机理无层级差异)
- **半共享**:操作票模板、五防规则(主框架共享,层级覆盖)
- **完全私有**:本层调度特定流程(如某省 N-X 准则)

---

### C.6 跨厂商互通(华为 / 南瑞 / 国电南自 skill 共存)

#### 业界经验:JFrog 跨工具统一思路 + Tessl pin-to-commit + Anthropic 12 月开源 Agent Skills 跨平台规范

#### 核心矛盾

不同厂家:
- D5000 / OPEN 5000 / NetMate 数据接口不同
- 设备命名规则不同(双重编号格式 / 五防系统接口 / SCADA 字段名)
- MCP server endpoint 不同
- 但**业务逻辑应当一致**(220kV 检修步骤不应该因厂家而异)

#### 解决方案:Adapter 模式

```
┌──────────────────────────────────────┐
│  调度规范定义的 SOP skill            │
│  line-maintenance-sop (vendor-agnostic)│
│  调用: query-equipment-ledger          │
│         calculate-n-1                  │
│         verify-five-prevention         │
└──────────────────┬───────────────────┘
                   │ 调用 atomic skill (interface)
                   ↓
┌────────────────────────────────────────┐
│  抽象原子 skill 接口                   │
│  query-equipment-ledger (signature)    │
│  Input: substation_id, equipment_list  │
│  Output: {equipment[], 双重编号, ...}  │
└──────────────────┬─────────────────────┘
                   │ 实现选择(运行时绑定)
        ┌──────────┼──────────┬──────────┐
        ↓          ↓          ↓          ↓
   ┌─────────┐ ┌──────────┐ ┌─────────┐ ┌─────────┐
   │ 华为     │ │ 南瑞     │ │ 南自    │ │ 自研    │
   │ adapter │ │ adapter │ │ adapter │ │ adapter │
   └─────────┘ └──────────┘ └─────────┘ └─────────┘
```

#### 实施要点

1. **接口标准化由调度规范定义**(vendor-agnostic)
   - 每个原子 skill 定义统一 input/output schema
   - 厂家必须实现这个接口

2. **实现交给厂家**
   - 厂家在自己的 namespace 下发布 adapter skill
   - 例:`huawei.query-equipment-ledger:1.0`、`nari.query-equipment-ledger:2.1`
   - 内部调度引擎通过 namespace 绑定选择实现

3. **运行时绑定(runtime binding)**
   - 配置层声明该调度中心用哪个厂家
   - 同一个 SOP 在不同省调可绑不同 adapter

4. **certification 互认**
   - 调度规范配套 contract test suite
   - 厂家 adapter 跑通即获 certified
   - 类似 EAASP L1 runtime 7-runtime certification 模式

5. **降级互通**
   - 任何 vendor adapter 离线时,调度规范定义统一 fallback(如读静态台账缓存)
   - 不允许"adapter 没来时 SOP 直接报错"

---

### C.7 Quality Gate(投产前必经审核)

汇总 Anthropic enterprise + Tessl skill review (oauth: 22%→100%, fastify: 48%→100%) + Snyk ToxicSkills 36% prompt injection 数据:

#### Quality Gate Checklist(投产前必经,每条都要给具体验收方法)

| # | 检查项 | 自动 / 人工 | 通过标准 | 工具 |
|---|------|-----------|--------|------|
| **1** | frontmatter 合法 | 自动 | name + description 满足规范、长度合格 | yamllint + custom validator |
| **2** | description 触发率 | 自动 | ≥20 should-trigger 测例 + ≥10 should-not-trigger,准确率 ≥85% | agentskills.io trigger eval 框架 |
| **3** | description 评分 | 自动 | Tessl 风格质量评分 ≥80 | Tessl-style scorer |
| **4** | 安全扫描 | 自动 | 0 critical(prompt injection / 外联 / 凭证泄露) | Snyk agent-scan / 自研 |
| **5** | SKILL.md body ≤500 行 / ≤5000 token | 自动 | 单文件 token 计数 | tiktoken / claude-tokenizer |
| **6** | 无 deeply nested references | 自动 | references 全部 1 级深 | 静态分析 |
| **7** | 单元测试覆盖 | 自动 | 关键 workflow 步骤覆盖率 ≥80% | pytest / custom test runner |
| **8** | 真实任务 evaluation | 自动 + 人工 | ≥10 representative tasks + 评分 | Anthropic eval framework |
| **9** | Critical Rules / Boundaries 段存在 | 自动 | grep 关键 section header | static check |
| **10** | regulation_refs 全溯源 | 自动 | 每条规程引用 → 真实数据库 lookup | regulation DB validator |
| **11** | mechanism_constraints 反例测试 | 自动 | 至少 1 条"AI 违反约束"测试 reject | custom test |
| **12** | 风险类 skill 双签 | 人工 | risk_class=A skill 必须 业务专家 + 安全官 双签 | manual review |
| **13** | 模型兼容性 | 自动 | 在声明的 models 列表上各跑一遍 | multi-model eval |
| **14** | dependency graph 无循环 | 自动 | 依赖闭环检测 | graph analyzer |
| **15** | invocation budget 估算 | 自动 | 平均 invocation token cost < 阈值 | profile |

#### 投产 Gate 决策表

```
若所有 1-15 项通过:                            → Pilot
若 1-9, 13-15 通过, 10-11 暂时 deferred:        → Pilot 但带 known-defect tag
若 1-9 通过但 10-12 任一不过:                   → REJECT, 退回 Review
若 4 (安全) 任一项 critical 失败:                → BLOCK, 不允许 Pilot
若 risk_class=A 但 12 (双签) 缺失:              → BLOCK
```

---

## §D. Open Issues / 待领域专家验证

### D.1 Skill 测试方法(单元 / 集成 / 反例 / canary)

研究范围内只看到通用 best practices,缺**调度场景的高保真 mock framework**:
- SCADA 量测如何 mock?(实时性 vs 历史性)
- 五防闭锁系统如何用 fixture 替代?
- N-1 计算的 reference dataset 是否要标准化?
- 控制类 skill 的 reject 测试如何避免污染真实电网?

**待验证问题**:调度训练仿真系统(DTS)能否被 skill registry 调用作为 evaluation backend?

### D.2 Skill 安全审计(red team)

研究中找到 Snyk ToxicSkills 报告(36% prompt injection),但缺**调度场景的 red team playbook**:
- 厂家 adapter 是否会成为供应链 backdoor 入口?
- LLM 是否可被诱导越权(从查询 skill 越到控制 skill)?
- 跨调度层级是否能被诱导越权(地调 skill 触发省调 SOP)?

**建议**:类似金融业 SAST/DAST,调度业应有"DAST for skill" — 模拟攻击者诱导 skill 越权。

### D.3 Skill 知识图谱(skill 之间的 dependency graph)

研究找到 Graph of Skills 论文(arXiv 2604.05333)和 LobeHub skill-router,但**调度场景的具体形态未定**:
- 调度 skill 库一般会有多少 skill?100? 1000?
- skill 间依赖深度多深?是否要限制?
- 是否需要"中央 graph 服务" 还是分布式声明?

### D.4 Skill 国际化(多语言 SKILL.md)

国调中心场景内为中文,但跨厂家 / 跨国互通(如 IEC TC57 接轨):
- description 多语言版本如何同步?
- regulation_refs 中英对照?
- 国际客户购买 / 出口场景下如何分发?

### D.5 Skill 性能 profile(token / 延迟 / 触发频率)

调度运行环境对延迟敏感(seconds_AGC),但研究范围内未见**具体 budget**:
- 一个 SOP 调用多少原子 skill 算合理?
- token cost 上限怎么定?
- skill 触发延迟监控指标怎么定?

### D.6 Skill 与人机协作模式

调度规程要求"人在回路"(双人审批、复核签字),但**skill body 内如何写人机协作?**:
- "MUST get human approval"如何机器可识别 / 可监控?
- "ASK user X" 是否计入 skill latency?
- 调度员 override AI 建议的记录如何反馈到 skill iteration?

### D.7 Skill Lifecycle 与电力行业现有标准化办公流程的衔接

电力行业有成熟的标准化办公室流程(DL/T 标准、行业规程、企业标准),**skill 生命周期如何挂接?**:
- skill 从 Pilot → Production 是否要走标准化办公室备案?
- 厂家 skill 互认是否要走 IEC TC57 流程?
- 历史归档与运维档案系统对接?

---

## §E. Top 5 推荐:对外方案 v0.1 应吸纳的关键观点

### E.1 显式区分原子 skill vs SOP skill,把 taxonomy 写进规范第一章

**理由**:agentskills.io 不区分(留给设计者),Semantic Kernel 走 Plugin/Workflow 分层,CrewAI 用 Task/Crew,LangGraph 用 Tool/Graph。**整个行业都在分,只有 Anthropic 标准没标**。
**调度场景特别需要**:控制类 skill 必须 SOP 静态调度 + 原子内嵌机理校核,这是治理 mandatory。
**对外方案动作**:在 §3 增加新条 §3.6 "原子 / SOP 二分",规定:
- skill_type 字段在 frontmatter 必填
- atomic skill MUST 单一 verb+noun + 不调用其他 skill + 内嵌 reject
- sop skill MUST 显式 numbered workflow + 控制类 MUST 静态调度

### E.2 把 description 评分 + Quality Gate 落到投产前自动检查

**理由**:Tessl × Snyk 的 oauth 22%→100%、fastify 48%→100% 数据证明 **description 是 skill 触发命门;不评 description 等于不查 skill 能不能用**。Snyk ToxicSkills 36% 含 prompt injection 证明**安全扫描是非选项**。
**对外方案动作**:§4 强制等级章节加新条:
- Quality Gate 15 项检查(本文 §C.7)是 MANDATORY
- description 触发准确率 ≥85% 是 MANDATORY
- risk_class=A 必双签是 MANDATORY

### E.3 Lifecycle (Draft→Review→Pilot→Production→Deprecated→Retired) 写进规范并定义每阶段 governance hook

**理由**:Nacos 3.2 的 5 阶段已是工业级最佳实践;调度行业天然有标准化办公室 + 班长签字流程,可直接挂接。
**对外方案动作**:增加 §6 "演进机制"扩为 "§6 Skill 生命周期",定义:
- 每阶段进入 / 退出条件
- 每阶段必经 governance hook(自动 + 人工)
- 投产后监控 SLI(invocation rate / drift / latency)

### E.4 跨厂商互通用"接口由规范定 + 实现由厂家做 + adapter 互认"模式,不要让厂家分裂规范

**理由**:JFrog 的 walled garden 教训 + Tessl pin-to-commit 思想。直接 fork 厂家版本会导致 5 年后规范分裂成 N 个不可调和的派系。
**对外方案动作**:§5 反向边界章节加:
- 调度规范 ONLY 定义原子 skill 接口(input/output schema) + SOP 业务逻辑
- vendor adapter 实现 NOT 在规范内
- 但配套 contract test suite 在规范内,厂家通过即认证
- 模仿 EAASP L1 runtime 7-runtime certification 模式

### E.5 把 Gotchas section 列为 SKILL.md MANDATORY 段,作为业务专家"沉淀经验"的载体

**理由**:Anthropic 实测发现这是"highest-value content";cashandcache 40 skills 调研显示 example 不足 / Gotcha 缺失是失效模式 #2 的主因。**调度行业每条 Gotcha = 一次"差点出事"的反措沉淀**,这是规程做不到的(规程是 declarative;Gotcha 是反直觉具体事实)。
**对外方案动作**:§3 各条 MANDATORY 内加细则:
- skill body MUST 有 `## Gotchas` 段
- Gotchas 须从历史误操作 / 反措通报 / 缺陷清单 / 仿真复盘提取
- Gotchas 数量 ≥3(空段不通过 Quality Gate)
- 每次实际事故 / 误操作 → 必须回溯到对应 skill 加 Gotcha(governance hook)

---

*References*:

- Anthropic Skills 官方文档:
  - `https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices`
  - `https://platform.claude.com/docs/en/agents-and-tools/agent-skills/enterprise`
  - `https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills` (2025-10-16, updated 2025-12-18)
  - The Complete Guide to Building Skills for Claude (32-page PDF, `resources.anthropic.com`)
- agentskills.io 开放规范(Anthropic 2025-12 开源,跨平台 Claude/Codex/Cursor/Gemini CLI/OpenCode):
  - `https://agentskills.io/specification`
  - `https://agentskills.io/skill-creation/best-practices`
  - `https://agentskills.io/skill-creation/optimizing-descriptions`
- 企业级 Skill Registry / Governance 实践:
  - JFrog Agent Skills Registry: `https://jfrog.com/blog/agent-skills-new-ai-packages` (NVIDIA AI-Q reference architecture)
  - Tessl Registry × Snyk: `https://snyk.io/blog/snyk-tessl-partnership` (oauth 22%→100%, ToxicSkills 36% prompt injection)
  - Alibaba Nacos 3.2 Skill Registry: `https://www.alibabacloud.com/blog/nacos-3-2-skill-registry-...` (5-stage lifecycle, RBAC, namespace)
- 失效模式数据:
  - cashandcache 40 skills 失败分析: `https://cashandcache.substack.com/p/i-analyzed-40-claude-skills-failures` (top 3 failure modes)
  - MindStudio context-rot: `https://www.mindstudio.ai/blog/context-rot-claude-code-skills-bloated-files`
  - Snyk ToxicSkills research (3,984 ClawHub skills sample)
- Agent 框架对比:
  - Microsoft Semantic Kernel: `https://learn.microsoft.com/en-us/semantic-kernel/concepts/{plugins,planning}`
  - CrewAI: `https://docs.crewai.com/en/concepts/{tasks,agents,crews,flows}`
  - LangGraph: `https://docs.langchain.com/oss/python/concepts/products`
  - Microsoft Agent Framework Workflows / AutoGen: `https://learn.microsoft.com/en-us/agent-framework/workflows/`
- Skill 治理研究:
  - Graph of Skills: `arXiv:2604.05333`
  - Skill Instruction File Dependency Resolution: tdcommons.org Defensive Publication
  - VoltAgent/awesome-agent-skills (1000+ real-world skills, 18.8k stars): `github.com/VoltAgent/awesome-agent-skills`
- 电力调度业务侧:
  - GB/T 33590《智能电网调度控制系统技术规范》系列(术语 / 体系架构等)
  - 基于专家系统的电力调度操作票审查: `ceeia.com/ewebeditor/uploadfile/...`
  - 基于语义增强的电网故障处置预案匹配方法 (能源电力期刊网)
  - 调度故障处置预案模板[110千伏变电站单线运行] (zhinengdianli.com)
  - 设备检修标准化作业流程: 水利水电期刊
- 备注:本文电力调度业务规程引用(GB/T、DL/T 编号)为示例性参考,正式发布前需电力行业专家校对实际有效编号
