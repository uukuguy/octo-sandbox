# Grid

## What This Is

Grid 是一个 Rust-centric 的 agent runtime 技术栈,围绕 `grid-engine` 与 `grid-runtime` 构建。它有两条产品腿(per ADR-V2-023):**Leg A — EAASP 集成(当前主战场)** 是把 Grid 作为 EAASP(Enterprise-Agent-as-a-Service Platform,上游另一个团队负责的 L2/L3/L4 平台)的旗舰 L1 runtime 经 gRPC 暴露;**Leg B — Grid 独立产品(dormant)** 是 `grid-platform` / `grid-server` / `grid-desktop` + `web-platform/` 的多租户/单租户/桌面端形态,目标客户是 "想要 Grid 但不经过 EAASP" 的企业。

## Core Value

**作为 substitutable L1 runtime 通过 16-method gRPC contract(`proto/eaasp/runtime/v2/runtime.proto`)被 EAASP L2-L4 调用,且任何符合 contract-v1.1 的对比 runtime 都能替换它。** 这个可替换性是 Grid 在 Leg A 的不可妥协约束 —— 它意味着 grid-engine 不能依赖未文档化行为,且本仓库内 6 个 comparison runtime(claude-code / goose / nanobot / pydantic-ai / claw-code / ccb)是契约的活体测试。

## Requirements

### Validated

<!-- Existing capabilities — proven by Phase 2 → 4a delivery. -->

- ✓ **L1 RuntimeService 16-method gRPC contract** —— `proto/eaasp/runtime/v2/runtime.proto` 冻结,7 runtime × contract-v1.1 通过(`make v2-phase3-e2e` 112/112 PASS)
- ✓ **ChunkType 闭枚举契约**(ADR-V2-021,Phase 3.5) —— 8 wire 值跨 7 runtime 1:1 一致,proto + ccb TS guard CI 双锁
- ✓ **Hook envelope contract**(ADR-V2-006) —— 子进程 stdin JSON 包络对 Pre/PostToolUse/Stop 三事件,Rust↔Python 通过 `hook_envelope_parity_test.rs` 跨语言一致
- ✓ **Two-leg shared core 边界**(ADR-V2-023 P1) —— `grid-types` / `grid-engine` / `grid-sandbox` / `grid-hook-bridge` 在两条腿中保持单一 codebase,Phase 4a 验证 0 Leg-B 假设泄漏到 shared crate
- ✓ **L2 内存混合检索**(ADR-V2-015) —— FTS5 + HNSW + time-decay 三路融合,`tools/eaasp-l2-memory-engine` 实现
- ✓ **PreCompact hook 协议**(ADR-V2-018) —— iterative summary + 跨压缩 token 预算
- ✓ **Tool namespace 契约**(ADR-V2-020) —— L0/L1/L2 工具分层,`ToolLayer` enum + `RequiredTool` parser
- ✓ **L1 runtime 生态策略**(ADR-V2-017) —— 主力(Grid)+ 样板(claude-code / nanobot / pydantic-ai)+ 对比(goose / claw-code / ccb)三轨,hermes 冻结
- ✓ **Stop hook + Scoped-hook executor**(Phase 2 S3.T4/T5) —— `StopHookDecision::{Noop, InjectAndContinue}` 三轮再注入上限
- ✓ **AgentLoop 通用性**(ADR-V2-016) —— capability matrix + Eager probe + tool_choice=Required + 不支持 provider 优雅退出
- ✓ **Phase-driven 开发流水**(Phase 2 → 4a 共 14 个归档 phase) —— ADR governance plugin + Deferred ledger SSOT
- ✓ **Debt 水位归零**(2026-04-20 @ commit `8629505`) —— D148/D149/D151/D152/D153/D154/D155 全 ✅ CLOSED 后无新 P1-active

### Active

<!-- Phase 4 待办 + Phase 4a project review 发现的 P0/P1。 -->

- [ ] **Phase 4 主决策:Leg A 继续硬化 vs Leg B 激活**(per ADR-V2-023 §P5 触发条件) —— Phase 4.1 先 socratic discuss
- [ ] **修 P0:`docs/design/EAASP/L1_RUNTIME_ADAPTATION_GUIDE.md` §4 stale chunk_type wire 值** —— 列的是 Phase 3.5 之前的 `"text"` / `"tool_call"` / `"hook_fired"` / `"pre_compact"`,新 L1 runtime 作者会被误导
- [ ] **澄清 P0:D120 状态** —— `DEFERRED_LEDGER` 标 P1-defer → Phase 2.5 W1,但 `phase_stack` 显示 Phase 2.5 100% 完成,二者矛盾,需断定 D120 是真已 closed 还是 silent descope
- [ ] **创建 P1:`docs/reviews/strategy-grid-two-leg-checklist.md` + `.github/CODEOWNERS`** —— ADR-V2-023 §Enforcement 引用了前者但文件不存在;CODEOWNERS 无 Leg-B dormancy 强制规则

### Out of Scope

- **`grid-sandbox` 仓库改名** —— per ADR-V2-023 §P6,推迟到 Leg B 激活后再讨论
- **`git push origin main`** —— 累积 ~14 unpushed commits(Phase 3.5 + 3.6 + 4a + cutover prep + 接管 commits),保留人类决策
- **Phase 0–2.5 历史 retrofit**(Phase 4a project review 发现 sign_off_commit 字段缺失) —— 接受历史不完美,git history 为准,不回填
- **132 个历史 plan 文件 + 14 archived phase 迁入 GSD ROADMAP.md** —— 冻结为只读历史存档,GSD 仅管 Phase 4 起的新工作
- **F4 lint 52 个 module-overlap 警告 reconcile** —— Phase 4a session-04-26 audit 已确认无 Decision-text 矛盾,advisory-only 接受
- **EAASP 上游 `tools/eaasp-*/` 自动 sync 机制** —— 上游团队独立项目,继续手动追平
- **超出 Leg A 的 Grid Platform / Server / Desktop / Web 增量功能开发** —— Leg B dormant,激活前任何 PR 触碰这几个 crate 需 §P5-style justification
- **替换现有 Plan 流水到 `docs/plans/2026-*-plan.md` 单文件结构** —— GSD 用 `.planning/phases/<phase>/PLAN.md` 多目录结构,各管各的

## Context

**Brownfield 切换背景**(2026-04-26):本项目从 2026-04-04 起在 dev-phase-manager + superpowers 体系下推进,经过 14 个归档 phase(Phase BA → Phase 4a)交付 EAASP v2.0 全部里程碑(Phase 2 / 2.5 / 3 / 3.5 / 3.6 / 4a)。Phase 4a 末尾 debt 水位归零后,切换到 GSD 体系是因为 GSD 的 **workstreams + resume-work + plan-checker + map-codebase** 在 brownfield + 多 workstream 场景比 dev-phase-manager 更合适。

**项目所处生态位**:Grid 是 EAASP(上游另一团队的 Enterprise-Agent-as-a-Service Platform)的 L1 runtime 候选之一。本仓库的 `tools/eaasp-*/` 是 EAASP L2/L3/L4 的**本地高保真 shadow**(per ADR-V2-023 P3),不是生产 EAASP。Leg A 的契约对接由本仓库的 7 个 L1 runtime 集体验证,任何 contract-v1.1 通过的 runtime 都能在 EAASP 中替换 Grid。

**技术栈成熟度**:13 Rust crate(~178K LOC)+ 5 Python lang/runtime + 9 EAASP tools(~29K LOC)+ 226 test files。Cargo workspace 严格依赖纪律(`[workspace.dependencies]` 40+ pin),Python 全 `uv` 管理,Pyright `pyrightconfig.json` 9 per-env executionEnvironments,proto codegen 走 `scripts/gen_runtime_proto.py` 单 SoT。`unsafe` 全工程零块。

**治理底座**:15 ADR(`docs/design/EAASP/adrs/ADR-V2-001..V2-023`),F1-F5 lint 由 ADR governance plugin 强制(`/adr:audit` + `.github/workflows/adr-audit.yml`)。Deferred 账本 `docs/design/EAASP/DEFERRED_LEDGER.md` 是跨 phase 单 SSOT,**保留作 GSD 例外**(GSD 自身的 backlog 不取代它)。

## Constraints

- **Tech stack(冻结)**: Rust 1.75+ edition 2021,Python 3.12+(uv 管理),TypeScript 5.x(Bun),Protobuf 3,gRPC(tonic + grpcio),SQLite + HNSW
- **Authoritative source 优先级(锁在 CLAUDE.md)**: ADR(`docs/design/EAASP/adrs/`)> EAASP/(子目录)> Grid/(子目录)> 代码 > root-level `docs/design/*.md`(后者全是 PRE-EAASP-v2 LEGACY,2026-02 至 03 月,不可作为当前架构引用)
- **Two-leg P1 规则(ADR-V2-023)**: shared core(`grid-types` / `grid-engine` / `grid-sandbox` / `grid-hook-bridge`)对两条腿都要工作。No leg-specific branches in core code
- **L1 contract 不可破坏**: 修改 `proto/eaasp/runtime/v2/*.proto` 必须经 ADR 走流水(F4 reviewer 强制)
- **Hook envelope 跨 runtime 一致**: ADR-V2-006 §2-§3 envelope shape 在 Rust + Python 两侧通过 `hook_envelope_parity_test.rs` 验证,新增运行时必须通过
- **No live LLM in unit tests**: 所有 unit test mock provider 或 monkeypatch SDK call;live LLM 只在 e2e harness 与 manual runbook
- **Commit 格式**: subject ≤72 chars + 强制 footer `Generated-By: Claude (claude-<model>) via Claude Code CLI` + `Co-Authored-By: claude-flow <ruv@ruv.net>`
- **Test discipline(per CLAUDE.md)**: 不自动跑全 workspace test suite。targeted test only;full run 需先问 user
- **Documentation language**: CLAUDE.md / README.md → English;`docs/design/`/`docs/plans/` → Chinese;ADR 双语标题 + 英文 frontmatter

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| **DEFERRED_LEDGER.md 作为单 SSOT 保留(GSD 例外)** | GSD backlog 是 999.x phase 散文件,会断 D87→D148→D152 跨 phase trace。本项目 14 phase 全部用 ledger 追了 100+ D-item,这套机制成熟,不替换 | ✓ Good — sticky |
| **WORK_LOG.md 继续 prepend-on-top(GSD 例外)** | `.planning/STATE.md` 是 current state,WORK_LOG 是时间线 history,二者并存。STATE 给 resume-work,WORK_LOG 给 archeology | ✓ Good — sticky |
| **ADR plugin 完全独立于 GSD** | `/adr:*` 与 GSD 不耦合,15 ADR 已成 governance 底座。继续 PreToolUse `adr-guard.sh` 强制 + F1-F5 CI lint | ✓ Good — sticky |
| **Historical phases(132 plan + 14 archived)冻结不迁移** | 自动迁入 ROADMAP.md 风险大、价值低,git log 已记录。Phase 4 起步从 .planning/ 干净开 | ✓ Good — locked |
| **Leg A vs Leg B 决策推迟到 Phase 4.1** | 触发条件在 ADR-V2-023 §P5,需先 socratic discuss 而非预判 | — Pending |
| **Two-stage review 用 superpowers 而非 GSD 单 reviewer** | Phase 4a 经验:T5/T6 高风险 task 靠 superpowers 抓出 I1/I2/I3 类细节 issue。GSD `gsd-code-review` 是 broad-stroke,互补使用 | ✓ Good — locked via REVIEW_POLICY.md(Step 3) |
| **新 phase 高风险 task 靠 PLAN.md frontmatter `review_protocol: superpowers-two-stage` 显式标记** | 比"plan-checker 动态决定"更可控,比"人工每次说"更标准化 | — Pending REVIEW_POLICY.md 落地 |
| **GSD 模型 profile = Quality** | Phase 4 决策阶段值得 Opus 跑 researcher / roadmapper;成本可接受 | ✓ Good |
| **GSD parallel plan execution 打开** | Phase 2.5 W1∥W2 并行经验证明本项目适合 | ✓ Good |
| **Granularity = Standard** | 5-8 phases / milestone, 3-5 plans / phase。匹配 Phase 2 / 3 历史粒度 | ✓ Good |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---

*Last updated: 2026-04-26 after GSD takeover initialization (post-Phase-4a, brownfield from dev-phase-manager + superpowers).*
