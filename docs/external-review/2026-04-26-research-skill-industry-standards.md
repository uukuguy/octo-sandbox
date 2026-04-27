# Research: Agent / Skill 工业标准 survey

**研究日期**: 2026-04-26
**目的**: 为电力调度 AI 智能体 skill 规范的 L1 通用层提供引用基础
**作用域**: 仅业界公开可引用的标准 / 协议 / 规范，逐条评估 "是什么 / 解决什么 / 调度 skill 该如何继承或改造"
**读者假设**: 已熟悉 MCP、Skills、Harness、Tool calling 概念
**口吻**: 跨标准对比的技术评估报告（非营销文案）

---

## §0. 研究方法说明

- 所有标准的版本与状态以 2026-04-26 当日可查的官方页面为准；本文每条都附上 spec URL。
- 凡是仅凭训练数据未独立 verify 的细节，明确标记 "未独立 verify"。
- 标准之间存在重叠（如 MCP 内部用 JSON-RPC 2.0 + JSON Schema），本文按 "调度 skill L1 应不应该单独引" 的视角分别评估，而非按 "是否技术上被嵌入" 评估。
- 推荐继承级别用 RFC 2119 关键字：MUST / SHOULD / MAY / SHOULD NOT。

---

## §1. Skill / Tool 调用协议层

### 1.1 MCP (Model Context Protocol)

- **定义**: 由 Anthropic 2024-11 主导发起，2025 年成为多厂商支持的开放协议；底层走 JSON-RPC 2.0，定义 LLM Host ↔ MCP Server 之间如何暴露 **Tools / Resources / Prompts** 三类原语。
- **当前状态**: 多版本演进中，最新为 `2025-11-25` revision（一周年大改版）。前序版本 `2025-06-18` / `2025-03-26` 仍被广泛部署。Wikipedia 与多家厂商（Anthropic、OpenAI、Google、微软）已公开支持。可视为 **事实工业标准**，但尚未进入 IETF/ISO/IEC 形式标准化通道。
- **核心能力**:
  - **Tools**: 通过 `tools/list` / `tools/call` 暴露可被 LLM 决策调用的能力。每条 tool 定义 `name` / `description` / `inputSchema`（JSON Schema）/ `annotations`（如 readOnlyHint / destructiveHint / idempotentHint）。
  - **Resources**: 通过 `resources/list` / `resources/read` 暴露只读上下文（文件、DB 记录、API 响应）。资源使用 URI 标识，支持订阅 / 推送更新。
  - **Prompts**: 通过 `prompts/list` / `prompts/get` 暴露参数化的 prompt 模板，由用户/host 主动选用而非 LLM 自动调用——语义上对应"工作流入口"。
  - **Authorization**（2025-11-25）: 推荐 OAuth 2.1 + Dynamic Client Registration (RFC 7591) 作为远程 server 的认证基线。
  - **Streamable HTTP 传输**（2025-06-18+）: 替代早期 SSE 单向流，统一同步与异步能力。
  - **Async Tasks**（2025-11-25 新增）: 跨 tools/resources/prompts 的统一异步任务模型，支持长任务。
- **schema 表达力**: tool 的入参用 JSON Schema 2020-12（无版本绑定，由 server 实现负责）；返回值是结构化的 `content` 数组（text / image / resource_link / structured 内嵌任意 JSON Schema 定义）。错误模型继承 JSON-RPC 2.0 错误码 + MCP 自定义码段。
- **电力调度 skill 能否直接采用**: **yes-with-caveats**
  - 直接可用：tool 列表、参数 schema、错误模型、流式输出（Streamable HTTP）。
  - 须补充：调度场景的 *实时性 SLA*、*tool 风险等级标注*（destructiveHint 不够细：调度需要"只观测 / 可逆调度 / 不可逆遥控"三级以上）、*双人审批 / 操作票编号* 这类工业操作约束（在 annotations 内自定义命名空间）。
- **如果采用 L1 须显式引用的子集**:
  1. JSON-RPC 2.0 信封 + 错误码段
  2. `tools/list` + `tools/call` 接口语义
  3. `inputSchema` 必须是 valid JSON Schema 2020-12
  4. tool 返回 `content` 数组结构
  5. authorization 走 OAuth 2.1 + DCR（远程 server 场景）
  6. `tools/list_changed` 通知机制（保证 tool 集合可热更新）
- **调度场景需补的部分**:
  - 风险分级（命名空间 `x-grid-*` 扩展 annotations，避免与 MCP 核心字段冲突）
  - 工单 / 操作票绑定字段（票号、签发人、监护人）
  - 强一致幂等 key（防止重复下发遥控）
  - 实时性 SLA（响应上界毫秒数）
- **推荐继承级别**: **MUST** —— L1 唯一必须强制继承的 tool 调用协议；不再造轮子。

参考: <https://modelcontextprotocol.io/specification/2025-11-25> · <https://modelcontextprotocol.io/specification/2025-11-25/server/tools> · <https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization>

---

### 1.2 OpenAI Function Calling / Tools API + Structured Outputs

- **定义**: OpenAI 私有 API 形态，但因为生态体量大、被 OpenAI-compatible 服务（OpenRouter、vLLM、Anthropic 兼容层、Groq 等）广泛复制，事实成为 LLM tool 调用的另一条主流路径。包含 `tools` 数组（type=`function`, function.parameters 为 JSON Schema）+ `tool_choice`（`auto`/`none`/`required`/具名）+ `Structured Outputs`（response_format = json_schema 强约束）。
- **当前状态**: 厂商私有协议，无形式 spec；OpenAI API Reference 是事实文档。各兼容厂商在某些字段（如 `strict: true`、parallel tool calls）上行为不一致。
- **核心能力**:
  - 工具定义嵌入到 chat completions 请求里，与对话历史 in-band。
  - Structured Outputs (2024-08+) 支持 `strict: true` 保证 100% schema 合规（受限子集：no `oneOf` 顶层、no `pattern`、`additionalProperties: false` 必须显式）。
  - parallel tool calls 默认开启，可关闭。
- **schema 表达力**: JSON Schema，但 `strict` 模式下是 JSON Schema 的真子集——这是与 MCP 最关键的差别，调度场景如果用 strict，复杂 schema 必须先简化。
- **电力调度 skill 能否直接采用**: **yes-with-caveats**
  - 调度 skill 的入参 schema 描述方式可以与 MCP **保持一致**（都是 JSON Schema），所以"在 LLM 端把 MCP tool 投影成 OpenAI function 数组"是机械变换。
  - 但 **协议不应当作为 L1 的 wire 协议** —— 它是 LLM 调用方向的内部细节，不是 host ↔ server 的网络协议。
- **不足**: 私有协议、无版本治理、strict mode 子集限制、parallel call 跨厂商语义不一致。
- **推荐继承级别**: **MAY** —— 仅作 LLM 客户端内部投影格式参考；L1 wire 层 SHOULD NOT 直接暴露 OpenAI function 数组。

参考: <https://developers.openai.com/api/docs/guides/function-calling> · <https://developers.openai.com/api/docs/guides/structured-outputs>

---

### 1.3 Anthropic Skills（SKILL.md 格式）+ agentskills.io 开放标准

- **定义**: Anthropic 2025-10-16 在 Claude 生态发布 "Agent Skills"，2025-12-18 将其作为 **开放标准** 发布到 `agentskills.io`（Apache 2.0）。一个 skill 是一个目录，根含 `SKILL.md`（YAML frontmatter + Markdown body）。frontmatter 有强制字段 `name` / `description`（1-1024 字符）和可选字段 `allowed-tools` / `metadata` 等。运行时启动时 **只加载所有 skill 的 frontmatter**（progressive disclosure），仅在 description 命中触发条件时才载入 body。
- **当前状态**: 已 cross-platform（Microsoft Agent Framework、GitHub Copilot、LangChain 都在跟进）。开放标准发布时间较短，schema 仍在演进。
- **核心能力**:
  - **Skill 包结构**: `SKILL.md` + 同目录 scripts、resources、子文档。
  - **YAML frontmatter**: 至少 `name` + `description`；`description` 同时承担 "做什么 + 何时使用" 双重作用，是 host 路由 skill 的依据。
  - **Progressive disclosure**: 减少上下文占用——startup 仅扫描 frontmatter，命中后 lazy load。
  - **Allowed-tools**: 声明 skill 内能调用的 tool 白名单（MCP tool URI 形式）。
  - **Hooks 概念**: PreToolUse / PostToolUse / Stop 等钩子点（Claude Code 实现，非 SKILL.md spec 强制部分；agentskills.io spec 当前不强制）。
- **schema 表达力**: YAML 自身无强约束 schema，依赖 host 实现做 lint。
- **电力调度 skill 能否直接采用**: **yes-with-caveats**
  - 包结构与 progressive disclosure 直接可用，对调度场景非常合适——调度 skill 数量会很多，全量加载不现实。
  - frontmatter 必须扩展：调度场景需 `risk_class`、`required_certifications`、`affected_voltage_levels`、`requires_two_person_approval` 等约束字段；这些用 `metadata.x-grid-*` 命名空间挂在 frontmatter 下。
- **不足 / 调度场景需补的部分**:
  - 当前 spec 没有强 schema lint，工业场景需自定义 frontmatter JSON Schema 做 CI 校验。
  - hooks 的事件清单未标准化，必须与 L1 hook 事件枚举绑定。
  - 没有 skill 之间的依赖与版本兼容声明（调度 skill 间存在依赖，需补）。
- **推荐继承级别**: **MUST** —— L1 skill 包格式必须继承 SKILL.md + agentskills.io 开放规范，自定义字段走 `x-grid-*` 命名空间。

参考: <https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills> · <https://agentskills.io/specification> · <https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview>

---

### 1.4 LangChain Tools / LangGraph

- **定义**: LangChain 1.0 / LangGraph 1.0（2025 GA）的 tool 抽象 = Python/JS 函数 + `@tool` 装饰器 + Pydantic / Zod 输入模型。LangGraph 是 graph-based agent orchestration，nodes 间通过 typed state 传递。LangChain 在 2026-03 发布 "LangChain Skills"（agent 用的 SKILL.md 兼容指令包）。
- **当前状态**: 框架级抽象，非 wire 协议；与 MCP 互通靠 `langgraph-mcp-adapters`（双向桥）。
- **核心能力**:
  - tool 抽象 = Python callable + 类型注解 → 自动派生 JSON Schema。
  - LangGraph 的 typed state graph 支持 checkpointing、time-travel debugging、人机回路。
  - 与 MCP 已建立官方 adapter。
- **schema 表达力**: 继承 Pydantic / Zod → JSON Schema。
- **电力调度 skill 能否直接采用**: **no（作为 L1 协议）/ yes（作为客户端实现）**。
  - 调度 L1 不应当指定具体编程框架。LangChain/LangGraph 是 client side 实现，不是 host ↔ server wire 协议。
  - 但 LangGraph 的 state checkpointing 思想对调度场景的"操作可追溯 / 可回放"有借鉴意义——L1 可作为 informative reference。
- **推荐继承级别**: **MAY** —— 仅作客户端实现选项；L1 spec **不引** 任何具体框架名。

参考: <https://www.langchain.com/blog/langchain-skills> · <https://www.langchain.com/langgraph>

---

### 1.5 AutoGen / CrewAI 的 agent skill 抽象

- **定义**: AutoGen（Microsoft Research，现为 Microsoft Agent Framework 一部分）以 conversation 为中心，agent = 拥有 tool 的对话参与者。CrewAI 以 role-based crew 为中心，agent 有 role / goal / backstory / tools 字段。两者都用 Python 装饰器声明 tool。
- **当前状态**: 框架级抽象，无 wire 协议化的 spec。Microsoft Agent Framework 在 `learn.microsoft.com` 公开了 "Agent Skills" 文档，但语义与 Anthropic Agent Skills 部分重合（packaging 模型相似、name 相同），需仔细辨别。
- **schema 表达力**: 框架内部约定。
- **电力调度 skill 能否直接采用**: **no** —— 与 LangChain/LangGraph 同属"客户端实现选项"，不应作为 L1 协议。
- **推荐继承级别**: **MAY** （informative reference）。

参考: <https://learn.microsoft.com/en-us/agent-framework/agents/skills> · <https://crewai.com/>

---

### 1.6 Spring AI / LangChain4j 的 tool 抽象（企业 Java 生态）

- **定义**: Spring AI 提供 `@Tool` 注解 + `ToolCallback` 抽象，Java 方法即 tool；LangChain4j 类似。Spring AI 也内置 MCP 客户端 / 服务端支持。
- **当前状态**: 框架级 SDK，对企业 Java 栈非常重要——电力调度集成商有大比例 Java/Spring 团队。
- **schema 表达力**: 通过 Bean Validation + Jackson 派生 JSON Schema。
- **电力调度 skill 能否直接采用**: **no（作为 L1 协议）/ yes（作为 Java 端实现指南）**。
- **推荐继承级别**: **MAY** —— L1 spec 中可作为 "Java 实现 reference" 列出，**不强制**。

参考: <https://docs.spring.io/spring-ai/reference/api/tools.html>

---

### 1.7 Google A2A (Agent-to-Agent) 协议

- **定义**: Google 2025-04 发起，2026-02 与 IBM 联合并入 Linux Foundation 治理。定位 **agent ↔ agent** 协作（与 MCP 的 LLM ↔ tool 正交）。基于 HTTP + JSON-RPC，定义 AgentCard（能力广告）、Task（任务对象）、Message（消息流）。
- **当前状态**: 公开标准，治理已进 Linux Foundation。Spec 版本演进中，多供应商承诺支持但实际部署度有限。
- **核心能力**: agent 互发现、任务委派、流式状态、跨厂商认证。
- **电力调度 skill 能否直接采用**: **MAY (informative)**
  - 调度场景目前以 "单 host + 多 skill" 为主，跨 agent 协作不是 L1 当前必需场景。
  - 但若将来出现"调度大模型 ↔ 配电网大模型"协作，A2A 是事实候选。L1 应保留扩展空间但不强制。
- **推荐继承级别**: **MAY**（标记为 future-proof reference）。

参考: <https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability>

---

## §2. Schema / 接口定义层

### 2.1 JSON Schema (Draft 2020-12)

- **定义**: 现行最新 IETF JSON Schema 草案；Anthropic / OpenAI tool 入参事实标准；OpenAPI 3.1 也对齐到 2020-12。
- **当前状态**: IETF 草案（status = Internet Draft，未 RFC 化），但事实工业标准；后续草案 2025-XX 在演进中（未独立 verify 具体编号）。
- **核心能力**: 类型约束、范围、enum、引用、组合（allOf/anyOf/oneOf）、`unevaluatedProperties` / `unevaluatedItems`、`$dynamicRef`。
- **电力调度 skill 能否直接采用**: **yes** —— L1 入参 / 出参 / frontmatter 字段约束的唯一选择。
- **调度场景注意**:
  - 浮点精度：JSON 数字不是 IEEE 754 显式约束，调度涉及电压/电流/相角的高精度 reading 时，需在 schema 用 `multipleOf` 或字符串化（`"type": "string", "pattern": "^-?\\d+\\.\\d{6}$"`）锁定精度。
  - 时间戳：必须用 `format: date-time`（RFC 3339），并在 L1 强制 UTC + 纳秒精度（默认 `format: date-time` 只到秒/毫秒，需扩展）。
  - 单位标注：JSON Schema 没有单位概念，调度必须扩展 `x-unit` 字段（如 `kV`, `MW`, `Hz`）。
- **推荐继承级别**: **MUST**。

参考: <https://json-schema.org/draft/2020-12>

---

### 2.2 OpenAPI 3.1.x

- **定义**: REST API 描述语言；3.1.0（2021）+ 后续 3.1.x 维护版。3.1 正式与 JSON Schema 2020-12 对齐（之前版本是 OAS-flavored JSON Schema 子集）。
- **当前状态**: OpenAPI Initiative / Linux Foundation 治理。3.1 仍是稳定主流。
- **核心能力**: paths、operations、security schemes、webhooks（3.1 新增）、callbacks。
- **电力调度 skill 能否直接采用**: **yes-with-caveats**
  - skill 内部如果暴露 REST API（如调度数据查询），用 OpenAPI 3.1 描述合适。
  - 但 skill 自身的 manifest 不是 REST 服务而是包定义，OpenAPI 不是首选——SKILL.md frontmatter + JSON Schema 才是。
- **推荐继承级别**: **SHOULD** —— skill 内部 sub-API 描述用 OpenAPI 3.1；skill 主 manifest **不用** OpenAPI。

参考: <https://github.com/oai/openapi-specification/blob/main/versions/3.1.0.md>

---

### 2.3 Protobuf 3 / gRPC

- **定义**: Google 主导的二进制序列化 + RPC 协议；强类型、跨语言、可演化（field number 不可重用是关键约束）。
- **核心能力**: schema-first、IDL 生成多语言代码、双向 streaming、HTTP/2 多路复用。
- **电力调度 skill 能否直接采用**: **MAY**
  - skill 与底层 EMS / SCADA / 量测系统对接时，gRPC 是实际工业候选（部分电力厂商已用）。
  - 但 skill 协议本身不应锁死 gRPC——MCP/JSON-RPC over HTTP 已经覆盖 LLM 端。
- **推荐继承级别**: **MAY** —— 作为 L1 描述 "skill ↔ 后端工业系统" 的可选传输；**不作 host ↔ skill 主协议**。

---

### 2.4 JSON-RPC 2.0

- **定义**: jsonrpc.org 2010 发布的轻量级 RPC 信封。错误码段：-32700 Parse / -32600 Invalid Request / -32601 Method Not Found / -32602 Invalid Params / -32603 Internal Error / -32000~-32099 Server-defined。
- **当前状态**: 稳定 16 年；MCP 全面采用。
- **核心能力**: request / response / notification / batch、错误对象、id 映射。
- **电力调度 skill 能否直接采用**: **MUST** —— 通过 MCP 间接强制继承。
- **推荐继承级别**: **MUST**。

参考: <https://www.jsonrpc.org/specification>

---

## §3. 版本 / 演化层

### 3.1 SemVer 2.0.0 (Semantic Versioning)

- **定义**: MAJOR.MINOR.PATCH。MAJOR=破坏性、MINOR=向后兼容新功能、PATCH=向后兼容修复。
- **电力调度 skill 能否直接采用**: **yes**。skill 包版本、L1 spec 版本、wire schema 版本都用 SemVer。
- **推荐继承级别**: **MUST**。

### 3.2 CalVer

- **定义**: 日期版本（如 `2025.11.25`）。MCP spec 自身用 CalVer。
- **电力调度 skill 能否直接采用**: **MAY** —— 调度规范文档版本可以用 CalVer（与软件版本区分），但 skill 包 / wire schema **不建议** CalVer（缺破坏性语义）。
- **推荐继承级别**: **MAY**（仅 spec 文档版本）。

### 3.3 Backwards-compatibility 实践

- **核心规则**:
  - Protobuf field number 永不重用（删除字段必须 `reserved`）。
  - JSON Schema additive 演化：新增 optional 字段是 minor；改 required 或类型是 major。
  - MCP `tools/list_changed` 通知保证客户端能感知 tool 变化。
- **调度场景特殊压力**: 调度 skill 一旦上线，**回滚成本极高**（操作员肌肉记忆 + 操作票模板锁定参数名）。L1 必须在 skill 规范里强制 deprecation 期 + dual-name 兼容窗口。
- **推荐继承级别**: **MUST**（结合 SemVer + Protobuf field number 规则 + MCP list_changed 通知）。

---

## §4. 安全 / 权限层

### 4.1 OAuth 2.1 + RFC 7591 Dynamic Client Registration

- **定义**: OAuth 2.1（IETF Draft，整合 OAuth 2.0 + RFC 7636 PKCE 必选 + 移除 implicit flow + redirect_uri 精确匹配）。Dynamic Client Registration 让客户端在不预先注册的情况下接入。
- **当前状态**: OAuth 2.1 仍 IETF Draft（未独立 verify 是否已 RFC 化），但事实部署广泛；MCP 2025-11-25 推荐用 OAuth 2.1 + DCR。
- **电力调度 skill 能否直接采用**: **yes-with-caveats**
  - 调度内网常用 IAM（统一身份）+ 内部签发 JWT，OAuth 2.1 mapping 自然。
  - DCR 在内网 zero-trust 场景非常有用（agent / skill / mcp server 动态注册），但 **必须配合 mTLS + 白名单 issuer**，否则在工业网安全等保 III 级以上无法过审。
- **推荐继承级别**: **SHOULD** （skill ↔ 远程 MCP server 时）。

参考: <https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization> · <https://oauth.net/2/dynamic-client-registration>

### 4.2 OIDC

- **定义**: OAuth 2.0 之上的身份层；id_token (JWT) 标准。
- **推荐继承级别**: **SHOULD** —— 与 OAuth 2.1 一起作为 L1 默认身份层。

### 4.3 JWT (RFC 7519)

- **定义**: JSON Web Token。承载 claims，HS256/RS256/ES256 等签名。
- **电力调度 skill 能否直接采用**: **yes**。L1 用 JWT 承载 user / session / capability claims。
- **推荐继承级别**: **MUST**（作为 token 格式）。

### 4.4 W3C Verifiable Credentials 2.0

- **定义**: 2025-05-15 W3C 正式 Recommendation。VC Data Model v2.0 + VC-JOSE-COSE 签名（JOSE / SD-JWT / COSE）。
- **核心能力**: 三方信任模型（issuer / holder / verifier）+ tamper-evident 签名 + 选择性披露（SD-JWT）。
- **电力调度 skill 能否直接采用**: **MAY (informative)**
  - 调度操作员的 *岗位资质 / 持证状态* 用 VC 表达比内部数据库强（跨单位 / 检修人员临时进站场景）。
  - 但短期内国调中心更可能用国产 PKI / 数字身份证书，VC 是中长期演进目标。
- **推荐继承级别**: **MAY**（保留扩展位 `credential_envelope: "vc+jwt" | "x509+pkcs7"`）。

参考: <https://www.w3.org/TR/vc-data-model-2.0>

### 4.5 HMAC / API Key signing

- **定义**: AWS SigV4 / GitHub Webhook 等私有派生方案的总称。
- **电力调度 skill 能否直接采用**: **yes** （内部低风险 skill / 离线工具调用）。
- **推荐继承级别**: **MAY**（仅低风险 skill）。

### 4.6 mTLS (RFC 8705 OAuth 2.0 Mutual-TLS Client Authentication)

- **定义**: 双向 X.509 证书。
- **电力调度 skill 能否直接采用**: **MUST** —— 调度 II/III 区横向边界部署的 skill 通信 **必须** mTLS（等保要求）。
- **推荐继承级别**: **MUST**（生产环境 host ↔ skill 通信）。

---

## §5. 可观测性 / 审计层

### 5.1 OpenTelemetry + GenAI Semantic Conventions

- **定义**: CNCF graduated 项目。GenAI Semantic Conventions 2024-2026 持续演进，定义 LLM call / tool call / agent call 的 span 属性、metric instrument、event 字段。
- **当前状态**: GenAI conventions 仍 Experimental（spec 自己声明），但 Datadog / Dynatrace / Honeycomb / 阿里云 ARMS / 腾讯云 APM 已支持。
- **核心能力**:
  - **span**: `gen_ai.system`、`gen_ai.request.model`、`gen_ai.usage.input_tokens` / `output_tokens`、tool call span 类型。
  - **metric**: token usage counter、latency histogram。
  - **event**: prompt / completion content（opt-in，避免泄漏 PII）。
- **电力调度 skill 能否直接采用**: **MUST**
  - 工业场景的 *调度可追溯性* 要求与 distributed tracing 高度契合。
  - 调度场景须扩展 `x-grid-*` span 属性：`x-grid.work_ticket_id`、`x-grid.operator_id`、`x-grid.risk_class`、`x-grid.affected_substations`。
- **推荐继承级别**: **MUST**。

参考: <https://opentelemetry.io/docs/specs/semconv/gen-ai/> · <https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans>

### 5.2 W3C Trace Context (Level 1 / Level 2)

- **定义**: 标准 HTTP header `traceparent` + `tracestate` 跨进程传递 trace 上下文。
- **当前状态**: Level 1 (REC 2021)；Level 2 Working Draft 2024-03。
- **核心格式**: `traceparent: <version>-<trace-id>-<parent-id>-<trace-flags>`，例 `00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01`。
- **电力调度 skill 能否直接采用**: **MUST** —— 跨 host / skill / 后端工业系统的 trace 必须用 W3C trace context；不能各家用各家的 X-Request-Id 派生。
- **推荐继承级别**: **MUST**。

参考: <https://www.w3.org/TR/trace-context>

### 5.3 CloudEvents 1.0

- **定义**: CNCF graduated 项目。事件信封标准（context attributes：id / source / specversion / type + 可选 time / subject / data 等）。两种编码：structured（整事件 in body）、binary（attributes in headers + data in body）。
- **当前状态**: v1.0.x 稳定；CloudEvents SQL v1（2024）扩展过滤能力。
- **电力调度 skill 能否直接采用**: **SHOULD**
  - skill 触发的事件 / 调度异步通知用 CloudEvents 信封，跨平台互操作好。
  - 调度场景须扩展 `x-grid.alarm_priority`、`x-grid.affected_substation_ids` 字段。
- **推荐继承级别**: **SHOULD**（事件/异步消息场景）。

参考: <https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md>

### 5.4 RFC 5424 Syslog（结构化版） / RFC 3164（BSD 旧版）

- **定义**: 系统日志格式。RFC 5424 是结构化版，含 facility / severity / timestamp / hostname / app-name / msgid / structured-data / msg。
- **当前状态**: 工业 SOC 部署主流。
- **电力调度 skill 能否直接采用**: **SHOULD** （安全审计层；与 OpenTelemetry log signal 并行）。
- **推荐继承级别**: **SHOULD** （与等保 / 网安要求对接时强制）。

---

## §6. 数据流 / 流式输出

### 6.1 Server-Sent Events (SSE)

- **定义**: HTML5 标准；HTTP keepalive 单向 server→client 推送；`text/event-stream` MIME；自动重连。
- **电力调度 skill 能否直接采用**: **SHOULD** （L1 默认流式输出）。
- **不足**: 单向；浏览器以外的客户端实现质量参差；HTTP/1.1 长连接消耗。
- **推荐继承级别**: **SHOULD**（默认 LLM token 流式输出）。

### 6.2 WebSocket (RFC 6455)

- **定义**: 双向全双工。
- **电力调度 skill 能否直接采用**: **MAY** —— 仅在 host ↔ skill 需要双向交互（如 long-running tool 中间询问用户）时。
- **推荐继承级别**: **MAY**。

### 6.3 gRPC streaming（server-stream / client-stream / bidi-stream）

- **定义**: HTTP/2 多路复用 + protobuf 帧。
- **电力调度 skill 能否直接采用**: **MAY** —— 内网 skill ↔ 工业系统接驳；不作 host ↔ skill 协议。
- **推荐继承级别**: **MAY**。

### 6.4 NDJSON (newline-delimited JSON) / JSON Lines

- **定义**: 每行一个 JSON 对象；`application/x-ndjson` MIME。
- **现状**: MCP Streamable HTTP 部分场景、A2A streaming、部分 LLM 厂商 / Ollama 默认。
- **电力调度 skill 能否直接采用**: **SHOULD** —— SSE 之外的备选流式格式；尤其非浏览器客户端环境（CLI / 工业终端）。
- **推荐继承级别**: **SHOULD** （与 SSE 并列 alternate）。

### 6.5 MCP Streamable HTTP

- **定义**: MCP 2025-06-18 引入的统一传输。本质是 HTTP POST + 单 endpoint，可同步可异步可流式（response 用 SSE / 否则用普通 JSON）。
- **电力调度 skill 能否直接采用**: **MUST** （随 MCP 一并继承）。
- **推荐继承级别**: **MUST**。

---

## §7. 模型治理 / 评估

> 本节是 *工业实践方法论*，没有形式 spec；L1 推荐方法论而非强制框架。

### 7.1 OpenAI Evals

- **定义**: OpenAI 开源 eval 框架；YAML 任务定义 + Python grader。
- **L1 角色**: methodology reference。

### 7.2 DeepEval / RAGAS

- **定义**: 都是面向 RAG / agent 的评估指标库。RAGAS 主打 Faithfulness / Answer Relevancy / Context Recall / Context Precision；DeepEval 包含 14+ 指标 + agent / chatbot 评估，并集成 RAGAS 子集。
- **现状**: 开源主流；DeepEval 有 G-Eval、Hallucination 等 LLM-as-judge 指标。
- **L1 角色**: methodology reference。

### 7.3 HELM / lm-eval-harness

- **定义**: HELM (Stanford) / EleutherAI lm-evaluation-harness 是 *基础模型* benchmark 集；不是 agent skill 评估。
- **L1 角色**: out-of-scope（基础模型选型层）。

### 7.4 推荐方法论

- 调度 skill **不能** 仅靠 LLM-as-judge —— 工业场景要求 deterministic、可复现、可解释。
- L1 应推荐 **三层评估**:
  1. **Schema 合规层**: 自动 schema 校验（JSON Schema 2020-12）。
  2. **行为层**: scenario replay（fixture-based deterministic 测试，如 cassette / VCR pattern），覆盖典型工况。
  3. **人工 sign-off 层**: 高风险 skill 必须人工标注 + 双盲评审。
- LLM-as-judge **仅作辅助信号**，不作 gate。
- **推荐继承级别**: 方法论 **SHOULD**；具体框架 **MAY**。

参考: <https://deepeval.com/> · <https://docs.ragas.io/>

---

## §8. 电力 / 工业相关业界标准（接驳层，非 skill 协议层本身）

### 8.1 IEC 61970 / IEC 61968 (CIM - Common Information Model)

- **定义**: IEC 61970 = EMS（能量管理系统）应用程序接口 + CIM 数据模型；IEC 61968 = DMS（配电管理系统）扩展。最新版 IEC 61970-302:2024 + IEC 61970-457:2024。
- **角色**: 电网设备 / 拓扑 / 量测的 **行业共享语义模型**（UML / RDF / XML schema 表达）。
- **与 skill 协议的关系**:
  - **不替代** skill 协议；CIM 是 skill 操作的 *领域数据模型*，不是 skill 包格式或调用协议。
  - 但 skill 的 input/output schema **应该引用 CIM 类**（例如 skill 入参 `equipment_mrid` 类型应映射到 `cim:Equipment.mRID`）。
  - 这一层属于 L2（电力行业层），但 L1 必须 **保留扩展位**：skill schema 字段允许标注 `x-cim-class: "cim:ACLineSegment"` 之类的 referent。
- **推荐继承级别**: L1 层 **MAY** 引用（保留扩展点）；L2 层 **MUST**。

参考: <https://webstore.iec.ch/en/publication/68152> · <https://en.wikipedia.org/wiki/Common_Information_Model_(electricity)>

### 8.2 IEC 61850（变电站自动化通信）

- **定义**: 变电站内 IED（智能电子设备）通信标准。包含 SCL（Substation Configuration Language，XML 格式）+ MMS / GOOSE / Sampled Values 协议。
- **角色**: 厂站层 protocol，与 skill 协议不在同一层。
- **与 skill 协议的关系**:
  - skill 不直接走 IEC 61850 wire；skill 通过中间网关访问 SCADA/EMS 后才接 61850。
  - 但 skill 入参中的 *测点名* / *设备引用* 可能采用 61850 LN/DO/DA 路径表达（例如 `XCBR1.Pos.stVal`），L1 schema 应允许此命名空间。
- **推荐继承级别**: L1 层 **MAY**（保留命名空间扩展）；L2/L3 层会强引。

参考: <https://www.kth.se/social/upload/516524f5f276543c50c12a6e/Lecture%20%236%20-%20%20IEC%2061850.pdf>

---

## §9. 推荐：电力调度 skill 规范 L1 通用层应引的标准清单

按 RFC 2119 强度分级。

### 9.1 必须引（MUST）

| 标准 | 引用范围 | 理由 |
|------|---------|------|
| **MCP 2025-11-25**（含其 OAuth 2.1 + DCR 子集） | host ↔ skill server 主 wire 协议 | LLM 端事实标准；不引则自造轮子且生态隔离 |
| **JSON-RPC 2.0** | RPC 信封 + 错误码 | MCP 内部依赖；间接 MUST |
| **JSON Schema 2020-12** | tool 入参 / 返回 / SKILL.md frontmatter 字段约束 | 业界 tool calling 公约；调度 schema 必须严格可校验 |
| **Anthropic Agent Skills / agentskills.io** | skill 包格式 + frontmatter + progressive disclosure | 唯一公开 skill 包标准；自定义字段走 `x-grid-*` |
| **SemVer 2.0.0** | skill 包 / wire schema / spec 文档版本 | 破坏性 / 兼容性语义清晰 |
| **JWT (RFC 7519)** | session / capability token 格式 | 与 OAuth 2.1 / OIDC 自然衔接 |
| **mTLS (RFC 8705)** | 生产环境 host ↔ skill 通信 | 等保 III 级以上工业要求 |
| **OpenTelemetry + GenAI Semantic Conventions** | trace / metric / log signal | 调度可追溯性硬要求；扩展 `x-grid.*` 属性 |
| **W3C Trace Context Level 1** | trace 跨进程传递 | 不引则各厂自造 X-Request-Id 派生不互通 |
| **MCP Streamable HTTP** | 流式 LLM token 输出 | MCP 当前推荐传输 |
| **Backwards-compat 实践**（Protobuf field number 不重用 + JSON Schema additive 演化 + `tools/list_changed`） | 演化纪律 | 调度 skill 上线后回滚成本极高 |

### 9.2 推荐引（SHOULD）

| 标准 | 引用范围 | 理由 |
|------|---------|------|
| **OAuth 2.1** | 远程 MCP server 认证 | MCP 2025-11-25 已推荐；但内网部分场景可降级到 mTLS-only |
| **OIDC** | 用户身份层 | 与 OAuth 2.1 自然组合 |
| **OpenAPI 3.1** | skill 内部 sub-API 描述 | 不作 skill manifest 主格式 |
| **CloudEvents 1.0** | skill 触发的异步事件信封 | 跨平台事件互操作 |
| **RFC 5424 Syslog** | 安全审计日志 | 与等保 / SOC 系统对接 |
| **SSE** | LLM token 流式默认输出 | MCP Streamable HTTP 子集；浏览器友好 |
| **NDJSON** | 非浏览器客户端的备选流式 | CLI / 工业终端环境 |

### 9.3 可选引（MAY）

| 标准 | 引用范围 | 理由 |
|------|---------|------|
| **gRPC + Protobuf 3** | skill ↔ 后端工业系统接驳层 | 已有部分电力厂商在用；不作 host ↔ skill 主协议 |
| **WebSocket** | 双向交互场景 | 边缘场景 |
| **W3C Verifiable Credentials 2.0** | 操作员资质 token | 中长期演进；保留 `credential_envelope` 扩展位 |
| **HMAC / API Key signing** | 低风险离线 skill | 不作生产 skill 主认证 |
| **W3C Trace Context Level 2** | trace propagation 进阶 | Level 1 足够；Level 2 跟进观察 |
| **CalVer** | spec 文档版本 | skill 包 / wire schema 不用 |
| **Google A2A** | agent ↔ agent 协作 | 当前场景不需要；保留扩展位 |
| **OpenAI Function Calling shape** | LLM 客户端内部 tool 投影 | 仅作 reference，不作 wire 协议 |
| **CIM (IEC 61970/61968)** | schema 字段标注 | L1 仅保留 `x-cim-class` 扩展位；L2 强引 |
| **IEC 61850 LN/DO/DA 命名空间** | 测点引用 | L1 允许此命名空间；L2 强引 |
| **DeepEval / RAGAS** | 评估方法 | 不作强制框架 |

### 9.4 显式不引（REJECT / SHOULD NOT）

| 标准 / 框架 | 不引理由 |
|------------|---------|
| **OpenAI Function Calling 作为 wire 协议** | 私有；与 MCP 重复；strict mode 是 JSON Schema 真子集，调度复杂 schema 不兼容 |
| **LangChain / LangGraph / AutoGen / CrewAI / Spring AI 作为 L1 协议** | 框架级实现选项，非协议；锁定具体编程栈违反"L1 通用"原则 |
| **HELM / lm-eval-harness** | 基础模型 benchmark，与 skill 评估错层 |
| **CalVer 用作 skill 包 / wire schema 版本** | 缺破坏性 / 兼容性语义 |
| **OpenAPI 3.1 作为 skill 主 manifest** | OpenAPI 描述 REST API；skill manifest 是包定义，错位 |
| **RFC 3164（旧 Syslog）** | 已被 RFC 5424 结构化版替代 |
| **JSON-RPC 2.0 over WebSocket 自定义信封** | 已有 MCP 标准信封；自造无收益 |

---

## §10. 标准之间的协作关系（mental model）

调用一次电力调度 skill 涉及到的标准链路：

```
[调度员/调度上层 Host]
        │
        │ 1. mTLS 握手 (RFC 8705)
        │ 2. OAuth 2.1 access_token + JWT (RFC 7519) 携带操作员身份
        │ 3. W3C traceparent header 注入
        ▼
[MCP Server / Skill 容器]
        │
        │ 4. JSON-RPC 2.0 信封  →  tools/call
        │    method = "tools/call"
        │    params.name = "grid.dispatch.adjust_setpoint"
        │    params.arguments = { ... }   ← JSON Schema 2020-12 校验
        │                                    schema 字段含 x-cim-class / x-unit 扩展
        ▼
[Skill Body 执行]
        │
        │ 5. SKILL.md frontmatter (allowed-tools / risk_class / x-grid-*)
        │    驱动 host 做风险拦截 / 双人审批 gate
        │
        │ 6. OpenTelemetry span 全程贯穿
        │    gen_ai.tool.name + x-grid.work_ticket_id + x-grid.operator_id
        │
        │ 7. 后端工业系统接驳:
        │     ├─ EMS / SCADA → CIM (IEC 61970/61968) 数据模型
        │     ├─ 变电站 IED → IEC 61850 SCL / MMS / GOOSE
        │     └─ 第三方服务 → gRPC + Protobuf  /  REST + OpenAPI 3.1
        ▼
[Skill 返回]
        │
        │ 8. MCP Streamable HTTP 响应:
        │    成功 → SSE/NDJSON token 流  +  最终 structured content
        │    失败 → JSON-RPC 2.0 error (-32xxx)  +  CloudEvents 异步告警
        ▼
[Host 渲染 + 审计落盘]
        │
        │ 9. RFC 5424 Syslog → 安全审计
        │ 10. CloudEvents 1.0 → 异步事件总线（告警/工单触发）
```

**纵向（垂直）**：身份 → 鉴权 → 协议 → schema → 执行 → 观测 → 审计。

**横向（同层）多备选时的优先级**：
- 流式：SSE > NDJSON > WebSocket（按交互复杂度递增）
- 认证：mTLS（必备）+ OAuth 2.1+OIDC+JWT（应用层）+ HMAC（仅低风险离线）
- 事件：CloudEvents > 自定义信封
- Schema：JSON Schema 2020-12（顶层）；Protobuf 仅在工业系统接驳层

---

## §11. 调度场景对 L1 通用标准的特殊压力（仅指出，L2 接手）

> 本节列出 L1 标准在调度场景会遇到的"压力点"，*不解决*，留给 L2（电力调度行业层）。

| 压力维度 | 现象 | L1 标准的局限 | 转交 L2 |
|---------|------|--------------|---------|
| **硬实时 SLA** | 调度操作要求 *响应上界*（如 < 200ms 必须返回首 token；遥控 < 1s 必须确认） | MCP / OpenAI Function Calling / OTel 都是 best-effort，无 deadline 字段；JSON-RPC 无超时语义 | L2 在 frontmatter 增 `x-grid.sla.deadline_ms` + harness 端做 deadline 拦截 |
| **可解释性 / 可问责性** | "为什么决策切除该线路？"必须有 chain-of-evidence，不能只有 LLM 输出 | JSON Schema 不带"为什么"信息；OTel span 字段是事后审计而非实时解释 | L2 强制 skill 返回 `evidence_anchors` 数组（指向证据链） |
| **风险分级精细度** | MCP `destructiveHint` 只有 boolean | 二值不够：调度需要至少 4 级（read-only / advisory / reversible-control / irreversible-control） | L2 frontmatter `x-grid.risk_class: I/II/III/IV` |
| **双人 / 多人审批** | 高风险遥控操作要求 *监护人 + 操作员* 双签 | MCP 无人机回路语义；agentskills.io 无 multi-party gate | L2 frontmatter `x-grid.requires_two_person_approval` + harness 拦截 |
| **操作票绑定** | 每次调度操作必须挂操作票号 | 无标准字段 | L2 frontmatter / span attribute `x-grid.work_ticket_id` 强制 |
| **可逆性 / 演练模式** | 必须支持 dry-run / shadow execution / 演练沙盒 | MCP 无标准 dry-run 标志（部分 server 自定义） | L2 强制 `dry_run: bool` 入参 + harness 端二级隔离 |
| **CIM / IEC 61850 语义对齐** | 调度 skill 涉及设备 mRID / 量测 LN/DO/DA | L1 仅保留扩展位，无强制 | L2 强制 schema 引用 CIM 类 + 61850 命名空间 |
| **国密 / 商密合规** | SM2/SM3/SM4 替代 RSA/SHA-256/AES | OAuth 2.1 / JWT / mTLS 默认 IETF 算法套件 | L2 算法白名单 + 国密 PKI / 国产 OS / 国产数据库适配 |
| **物理/电气/网络分区隔离** | 三道安全边界（生产控制大区 / 信息内网 / DMZ） | 单一 wire 协议无法满足跨边界 gateway 语义 | L2 定义 skill 部署 zone + 跨 zone 协议降级规则 |
| **量测精度 / 单位标注** | 电压/电流/相角的物理量精度与单位 | JSON Schema 无单位概念；浮点精度无显式约束 | L2 强制 schema 字段 `x-unit` + 精度字符串化 |
| **国调中心特有的"四个不发生"约束** | 不能误导致大面积停电 / 误操作 / 设备损毁 / 人身伤亡 | L1 无业务安全模型 | L2 风险分类矩阵 + 不可达操作黑名单 |

L1 的职责是 **不挡 L2 的扩展路**：
- frontmatter / schema 字段允许 `x-grid-*` 命名空间；
- span 属性允许 `x-grid.*` 命名空间；
- 错误码段 `-32000~-32099` 供 L2 定义业务错误；
- 版本演化采纳 SemVer，让 L2 加约束时是 minor 而非 major。

---

## References

### 协议 / Skill 层
- MCP Specification 2025-11-25 — <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP Tools — <https://modelcontextprotocol.io/specification/2025-11-25/server/tools>
- MCP Authorization — <https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization>
- Anthropic Agent Skills — <https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills>
- agentskills.io Specification — <https://agentskills.io/specification>
- Claude Agent Skills Doc — <https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview>
- OpenAI Function Calling — <https://developers.openai.com/api/docs/guides/function-calling>
- OpenAI Structured Outputs — <https://developers.openai.com/api/docs/guides/structured-outputs>
- LangChain Skills — <https://www.langchain.com/blog/langchain-skills>
- Microsoft Agent Skills — <https://learn.microsoft.com/en-us/agent-framework/agents/skills>
- Spring AI Tool Calling — <https://docs.spring.io/spring-ai/reference/api/tools.html>
- Google A2A — <https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability>

### Schema / 接口
- JSON Schema Draft 2020-12 — <https://json-schema.org/draft/2020-12>
- OpenAPI 3.1.0 — <https://github.com/oai/openapi-specification/blob/main/versions/3.1.0.md>
- JSON-RPC 2.0 — <https://www.jsonrpc.org/specification>

### 安全
- W3C Verifiable Credentials Data Model 2.0 — <https://www.w3.org/TR/vc-data-model-2.0>
- OAuth 2.0 Dynamic Client Registration — <https://oauth.net/2/dynamic-client-registration>
- OAuth 2.0 Mutual-TLS — RFC 8705

### 可观测性
- OpenTelemetry GenAI Conventions — <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
- OTel GenAI Agent Spans — <https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans>
- W3C Trace Context — <https://www.w3.org/TR/trace-context>
- W3C Trace Context Level 2 — <https://www.w3.org/TR/trace-context-2>
- CloudEvents Spec — <https://github.com/cloudevents/spec/blob/main/cloudevents/spec.md>
- RFC 5424 Syslog Protocol — <https://datatracker.ietf.org/doc/html/rfc5424>

### 数据流
- HTML5 SSE — <https://html.spec.whatwg.org/multipage/server-sent-events.html>
- WebSocket — RFC 6455
- gRPC — <https://grpc.io/docs/>

### 评估
- DeepEval — <https://deepeval.com/>
- RAGAS — <https://docs.ragas.io/>

### 电力 / 工业
- IEC 61970-302:2024 (CIM Dynamics) — <https://webstore.iec.ch/en/publication/68152>
- IEC 61970-457:2024 — <https://webstore.iec.ch/en/publication/68910>
- CIM (Wikipedia) — <https://en.wikipedia.org/wiki/Common_Information_Model_(electricity)>
- IEC 61968 (Wikipedia) — <https://en.wikipedia.org/wiki/IEC_61968>
- IEC 61850 Engineering — KTH lecture notes — <https://www.kth.se/social/upload/516524f5f276543c50c12a6e/Lecture%20%236%20-%20%20IEC%2061850.pdf>

---

*备注：本文所有标准内容基于 2026-04-26 之前可公开访问的 spec 页面与训练数据综合判断；细节性条款（如 OAuth 2.1 是否已 RFC 化、JSON Schema 是否已发布 2025-XX 草案）建议在最终发布前由人工 verify 一次。*
