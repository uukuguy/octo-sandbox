# Grid Platform 下一会话指南

**最后更新**: 2026-04-18 06:00 GMT+8
**当前分支**: `main`
**当前状态**: EAASP v2.0 **Phase 2.5 (L1 Runtime Ecosystem + goose + nanobot) 🟢 COMPLETED 25/25** — 下一阶段 **Phase 3** 待规划

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
- [x] **EAASP v2.0 Phase 2.5** — L1 Runtime Ecosystem + goose + nanobot (25/25, +10 新回归测试, sign-off E2E PASS exit 0)
- [ ] **EAASP v2.0 Phase 3** — goose ACP full wiring + pydantic-ai/claw-code/ccb + 工具命名空间治理（下一步）

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

## Phase 2.5 成果总结（25/25 🟢 Completed 2026-04-18）

### Stage 完成情况

| Stage | 任务数 | 状态 | 关键产出 |
|-------|-------|------|---------|
| **S0 合约套件 v1 + D120** | 6/6 ✅ | 35 contract cases + Rust HookContext envelope parity，contract-v1.0.0 tag local-only |
| **S1 W1 goose-runtime** | 7/7 ✅ | `crates/eaasp-goose-runtime/` + Docker 容器 F1 gate + stdio proxy hook MCP + 16 gRPC |
| **S1 W2 nanobot-runtime** | 6/6 ✅ | `lang/nanobot-runtime-python/` + OpenAI-compat provider + multi-turn agent loop + 16 gRPC |
| **S2 文档** | 2/2 ✅ | L1_RUNTIME_ADAPTATION_GUIDE.md + L1_RUNTIME_COMPARISON_MATRIX.md |
| **S3 CI 门控** | 2/2 ✅ | Makefile v2-phase2_5-e2e + GitHub Actions matrix |
| **S4 人工 E2E** | 2/2 ✅ | Runbook + **sign-off E2E PASS exit 0** |

### Sign-off 过程挖出并治本的 7 类结构债

1. BROADCAST_CAPACITY 256→4096（Done chunk 丢失）
2. EAASP_TOOL_FILTER env 逻辑恢复（055badf squash 丢失）
3. KG/MCP-manage + AgentTool/QueryAgentTool 尊重 tool_filter
4. Stop ctx 注入 evidence_anchor_id / draft_memory_id
5. SKILL_DIR/hooks/ 完整 materialize（之前只写 SKILL.md）
6. L4 token-level text_delta/thinking 聚合（612→35 events/session）
7. Stop hook 脚本读顶层 envelope 字段

### 新增长期资产

- `scripts/eaasp-e2e.sh` — E2E 唯一入口，log_todo/SKIP 分类 + 每条 TODO 显式引用覆盖测试
- `docs/design/EAASP/E2E_VERIFICATION_GUIDE.md` — Living Document（§5.5 人工分步 + §5.6 演进承诺 + §7 Phase 收尾历史）
- `scripts/dev-eaasp.sh` — 起全 4 runtime + 每服务落盘 `.logs/latest/*.log`

### 10 个新回归测试（全 PASS）

- `tools/eaasp-l4-orchestration/tests/test_chunk_coalescing.py` 5 tests
- `crates/grid-engine/tests/phase2_5_regression.rs` 3 tests
- `crates/grid-runtime/tests/scoped_hook_wiring_integration.rs` +2 tests

---

## Phase 3 规划（下一步优先级）

**Phase 3 主题**：L1 生态功能完整性 + 工具命名空间治理 + 对比 runtime 评估

### 优先级

1. **D144: goose-runtime Send 完整 ACP 接线**
   - 当前 Send 是 stub（返回单个 done chunk）
   - 通过 GooseAdapter.stream 驱动真实 goose ACP subprocess
   - 事件映射 ACP → AgentEvent (CHUNK / TOOL_CALL / TOOL_RESULT / STOP)

2. **D144: nanobot-runtime ConnectMCP + 工具注入**
   - 当前 ConnectMcp 是空实现，AgentSession 永远用空 tools 列表
   - 真实实现 stdio MCP client + 工具注册到 AgentSession
   - Stop Hook dispatch（当前只有 PostToolUse）

3. **grid-engine 工具命名空间架构治理**
   - 内置 L0/L1 工具（memory_recall/timeline/graph_*/bash/file_read/agent/...）与 L2 MCP 工具（memory_search/read/write_*/confirm/...）命名空间混乱
   - skill 作者无法系统性控制 LLM 选择
   - 本次 Phase 2.5 靠 EAASP_TOOL_FILTER=on 和 executor.rs gate 打了补丁，Phase 3 需系统重构

4. **对比 runtime 评估**（ADR-V2-017 计划）
   - pydantic-ai / claw-code / ccb 对比评估
   - 拓展 L1 生态样本

5. **补 E2E harness 覆盖 TODO 项**（8 项）
   - B1 ErrorClassifier E2E harness（错误注入）
   - B2 graduated retry 日志解析
   - B5/B6 memory_confirm + 状态机定制 skill
   - B7 聚合溢出 blob_ref 造大 tool output
   - B8 PreCompact 长对话模拟
   - B3/B4 HNSW + 混合检索样本集

6. **Phase 2.5 历史遗留 P1-defer**（未处理）：
   - **D130 token consolidation** — AgentExecutor 持 session-lifetime parent token
   - **D78 event payload embedding** — 与 memory semantic 共 HNSW
   - **D94 MemoryStore 单例 refactor**（收尾 D12）
   - **D98 HybridIndex HNSW 持久化**（当前每次 search 重建）
   - **D108 hook script bats/shellcheck 自动回归**
   - **D125 events/stream burst cap**（if L1 >1k/sec）

### Phase 3 启动命令

```
/dev-phase-manager:start-phase "Phase 3 — L1 Runtime Functional Completeness"
```

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

## ⚠️ Deferred 未清项（Phase 3 启动时必查）

> Single Source of Truth：
> [`docs/design/EAASP/DEFERRED_LEDGER.md`](../design/EAASP/DEFERRED_LEDGER.md)

**Phase 2.5 sign-off 遗留（Phase 3 首选）**：
- **D144** — nanobot/goose ConnectMCP 工具注入（nanobot Send 骨架无工具；goose Send 是 stub）
- grid-engine 工具命名空间架构治理（内置 L0/L1 vs L2 MCP 命名冲突系统设计）
- E2E harness 补齐 TODO 8 项（B1-B8 自动化触发）

**Phase 2 → Phase 3 历史 P1-defer（未处理）**：
- **D130** — session-lifetime parent token consolidation (S4.T4 遗留)
- **D78** — event payload embedding
- **D94** — MemoryStore 单例 refactor
- **D98** — HybridIndex HNSW 持久化
- **D117** — 原 D50 Prompt executor (用户同意推迟)
- **D108** — hook script bats/shellcheck 自动回归
- **D125** — events/stream burst cap

**Phase 2.5 closed**：D120 (S0.T3 inline) / D141 (S1.W1.T2.5 F1 gate) / D142/D143 (S3 CI batch) 等

**Phase 2 closed**：D87/D88/D83/D84/D85/D86/D89/D124/D60/D51/D53 + 其他 10 项

---

## 会话启动建议（Phase 3）

1. `/dev-phase-manager:start-phase "Phase 3 — L1 Runtime Functional Completeness"`
2. 复核 DEFERRED_LEDGER.md 筛选 P1-defer + D144 组队立项
3. 先定工具命名空间架构治理方案（本 Phase 核心价值点）
4. 再用"治理方案"驱动 goose ACP / nanobot MCP 的接线重构

---

## 注意事项

- **hermes-runtime 冻结**：ADR-V2-017 正式由 goose + nanobot 替代样板位，代码保留未清
- **goose-runtime Send stub 是 Phase 2.5 scope 内的已知限制**：合约套件 v1 对应测试已 XFAIL，Phase 3 接 ACP 后转 GREEN
- **Deferred Ledger** 是 Phase 2+ 的 D 编号 single source of truth
- **Checkpoint archive**：Phase 2.5 的 `.checkpoint.json` 在 end-phase 时归档为 `.checkpoint.archive.json`
- **E2E 唯一入口**：`bash scripts/eaasp-e2e.sh`，持续演进由 `docs/design/EAASP/E2E_VERIFICATION_GUIDE.md` 规范
