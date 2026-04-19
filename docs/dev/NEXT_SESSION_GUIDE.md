# Grid Platform 下一会话指南

**最后更新**: 2026-04-19 22:30 GMT+8
**当前分支**: `main`
**当前状态**: EAASP v2.0 **Phase 3 🟢 COMPLETE 35/35** @ `8ee05fe` (sign-off 2026-04-18) + ADR Governance W1+W2 (out-of-plan) 2026-04-19 → **Phase 3.5 🟡 READY TO START**

- **下一 Phase**: **Phase 3.5 — chunk_type 契约统一 (ADR-V2-021 落地)**
- **Plan**: `docs/plans/2026-04-19-v2-chunk-type-unification.md`（10.2 K，swarm-ready，hierarchical max-agents=8，5 stages S0-S5）
- **ADR**: `docs/design/EAASP/adrs/ADR-V2-021-chunk-type-contract-freeze.md`（**Proposed**）

---

## 完成清单

- [x] Phase A-Z — Core Engine + Eval + TUI + Skills
- [x] Phase AA-AF — Sandbox/Config/Workspace architecture
- [x] Phase AG-AI — Memory/Hooks/WASM enhancement
- [x] Phase AJ-AO — 多会话/安全/前端/服务器
- [x] Phase AP-AV — 追赶 CC-OSS + 安全对齐
- [x] Phase AW-AY — 工具/Agent/SubAgent 体系
- [x] Phase AZ — Cleanup/Transcript/Completion
- [x] Phase BA — Octo to Grid 重命名 + TUI 完善
- [x] Phase BB-BC — TUI 视觉升级 + Deferred 补齐
- [x] Phase BD — grid-runtime EAASP L1 (6/6, 37 tests)
- [x] Phase BE — EAASP 协议层 + claude-code-runtime (6/6, 93 tests)
- [x] Phase BF — L2 统一资产层 + L1 抽象机制 (7/7, 30 tests)
- [x] Phase BG — Enterprise SDK 基石 (6/6, 107 tests)
- [x] Phase BH-MVP — E2E 全流程验证 (7/7+D3/D5/D10, 71 tests)
- [x] Phase BI — hermes-runtime (6/6, 12 tests) — **冻结，由 Phase 2.5 goose-runtime 替代**
- [x] **EAASP v2.0 Phase 0** — Infrastructure Foundation
- [x] **EAASP v2.0 Phase 0.5** — MVP 全层贯通
- [x] **EAASP v2.0 Phase 0.75** — MCP 端到端通路
- [x] **EAASP v2.0 Phase 1** — Event-driven Foundation (13/13, 124 tests, 2 runtime E2E)
- [x] **EAASP v2.0 Phase 2** — Memory and Evidence (23/23, ~170 新测试)
- [x] **EAASP v2.0 Phase 2.5** — L1 Runtime Ecosystem + goose + nanobot (25/25, +10 新回归测试, sign-off E2E PASS)
- [x] **EAASP v2.0 Phase 3** — L1 Runtime Functional Completeness (**35/35 ✅** @ 8ee05fe 2026-04-18, 7-runtime contract v1.1 全 PASS / 22 XFAIL, E2E B1-B8 112 pytest PASS)
- [x] **ADR Governance W1+W2** (out-of-plan, triggered by chunk_type drift) — ADR-V2-022 meta-ADR Accepted + 14 ADRs backfilled + vendor pattern + global PreToolUse guard
- [ ] **EAASP v2.0 Phase 3.5** — chunk_type 契约统一（下一步）

---

## Phase 3 成果总结（35/35 🟢 Completed 2026-04-18）

### Stage 完成情况

| Stage | 任务数 | 状态 | 关键产出 |
|-------|-------|------|---------|
| **S1 工具命名空间治理** | 8/8 ✅ | ADR-V2-020 Accepted，`ToolLayer` enum (L0/L1/L2)，dual-key `ToolRegistry`，skill-declared filter，`contract-v1.1.0` tag (23 new cases) |
| **S2 Phase 2 P1-defer 清债** | 9/9 ✅ | 7 项 P1-defer closed: D130 / D78 / D94 / D98 / D117 / D108 / D125 + DEFERRED_LEDGER 归档 |
| **S3 D144 + 对比 runtime + E2E** | 18/18 ✅ | goose ACP 接线 + nanobot ConnectMcp/Stop + pydantic-ai/claw-code/ccb 三对比 runtime 全 42 PASS 22 XFAIL + E2E B1-B8 112 pytest + `make v2-phase3-e2e` + 7-runtime matrix + §12 TS/Bun 指南 |

### 关键技术成果

- **7 runtimes × contract v1.1 全 PASS / 22 XFAIL**：grid / claude-code / goose / nanobot / pydantic-ai / claw-code / ccb
- **skill-extraction E2E 跑通**：nanobot / pydantic-ai / claw-code / ccb 全通（8/8 PASS on nanobot 样板）
- **`make v2-phase3-e2e`**：一键跑 B1-B8，112 pytest PASS
- **人工 runbook**：`phase3-verification-log.txt` 7 runtime 全 PASS + Group A Step 4a nanobot PRE_TOOL_USE ≥ 5 + Stop hook `evidence_anchor_id` + STOP reason=complete @ 9abe562
- **ADR-V2-020 Accepted**：L0/L1/L2 tool namespace 硬约束 + dual-key ToolRegistry + skill-declared filter > EAASP_TOOL_FILTER env (deprecated)
- **7 项 P1-defer closed**：D130 CancellationTokenTree, D78 EventEmbeddingIndex, D94 MemoryStore singleton, D98 HNSW cache, D117 PromptExecutor trait, D108 bats hook regression, D125 EventBus backpressure

### 测试增量

- S2 清债：121 pytest + 22 bats + cargo check clean
- S3 E2E B1-B8：112 pytest（B1/B2 ErrorClassifier 26 + B3/B4 HNSW 20 + B5/B6 memory-confirm 22 + B7/B8 aggregate spill 44）
- 回归：零回归，每 stage 带 targeted 证据

### 签出之后的 Post-Phase 修补（2026-04-19 nanobot hotpatch）

- `8239239` fix(nanobot-runtime): wire real MCP tool executor with exact routing
- `8e3ec91` fix(eaasp/phase3): restore dot-notation in SKILL.md required_tools + normalize in Initialize
- `9abe562` fix(nanobot-runtime): wire PRE_TOOL_USE + Stop hook with evidence_anchor_id

### OUT-OF-PLAN: ADR Governance W1+W2（2026-04-19，triggered by chunk_type drift）

Phase 3 sign-off 后，验收 nanobot 时暴露 `SendResponse.chunk_type` 跨 runtime 漂移（7 runtime 发 7 种值），CLI 只认 grid 那套，session 实际跑完但显示 `0 chars`。这是"约定俗成 ≠ 可执行契约"的经典表现——推动了 ADR 治理基础设施的建立：

- **ADR-V2-022 meta-ADR Accepted** — 3-type taxonomy（contract/strategy/record）+ 4 enforcement levels + F1-F5 lint + lifecycle state machine
- **全局插件** `~/.claude/skills/adr-governance/` — 10 Python scripts + 3 templates + VERSION 1.0.0
- **15 slash commands** `~/.claude/commands/adr-*.md` + `adr-architect` agent
- **14 grid-sandbox ADRs frontmatter-backfilled**；V2-004 downgraded to `docs/plans/completed/`；6 contract traces backfilled；2 F5 stale path typos fixed
- **`.adr-config.yaml`** + **`.github/workflows/adr-audit.yml`** + **AUDIT-2026-04-19.md** + **CLAUDE.md** §ADR Governance
- **Vendor pattern**：`/adr:init` 创建 `.adr-plugin/scripts/` 给 CI（已在本 repo vendored @ `f3b4198`）
- **PreToolUse hook `adr-guard.sh`** 全局启用 — 3-layer defense（SKILL + CLAUDE + hook）
- **ADR-V2-021 chunk_type contract freeze Proposed** + plan `docs/plans/2026-04-19-v2-chunk-type-unification.md`
- Health: F1 0 (was 6), F2 2 (V2-021 future only), F5 0 (was 2), 8 contract traced (was 2), 1 archived
- Commits: `99efb61` (W1), `de6b3f9` (W2.1), `f3b4198` (vendor), `3017478` (close-out)

---

## Phase 3.5 规划（设计 & 计划已锁）

**Phase 3.5 主题**：把 `SendResponse.chunk_type` 从自由 string 升级为 proto enum，一次性消除 7 个 L1 runtime 的取值漂移，让"每个 runtime 遵守同一套契约"成为 CI 硬门。

### 已产出

- ✅ **ADR-V2-021 Proposed**：`docs/design/EAASP/adrs/ADR-V2-021-chunk-type-contract-freeze.md`
- ✅ **Plan**：`docs/plans/2026-04-19-v2-chunk-type-unification.md`

### Stage 编排（5 stages）

| Stage | Tasks | 主题 |
|-------|-------|------|
| **S0** | 1 | proto contract freeze — 改 proto + 重生成 stub（**串行，先导**） |
| **S1** | 7 | 6 runtime 并行改发送端（grid/claw-code/goose/claude-code/nanobot/pydantic-ai/ccb） |
| **S2** | 2 | consumer 消费端 — L4 SSE 序列化 + CLI 白名单 |
| **S3** | 3 | 契约测试硬门 — `test_chunk_type_contract.py` + `.github/workflows/phase3-contract.yml` + E2E 回归 |
| **S4** | 3 | 人工 E2E — 重跑 threshold-calibration on nanobot + 其他 6 runtime smoke + log 归档 |
| **S5** | 3 | ADR 终结 — V2-021 Accepted + MEMORY.md + 实施记录 |

### Sign-off 标准

- [ ] proto `ChunkType` enum 定义 + stub 重生成
- [ ] 7 runtime 只发合法 `ChunkType`；非法值编译/运行期失败
- [ ] CLI + L4 SSE 消费端基于枚举，不含 fallback / 同义词
- [ ] 契约测试用例进 CI matrix，每 runtime 每次 push 必过
- [ ] 人工 E2E `eaasp session run -s threshold-calibration -r nanobot-runtime` CLI 显示完整 text + 合理 event 计数
- [ ] `cargo check --workspace` + 所有 Python runtime pytest + `bun test` 全 PASS
- [ ] `make v2-phase3-e2e` 全 PASS（112 pytest 仍通过）
- [ ] ADR-V2-021 状态 → Accepted + MEMORY.md 归档

### 风险（已在 plan §Risk Register 登记）

- R1 proto enum 改动 → 所有 stub 重生成 → S0 到 S1 完成前**不可 push 中间状态**
- R2 gRPC enum ↔ JSON wire 小写映射只放 L4 SSE 序列化层
- R3 契约测试需要真实 runtime matrix — 已有基础设施
- R4 L4 done summary `response_text=""` bug 修复点在 S2.T1
- R5 thinking chunk 可选，不作强制

### Phase 3.5 执行启动

选执行模式：
- **Mode 1 Subagent-Driven**：RuFlo swarm + Task tool 分派 coder/reviewer（推荐，符合 CLAUDE.md 铁律）
- **Mode 2 Parallel Session**：单开 session 跑 `/superpowers:executing-plans`

启动第一个任务：
```
S0.T1 proto contract freeze — 改 common.proto + runtime.proto + 重生成 stub
```

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| proto 源 | `proto/eaasp/runtime/v2/common.proto` + `runtime.proto` |
| L1 Grid Runtime | `crates/grid-runtime/`（主力） |
| L1 Claude Code Runtime | `lang/claude-code-runtime-python/`（样板） |
| L1 Goose Runtime | `crates/eaasp-goose-runtime/` |
| L1 Nanobot Runtime | `lang/nanobot-runtime-python/` |
| L1 Pydantic-AI Runtime | `lang/pydantic-ai-runtime-python/` |
| L1 Claw-Code Runtime | `crates/eaasp-claw-code-runtime/` |
| L1 CCB Runtime | `lang/ccb-runtime-ts/` |
| L2 Memory Engine | `tools/eaasp-l2-memory/` |
| L2 Skill Registry | `tools/eaasp-skill-registry/` |
| L3 Governance | `tools/eaasp-governance/` |
| L4 Orchestration | `tools/eaasp-l4-orchestration/` |
| SDK | `sdk/python/src/eaasp/` |
| CLI v2 | `tools/eaasp-cli-v2/` |
| Core Engine | `crates/grid-engine/` |
| E2E Tests | `scripts/verify-v2-phase2.{sh,py}` + `scripts/phase3-runtime-verification.sh` + `tests/contract/cases/` |
| Deferred Ledger | `docs/design/EAASP/DEFERRED_LEDGER.md` |
| ADR Governance | `~/.claude/skills/adr-governance/`（全局）+ `.adr-plugin/`（本 repo vendored） |

---

## Makefile 常用目标

```bash
# Phase 2 E2E 验收
make v2-phase2-e2e          # 默认：SKIP_RUNTIMES=true, 14 assertions
make v2-phase2-e2e-full     # 带两 runtime 6-step
make test-phase2-batch-ab   # S2 + S3 batch 回归

# Phase 3 E2E 验收
make v2-phase3-e2e          # B1-B8 全跑 112 pytest
make v2-phase3-e2e-rust     # Rust 侧回归

# 多 runtime 验证
make verify-dual-runtime    # 构建 + 启动双 runtime + certifier verify

# L2 Memory
make l2-memory-setup / l2-memory-start / l2-memory-test

# Skill Registry
make skill-registry-setup / skill-registry-start / skill-registry-test
```

---

## ⚠️ Deferred 未清项

> Single Source of Truth：
> [`docs/design/EAASP/DEFERRED_LEDGER.md`](../design/EAASP/DEFERRED_LEDGER.md)

**Phase 3 → Phase 3.5 历史 (closed by Phase 3)**：
- ✅ D130/D78/D94/D98/D117/D108/D125 全 closed (S2 清债)
- ✅ D144 goose/nanobot ConnectMCP 工具注入 (S3 收尾)
- ✅ grid-engine 工具命名空间治理 (S1 ADR-V2-020)
- ✅ E2E harness B1-B8 自动化触发 (S3.T12-T16)

**Phase 2 historical closed**：D87/D88/D83/D84/D85/D86/D89/D124/D60/D51/D53 + 其他 10 项

Phase 3.5 本身没有历史 Deferred 继承——它是 ADR-V2-021 专项落地。

---

## 会话启动建议（Phase 3.5）

1. `/dev-phase-manager:start-phase "Phase 3.5 — chunk_type Unification (ADR-V2-021)"`
2. 复核 `docs/plans/2026-04-19-v2-chunk-type-unification.md` 5 stages
3. S0.T1 改 proto —— 单 commit 先行，**stub 重生成后 cargo/pytest 一定挂**（这是预期）
4. S1 启动 6-coder 并行（grid 已合规，改字段类型即可）
5. 先跑 S3.T1 契约测试锁定白名单，再开 S4 人工 E2E

---

## 注意事项

- **零兼容切换**：Phase 3.5 不做双写、不做同义词表、不做 feature flag（用户明确否决，ADR §3）
- **gRPC enum wire**：走 `lower_snake_case`（Google Cloud / Stripe 风格），L4 SSE 单点映射
- **contract-v1.1.0 tag**：local-only，不推远端；Phase 3.5 可能需要 bump 到 v1.2.0（见 plan §S3.T2）
- **ADR Governance**：新增 ADR 用 `/adr:new --type contract|strategy`，禁止手写 frontmatter；PreToolUse guard 会拦截
- **E2E 唯一入口**：`bash scripts/eaasp-e2e.sh` + `make v2-phase3-e2e`
- **Checkpoint archive**：Phase 3 的 `.checkpoint.json` 被归档为 `.checkpoint.archive.json`（end-phase 执行）
