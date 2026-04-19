---
id: ADR-V2-017
title: "L1 Runtime 生态策略（主力 + 样板 + 对比 三轨）"
type: strategy
status: Accepted
date: 2026-04-14
phase: "Phase 2 — Memory and Evidence (策略级 ADR，影响 Phase 2.5+ 全部 L1 runtime 演进)"
author: "Jiangwen Su"
supersedes: []
superseded_by: null
deprecated_at: null
deprecated_reason: null
enforcement:
  level: strategic
  trace: []
  review_checklist: "docs/design/EAASP/adrs/ADR-V2-017-l1-runtime-ecosystem-strategy.md"
affected_modules:
  - "crates/grid-runtime/"
  - "crates/eaasp-goose-runtime/"
  - "crates/eaasp-claw-code-runtime/"
  - "lang/claude-code-runtime-python/"
  - "lang/hermes-runtime-python/"
  - "lang/nanobot-runtime-python/"
  - "lang/pydantic-ai-runtime-python/"
  - "lang/ccb-runtime-ts/"
  - "tests/contract/"
  - "Makefile"
related: [ADR-V2-004, ADR-V2-016]
---

# ADR-V2-017 — L1 Runtime 生态策略（主力 + 样板 + 对比 三轨）

**Status:** Accepted
**Date:** 2026-04-14
**Phase:** Phase 2 — Memory and Evidence (策略级 ADR，影响 Phase 2.5+ 全部 L1 runtime 演进)
**Author:** Jiangwen Su
**Related:** ADR-V2-004 (L4 real gRPC binding), ADR-V2-016 (agent loop generic principle)

---

## Context / 背景

Phase 1 E2E 验证暴露 `hermes-runtime-python` 多处系统性问题：stdio MCP 缺失 (D88)、fork+grpc+asyncio 三层耦合导致 `F ev_epoll1_linux.cc:1102` 致命崩溃、auxiliary_client 无法被 L4 注入、session 生命周期失控、monkey-patch 叠加混乱。**修复预估 4-7 人日**，且由于 hermes-agent 是黑盒第三方依赖，修复后稳定性仍不可保障。

同时，调研 3 个开源 Claude Code 衍生项目 (`goose`/`CCB`/`claw-code`) 发现它们在 MCP、Provider 注入、多步 agent loop、hook 系统等能力上**部分强于 grid-engine 当前实现**。grid-engine 是 2026-02 新起的自研项目，成长过程中需要**持续有对比的强手相伴**，而非孤立迭代。

EAASP v2.0 的核心价值是 **L1 契约开放**——任何满足 `eaasp.runtime.v2` proto 16 方法契约 + 7 种 event_type + MCP bridge + skill 执行的 runtime 都可以接入。我方不应只维护一个 runtime，而应：

1. 提供各技术栈 / 各设计范式的**样板 runtime**，作为社区接入契约的参考实现
2. 持续引入外部**对比 runtime**，用源码对标 grid-engine 设计缺陷，借鉴强手做法

---

## Decision / 决策

采用 **三轨 L1 runtime 生态策略**：

### 轨道 1 — 主力 (我方核心产品)

| Runtime | 语言 | 角色 |
|---------|------|------|
| **grid-runtime** | Rust | 基于 grid-engine，我方持续投入的核心 L1 实现，功能最全 |

### 轨道 2 — 样板 (我方各技术栈示范实现)

给社区看"怎么接入 L1 契约"，每个样板突出一种技术栈/设计范式：

| 样板 | 语言/框架 | 典型价值 | 状态 |
|------|----------|---------|------|
| **hermes-runtime** (T1) | Python / hermes-agent | 外挂 agent framework 接入样板 | ⏸️ **暂停**（系统性问题多，详见本 ADR "hermes 冻结") |
| **nanobot-runtime** (T2) | Python / nanobot | 轻量 Python runtime 接入样板 | 🆕 Phase 2.5 W2 |
| **pydantic-ai-runtime** (T3) | Python / pydantic-ai | 类型化 Python + Provider 抽象 样板 | 🆕 Phase 3 |
| **claude-code-runtime** | Python / Anthropic SDK | 官方 SDK wrap 样板 | ✅ 已有 |

### 轨道 3 — 对比 (外部开源 CC 项目 wrap)

**不是产品**，是**长期对比基础设施**。grid-runtime 能力落后时，对比能区分是"模型问题"还是"grid-engine 设计问题"，源码作学习材料：

| 对比 runtime | 语言 | 突出能力 | 状态 |
|--------------|------|---------|------|
| **goose-runtime** | Rust / Block 官方 | MCP 原生 / 50+ provider / 子进程隔离 | 🆕 Phase 2.5 W1（对比首选）|
| **claw-code-runtime** | Rust / UltraWorkers | 模块化 9-crate / ProviderRuntimeClient 抽象 | 🆕 Phase 3 |
| **ccb-runtime** | TS / Bun | 13 hook 完整 / Claude Code 反编译还原 | 🆕 Phase 3（**仅内部对比用，不商用，不法律评估阻塞**）|

---

### hermes 冻结决策

`lang/hermes-runtime-python/` **立即冻结**，不再投入修复 D88 / fork 崩溃 / auxiliary 注入 / monkey-patch 等 5 个系统性问题。

- **保留代码**（供 Phase 2.5 样板实装对比参考）
- **Makefile** 为所有 `hermes-runtime-*` 目标打印 deprecation 警告，指向本 ADR
- **文档** 在 `EVOLUTION_PATH.md` / `CLAUDE.md` 明示 hermes 为"历史样板，不推荐新部署"
- **Phase 2 S1.T2** 从"修 hermes bug"重定义为"hermes 冻结 + 启动 goose 对比实装"，完整工作挪到 Phase 2.5

### Phase 2.5 新阶段

在 Phase 2 S2 (L2 Memory Engine) 完成后**插入 Phase 2.5 "Runtime 生态首批落地"**，不阻塞 Phase 2 S1 其它小修 (T3/T4/T5/T6/T7) 和 S2 Memory 增强。

Phase 2.5 交付：
1. 共享契约测试套件 `tests/contract/`（任何新 runtime 接入跑同一套）
2. `docs/design/EAASP/L1_RUNTIME_ADAPTATION_GUIDE.md` 通用接入指南
3. `docs/design/EAASP/L1_RUNTIME_COMPARISON_MATRIX.md` 对比能力矩阵
4. **goose-runtime** 完整 16 方法实装（Phase 2.5 W1）
5. **nanobot-runtime** T2 样板实装（Phase 2.5 W2）

### 每个 runtime 的统一交付基线

```
<runtime>/
├── proto/ → symlink eaasp.runtime.v2
├── src/
│   ├── service.{rs,py}      # 16 方法实现
│   ├── mcp_bridge           # stdio + SSE 双支持
│   ├── event_mapper         # 本 runtime 事件 → EAASP 7 种 event_type
│   ├── provider_inject      # OPENAI_BASE_URL 贯通 (从 hermes 教训学到)
│   └── session_lifecycle    # Initialize/Terminate 对齐
├── tests/
│   └── contract_test.*      # 跑通用契约测试套件
└── README.md                # 5 min 上手 + 契约适配笔记
```

---

## Consequences / 后果

### Positive

- **契约驱动**：L1 开放生态得到真实验证，不止于纸面 proto。通用契约测试集跑绿 = 接入合格。
- **grid-engine 成长有对标**：每个新能力落地前先看对比 runtime 有没有更好解，避免闭门造车。
- **样板多样性**：社区接入 L1 有 3+ 种技术栈参考（Rust / Python 轻量 / Python 类型化 / Python SDK），不绑死某种设计。
- **hermes 停损**：避免继续在黑盒 hermes-agent + fork+grpc 死穴上花时间。
- **技术栈统一方向**：主力 + 对比均 Rust（goose/claw-code），减少 Python 运行时依赖面积，长期运维成本下降。

### Negative

- **实装工作量翻倍**：Phase 2.5 需一次性做 goose + nanobot 两个新 runtime (~8-12 人日)，加上通用契约测试套件 (~3 人日) 和文档 (~2 人日)。
- **维护面扩大**：未来同时维护 grid-runtime + nanobot-runtime + pydantic-ai-runtime + goose-runtime 四条线，每次 proto 变更都要同步。
- **样板与对比的边界需清晰**：ccb-runtime **仅限内部对比学习，不用于商业发布**，这一限制须在对比矩阵和 README 中明示。
- **hermes 冻结会留下历史代码**：不删除、不修复，会让新贡献者困惑。需要 README 明示 DEPRECATED 状态。

### Risks

- **goose wrap 成本低估**：goose 是完整 agent framework (UI + recipe + CLI)，我们只用 runtime core，gRPC bridge 层设计不当会引入耦合。缓解：W1 第一天写架构骨架 + spike 验证，再决定 commitment。
- **对比 matrix 流于形式**：如果 matrix 只是填表不指导决策，对比 runtime 变摆设。缓解：ADR-V2-016 以后每次 grid-engine loop 改动必须在 matrix 勾选"是否参考对比 runtime 做法"。
- **nanobot / pydantic-ai 源码需提供**：当前 `3th-party/` 无此两项，需确认获取路径。

---

## Affected Modules / 影响范围

| Module | Impact |
|--------|--------|
| `lang/hermes-runtime-python/` | 冻结，Makefile 加 deprecation 警告；不删除代码 |
| `Makefile` | `hermes-runtime-*` 目标打印 deprecation 信息；新增 `runtime-test-all` / `runtime-matrix` 目标 (Phase 2.5) |
| `docs/design/EAASP/EAASP_v2_0_EVOLUTION_PATH.md` | 新增 Phase 2.5 章节；hermes 标注 DEPRECATED |
| `docs/design/EAASP/L1_RUNTIME_ADAPTATION_GUIDE.md` | **新建** (Phase 2.5 W1) |
| `docs/design/EAASP/L1_RUNTIME_COMPARISON_MATRIX.md` | **新建** (Phase 2.5 W1) |
| `tests/contract/` | **新建通用契约测试集** (Phase 2.5 W1) |
| `crates/eaasp-goose-runtime/` | **新建** (Phase 2.5 W1) |
| `lang/nanobot-runtime-python/` | **新建** (Phase 2.5 W2) |
| `lang/pydantic-ai-runtime-python/` | **新建** (Phase 3) |
| `crates/eaasp-claw-code-runtime/` | **新建** (Phase 3) |
| `lang/eaasp-ccb-runtime-ts/` | **新建** (Phase 3，内部对比用) |
| `docs/plans/2026-04-14-v2-phase2-plan.md` | S1.T2 重定义为 "hermes freeze" (0.5pd)；工作挪到 Phase 2.5 |
| `.checkpoint.json` | current_task 切到 S1.T6 (ErrorClassifier，无 Python runtime 依赖) |

---

## Alternatives Considered / 候选方案

### Option A: 硬修 hermes（推迟 goose）

保留 hermes 作唯一 Python 样板，硬修 D88 / fork / auxiliary 5 个问题 (4-7 人日)，后续再谈引入 goose。

**优点**：最小改动，Phase 2 路线不变。
**缺点**：hermes-agent 黑盒，修完不保证稳。4-7 人日只换来"不崩"，没有为生态加分。grid-engine 仍孤立成长。
**拒绝理由**：低回报投资。

### Option B: 只做主力 grid-runtime，不做样板不做对比

集中精力 grid-runtime，放弃生态多样性。

**优点**：维护面最小。
**缺点**：违背 EAASP 契约开放初衷；grid-engine 无对比基准，设计缺陷难发现。
**拒绝理由**：长期战略损失。

### Option C: 三轨并举（本 ADR 采纳）

主力持续 + 样板示范 + 对比借鉴。分阶段落地：Phase 2.5 首批 goose + nanobot。

**优点**：契约开放得到真实验证；grid-engine 有对标；样板多样性吸引社区。
**缺点**：工作量大；需长期维护多个 runtime。
**采纳理由**：EAASP 契约开放战略的唯一贴合方案。

### Option D: 只做对比不做样板

跳过 nanobot / pydantic-ai 这类样板，只 wrap goose / claw-code 做对比。

**优点**：对比首要目标 (grid-engine 成长标杆) 直接满足，工作量小。
**缺点**：社区接入 L1 缺参考，"契约开放"流于口号。
**部分采纳**：样板仍要做，但可以延后（nanobot 进 Phase 2.5，pydantic-ai 进 Phase 3），不必一次全齐。

---

## References / 参考

- EAASP v2.0 spec §3.1 Phase 2 + §4 L1 contract
- `docs/plans/2026-04-14-v2-phase2-plan.md` S1.T2 原始任务定义
- `docs/design/EAASP/AGENT_LOOP_ROOT_CAUSE_ANALYSIS.md` — D87 根因（grid-engine vs 对比 runtime 差异实证）
- `docs/design/EAASP/AGENT_LOOP_PATTERNS_TO_ADOPT.md` — hermes / claude-code / goose / codex 四路 loop 对比（本 ADR 对比策略的直接先例）
- `3th-party/eaasp-runtimes/goose/` — 对比 runtime 源码
- `3th-party/claude-code-opensource/CCB/` / `claw-code/` — 对比 runtime 源码
- Phase 1 E2E 日志 — hermes fork+grpc 致命崩溃 + auxiliary 401 实证
