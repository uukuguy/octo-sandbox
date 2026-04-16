# L1 Runtime 能力对比矩阵

## 概述

本文档汇总 EAASP v2.0 体系中四个 L1 Runtime 实现的核心能力、合约测试通过情况及已知限制。矩阵数据来源于各 runtime 的 `GetCapabilities` gRPC 实现、合约测试结果（Phase 2 S0 checkpoint @ `cfda161`/`fd1abbf`）及 Phase 2.5 开发记录。目标读者为需要选型或评估 L1 Runtime 适配成本的开发者。

---

## 主能力对比矩阵

| 能力维度 | grid-runtime | claude-code-runtime | eaasp-goose-runtime | nanobot-runtime |
|----------|:------------:|:-------------------:|:-------------------:|:---------------:|
| **实现语言** | Rust | Python | Rust | Python |
| **Provider 支持** | OpenAI-compat / OpenRouter（多模型） | Anthropic only（单模型） | Goose 内置（可对接多种） | OAI-compat（3 个 env vars） |
| **Native MCP 支持** | ✅ | ✅ | ✅ Goose 原生 | ⚠️ 存根 |
| **Native Hook 支持（ADR-V2-006）** | ✅ | ✅ | ❌ 存根 | ❌ 存根 |
| **Native Skills 支持** | ✅ | ✅ | ❌ | ❌ |
| **tool_choice=Required 支持** | ✅ 动态探测（main.rs Eager probe） | ✅（Anthropic SDK 原生） | ⚠️ 依赖 Goose 内部实现 | ⚠️ 依赖下游 OAI provider |
| **PreCompact Hook（ADR-V2-018）** | ✅（Phase 2 S3.T1） | ✅（Phase 2 S3.T1） | ❌ | ❌ |
| **部署模型（ADR-V2-019）** | shared（默认）/ per_session（env toggle） | per_session | shared | shared |
| **Runtime Tier** | aligned / harness | aligned | framework | aligned |
| **合约测试 v1 通过情况** | 13 / 22 PASS | 18 / 17 PASS¹ | 10 SKIP（需 goose 二进制） | 配置已 wire，未完整 green run |

> ¹ claude-code-runtime 比预期多通过 1 个测试（@ `fd1abbf`）。

**图例**：✅ = 完整实现；⚠️ = 部分支持或依赖外部条件；❌ = 未实现（Phase 2.5 scope 外）；存根 = 接口存在但无真实逻辑

---

## 各 Runtime 简介

### grid-runtime（主力 Runtime）

`crates/eaasp-goose-runtime/` 之外的核心 Rust 实现，承载 EAASP harness 层完整能力。原生支持 Hook（ADR-V2-006）、Scoped Hook Executor（ADR-V2-006 §2-§10）、Stop Hooks（Phase 2 S3.T4）、PreCompact Hook（ADR-V2-018）及 Hybrid Memory（S2 全系列）。`main.rs L103-118` 实现 tool_choice Eager probe，动态探测 provider 能力并记录到 `ProviderCapabilityMatrix`，适用于需要多模型支持和完整企业能力的生产部署。

### claude-code-runtime（样板 Runtime）

`lang/claude-code-runtime-python/` Python 实现，深度集成 Anthropic SDK，上下文压缩（compaction）由 SDK 原生处理，PreCompact Hook 通过 ADR-V2-018 协议接入。合约测试通过率最高（18/17），适用于 Anthropic-only 场景及作为其他 Python runtime 的实现参考。部署模型为 per_session，每个 gRPC 会话独立生命周期。

### eaasp-goose-runtime（对比 Runtime，Phase 2.5 开发中）

`crates/eaasp-goose-runtime/` 新增 Rust crate（Phase 2.5 W1），通过 ACP/stdio subprocess 方式集成 Block Protocol 的 Goose 框架。MCP 能力为 Goose 原生提供，Hook 注入通过 `eaasp-scoped-hook-mcp` stdio 代理在 `tools/call` 层拦截实现（ADR-V2-006 §2/§3 Method A）。当前 `SendRequest` 尚未完整对接（Phase 2.5 W1.T3 stub），依赖本地安装 `goose` 二进制，CI 环境默认 skip。适用于评估 Goose 生态与 EAASP hook 体系的兼容性。

### nanobot-runtime（样板 Runtime，Phase 2.5 开发中）

`lang/nanobot-runtime-python/` Python 实现（Phase 2.5 W2），定位为 OAI-compat provider 接入样板。使用 3 个标准 env vars（`NANOBOT_OPENAI_API_BASE` / `NANOBOT_OPENAI_API_KEY` / `NANOBOT_OPENAI_MODEL`），契合 OpenRouter 等兼容接口场景。`ConnectMCP` 接受调用但内部无真实 MCP wiring；Hook 相关方法均为存根。合约测试配置已在 Phase 2.5 W2.T5 wire（@ `602c1dc`），完整 green run 待后续 sprint 完成。

---

## 合约测试覆盖情况

合约测试套件定义于 `tests/contract/`，基于 Phase 2 S0.T4-T6（@ `cfda161`/`fd1abbf`/`d17cdb8`）实现，包含 `hook_probe.py`、mock server、probe-skill 三类核心 fixture。

| Runtime | 测试状态 | PASS | SKIP | XFAIL / 备注 |
|---------|---------|:----:|:----:|:------------|
| grid-runtime | 部分通过 | 13 | — | 9 个未通过；D140: `HookContext::with_event` 缺失导致 hook_envelope 验证失败（3-5 LOC 修复） |
| claude-code-runtime | 高通过率 | 18 | — | 比预期 +1；D136 grid hook-not-fired（mock OpenAI tool_calls shape 问题，与 claude-code 无关） |
| eaasp-goose-runtime | 本地跳过 | 0 | 10 | dry-run 0.15s；需本地安装 `goose` 二进制（GOOSE_BIN env）；D141: goose 未在 CI 安装（P1 defer） |
| nanobot-runtime | 配置阶段 | — | — | W2.T5 @ `602c1dc` 已 wire 合约测试 config；完整 green run 为 Phase 2.5 后续任务 |

---

## 已知限制与未来工作

### 当前 Stub 项

| Runtime | Stub 项 | 影响 |
|---------|---------|------|
| eaasp-goose-runtime | `SendRequest` 未完整实现 | 无法真实执行 agent turn，仅能测试 gRPC 接口连通性 |
| eaasp-goose-runtime | Hook 注入为 MCP 代理层（非 native） | Hook 触发链路经过额外 stdio 跳转，延迟较高 |
| nanobot-runtime | `ConnectMCP` 无真实 wiring | MCP 工具不可用于 agent 执行 |
| nanobot-runtime | Hook 方法（PreToolUse/PostToolUse/Stop）均为存根 | 无法触发 EAASP scoped hook 逻辑 |
| grid-runtime | `HookContext::to_json/to_env_vars` 预 ADR-V2-006 schema | 缺少 `event`/`skill_id` 等字段（D120，Phase 2.5 cert 前必须修复） |
| grid-runtime / claude-code-runtime | 未完整实现 `EAASP_DEPLOYMENT_MODE` env toggle | 部署模式切换依赖手动配置（D142/D143） |

### Deferred 任务参考

- **D142**：grid-runtime 补充 `EAASP_DEPLOYMENT_MODE` env 合规（ADR-V2-019），优先级 P2，S3 CI 批次
- **D143**：claude-code-runtime 补充 `EAASP_DEPLOYMENT_MODE` env 合规（ADR-V2-019），优先级 P2，S3 CI 批次
- **D140**：grid-engine `HookContext::with_event` 调用缺失（3-5 LOC），修复后可解锁 grid 合约测试 0→5 hook_envelope PASS
- **D141**：goose 二进制未在 CI 安装，P1 defer，阻塞 eaasp-goose-runtime W1.T3/T4/T5

### 未来工作方向

1. **Phase 2.5 W1 完成**：eaasp-goose-runtime `SendRequest` 完整实现，解除 stub 状态
2. **Phase 2.5 W2 完成**：nanobot-runtime MCP wiring + Hook 实现，合约测试 green run
3. **D120 修复**：grid-runtime `HookContext` 字段补全，满足 ADR-V2-006 §2 envelope schema
4. **D142/D143 实现**：两个主力 runtime 的 `EAASP_DEPLOYMENT_MODE` env 合规，支持 ADR-V2-019 per_session toggle
5. **Phase 3 扩展**：pydantic-ai runtime + claw-code + ccb 内部对比 runtime（ADR-V2-017 三轨规划）

---

## 参考文档

| 文档 | 路径 |
|------|------|
| ADR-V2-006: Hook Envelope Contract | `docs/design/EAASP/adrs/ADR-V2-006-*.md` |
| ADR-V2-017: L1 Runtime 生态策略 | `docs/design/EAASP/adrs/ADR-V2-017-l1-runtime-ecosystem-strategy.md` |
| ADR-V2-018: PreCompact Hook | `docs/design/EAASP/adrs/ADR-V2-018-*.md` |
| ADR-V2-019: L1 Runtime 部署容器化 | `docs/design/EAASP/adrs/ADR-V2-019-*.md` |
| L1 Runtime 适配指南 | `docs/design/EAASP/L1_RUNTIME_ADAPTATION_GUIDE.md` |
| L1 Runtime 候选分析 | `docs/design/EAASP/L1_RUNTIME_CANDIDATE_ANALYSIS.md` |
| Provider Capability Matrix | `docs/design/EAASP/PROVIDER_CAPABILITY_MATRIX.md` |
| Deferred Ledger | `docs/design/EAASP/DEFERRED_LEDGER.md` |
| Phase 2.5 Design | `docs/design/PHASE_2_5_DESIGN.md` |
