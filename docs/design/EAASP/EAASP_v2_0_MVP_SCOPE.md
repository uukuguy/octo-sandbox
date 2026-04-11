# EAASP v2.0 MVP Scope（圈 2）

> **文档性质**：Phase 0 MVP 的范围定义与前期资产评估。本文档是 Phase 0 的唯一范围权威。
> **上层文档**：`docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md`
> **执行计划**：`docs/plans/2026-04-11-v2-mvp-phase0-plan.md`
>
> **创建日期**：2026-04-11
> **对齐规范**：v2.0 Design Specification §19 Phase 0 baseline + 扩展 Memory Engine

---

## 一、MVP 定位

> **Infrastructure MVP — 做精做强骨架，一个真实"记忆累加" skill 贯穿验证所有核心能力。**

### 1.1 必须证明的命题

MVP 完成的**唯一验收标准**：

> **"同一个用户、不同时间、两次独立的 session，在'阈值校准助手'skill 下，
> 第二次 session 能从 L2 Memory Engine 正确读取第一次 session 沉淀的 memory file
> 和 evidence anchors，并在第二次执行中引用它们。整个链路每一步的 hook 都在
> managed-settings 的治理下正确触发，所有跨层调用都走 5 个 REST contract 或 MCP 协议。"**

如果这条命题成立，则以下能力都被同时证明：
- ✅ 16 方法 Runtime Interface Contract
- ✅ 结构化 SessionPayload（P1-P5 priority blocks）
- ✅ 三向握手（three-way handshake）
- ✅ 至少 3 种 hook handler type 可用（command + http + prompt）
- ✅ Deny-always-wins 跨 scope（managed + skill-frontmatter）
- ✅ 5 个 L3/L4 REST API Contract 有最小实现
- ✅ L2 Skill Repository 4 阶段 promotion pipeline
- ✅ L2 Memory Engine 三层存储最小版
- ✅ 6 个 Memory MCP tool
- ✅ L1 → L2 memory write channel
- ✅ L4 → L2 context assembly
- ✅ Evidence anchor 写入与跨 session 读出
- ✅ 至少 2 个 T1 runtime 通过 certifier（grid-runtime Rust + hermes 或 claude-code Python）

---

## 二、能力清单（圈 2 = 圈 1 + 资产记忆基础）

### 2.1 圈 1：契约与治理核心（必须全部交付）

| # | 能力 | 验收方式 |
|---|---|---|
| C1 | L1 Runtime Interface: 16 方法全部在 proto + Rust trait + Python stub 中定义 | `cargo check` + `python -m pytest stubs/` 通过 |
| C2 | Certifier 只验 12 MUST core，4 Optional 方法运行时不存在不 fail | certifier 对 grid-runtime 和 hermes 都能出 `PASS` 报告 |
| C3 | 结构化 SessionPayload：P1 policy_context / P2 event_context / P3 memory_refs / P4 skill_instructions / P5 user_preferences，带 priority + removable flag | proto message + contract.rs struct + 单元测试验证 "P1 never removable, P5 trimmed first" |
| C4 | Hook handler types: command / http / prompt 三种都有一个真实样例 agent handler 作为占位（Phase 3 再接 OPA） | 每种类型至少一个 fixture hook 在 MVP 的 skill 中被触发并记录到 audit |
| C5 | 14 lifecycle events：proto 和代码里定义全部 14 个 event type；MVP 只**实际触发** 9 个 L1 event + SubagentStop（即使 subagent 不用）+ PostSessionEnd | trait 完整 + telemetry 里可以看到至少 7 种 event type 被写出 |
| C6 | Managed-settings.json 通过 L3 `PUT /v1/policies/managed-hooks` 原子下发给所有 L1 实例 | 能通过 curl/cli 改一个 hook 的 enforce/shadow 模式并在下一次 session 生效 |
| C7 | **Deny-always-wins** 跨 scope：managed hook + skill-frontmatter hook 同时定义，其中一个 deny 能阻断 tool call | 集成测试断言：skill frontmatter 的 PreToolUse deny 被 managed hook 允许时整体仍 deny |
| C8 | 三向握手：L4 → L3 session create → L1 runtime select → L3 hook attach → L1 bootstrap → SessionStart 事件 | 集成测试覆盖完整序列 9 步 |
| C9 | 5 个 L3/L4 REST API Contract 有最小实现：<br/>1. Policy Deployment<br/>2. Intent Gateway<br/>3. Skill Lifecycle<br/>4. Telemetry Ingest<br/>5. Session Control | OpenAPI spec + FastAPI 路由 + 集成测试 |
| C10 | 至少 2 个 T1 runtime 可互换：`grid-runtime`（Rust）+ `claude-code-runtime` 或 `hermes-runtime`（Python） | 同一个 session 指定不同 runtime id 都能执行完 skill |

### 2.2 圈 2 增量：资产与记忆基础

| # | 能力 | 验收方式 |
|---|---|---|
| A1 | L2 Skill Repository：存储 + 4 阶段 promotion pipeline（draft→tested→reviewed→production） + 7 MCP tools（skill_search/read/list_versions/submit_draft/promote/dependencies/usage） | 能把"阈值校准助手"从 draft 提升到 production |
| A2 | Skill 结构：YAML frontmatter 包含 scoped hooks（PreToolUse/PostToolUse/Stop），prose 是真实业务指令，runtime affinity 声明 | SKILL.md 符合 v2.0 §7.3 结构 |
| A3 | L2 Memory Engine Layer 1（Evidence Anchor Store）：append-only immutable + anchor_id / event_id / session_id / data_ref / snapshot_hash / source_system / rule_version / timestamps | 单元测试 + 集成测试 |
| A4 | L2 Memory Engine Layer 2（File-based Memory）：memory_id / scope / category / content / evidence_refs / status（agent_suggested→confirmed→archived） / version | file-based memory CRUD + status 状态机 |
| A5 | L2 Memory Engine Layer 3（Hybrid Retrieval Index）**最小版**：仅 keyword 检索 + 时间衰减加权；semantic 延后到 Phase 2 | memory_search 能返回按相关性排序的结果 |
| A6 | 6 个 Memory MCP tools：memory_search / memory_read / memory_write_anchor / memory_write_file / memory_list / memory_archive | MCP server 暴露 + eaasp-cli-v2 能调用 |
| A7 | L1 → L2 Memory Write Channel：PostToolUse hook 触发 write_anchor；Stop hook 触发 write_file | skill 执行过程中真实写入 anchor 和 memory file |
| A8 | L4 → L2 Context Assembly：session 创建时调用 `POST /api/v1/memory/search`，top-K 结果塞进 SessionPayload P3 memory_refs | 集成测试：第一次 session 写入 memory；第二次 session 创建时 P3 包含引用 |
| A9 | Evidence Chain 闭环：skill 产出的结论 JSON 必须包含 `evidence_anchor_id` 字段；Stop hook 校验该字段不为空 | Stop hook 的 prompt handler 评估 output 包含 anchor_id |
| A10 | L3 Audit Service：接收 async PostToolUse HTTP hook 的 telemetry；存 SQLite | 能查询到 skill 每次 tool call 的记录 |

### 2.3 L5 能力（用 CLI 模拟）

| # | 能力 | 验收方式 |
|---|---|---|
| L1 | `eaasp-cli-v2 session create` 命令（模拟 L5 portal 触发） | 触发三向握手 |
| L2 | `eaasp-cli-v2 session send <msg>` 向 session 发消息并流式打印响应 | SSE chunk 显示 |
| L3 | `eaasp-cli-v2 session show <id>` 展示 session 状态 + 4 卡数据（event/evidence/action/approval），MVP 只渲染 event card 和 evidence pack | 4 卡 data model 可查询，前两卡可打印 |
| L4 | `eaasp-cli-v2 memory search` / `memory read` / `memory list` | 直接打到 L2 MCP |
| L5 | `eaasp-cli-v2 skill list` / `skill run <id>` / `skill promote <id> <stage>` | 直接打到 L2 skill repo |
| L6 | `eaasp-cli-v2 policy deploy` / `policy mode <hook_id> <enforce|shadow>` | 打到 L3 policy API |

---

## 三、前期资产评估表

> **评估原则**：符合 v2.0 语义 → KEEP；部分符合 → REFACTOR；不符合 → SCRAP + 按 v2.0 新建。**不做向后兼容**。

### 3.1 评估结果速览

| 判决 | 数量 | 组件 |
|---|---|---|
| 🟢 **KEEP**（扩展） | 3 | `grid-hook-bridge`, `eaasp-skill-registry`, `eaasp-mcp-orchestrator` |
| 🟡 **REFACTOR**（内部重构到 v2.0） | 5 | `grid-runtime`, `eaasp-certifier`, `hermes-runtime-python`, `claude-code-runtime-python`, `sdk/python` |
| 🔴 **SCRAP**（归档 + 新建） | 3 | `proto/eaasp/runtime/v1/`, `tools/eaasp-governance/`, `tools/eaasp-session-manager/` |
| 🟢 **KEEP**（文档已归档） | — | v1.7/v1.8 设计文档已 → `docs/design/Grid/archive/v1.8/` |
| 🆕 **新建** | 5 | `proto/eaasp/runtime/v2/`, `tools/eaasp-l3-governance/`, `tools/eaasp-l4-orchestration/`, `tools/eaasp-l2-memory-engine/`, `tools/eaasp-cli-v2/` |

### 3.2 详细判决

#### 🟢 KEEP（扩展保留）

**K1. `crates/grid-hook-bridge/`** (Phase BE W2, 11 tests, Rust)
- **保留原因**：HookBridge trait + InProcess + gRPC bidirectional streaming 完全对齐 v2.0。
- **扩展**：
  - 新增 T0 SSE 模式（Phase 4 才用，MVP 占位即可）
  - 新增 credential injection for T2/T3（Phase 3）
  - 新增 shadow mode 支持（hook 配置多一个 mode 字段，bridge 按 mode 决定是否返回 decision）
  - MVP 只加第 3 条

**K2. `tools/eaasp-skill-registry/`** (Rust, port 8081)
- **保留原因**：Git-backed + SQLite + 4 阶段 promotion pipeline 对齐 v2.0 §7.5。
- **扩展**：
  - SKILL.md YAML frontmatter 解析器扩展 scoped hook 声明
  - 7 MCP tools 补齐（当前可能只有部分）
  - Organizational scoping 字段（MVP 阶段先支持 enterprise-wide 单 scope）
  - Usage analytics engine stub

**K3. `tools/eaasp-mcp-orchestrator/`** (Rust, port 8082)
- **保留原因**：MCP server 生命周期管理对齐 v2.0 §7.1。v2.0 把 MCP Registry 的治理职责挪到 L3，但"实际启停 MCP server"依然在 L2。
- **扩展**：
  - MCP security tier classification（read-only / write / critical）
  - tool side-effect 声明
  - connector_grant / connector_revoke 接口
  - MVP 只加第 1 条即可

#### 🟡 REFACTOR（内部按 v2.0 重新装配）

**R1. `crates/grid-runtime/`** (Phase BD, 37 tests, Rust)
- **重构原因**：骨架（session lifecycle, hook activation, telemetry）对，但 proto 契约、SessionPayload 结构、hook 事件数量全部要改。
- **改造点**：
  - `contract.rs` 按新 `proto/eaasp/runtime/v2/runtime.proto` 重写
  - `SessionPayload` 从扁平改为结构化 P1-P5
  - Hook 事件扩展到覆盖 9 个 L1 event（SessionStart/UserPromptSubmit/PreToolUse/PostToolUse/PostToolUseFailure/PermissionRequest/Stop/SubagentStop/PreCompact）
  - 新增 `emit_event` stub 接口（不决 ADR，但留占位）
  - 测试断言按 v2.0 重写，预计保留 60-70% 测试
- **体量估计**：~1-2 周

**R2. `tools/eaasp-certifier/`** (Phase BE W3, 6 tests, Rust)
- **重构原因**：当前应该是验 16 方法全部；v2.0 明确"certifier 只测 12 MUST core"。
- **改造点**：
  - 标注 12 MUST / 4 Optional
  - 按 v2 proto 重写测试契约
  - 增加 blind-box quality 测试 scaffolding（stub，Phase 6 才启用）
- **体量估计**：~3-5 天

**R3. `lang/hermes-runtime-python/`** (Phase BI, 12 tests, Python)
- **重构原因**：Hermes 作为 T2 Aligned 在 v2.0 下概念正确，但 proto stub 和 SessionPayload 要换。
- **改造点**：
  - 重新生成 python stubs from v2 proto
  - SessionPayload 适配 priority blocks
  - Hook 事件映射到 14 lifecycle
  - 保留 `HookBridge monkey-patch hermes.handle_function_call` 技巧
- **体量估计**：~1 周

**R4. `lang/claude-code-runtime-python/`** (Phase BE W4-W6, 55 tests, Python)
- **重构原因**：同 R3。Claude Code 是 T1 Harness（native hooks），adapter 更薄。
- **改造点**：
  - 重新生成 python stubs
  - SessionPayload 适配
  - Skill loader 支持 frontmatter scoped hook 激活
  - `claude-agent-sdk` 子进程 spawn 逻辑保留
- **体量估计**：~1 周

**R5. `sdk/python/`** (Phase BG, 107 tests)
- **重构原因**：SDK 作为"业务工程师 skill 创作工具"在 v2.0 下依然对应 §3.1 Skill Creation Pipeline。但 skill schema 要加新字段。
- **改造点**：
  - `specs/` 里 skill schema 增加 scoped hook frontmatter schema
  - `specs/` 里 skill schema 增加 evidence_anchor 字段（runtime 产出时校验）
  - organizational scope 字段
  - sandbox 模块对齐 v2.0 Sandbox Isolation Tier（先只保留 Standard，Kernel/Hardware 推迟）
- **体量估计**：~1 周

#### 🔴 SCRAP（归档 + 按 v2.0 新建）

**S1. `proto/eaasp/runtime/v1/runtime.proto`** (~120 行)
- **丢弃原因**：v1.3 只有 12 方法、扁平 SessionPayload、无完整 hook event taxonomy。
- **新建**：`proto/eaasp/runtime/v2/runtime.proto`
  - 16 方法（12 MUST + 4 Optional 明确标注）
  - 结构化 SessionPayload（P1-P5 priority blocks + removable flag）
  - 14 lifecycle event type 枚举
  - EmitEvent 占位（ADR 待定）
  - MemoryRef / EventContext / PolicyContext / SkillInstructions 等子 message
  - credential_mode 字段在 capabilities

**S2. `tools/eaasp-governance/`** (Python FastAPI)
- **丢弃原因**：v1.8 的 L3 "thick" 治理 + session 管理混合体，v2.0 的 L3 是 "thin" 治理（§6 明确）。内部语义与 v2.0 L3 分界不同，改不如新写。
- **归档到**：`tools/eaasp-governance/` → `archive/v1.8/tools/eaasp-governance/`
- **新建**：`tools/eaasp-l3-governance/`
  - Policy Engine + Policy Compiler
  - Approval Gates
  - Audit Service
  - Evidence Chain Manager
  - MCP Registry governance 层（实际启停还在 L2 MCP Orchestrator）
  - 暴露 L3 部分的 REST API Contract

**S3. `tools/eaasp-session-manager/`** (Python FastAPI, four-plane)
- **丢弃原因**：v1.8 "four-plane" 是 session-centric；v2.0 L4 是 event-centric（Event Engine 是中心）。
- **归档到**：`tools/eaasp-session-manager/` → `archive/v1.8/tools/eaasp-session-manager/`
- **新建**：`tools/eaasp-l4-orchestration/`
  - Session Orchestrator（MVP 够用）
  - Event 对象 + 三级生命周期（Event Room / Event / Session） — MVP 里 Event Room = 单 session，Event Engine 占位
  - 暴露 L4 部分的 REST API Contract（Intent Gateway 的最小版，即"L4 自己 POST /v1/intents/dispatch 给自己"）
  - L4 → L2 Memory search 调用
  - L4 → L3 三向握手发起
  - Session Event Stream **占位**：用 in-process SQLite append-only 表 + 一个写入接口，接口按 v2.0 §5.5 定义，但后端实现最简

#### 🟢 ARCHIVE（文档）

- v1.7 PDF / v1.8 docx / v1.8 markdown 全部已归档到 `docs/design/Grid/archive/v1.8/`（2026-04-11 迁移完成）
- `docs/plans/2026-04-10-eaasp-m1-phase0-scaffold.md` → 执行计划创建后归档到 `docs/plans/archive/`

### 3.3 新建组件清单

| 新建 | 路径 | 主语言 | 端口 | 取代 |
|---|---|---|---|---|
| v2 proto | `proto/eaasp/runtime/v2/runtime.proto` | protobuf | - | v1 proto |
| L3 治理 | `tools/eaasp-l3-governance/` | Python FastAPI | 8083 | eaasp-governance |
| L4 编排 | `tools/eaasp-l4-orchestration/` | Python FastAPI | 8084 | eaasp-session-manager |
| L2 Memory Engine | `tools/eaasp-l2-memory-engine/` | Python FastAPI + MCP | 8085 | - |
| L5 CLI 模拟 | `tools/eaasp-cli-v2/` | Python typer | - | - |

---

## 四、验证用的真实 Skill：阈值校准助手

### 4.1 Skill 业务描述

**场景**：电网运行中的某类设备（如变压器）有一组运行阈值（温度、负载率、油中溶解气体等），
这些阈值需要根据历史数据定期校准。Agent 的职责：
1. 读取最近一段时间的 SCADA 数据快照（通过 MCP 调 mock 数据源）
2. 基于历史数据给出新的阈值建议
3. 把数据快照作为 evidence anchor 写入 L2
4. 把阈值建议作为 memory file（status: agent_suggested）写入 L2
5. 下次 session（可能是几小时/几天后）被触发时：
   - 从 L2 捞出上次的 memory file
   - 重新读取最新 SCADA 快照
   - 对比新旧数据
   - 要么 confirm 上次的建议（memory file 状态推进到 confirmed）
   - 要么基于新 evidence 提出修订（写新的 memory file，旧的 archive）

### 4.2 为什么这个 skill 能覆盖圈 2 的 95% 能力

| 能力 | 如何被验证 |
|---|---|
| 16 方法契约 | initialize / send / loadSkill / onToolCall / onToolResult / onStop / emitTelemetry / connectMCP / getCapabilities / getState / terminate 都被触发 |
| 结构化 SessionPayload P1-P5 | P1 policy_context 带 `require_evidence_anchor=true`；P3 memory_refs 在第二次 session 填入上次 memory；P4 skill_instructions 是本 skill 的 prose |
| Scoped hook | skill frontmatter 的 PreToolUse 拦截"写 SCADA 命令"类 tool（read-only 通过）；PostToolUse 评估 tool output 包含数据快照；Stop hook 校验输出含 evidence_anchor_id |
| Managed hook | managed-settings 里定义一个全局 audit hook（async PostToolUse HTTP hook）把每次 tool call 写到 L3 audit |
| Deny-always-wins | 故意在 skill frontmatter 允许 "写 SCADA"，managed hook deny → 整体 deny（集成测试断言） |
| Evidence anchor 写入 | `memory_write_anchor` 调用 |
| Memory file 跨 session | 第一次 session 写 file；第二次 session 的 L4 → L2 context assembly 捞出并填入 P3 |
| Context assembly 优先级 | P3 memory_refs 被 runtime 按优先级装入 system prompt，P5 被裁剪时 P3 保留 |

---

## 五、明确不做（Non-Goals）

| # | 不做项 | 原因 | 目标 Phase |
|---|---|---|---|
| N1 | L4 Event Engine 真实 ingest→dedup→cluster 管线 | 需要 electrical topology ontology 和 ADR-V2-003 | Phase 1 |
| N2 | Session Event Stream 真实后端（Kafka/NATS/S3） | 需要 ADR-V2-002 | Phase 1 |
| N3 | L5 Web UI 或 IM bot | CLI 已经能模拟 L5 endpoint | Phase 5 |
| N4 | OPA/Rego policy backend 实际集成 | 需要 ADR-V2-005 | Phase 3 |
| N5 | 5 阶段审批链（Plan→Check→Draft→Approve→Execute）完整流程 | MVP 只验 Plan + 简单 approval | Phase 3 |
| N6 | Deterministic Verifier（rule engine / simulation） | Phase 3 内容 | Phase 3 |
| N7 | A2A Router / ReviewSet / 多智能体并行评审 | Phase 4 | Phase 4 |
| N8 | T0 Managed Harness adapter | Phase 4 | Phase 4 |
| N9 | Sandbox Isolation Tier（gVisor/Kata/Firecracker） | MVP 只用 Standard Docker | Phase 3 |
| N10 | L2 Memory Engine semantic retrieval (vector) | MVP 只做 keyword + time-decay | Phase 2 |
| N11 | L2 Ontology Service | Phase 6 | Phase 6 |
| N12 | Multi-tenancy | Phase 6 | Phase 6 |
| N13 | Hook Shadow Mode 全量实现（effectiveness metrics 采集、assumption health dashboard） | Phase 5 | Phase 5 |
| N14 | Skill Extraction meta-skill | Phase 2 | Phase 2 |
| N15 | Prompt hook 和 agent hook 的真实 LLM 调用 | MVP 用 mock handler 即可 | Phase 2+ |

---

## 六、目标目录结构（MVP 完成时）

```
grid-sandbox/
├── proto/
│   └── eaasp/runtime/
│       ├── v1/          # 【删除】旧 proto
│       └── v2/          # 【新建】runtime.proto（16 方法，P1-P5）
│
├── crates/
│   ├── grid-runtime/        # 🟡 REFACTOR — T1 Harness (Rust)
│   ├── grid-hook-bridge/    # 🟢 KEEP — hook bridge (Rust, sidecar)
│   ├── grid-engine/         # （域内 crate，不动）
│   ├── grid-server/         # （workbench，不动）
│   └── ...
│
├── lang/
│   ├── claude-code-runtime-python/  # 🟡 REFACTOR — T1 Harness (Python)
│   └── hermes-runtime-python/       # 🟡 REFACTOR — T2 Aligned (Python)
│
├── sdk/
│   └── python/              # 🟡 REFACTOR — Skill creation SDK
│
├── tools/
│   ├── eaasp-certifier/            # 🟡 REFACTOR — contract verifier
│   ├── eaasp-skill-registry/       # 🟢 KEEP+EXT — L2 skill repo
│   ├── eaasp-mcp-orchestrator/     # 🟢 KEEP+EXT — L2 MCP server mgr
│   ├── eaasp-l3-governance/        # 🆕 NEW — L3 thin governance
│   ├── eaasp-l4-orchestration/     # 🆕 NEW — L4 orchestration
│   ├── eaasp-l2-memory-engine/     # 🆕 NEW — L2 memory engine
│   ├── eaasp-cli-v2/               # 🆕 NEW — L5 simulator
│   └── archive/v1.8/               # 🔴 归档
│       ├── eaasp-governance/
│       └── eaasp-session-manager/
│
└── docs/
    ├── design/
    │   ├── EAASP/                   # 🆕 新目录，只放有效 v2.0 文档
    │   │   ├── EAASP-Design-Specification-v2.0.docx  # 权威
    │   │   ├── EAASP_v2_0_Platform_Product_Forms.docx
    │   │   ├── EAASP_v2_0_EVOLUTION_PATH.md          # 长期记忆
    │   │   ├── EAASP_v2_0_MVP_SCOPE.md               # 本文档
    │   │   ├── GRID_CURRENT_STATE_2026-04-10.md      # 前期资产审计
    │   │   └── adrs/                                  # ADR 目录（按需创建）
    │   └── Grid/
    │       ├── GRID_PRODUCT_DESIGN.md                 # 域产品设计
    │       ├── GRID_UI_UX_DESIGN.md
    │       ├── GRID_CRATE_SPLIT_DESIGN.md
    │       └── archive/v1.8/                          # v1.7/v1.8 归档
    └── plans/
        ├── 2026-04-11-v2-mvp-phase0-plan.md          # 执行计划
        ├── .checkpoint.json                           # 会话 checkpoint
        └── archive/
            └── 2026-04-10-eaasp-m1-phase0-scaffold.md # 已归档
```

---

## 七、风险与依赖

### 7.1 风险

| # | 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 | v2 proto 设计不到位，Phase 1 `emitEvent()` ADR 解完后又要改 proto | 中 | 中 | proto 里给 `EmitEvent` 留占位 RPC（可选方法），ADR 决定是否转为 MUST |
| R2 | L2 Memory Engine keyword 检索效果差，MVP 的 skill 找不到相关记忆 | 中 | 高 | MVP 用确定性 key（user_id + skill_id + category）精确匹配作为 fallback |
| R3 | `deny-always-wins` 跨 scope 的测试难以覆盖所有组合 | 低 | 中 | 集成测试显式覆盖 4×4 矩阵（managed × skill 的 allow/deny/ask/修改） |
| R4 | 重构 `grid-runtime` 时旧测试大量失败，卡住进度 | 高 | 中 | 先标记旧测试 `#[ignore]`，新测试逐个替换，完成后删除 ignored |
| R5 | Python 和 Rust 两套 runtime 的 proto stub 不同步 | 中 | 中 | Makefile 里 `proto-gen` target 同时生成两边，CI 校验 |

### 7.2 外部依赖

- `proto/eaasp/runtime/v2/` 必须先行，下游 5 个组件都依赖
- `eaasp-l2-memory-engine` 必须在 `sdk/python` 重构之前存在（skill 要引用 memory schema）
- `eaasp-certifier` 重构要等 v2 proto 定稿

---

## 八、验收标准（出场条件）

Phase 0 MVP **完成的唯一标志**是下列端到端测试全部通过：

```bash
# E2E test: 跨 session 记忆累加
make v2-mvp-e2e

# 内部执行：
# 1. 启动 L2 memory engine / L2 skill registry / L3 governance / L4 orchestration
# 2. 启动两个 L1 runtime（grid-runtime + claude-code-runtime）
# 3. eaasp-cli-v2 提交"阈值校准助手" skill
# 4. eaasp-cli-v2 把 skill promote 到 production
# 5. eaasp-cli-v2 deploy managed-settings.json（含一个 audit hook）
# 6. eaasp-cli-v2 session create （runtime=grid-runtime）
# 7. CLI 发消息 "请校准 Transformer-001 的温度阈值"
# 8. 断言：evidence anchor 写入、memory file 写入（status=agent_suggested）
# 9. eaasp-cli-v2 session create （runtime=claude-code-runtime，即换一个 runtime）
# 10. CLI 发消息 "再校准一次 Transformer-001"
# 11. 断言：第二次 session 的 P3 memory_refs 包含上次的 memory_id
# 12. 断言：第二次 session 的输出引用了上次的 anchor
# 13. 断言：L3 audit 里有两次 session 的完整 tool call 记录
# 14. 断言：eaasp-cli-v2 memory search 能找到两次 session 写入的 anchor
# 15. certifier 对两个 runtime 都报 PASS
```

**如果上述 15 条全部通过，Phase 0 完成，进入 Phase 1（Event-driven foundation）。**
