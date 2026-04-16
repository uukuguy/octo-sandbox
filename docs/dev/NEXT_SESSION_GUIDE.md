# Grid Platform 下一会话指南

**最后更新**: 2026-04-16 03:30 GMT+8
**当前分支**: `main`
**当前状态**: EAASP v2.0 **Phase 2 (Memory and Evidence) 完成 23/23** — 下一阶段 **Phase 2.5 (Consolidation + goose-runtime)** 待启动

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
- [x] **EAASP v2.0 Phase 2** — Memory and Evidence (23/23, ~170 新测试, 0 P0 escalation)
- [ ] **EAASP v2.0 Phase 2.5** — Consolidation + goose-runtime（下一步）

---

## Phase 2 成果总结

### Stage 完成情况

| Stage | 任务数 | 状态 | 关键产出 |
|-------|-------|------|---------|
| **S0 ADR 决策** | 2/2 ✅ | ADR-V2-015 (semantic HNSW in-process) + ADR-V2-016 (agent loop 原则) |
| **S1 Runtime 修复 + 错误基础设施** | 7/7 ✅ | D87 根治 + hermes 冻结 (ADR-V2-017) + ErrorClassifier 14 variants + Graduated retry |
| **S2 L2 Memory Engine 增强** | 5/5 ✅ | Vector embedding + Hybrid retrieval + 7 MCP tools + 状态机 invariants + turn aggregate spill |
| **S3 PreCompact + Skill Extraction + Stop hooks + Scoped executor** | 5/5 ✅ | ADR-V2-018 PreCompact + Skill Extraction meta-skill + StopHookDecision + ADR-V2-006 scoped-hook envelope |
| **S4 CLI + E2E + 多 session 隔离** | 4/4 ✅ | CLI session close + SSE events --follow + 14 assertions acceptance gate + SessionInterruptRegistry |

### 关键技术成果

- **5 个新 ADR Accepted**：V2-006 (scoped-hook envelope)、V2-015 (semantic)、V2-016 (agent loop)、V2-017 (L1 runtime 生态)、V2-018 (PreCompact hook)
- **根治 5 个 runtime 深层 bug**：D87/D86/D83/D85/D84 全部 closed
- **SessionInterruptRegistry**：`Arc<DashMap<SessionId, CancellationToken>>` + dual-path cancel (registry flag + `AgentMessage::Cancel`)
- **L2 Memory Engine**：FTS + HNSW + time-decay 三路融合 + tenant-isolated dual-write + 7 MCP tools
- **Skill Extraction**：`examples/skills/skill-extraction/` 三文件 + 148-line fixture replay (12 pytest tests)
- **Thread-scoped interrupt**：7 unit + 5 integration 测试锁定 "cancel A → B 不受影响"

### 测试增量

- **新测试**: ~170 (Rust interrupt 7 + integration 5 + stop_hooks 8 等；Python L2 ~60 + L4 ~40 + cli-v2 ~30 + fixture replay 12)
- **回归**：每 stage commit 带 targeted 回归证据，零回归
- **End-to-end**：`make v2-phase2-e2e` 14/14 PASS (A10 SKIP_RUNTIMES-guarded)

---

## Phase 2.5 规划（下一步优先级）

**Phase 2.5 主题**：Consolidation + goose-runtime 替代 hermes

### 优先级（P1-defer 首选）

1. **goose-runtime 引入**（ADR-V2-017）— `crates/eaasp-goose-runtime/` 原生 stdio+SSE MCP，解决 D88/T2 样板空缺
2. **D130 token consolidation** — AgentExecutor 持 session-lifetime parent token，`parent.child()` 出 per-turn，消除 S4.T4 dual-path workaround
3. **D120 cross-runtime envelope parity** — Rust `HookContext::to_json/to_env_vars` 补齐 ADR-V2-006 §2/§3 (event/skill_id/draft_memory_id/evidence_anchor_id/created_at + GRID_EVENT/GRID_SKILL_ID env)，前置 goose 契约测试
4. **D78 event payload embedding** — 与 memory semantic 共 HNSW 架构落地
5. **D94 MemoryStore 单例 refactor**（收尾 D12）
6. **D98 HybridIndex HNSW 持久化**（当前每次 search 重建）
7. **D108 hook script bats/shellcheck 自动回归**
8. **D125 events/stream burst cap**（if L1 >1k/sec 需要）

### Phase 2.5 W1 共享契约测试集（ADR-V2-017 交付件）

- 共享契约测试集 — 所有 L1 runtime 必须通过
- L1 适配指南
- 对比矩阵（grid vs claude-code vs goose）
- `crates/eaasp-goose-runtime/`

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| L1 Grid Runtime | `crates/grid-runtime/`（主力） |
| L1 Claude Code Runtime | `lang/claude-code-runtime-python/`（样板） |
| L1 hermes-runtime | `lang/hermes-runtime-python/`（冻结） |
| L2 Memory Engine | `tools/eaasp-l2-memory/` |
| L2 Skill Registry | `tools/eaasp-skill-registry/` |
| L3 Governance | `tools/eaasp-governance/` |
| L4 Orchestration | `tools/eaasp-l4-orchestration/` |
| SDK | `sdk/python/src/eaasp/` |
| CLI v2 | `tools/eaasp-cli-v2/` |
| Proto | `proto/eaasp/` |
| Core Engine | `crates/grid-engine/` |
| E2E Tests | `scripts/verify-v2-phase2.{sh,py}` + `tests/e2e/` |
| Deferred Ledger | `docs/design/EAASP/DEFERRED_LEDGER.md` |

---

## Makefile 常用目标（Phase 2 新增）

```bash
# Phase 2 E2E 验收
make v2-phase2-e2e          # 默认：SKIP_RUNTIMES=true, 14 assertions
make v2-phase2-e2e-full     # 带两 runtime 6-step (需手动执行 runbook 部分)
make v2-phase2-e2e-build    # 构建 + E2E
make test-phase2-batch-ab   # S2 + S3 batch 回归

# 多 runtime 验证
make verify-dual-runtime    # 构建 + 启动双 runtime + certifier verify

# L2 Memory
make l2-memory-setup / l2-memory-start / l2-memory-test

# Skill Registry
make skill-registry-setup / skill-registry-start / skill-registry-test
```

---

## ⚠️ Deferred 未清项（Phase 2.5 启动时必查）

> Phase 2 产出 47 个新 Deferred (D91-D130)，Single Source of Truth：
> [`docs/design/EAASP/DEFERRED_LEDGER.md`](../design/EAASP/DEFERRED_LEDGER.md)

**P1-defer (Phase 2.5 首选)**：
- **D130** — session-lifetime parent token consolidation (S4.T4 遗留)
- **D120** — cross-runtime hook envelope parity (前置 goose 契约测试)
- **D117** — 原 D50 Prompt executor (用户同意推迟，仍 P1)
- **D78** — event payload embedding
- **D94** — MemoryStore 单例 refactor
- **D98** — HybridIndex HNSW 持久化

**P3-defer (Phase 2.5 polish / Phase 3 breaking)**：
- D92/D96/D97/D99-D101/D103-D104/D106-D107/D110/D118-D119/D121-D123/D126-D129

**Phase 2 closed**：D87/D88/D83/D84/D85/D86/D89/D124/D60/D51/D53 + 其他 10 项

---

## 会话启动建议（Phase 2.5）

1. `/dev-phase-manager:start-phase "Phase 2.5 - Consolidation + goose-runtime"`
2. 检查 Deferred Ledger P1-defer 项，选定 Phase 2.5 任务集
3. 参考 ADR-V2-017 W1 交付件清单起草 Phase 2.5 plan
4. 参考 ADR-V2-006 起草 goose-runtime 契约测试集

---

## 注意事项

- **hermes-runtime 冻结**：不再修 bug，Phase 2.5 goose-runtime 完整替代
- **reviewer 发现零积压**：Phase 2 所有 reviewer Critical/Major 均 inline-fixed 或路由到非阻塞 Deferred
- **Deferred Ledger** 是 Phase 2+ 的 D 编号 single source of truth，`MEMORY_INDEX.md` / `phase_stack.json` 以其为准
- **Checkpoint archive**：Phase 2 执行中期的 `.checkpoint.json` 在 end-phase 时会归档为 `.checkpoint.archive.json`
