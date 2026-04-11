# EAASP 中长期演进路线图

> ⚠️ **注意**: 本路线图基于 EAASP v1.7 规范。v1.8 后续开发请参考：
> **`docs/design/Grid/EAASP_v1.8_M1_IMPLEMENTATION_BLUEPRINT.md`**（2026-04-10）
> 本文档中的设计决策 KD-1~KD-5 在 v1.8 中仍然有效。

> **版本**: v1.0
> **创建日期**: 2026-04-06
> **基线**: Phase BD W1+W2 完成 @ f8b8e3d（grid-runtime crate + RuntimeContract + GridHarness）
> **权威参考**: `EAASP_-_企业自主智能体支撑平台设计规范_v1.7_.pdf`

---

## 一、现状盘点

### 1.1 已完成

| 产出物 | 状态 | 位置 |
|------|------|------|
| **EAASP 设计规范 v1.7** | 完成 | `docs/design/Grid/EAASP_*.pdf` |
| **runtime.proto v1.0** | 完成，13 方法 | `proto/eaasp/runtime/v1/runtime.proto` |
| **RuntimeContract trait** | 完成，13 方法 Rust 定义 | `crates/grid-runtime/src/contract.rs` |
| **GridHarness** | 完成，13 方法桥接 grid-engine | `crates/grid-runtime/src/harness.rs` |
| **grid-runtime crate** | 骨架完成，main.rs 待实现 gRPC | `crates/grid-runtime/` |
| **tonic-build** | 完成，proto 编译配置 | `crates/grid-runtime/build.rs` |
| **grid-engine** | 完整 agent 引擎 | `crates/grid-engine/` (2476+ tests) |
| **grid-cli / grid-studio** | 完整 CLI+TUI | `crates/grid-cli/` (499 tests) |

### 1.2 待建设

| 组件 | 对应规范章节 | 当前状态 |
|------|---------|--------|
| gRPC server | §6.5 | main.rs 空壳 |
| 遥测 schema + 转换 | §6.4, §8.4 | harness 有基础 emit_telemetry |
| Dockerfile | §11.5 | 不存在 |
| hook.proto | §6.3, §10.4 | 不存在 |
| HookBridge | §6.3 | 不存在 |
| eaasp-certifier | §7.2 | 不存在 |
| claude-code-runtime | §7.1 T1 | 不存在 |
| Enterprise SDK | §13/§14 反模式章节外 | 预设计文档有 |
| L3 治理层 | §4 | 不存在 |
| L2 技能资产层 | §5 | 不存在 |
| L4 人机协作层 | §3 | 不存在 |
| L3/L4 REST API（5 个契约）| §8 | 不存在 |

### 1.3 关键约束

- **EAASP 规范 v1.7 是权威**：所有实现必须与规范对齐，与预设计文档冲突时以规范为准
- **proto 已在 repo 顶层**：`proto/eaasp/runtime/v1/`，全局共享
- **L1 容器是临时的**：按会话创建，会话结束时销毁（规范 §11.5）
- **L1 不可直接访问 L4**：遥测按 L1 → L3 → L4 路径流动（规范 §11.8）
- **L3/L1 通过 hooks 通信**：不是 REST API，是会话创建时嵌入的强制机制（规范 §9）
- **Deny always wins**：任一作用域的 hook 拒绝即阻断（规范 §10.8）

---

## 二、设计决策（已确定）

以下决策来自规范 v1.7 和用户明确确认，后续开发必须遵守。

### KD-1: 运行时 Tier 定义（规范 §7.1 + 用户确认）

| Tier | 定义 | 代表 | Hook 执行 | 适配器厚度 |
|------|------|------|---------|---------|
| **T1 Harness** | 原生 hooks/MCP/skills，完美映射 13 契约 | Grid, Claude Code, Claude Agent SDK | 本地 native | 薄（payload 翻译） |
| **T2 Aligned** | 基本完善，HookBridge 补全缺失 hooks | Aider, Goose, Roo CLI, Cline CLI | HookBridge sidecar | 中（hook 桥接 + skill 翻译） |
| **T3 Framework** | 传统 AI 框架，需完整适配 | LangGraph, CrewAI, Pydantic AI | HookBridge 强制 | 厚（skill→graph/crew 翻译） |

### KD-2: 12 方法运行时接口契约（规范 §6.5）

规范定义 12 MUST 方法。当前实现为 13 方法（多一个 `health`）。

**注意**：规范 §6.5 列出 12 方法，`health` 未列入但在部署运维中必需。保留 `health` 作为扩展。

预设计文档新增了 `DisconnectMcp`、`PauseSession`、`ResumeSession` 三个 RPC——这些在规范 v1.7 中**未定义**。是否采纳需要评估：
- `PauseSession`/`ResumeSession`：规范 §8.5 有 session control 契约（POST/GET/DELETE），暂停/恢复语义合理
- `DisconnectMcp`：合理补充，与 `ConnectMcp` 对称

**决策**：proto v1.2 可以新增这三个方法作为**可选扩展**（方法存在但非 MUST），不违反规范 12 MUST 核心。

### KD-3: L1/L3 通信是 hooks，不是 REST（规范 §9）

L3 在会话创建时通过三方握手（§8.6）将受管 hooks 注入 L1。此后 L3 治理被嵌入 L1 内联执行，**无额外 L3 API 调用**。

- T1 Harness：hooks 直接加载到原生 hook 系统，零网络开销
- T2/T3：hook bridge 作为 sidecar 外部拦截

### KD-4: 容器化运行常态（规范 §11.5）

L1 容器是**临时的**：
- 会话开始时创建 pod
- 会话结束时 pod 销毁
- 状态通过 `get_state` 序列化写回 L4 会话存储
- **L1 数据只会在写入 L2（hooks）或 L3（MCP）后跨会话保留**（规范 §7.3）

这意味着 grid-runtime：
- **无本地持久化**：不依赖容器文件系统
- **SessionState = opaque bytes**：跨 runtime 不兼容是允许的（规范 §6.5 说 SHOULD）
- **terminate 必须 flush**：在退出前冲刷所有异步遥测（规范 §6.5）
- **health 反映可接受新 session**：不只是 provider ping

### KD-5: Enterprise SDK 与 L1 Runtime 是两个东西（用户明确）

- **L1 Runtime**（grid-runtime, claude-code-runtime）= 运行时引擎，EAASP 平台拉起的容器
- **Enterprise SDK**（eaasp-sdk-python/ts）= 企业业务开发者用的多语言 SDK，基于 7 个抽象概念
- 两者通过 gRPC L1 契约连接
- SDK 隐藏 gRPC，开发者只需理解 Agent/Skill/Tool/Policy/Playbook/Session/Message

### KD-6: proto 是全局共享的（规范隐含 + 用户确认）

```
proto/
├── eaasp/
│   ├── runtime/v1/runtime.proto    # 12+方法契约（已有）
│   ├── hook/v1/hook.proto          # HookBridge 协议（待建）
│   └── registry/v1/registry.proto  # L2 注册协议（远期）
```

### KD-7: 演进策略对齐规范 §12 五个阶段

| 规范阶段 | 时间 | 内容 | 我们的对应 |
|--------|------|------|---------|
| 阶段 1 | 1-4 周 | L1 内核 + 基础 hooks | Phase BD + BE 前半段 |
| 阶段 2 | 5-12 周 | L2 技能资产 + L1 抽象 | Phase BF~BG |
| 阶段 3 | 13-20 周 | L3 治理 + L4 基础 | Phase BH~BI |
| 阶段 4 | 21-30 周 | 完整 L4 + L3 成熟 + T3 | Phase BJ+ |
| 阶段 5 | 持续 | 生态扩展 | 长期 |

---

## 三、中长期路线图

### Phase BD（当前）— grid-runtime 完成

**目标**：Grid 作为 T1 Harness 可被 gRPC 调用，具备容器化部署能力。

| Task | 内容 | 产出 | 状态 |
|------|------|------|------|
| W1 | crate + proto + RuntimeContract trait | `contract.rs` + `runtime.proto` | ✅ |
| W2 | GridHarness 13 方法桥接 grid-engine | `harness.rs` | ✅ |
| **W3** | **proto v1.2 升级 + gRPC server** | `service.rs` + `config.rs` + `main.rs` | **NEXT** |
| **W4** | **遥测 schema 标准化** | `telemetry.rs` 对齐规范 §8.4 | pending |
| **W5** | **gRPC 集成测试** | `tests/` 内 gRPC client 端到端 | pending |
| **W6** | **Dockerfile + 容器化** | `Dockerfile` + Makefile | pending |

**W3 关键实现要点**：
- runtime.proto 新增 `DisconnectMcp`、`PauseSession`、`ResumeSession`（可选扩展）
- `service.rs`：tonic service impl，所有 gRPC ↔ Rust 类型转换
- `config.rs`：gRPC 端口、engine 配置、环境变量
- `main.rs`：初始化 GridHarness + 启动 tonic server
- 容器化常态考量：无本地持久化假设，terminate 必须 flush

**W5 说明**：原计划为 eaasp-certifier 独立工具，降级为 crate 内集成测试。certifier 独立工具留到 Phase BE。

**验收标准**：
- `cargo test -p grid-runtime -- --test-threads=1` 全部通过
- `grpcurl` 可调用 13+ 方法
- Docker 镜像可构建并启动

---

### Phase BE — EAASP 协议层 + 第二个 L1 Runtime

**目标**：建立全局协议、HookBridge、eaasp-certifier、第一个非 Rust L1 Runtime。

**对应规范阶段 1（L1 内核 + 基础 hooks）的收尾。**

| Wave | 内容 | 依赖 |
|------|------|------|
| **W1** | proto 全局化：`hook.proto` 新建 + `runtime.proto` 从 crate 内确认全局一致 | 无 |
| **W2** | HookBridge Rust 核心（`lang/hook-bridge/`）：`EvaluateHook` + `ReportHookDecision` | W1 |
| **W3** | eaasp-certifier（`tools/eaasp-certifier/`）：Mock L3 + 13 方法自动化验证 | W1 |
| **W4** | claude-code-runtime Python W1：项目骨架 + claude-agent-sdk 封装 + gRPC service | W1 |
| **W5** | claude-code-runtime Python W2：本地 hook 执行 + 遥测 + Skill System | W4 |
| **W6** | 集成验证：certifier 验证 grid-runtime + claude-code-runtime | W2, W3, W5 |

**关键技术决策**：

1. **claude-code-runtime 用 Python 先做**（SDK Python v0.1.56 更成熟，grpcio 成熟，国内 AI 生态更强）
2. **HookBridge 用 Rust 独立实现**（跨语言复用，性能最优）
3. **certifier 使用 `L3Client` trait**（Mock → Simulation → Real 透明替换）
4. **claude-code-runtime 是 T1 Harness**——本地执行 hooks，不需要 HookBridge 做 hook 执行；HookBridge 仅用于端到端测试

**目录结构新增**：
```
lang/
├── claude-code-runtime-python/     # T1 Python Harness
│   ├── pyproject.toml
│   └── src/
│       ├── __main__.py             # gRPC server 入口
│       ├── sdk_wrapper.py          # claude-agent-sdk 封装
│       ├── grpc_service.py         # 13 方法 gRPC service
│       ├── hook_executor.py        # 本地 hook 执行（T1）
│       ├── telemetry.py            # SDK callback → TelemetryEvent
│       ├── state_manager.py        # get_state / restore_state
│       └── mapper.py               # SDK event ↔ ResponseChunk
│
├── hook-bridge/                    # HookBridge Rust 核心
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── hook_executor.rs        # 策略评估引擎
│       └── policy_store.rs         # 策略存储
│
tools/
├── eaasp-certifier/                # 契约验证 + Mock L3
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                 # CLI: verify / mock-l3
│       ├── verifier.rs             # 13 方法逐一验证
│       ├── mock_l3.rs              # Mock L3
│       └── l3_client.rs            # L3Client trait
```

**验收标准**：
- `eaasp-certifier verify grpc://localhost:50051` 验证 grid-runtime 通过
- `eaasp-certifier verify grpc://localhost:50052` 验证 claude-code-runtime 通过
- HookBridge 能接收 T2/T3 的 hook 评估请求并返回决策
- 运行时池中有 2 个 T1 Harness

---

### Phase BF — L2 技能资产层 + L1 抽象机制 ✅

**对应规范阶段 2（第 5-12 周）。**

**状态**: 完成（7/7 tasks, 30 new tests @ 59bb58e）

**目标**：构建 L2 统一资产层（Skill Registry + MCP Orchestrator），L1 抽象机制（运行时选择器 + 盲盒对比）。

**实际产出**：

| 组件 | 内容 | 位置 | 测试 |
|------|------|------|------|
| SessionPayload L2 字段 | proto v1.3, skill_ids/skill_registry_url | `proto/eaasp/runtime/v1/runtime.proto` | 7 |
| L2 Skill Registry | REST API + SQLite + Git 追溯 | `tools/eaasp-skill-registry/` | 10 |
| L2 MCP Orchestrator | YAML 配置 + Shared 子进程管理 | `tools/eaasp-mcp-orchestrator/` | 4 |
| L1-L2 集成 | GridHarness initialize 从 L2 REST 拉取 | `crates/grid-runtime/src/l2_client.rs` | 4 |
| RuntimePool + Selector | Mock L3 运行时池管理 + 选择策略 | `tools/eaasp-certifier/src/` | 5 |
| 盲盒对比 | 并行执行 + 匿名展示 + 用户评分 | `tools/eaasp-certifier/src/blindbox.rs` | 3 |
| 设计文档 | L2 资产层架构设计 | `docs/design/Grid/EAASP_L2_ASSET_LAYER_DESIGN.md` | — |

**设计决策**（BF-KD1~KD12）：详见 `EAASP_L2_ASSET_LAYER_DESIGN.md`

**Deferred**: BF-D1~D10（Git 版本追溯集成、PerSession/OnDemand 模式、RBAC、ELO 统计等）

**原计划中以下功能调整为 Deferred**：
- L2 访问控制（RBAC）→ BF-D6，前置 L3 认证体系
- L1 适配器注册表 → BH L3 治理层
- L1 遥测采集器统一 schema → 已在 BD 完成基础版

---

### Phase BG — Enterprise SDK 基石 ✅

**对应规范阶段 2（第 5-12 周），SDK 部分。**

**状态**: 完成（6/6 tasks, 107 new tests @ ea0780c）

**目标**：构建 EAASP Enterprise SDK 的基石层（S1），让企业开发者可以创作、校验、推演 Skill。

**实际产出**：

| 组件 | 内容 | 位置 | 测试 |
|------|------|------|------|
| JSON Schema 规范 | 7 个抽象概念跨语言契约 | `sdk/specs/` | — |
| Pydantic 模型 | 7 个模型 + Skill SKILL.md 双向序列化 | `sdk/python/src/eaasp/models/` | 27 |
| 创作工具链 | parser + validator(8规则) + scaffold(4模板) + hook_builder | `sdk/python/src/eaasp/authoring/` | 21 |
| GridCliSandbox | subprocess 调用 grid binary | `sdk/python/src/eaasp/sandbox/grid_cli.py` | 13 |
| RuntimeSandbox | gRPC 直连 L1 Runtime | `sdk/python/src/eaasp/sandbox/runtime.py` | 28 |
| MultiRuntimeSandbox | 并行对比 + ConsistencyReport | `sdk/python/src/eaasp/sandbox/multi_runtime.py` | (含 W4) |
| CLI | init/validate/test/compare/submit 5 命令 | `sdk/python/src/eaasp/cli/` | 18 |
| L2 客户端 | SkillRegistryClient (submit_draft) | `sdk/python/src/eaasp/client/` | (含 W5) |
| HR 入职示例 | workflow-skill + PII hook + test cases | `sdk/examples/hr-onboarding/` | — |
| 设计文档 | SDK 演进蓝图 | `docs/design/Grid/EAASP_SDK_DESIGN.md` | — |

**关键设计决策**（BG-KD1~KD10）：详见 `EAASP_SDK_DESIGN.md`

**Deferred**: BG-D1~D10（Policy DSL, Playbook DSL, TypeScript SDK, GridServerSandbox, PlatformSandbox 等）

**7 个抽象概念**：Agent / Skill / Tool / Policy / Playbook / Session / Message

---

### Phase BH — L3 治理层 + L4 基础

**对应规范阶段 3（第 13-20 周）。**

**目标**：L3 策略引擎上线，managed-settings.json 真实部署，L4 会话管理器 + 管理控制台基础。

| 组件 | 内容 | 规范章节 |
|------|------|---------|
| L3 策略引擎 | RBAC 规则 + 条件策略 | §4.1 |
| L3 策略编译器 | 业务规则 → hook JSON + 脚本 | §10.2 |
| L3 审批闸门 | PreToolUse/PostToolUse/Stop 阻断 | §4.2 |
| L3 审计服务 | 接收 L1 遥测，结构化存储 | §4.3 |
| L3 Hook 部署服务 | managed-settings.json 原子分发 | §10.3 |
| L3 MCP 注册表 | 连接器生命周期管理 | §4.4 |
| L3 意图网关 | 事件翻译 + 路由 | §8.2 |
| L4 会话管理器 | 三方握手、L1 生命周期 | §3.3, §8.6 |
| L4 管理控制台 | 策略编辑器、运行时池管理 | §3.1 |
| L4 员工门户 | 多渠道接入 | §3.1 |
| 5 个 L3/L4 REST API 契约 | 策略部署、意图网关、技能生命周期、遥测采集、会话控制 | §8 |

**关键里程碑**：
- L3 治理上线，受管 hooks 在所有运行时上强制执行
- hook bridge 在 T2 智能体上验证通过
- L4 管理控制台可编辑策略并部署
- 5 个 API 契约全部部署

---

### Phase BI — 完整 L4 + T2 运行时扩展

**对应规范阶段 4 前半段。**

| 组件 | 内容 |
|------|------|
| L4 事件总线 | Webhook/Cron/CDC 触发 |
| L4 可观测性枢纽 | 仪表盘、告警、成本追踪 |
| L4 流程设计器 | 可视化 skill 编辑 |
| L4 审批收件箱 | 人工审批流 |
| L1 hook bridge 生产化 | T2 sidecar 稳定运行 |
| T2 Aider 适配器 | 第一个 T2 加入运行时池 |
| T2 Goose 适配器 | 第二个 T2 |
| 成本治理 | SessionStart hook 配额强制 |

---

### Phase BJ+ — L3 成熟 + T3 框架 + 生态

**对应规范阶段 4 后半段 + 阶段 5。**

| 组件 | 内容 |
|------|------|
| T3 LangGraph 适配器 | 厚适配器 + skill → graph 翻译 |
| T3 CrewAI 适配器 | 角色模型适配 |
| 多租户 | 企业→BU→部门→团队组织层级 |
| 运行时认证流水线 | 自动化接口契约测试 + 盲盒基准 |
| 技能市场 | 跨组织 skill 共享 |
| Enterprise SDK Java/Go/C# | 更多语言 |

---

## 四、Proto 演进计划

### runtime.proto 版本轨迹

| 版本 | 方法数 | 变更 |
|------|--------|------|
| **v1.0**（当前）| 13 RPC | 初始版本，13 方法 |
| **v1.2**（BD W3）| 16 RPC | +DisconnectMcp, +PauseSession, +ResumeSession |
| **v2.0**（BH）| 16+ RPC | 可能新增 L3 三方握手相关字段 |

### 新增 proto 文件

| 文件 | 阶段 | 用途 |
|------|------|------|
| `hook.proto` | BE W1 | HookBridge ↔ L1 协议 |
| `registry.proto` | BF | L2 MCP Server 注册协议 |

---

## 五、Deferred Items 汇总

### Phase BD Deferred

| ID | 内容 | 前置条件 |
|----|------|---------|
| BD-D1 | grid-hook-bridge crate（T2/3 sidecar） | BE HookBridge 核心完成 |
| BD-D2 | RuntimeSelector + AdapterRegistry | BF L1 抽象机制 |
| BD-D3 | 盲盒对比（dual output + vote） | BF 运行时池 2+ runtime |
| BD-D4 | managed-settings.json 分发机制 | BH L3 治理层 |
| BD-D5 | SessionPayload 组织层级（企业→BU→部门→团队） | BH L4 多租户 |

### Phase BE Deferred

| ID | 内容 | 前置条件 |
|----|------|---------|
| BE-D0 | L2 Registry 存储技术与 MCP Server 部署模型确认 | 用户确认 |
| BE-D1 | proto 从 crate 内迁移到 repo 顶层（已完成） | — |
| BE-D2 | L2 Registry MCP Server 注册协议 | BF |
| BE-D3 | HookBridge ↔ L3 双向流策略下发 | BH L3 真实部署 |
| BE-D4 | Pause/Resume 跨 runtime 验证 | certifier mock L4 |
| BE-D5 | buf breaking CI 配置 | proto 重构完成 |
| BE-D6 | T2 第一试点 = LangGraph | BI HookBridge 生产化 |
| BE-D7 | L1 ↔ L2 通讯协议确认（MCP stdio vs gRPC） | BF L2 建设 |

---

## 六、验收标准总表

| 阶段 | 验收标准 |
|------|---------|
| **BD** | grid-runtime gRPC server 可启动，13+ 方法可调用，Docker 镜像可构建 |
| **BE** | certifier 验证 grid-runtime + claude-code-runtime 通过；HookBridge 能评估 hook 请求 |
| **BF** | ✅ L2 Skill Registry REST API 上线；MCP Orchestrator Shared 模式；Grid/CC 盲盒对比可运行；30 new tests |
| **BG** | ✅ Python SDK S1 基石完成；107 tests；企业开发者可通过 SDK 创作/校验/推演 Skill |
| **BH** | L3 治理上线；managed hooks 跨运行时强制执行；5 个 API 契约部署；L4 门户+控制台 |
| **BI** | T2 Aider/Goose 加入运行时池；自动化工作流可无人值守执行；成本治理生效 |
| **BJ+** | T3 LangGraph 加入；运行时池 5+ 智能体；多租户部署；技能市场上线 |

---

## 七、开放问题（待用户确认后推进）

| # | 问题 | 影响阶段 | 选项 |
|---|------|---------|------|
| OQ1 | L2 Registry 存储技术 | BF | Git/YAML vs PostgreSQL vs Nacos |
| OQ2 | MCP Server 认证方式 | BF | 网络隔离 vs mcp-oidc |
| OQ3 | SKILL.md → L1 格式转换时机 | BE | Initialize 预转换 vs Send 按需 |
| OQ4 | L2 MCP Server 部署位置 | BF | 与 L3 捆绑 vs 独立部署区域 |
| OQ5 | Pause/Resume 对 L2 MCP 连接的影响 | BE | 保持连接 vs 重新连接 |
| OQ6 | `get_state` 是否需要跨 runtime 统一 schema | BE | 接受不兼容（opaque bytes）vs BaseState 规范 |

---

## 八、即时行动项（BD W3 启动前）

1. **更新 `runtime.proto` 到 v1.2**：新增 DisconnectMcp/PauseSession/ResumeSession
2. **更新 `contract.rs`**：RuntimeContract trait 加 3 个方法
3. **更新 `harness.rs`**：GridHarness 实现 3 个新方法（stub）
4. **实现 W3**：`service.rs` + `config.rs` + 更新 `main.rs`

然后依次完成 W4（遥测）→ W5（集成测试）→ W6（Dockerfile）。
