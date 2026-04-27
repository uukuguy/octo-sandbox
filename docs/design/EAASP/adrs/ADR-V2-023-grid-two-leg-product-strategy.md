---
id: ADR-V2-023
title: "Grid 双腿产品战略（EAASP 集成 + 独立 Platform）"
type: strategy
status: Superseded
accepted_at: 2026-04-19
date: 2026-04-19
phase: "Phase 3.5 预热"
author: "Jiangwen Su"
supersedes: []
superseded_by: ADR-V2-024
deprecated_at: 2026-04-28
deprecated_reason: "Superseded by ADR-V2-024 — 双轴模型 (engine vs data/integration) 替代 Leg A/B 二元框架, 详见 ADR-V2-024 Decision 段 + audit doc §5"
enforcement:
  level: strategic
  trace: []
  review_checklist: "docs/reviews/strategy-grid-two-leg-checklist.md"
affected_modules:
  - "crates/grid-engine/"
  - "crates/grid-runtime/"
  - "crates/grid-platform/"
  - "crates/grid-server/"
  - "crates/grid-cli/"
  - "crates/grid-sandbox/"
  - "crates/grid-desktop/"
  - "crates/grid-eval/"
  - "crates/grid-hook-bridge/"
  - "tools/eaasp-*/"
related:
  - ADR-V2-017
  - ADR-V2-020
  - ADR-V2-021
  - ADR-V2-022
---

# ADR-V2-023 — Grid 双腿产品战略（EAASP 集成 + 独立 Platform）

**Status:** Superseded by ADR-V2-024 (2026-04-28)
**Date:** 2026-04-19
**Phase:** Phase 3.5 预热
**Author:** Jiangwen Su
**Related:** ADR-V2-017 (L1 runtime ecosystem), ADR-V2-020 (tool namespace), ADR-V2-021 (chunk_type contract freeze), ADR-V2-022 (ADR governance)

---

## Context / 背景

截至 2026-04-19 Phase 3 sign-off，Grid 仓库 (`grid-sandbox`) 里已经沉淀了两类代码：

1. **围绕 Grid 自身的产品组件**（Rust crates）
   - `grid-engine` / `grid-runtime` — 核心 agent 运行时
   - `grid-cli` / `grid-server` / `grid-desktop` / `grid-sandbox` / `grid-eval` / `grid-hook-bridge` — 围绕 runtime 的增强工具链
   - `grid-platform` — 多租户企业交付 crate（当前 WIP，尚未真正启动实现）

2. **EAASP 架构的深度本地模拟器**（Python / Rust tools）
   - `tools/eaasp-l2-memory-engine` / `eaasp-l3-governance` / `eaasp-l4-orchestration` / `eaasp-skill-registry` / `eaasp-mcp-orchestrator` / `eaasp-certifier` / `eaasp-cli-v2`
   - `lang/*` 下 6 个非 Grid 的 L1 runtime 样板（`claude-code-runtime-python` / `nanobot-runtime-python` / `pydantic-ai-runtime-python` / `eaasp-goose-runtime` / `eaasp-claw-code-runtime` / `ccb-runtime-ts`；另有冻结的 `hermes-runtime-python`）

这些 EAASP-* 组件**不是本项目的产品目标**——它们是另一个独立项目 **EAASP (Enterprise-Agent-as-a-Service Platform)** 在本 repo 的**高保真本地镜像**，用于：
- 确保 `grid-engine` / `grid-runtime` 作为 EAASP 的 L1 实现时契约完全合规
- 在本 repo 就能跑通 L0-L4 全链路 E2E，不依赖外部 EAASP 部署
- 通过 6 个对比 L1 runtime 验证"契约可移植性"，指导 `grid-engine` 的接口设计

### 此前隐含但从未写下来的产品定位

Grid 同时瞄准两个市场：

**A. EAASP 集成市场（当前主战场）**
- Grid 作为 EAASP 平台签发的旗舰 L1 runtime
- `grid-engine` / `grid-runtime` 通过 EAASP 的 L2/L3/L4 被编排
- 客户用 EAASP 分发的 skill，Grid 负责执行
- 本 repo 的 `tools/eaasp-*/` 是"开发期对着空气拟合"用的，生产环境里跑的是真 EAASP

**B. Grid 独立市场（规划中，未启动）**
- `grid-platform` 作为独立的多租户企业交付形态
- 面向"不想用 EAASP、只要 Grid"的企业客户
- Grid 自己负责 auth / tenant / quota / 部署
- 避免 Grid 被 EAASP 单一上游绑死

### 为什么要把战略写下来

之前既没有 ADR 也没有产品文档说这件事，结果：
- `grid-platform` crate 和 `web-platform/` 前端长期 WIP，读代码的人分不清它是"在做"还是"历史遗留"
- `tools/eaasp-*/` 的定位对新人不清楚——看起来像项目主体的一部分
- 项目名命名讨论反复（grid-agent / grid-harness / grid-workbench / grid-stack），就是因为不同候选对应不同的单腿定位，没有战略共识就命不了名
- 未来如果 Grid 只服务 EAASP 就会变成"单一客户绑死"，产品风险高

---

## Strategic Decision / 战略决定

**Grid 明确采用双腿战略：既作为 EAASP 的旗舰 L1 runtime（腿 A），也保留并推进作为独立多租户产品的路径（腿 B）。两腿共享 `grid-engine` / `grid-runtime` 作为单一核心，工具链和交付形态分化。**

### 腿 A — EAASP 集成（主战场，当前优先）

- **核心产物**：`grid-engine`（core） + `grid-runtime`（L1 gRPC adapter）
- **交付方式**：由 EAASP 平台打包、编排、分发；Grid 只暴露符合 L1 契约的 runtime
- **在本 repo 的模拟**：`tools/eaasp-*/` + `lang/*` 对比 runtime，用于契约验证和集成测试
- **进度标志**：Phase 2 / Phase 2.5 / Phase 3 / Phase 3.5 持续投入，已有 7-runtime contract-v1.1.0 + 112 pytest E2E 基础设施

### 腿 B — Grid 独立产品（长期规划，尚未启动）

- **核心产物**：`grid-platform`（多租户 server） + `grid-server`（单租户 workbench） + `web/` / `web-platform/` 前端
- **交付方式**：Grid 自行打包发布，客户就地部署
- **定位**：给"不需要多 L1 编排"或"不想采购 EAASP"的企业使用
- **当前状态**：crate 骨架存在，功能未实现；前端尚未开发
- **激活触发条件**（见下面 P5）

### 共享 vs 专属组件清单

| 组件 | 腿 A 用 | 腿 B 用 | 说明 |
|------|:-------:|:-------:|------|
| `grid-engine` | ✅ | ✅ | 共享核心 |
| `grid-runtime` | ✅ | ✅ | 腿 A 走 gRPC；腿 B 走直接进程内调用或内部 IPC |
| `grid-sandbox` (crate) | ✅ | ✅ | 沙箱执行共享 |
| `grid-hook-bridge` | ✅ | ✅ | hook 机制共享 |
| `grid-types` | ✅ | ✅ | 类型定义共享 |
| `grid-cli` | ⚠️ 辅助 | ✅ | 腿 A 下 EAASP 有自己的 CLI；腿 B 下 `grid-cli` 是主要客户端 |
| `grid-eval` | ⚠️ 辅助 | ✅ | 腿 A 下 EAASP 有自己的 eval；腿 B 下 `grid-eval` 是主评测 |
| `grid-server` | ❌ | ✅ | 腿 B 专属（单用户 workbench） |
| `grid-platform` | ❌ | ✅ | 腿 B 专属（多租户 server） |
| `grid-desktop` | ❌ | ✅ | 腿 B 专属（Tauri 桌面应用） |
| `web/` | ❌ | ✅ | 腿 B 专属 |
| `web-platform/` | ❌ | ✅ | 腿 B 专属 |
| `tools/eaasp-*/` | ✅（模拟器） | ❌ | 腿 A 专属，腿 B 下不使用 |
| `lang/*` (6 对比 runtime) | ✅（对比） | ❌ | 腿 A 专属 |

### Grid Platform vs EAASP 的产品边界

避免 `grid-platform` 被当作"EAASP 竞品"的误读——两者不在同一层竞争：

| 维度 | EAASP | Grid Platform |
|------|-------|---------------|
| 定位 | **多 L1 runtime 编排平台** | **单 L1 runtime（Grid）的多租户企业部署** |
| L1 | 支持 ≥7 种 runtime 互换 | 只跑 Grid runtime |
| 客户画像 | 想同时使用多种 agent 框架、需要统一治理 | 选定 Grid 后只要多租户交付、不要跨 L1 编排 |
| 与 Grid 的关系 | Grid 作为 L1 候选之一 | Grid 是核心，Platform 是交付载体 |
| 重合场景 | 客户确定只要 Grid + 不要 EAASP 的编排能力 → Grid Platform 是简化方案 | 同左 |

两者**可以并存**：同一个 Grid core，打包成腿 A 时接入 EAASP，打包成腿 B 时自己交付。

---

## Principles / Guidelines

新功能和新 PR 按以下原则取舍：

1. **P1 — Core first, package later**
   凡属 `grid-engine` / `grid-runtime` / `grid-types` / `grid-sandbox` / `grid-hook-bridge` 的改动，**必须对两条腿都可用**。不允许为某一条腿专门在 core 里加分支。如果某功能两腿语义不同，应该在 adapter 层（`grid-server` vs EAASP 集成层）分化。

2. **P2 — 腿 A 当前绝对优先**
   Phase 3.5 / Phase 4（预计）仍聚焦 EAASP 集成。腿 B 的 `grid-platform` / `web*` 在当前 roadmap 中属于 **dormant（休眠）** 状态，不投入开发工时，只做 crate 骨架维护（保证能编译）。新 PR 如果修改 `grid-platform` / `web*`，reviewer 需要验证：这个改动是否真的必要？还是可以推迟？

3. **P3 — EAASP 模拟器的边界**
   `tools/eaasp-*/` 是腿 A 的**测试夹具**，不是 Grid 的产品。
   - ❌ 腿 B 任何组件**不得** import `tools/eaasp-*/` 的代码
   - ❌ 不要在 `tools/eaasp-*/` 里实现 Grid 独有的业务逻辑（那叫错地方了）
   - ✅ 允许 `tools/eaasp-*/` 与上游 EAASP 实际架构出现微调差异，但每次差异需在 commit 或 ADR 里说明是"模拟差异"还是"新发现的契约问题"

4. **P4 — 两腿下的 L1 实现等价**
   `grid-engine` 作为腿 A 的 L1 实现和作为腿 B 的核心，行为必须一致。tests/integration 必须覆盖两种调用方式（gRPC via `grid-runtime` + in-process via `grid-server`）。契约测试（`tests/contract/cases/`）以腿 A 为准，腿 B 复用。

5. **P5 — 腿 B 激活触发条件**
   腿 B 从 dormant 转为 active 需满足以下**任一**触发条件，并由新 ADR 正式登记：
   - EAASP 项目成熟度变化（延迟、停滞、定价不合理等），导致 Grid 需要独立交付能力作为风险对冲
   - 获得明确客户信号：≥2 个企业客户要求"不走 EAASP，直接买 Grid"
   - 行业标准演化：出现广泛采用的 agent runtime 标准（非 EAASP），Grid Platform 可以作为该标准的参考实现
   - 团队能力富余：核心腿 A 已经稳定到可以释放出 ≥30% 工程投入给腿 B

触发后的第一步：创建 ADR-V2-XXX 记录激活决定 + Phase 规划 `docs/plans/YYYY-MM-DD-grid-platform-activation.md`。

6. **P6 — 项目名决策推迟**
   基于双腿战略，当前无法给出一个既能涵盖"开发工作台"又能涵盖"多租户平台"的理想项目名。一些此前候选的局限：
   - `grid-workbench` — 过度锁定腿 B 的单租户 workbench 定位
   - `grid-agent` / `grid-harness` — 只表达了 engine/runtime 核心，无视整个 Grid 组件家族
   - `grid-platform` — 与 crate 名冲突，且过度锁定腿 B 的 platform 形态
   - `grid-stack` / `grid-core` — 可接受，但在项目快速演化期改名有迁移成本

**决定**：继续使用 `grid-sandbox` 作为仓库名。触发重命名的条件：
- 腿 B 激活（见 P5）
- Grid Platform v1 发布
- 或任何面向外部（客户 / 合作方 / 投资方）的正式 launch

重命名时一次性完成，配套 ADR 记录决策。候选名届时再基于产品状态重评。

---

## Consequences

### Positive

- **战略路径双重保险**：腿 A 有进度，腿 B 有选项，Grid 不被 EAASP 单一上游绑死
- **`grid-platform` / `web*` 的 WIP 状态被正式化**：从"模糊待定"变成"明确休眠，激活条件可验证"
- **`tools/eaasp-*/` 的身份明确**：模拟器，非产品
- **项目名决策解锁**：不再需要为命名反复纠结，等产品形态稳定再定
- **ADR 可追溯**：未来任何关于 platform 的讨论都有锚点

### Negative

- **路径锁定**：腿 B 长期处于 dormant 状态会产生 "paper crate" 风险——`grid-platform` 的接口设计与 `grid-engine` 演进脱节。需要周期性（每 milestone 末）回看是否仍可编译、接口是否仍合理
- **双腿测试成本**：P4 要求 core 改动覆盖两种调用方式，tests/integration 面积翻倍
- **模拟器维护成本**：`tools/eaasp-*/` 需要跟随 EAASP 上游演化更新，增加腿 A 的工作量（但这是腿 A 本来就要付的税，不是战略额外增加的）

### Risks

- **R1 腿 B 无限期休眠**：理论上可以激活但实践中一直没触发 → 缓解：每个主要 phase 结束时把 P5 触发条件评估结果写入 phase work log，保持可见度
- **R2 腿 A/B 接口分叉**：`grid-engine` 为了迎合某一条腿悄悄加分支违反 P1 → 缓解：`review_checklist` 明确包含"这个 core 改动两腿都合理吗？"问题；PR 审查强制检查
- **R3 EAASP 模拟器被当成产品代码**：有人在 `tools/eaasp-*/` 里写 Grid 独有业务 → 缓解：P3 明确边界；未来可加 CI lint（`tools/eaasp-*/` 禁止 import `grid-*` 业务 crate 的 guard）
- **R4 命名拖延变慢性病**：P6 推迟重命名后一直不改 → 缓解：任何 "P5 激活" 或 "v1 release" 触发时同步处理命名，锁定触发点

---

## Enforcement

### Level
`strategic` — 不产生编译/测试失败。落地依赖：
- 下游 `contract` ADR 通过 `related:` 引用本 ADR
- PR review 使用 `review_checklist`

### Review checklist

`docs/reviews/strategy-grid-two-leg-checklist.md` — 未来创建。初步条目：

- [ ] 此 PR 是否触碰 `grid-platform` 或 `web*`？如是，是否必须（见 P2 原则，原则上当前不应动）？
- [ ] 此 PR 在 `grid-engine` / `grid-runtime` / `grid-types` / `grid-sandbox` / `grid-hook-bridge` 任何一个里加了只服务某一条腿的分支？（违反 P1）
- [ ] 此 PR 在 `tools/eaasp-*/` 里实现了 Grid 独有业务逻辑？（违反 P3）
- [ ] 此 PR 是否声称"为腿 B 准备"而实际属于 P5 未触发的预热代码？（违反 P2）
- [ ] 如果 P5 触发条件满足，是否创建了对应的 ADR 和 Phase plan？

### Trace
`[]` — strategy ADR 不带可执行 trace。

---

## Alternatives Considered

### A. 单腿：Grid 只做 EAASP 的 L1，放弃独立产品路径

**Rejected** — 用户明确否决 ("不想 grid 项目只绑死在 EAASP 上")。单腿策略意味着：
- 长期取决于 EAASP 项目的成败
- 失去"不走 EAASP 也能卖 Grid" 的独立商业可能性
- `grid-platform` / `web*` crate 直接归档（但这些代码的存在本身说明当初就不想绑死）

### B. 单腿：Grid 只做独立产品（`grid-platform`），不深度适配 EAASP

**Rejected** — 与现实投入不符。Phase 1-3 的绝大多数工作（contract-v1.0 / v1.1、7 runtime 矩阵、E2E harness、skill extraction meta-skill 等）都是围绕 EAASP 契约展开。放弃腿 A 等于推倒重来。EAASP 目前是 Grid 最确定的产出口。

### C. 完全并行双腿：给两条腿分配相等工程投入

**Rejected** — 团队资源不足以支持两条腿都全速开发。P2 原则明确当前腿 A 优先，腿 B 进入可控休眠。

### D. 把 Grid Platform 从本仓库拆分到独立仓库

**Rejected (暂时)** — 腿 B 休眠状态不值得多仓库管理成本。P5 触发激活时可重新评估拆仓。

---

## Implementation

1. **当前（立刻）**：
   - 本 ADR 从 Proposed 走到 Accepted（`/adr:accept ADR-V2-023`）
   - 更新 `docs/dev/MEMORY_INDEX.md` 归档战略决定
   - 更新项目 `CLAUDE.md` 开头，以本 ADR 为锚点重写项目定位段落
   - `grid-platform` / `web*` / `web-platform/` 标记为 "dormant" 状态（README 或 crate doc 说明）

2. **中期（每个 milestone 末）**：
   - 在 phase work log 里评估 P5 触发条件当前状态
   - 回看 P1-P6 原则是否有违反实例，更新 `review_checklist`

3. **P5 触发时**：
   - 新 ADR (ADR-V2-XXX) 记录"腿 B 激活"决定 + 触发条件证据
   - 新 Phase plan 规划腿 B 的 MVP
   - 同步项目重命名决策（P6）

---

## History

- 2026-04-19: Proposed by Jiangwen Su. 双腿战略首次正式化；`grid-platform` / `web*` / `web-platform/` 进入 dormant 状态；项目重命名推迟到 P5 触发。
- 2026-04-19: Accepted. ADR 自身 F1/F2/F3/F5 全 PASS。`/adr:accept` 脚本因仓库存在 26 个 pre-existing ADR-V2-001..V2-020 之间 F4 cross-conflict (issue tracked separately; ADR-V2-022 acceptance 也采用相同 override 模式) 而阻止 exit 0，依 ADR-V2-022 established precedent 手动 flip status — 纯策略 ADR 且 F1-F3 干净。后续 F4 debt 由 `/adr:reconcile` 系列单独处理。
