# EAASP v2.0 Deferred Items 总账

> **Single Source of Truth** — 本文件是所有 Deferred 项的唯一权威登记处。
> 新增 / 关闭 / 迁移 D 编号都必须同步更新本文件，并在 commit message 引用 `Dxx`。

**最后更新**: 2026-04-14
**维护规则**: 每次 end-phase 或 Deferred 状态变更时更新 [状态变更日志](#状态变更日志) 并同步 [全局活跃清单](#全局活跃清单-eaasp-v20)。

---

## 重要发现：多命名空间

项目历史上的 D 编号**不是单一全局空间**，存在 **4 个独立命名空间**：

| 命名空间 | 来源 | 编号范围 | 状态 |
|----------|------|----------|------|
| **Legacy-Octo** | 旧 octo-sandbox (2026-03-xx phase 文档) | D1–D13（多个 plan 各自独立） | 均为 pre-EAASP，多已 ✅ 完成或并入后续 plan |
| **EAASP Phase 0** | `2026-04-11-v2-mvp-phase0-plan.md` | D1–D61 | 本 ledger 主体 |
| **EAASP Phase 1 Plan** | `2026-04-13-v2-phase1-plan.md` | D62–D66（推迟容器化） | 继承到 Phase 2/3 |
| **EAASP Phase 1 Design** | `PHASE1_EVENT_ENGINE_DESIGN.md` + ADR-V2-001/002/003 | D73–D80 | Event Engine 扩展方向 |
| **EAASP Phase 1 E2E** | 运行时暴露 (checkpoint.json) | D83–D89 | Phase 2 处理中 |

⚠️ **编号缺口**（非冲突，保留未用）:
- D67–D72: 规划未分配
- D81–D82: 规划未分配

**本 ledger 的主编号以 EAASP 命名空间为准（D1–D89）**。Legacy-Octo 的早期 D 编号仅在附录列出。

---

## 全局活跃清单 (EAASP v2.0)

**当前活跃（需处理）: 40 项 / 84 项总计**

### Phase 2 当前入口待办（5 项）

| ID | 标题 | 处理位置 | 优先级 |
|----|------|----------|--------|
| **D83** | grid-runtime ToolResult 缺 `tool_name` | S1.T4 | 中 |
| **D84** | CLI `session events --follow` SSE 未实现 | S4.T2 | 低 |
| **D85** | `STOP` event `response_text` 空串 | S1.T5 | 中 |
| **D86** | claude-code-runtime SDK wrapper 丢 `ToolResultBlock` | S1.T3 | 高 |
| **D89** | CLI `session close` 未实现 | S4.T1 | 低 |

### 最近完成（2026-04-14）

| ID | 标题 | 状态 | 证据 |
|----|------|------|------|
| **D87** | grid-engine agent loop 多步工作流早终止 | ✅ **FIXED** | ADR-V2-016 Accepted · commits `bdc4fd5` + `c0f98f9` + `8a738b1` · Multi-model E2E verified |
| **D88** | hermes-runtime stdio MCP 缺失 | ⏸️ **FROZEN / SUPERSEDED** | ADR-V2-017 Accepted · 由 Phase 2.5 goose-runtime 完整替代解决 |

---

## D 编号详细登记（EAASP 命名空间）

### D1–D15: Phase 0 S3 产生（L2/L3 服务基础设施）

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D1** | grid-runtime harness 接入 `payload.policy_context` (P1) | phase0 S3.T3 (2026-04-11) | ✅ closed | ADR-V2-004 S4.T2 4b-lite (2026-04-12) | — |
| **D2** | grid-runtime harness 接入 `payload.memory_refs` (P3) | phase0 S3.T3 | ✅ closed | ADR-V2-004 S4.T2 `build_memory_preamble` | — |
| **D3** | harness 接入 `payload.user_preferences` (P5) + `trim_for_budget()` | phase0 S3.T3 | 🟡 active | context budget 策略待定 | Phase 2 context engineering |
| **D4** | harness 接入 `payload.event_context` (P2) | phase0 S3.T3 | ✅ closed | Phase 1 ADR-V2-002 EventStreamBackend | — |
| **D5** | grpc_integration 测试迁移到 v2 telemetry envelope | phase0 S3.T3 | 🟡 active | EmitTelemetry Terminate 语义需先明确 | Phase 2 |
| **D6** | certifier 补充 SessionPayload P1–P5 字段断言 | phase0 S3.T3 | 🟡 active | D1–D4 落地后可断言 | Phase 2 |
| **D7** | EmitEvent 真实实现 (当前 `Status::unimplemented`) | phase0 S3.T3 | ✅ closed | Phase 1 ADR-V2-001 + Event Engine 上线 | — |
| **D8** | `access_scope` 真实 RBAC 执行 | phase0 S3.T1 | 🔴 planned | Phase 3 身份与租户模型 | Phase 3 |
| **D9** | `skill_usage` 返回真实遥测 | phase0 S3.T1 | 🔴 planned | L3 telemetry ingest + L2 聚合 | Phase 2 后续 |
| **D10** | S3.T1 MCP REST facade 升级为真 rmcp ServerHandler | phase0 S3.T1 | 🟡 active | L2/L3/L4 统一切换契机 | Phase 2+ |
| **D11** | skill-registry `scope` 过滤在 `LIMIT` 之后 (scope=X&limit=10 可能少于 10 条) | phase0 S3.T1 | 🟡 active | migration + 索引 | Phase 2+ |
| **D12** | L2 memory-engine connection-per-call 延迟浪费 | phase0 S3.T2 | 🟡 active | store 级长连接 | Phase 2 S2 |
| **D13** | L2 `archive()` 创建 "archive of archive"，FTS 仍可搜 | phase0 S3.T2 | 🟡 active | 归档检索语义明确 | Phase 2 S2 |
| **D14** | L2 `index._row_to_memory` 跨模块访问私有符号 | phase0 S3.T2 | 🟡 active | 重构为公共符号 | 技术债 |
| **D15** | L2 memory-engine 缺 `[tool.ruff]` / `[tool.mypy]` 配置块 | phase0 S3.T2 | 🟡 active | 统一 lint 配置 | 技术债 |

### D16–D26: Phase 0 S3.T3 (L3 governance)

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D16** | L3 policy_engine.deploy() 在 commit 前读 `created_at` | phase0 S3.T3 | 🟡 active | SQLite RETURNING 子句 | 技术债 |
| **D17** | L3 api.validate_session() `hook["hook_id"]` KeyError 风险 | phase0 S3.T3 | 🟡 active | 增加守卫 | 技术债 |
| **D18** | L3 validate_session() 对 `session_id` path param 不校验 | phase0 S3.T3 | 🟡 active | Path pattern 守卫 | 与 D29 合并 |
| **D19** | L3 switch_mode() 接受任意 hook_id 静默创建 override | phase0 S3.T3 | 🟡 active | warn 或 404 | 技术债 |
| **D20** | `_sanitize_errors()` 仅在 L3 定义，L2 也需要 | phase0 S3.T3 | 🟡 active | 抽到 `eaasp_common` | Phase 2+ |
| **D21** | L3 `managed_settings_versions` / `telemetry_events` 无保留策略 | phase0 S3.T3 | 🟡 active | TTL/archive 策略 | 运维侧 |
| **D22** | L3 无全局 FastAPI exception handler | phase0 S3.T3 | 🟡 active | 与 D28 合并 | Phase 2+ |
| **D23** | L3 无 loguru/logging 初始化 | phase0 S3.T3 | 🟡 active | 与 D31 合并 | Phase 2+ |
| **D24** | IDE Pyright missing-import 假阳性 (L1/L2/L3 同病) | phase0 S3.T3 | 🟡 active | pyrightconfig.json 或 workspace 配置 | DevEx |
| **D25** | L3 无并发部署 E2E (HTTP 栈) | phase0 S3.T3 | 🟡 active | real uvicorn load test | Phase 2+ |
| **D26** | L3 tests 用 `time.sleep(1.1)` 防撞秒 | phase0 S3.T3 | 🟡 active | 单调 tiebreaker 列 | 技术债 |

### D27–D45: Phase 0 S3.T4+ (L4 + CLI v2)

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D27** | L4 session_orchestrator `Initialize`/`Send` 占位 | phase0 S3.T4 | 🔄 **superseded by D54** | ADR-V2-004 精化为 D54 | 见 D54 |
| **D28** | L4 无全局 exception handler (D22 复现) | phase0 S3.T4 | 🟡 active | 与 D22 合并 | Phase 2+ |
| **D29** | L4 `/v1/sessions/{id}/*` path param 未校验 (D18 复现) | phase0 S3.T4 | 🟡 active | 与 D18 合并 | Phase 2+ |
| **D30** | L2/L3 `busy_timeout=5000` 未统一 | phase0 S3.T4 | 🟡 active | `eaasp_common.connect()` | Phase 2+ |
| **D31** | L4 无 loguru 初始化 (D23 复现) | phase0 S3.T4 | 🟡 active | 与 D23 合并 | Phase 2+ |
| **D32** | L4 无并发 `create_session` 压力测试 (D25 复现) | phase0 S3.T4 | 🟡 active | 与 D25 合并 | Phase 2+ |
| **D33** | L4 SESSION_CREATED 事件 payload 与 sessions 重复存储 | phase0 S3.T4 | 🟡 active | 改为 `{session_id: id}` 引用 | 技术债 |
| **D34** | L4 无 Intent → skill_id NLU 解析 | phase0 S3.T4 | 🔴 planned | Phase 1 NLU 或 L5 portal | Phase 3+ |
| **D35** | L4 无 WebSocket / SSE event streaming | phase0 S3.T4 | 🟡 active | 与 D84 关联 | **D84 Phase 2 S4.T2** |
| **D36** | L4 event window `(from_seq, to_seq, limit)` 无 cursor | phase0 S3.T4 | 🟡 active | 事件量 >10k 时补 cursor | Phase 3+ |
| **D37** | L4 `context_assembly` 硬编码 `allow_trim_p4=False` | phase0 S3.T4 | 🟡 active | runtime budget negotiation 后 | Phase 2 context |
| **D38** | L4 `L2Client.search_memory` 未传 `user_id` (跨租户泄漏风险) | phase0 S3.T4 | 🔴 planned | Phase 3 RBAC + L2 user_id 过滤 (D8 关联) | Phase 3 |
| **D39** | L4 `PolicyContext.policy_version` 用 `str(int)` 而非哈希 | phase0 S3.T4 | 🟡 active | managed_settings_version SHA-256 | Phase 1 evidence chain |
| **D40** | L4 `sessions.status` 只有 `created` 三态机未实现 | phase0 S3.T4 | 🔄 **superseded by D54** | 与 D27→D54 合并 | 见 D54 |
| **D41** | eaasp-cli-v2 `session list` 无后端 endpoint | phase0 S3.T5 | 🟡 active | L4 `GET /v1/sessions` 列表端点 | Phase 3 多租户 |
| **D42** | cli-v2 test_client 未覆盖 5xx exit_code=4 | phase0 S3.T5 | 🟡 active | 补测 | 技术债 |
| **D43** | cli-v2 pyproject `respx>=0.21` 未使用 | phase0 S3.T5 | 🟡 active | 删除 dep 或补对照测试 | 技术债 |
| **D44** | cli-v2 `cmd_session.show` 硬编码 `limit=100` | phase0 S3.T5 | 🟡 active | 暴露 flag 或分页 loop | Phase 2 S4 |
| **D45** | cli-v2 响应 shape 假设，未预期 shape → default exit 1 | phase0 S3.T5 | 🟡 active | 共享 response-shape guard | 技术债 |

### D46–D53: Phase 0 S4.T1 (Skill + Hook 扩展)

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D46** | Skill `access_scope` 无 RBAC / 命名空间校验 | phase0 S4.T1 | 🔴 planned | Phase 3 policy backend | Phase 3 |
| **D47** | mock-scada.py argparse stub，非真实 MCP stdio server | phase0 S4.T1 | ✅ closed | S4.T2 前置补齐 (2026-04-12) `tools/mock-scada/` | — |
| **D48** | `ScopedHookBody` 无 `matcher` / `tool_filter` 字段 | phase0 S4.T1 | 🟡 active | hook schema v2.1 | Phase 2+ |
| **D49** | `${SKILL_DIR}` 变量替换 runtime 未实装 | phase0 S4.T1 | ✅ closed | `substitute_hook_vars` helper (2026-04-12) | **runtime exec 侧仍见 D53** |
| **D50** | `ScopedHookBody::Prompt` prompt-hook executor loop 未实装 | phase0 S4.T1 | 🟡 active | Phase 2 prompt-hook runtime (调 LLM 做 yes/no 决策) | Phase 2+ |
| **D51** | Hook stdin envelope schema 未 ADR 化 | phase0 S4.T1 | 🟡 active | ADR-V2-006 (envelope 契约) | Phase 2+ |
| **D52** | SKILL.md prose 与 L2 MCP tool schema 参数名一致性 | phase0 S4.T1 | ✅ closed | 逐字对照验证 (2026-04-12) 零不匹配 | — |
| **D53** | D49 helper 已实现但两 runtime hook 执行路径未调用 | phase0 S4.T1 | 🟡 active | Phase 2 hook executor 设计 + runtime 接入 | **Phase 2 S3 hook executor** |

### D54–D61: Phase 0 S4.T2 (4b-lite + E2E verify)

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D54** | **L4→L1 真 gRPC binding** (supersedes D27) | phase0 S4.T2 / ADR-V2-004 | ✅ closed | Phase 0.5 S1 实装 | — |
| **D55** | proto3 submessage presence 应统一用 `HasField` | phase0 S4.T2 | 🟡 active | `has_field` 辅助或 lint rule | 技术债 |
| **D56** | `verify-v2-mvp.sh` 只清 SQLite，后端变化需同步 | phase0 S4.T2 | 🟡 active | 持久化后端变化时更新 | DevOps |
| **D57** | `harness_payload_integration.rs` 复制 `build_memory_preamble` 格式 | phase0 S4.T2 | 🟡 active | 升级为 `pub fn` | 技术债 |
| **D58** | `test_initialize_injects_memory_refs_preamble` 未走 Send 完整路径 | phase0 S4.T2 | 🟡 active | 用 SdkWrapper 替身补真 Send-path 测试 | 技术债 |
| **D59** | `Makefile::mcp-orch-start` 硬编码 `--port 8082` | phase0 S4.T2 | 🟡 active | 改为 18082 + EAASP_MCP_ORCHESTRATOR_PORT | 技术债 |
| **D60** | `verify-v2-mvp.py` assertion 11 `memory_id_1 in matched_ids` 降级 | phase0 S4.T2 | 🟡 active | L2 hybrid search 确定性排名后升级为硬 failure | Phase 2 S2 |
| **D61** | `threshold-calibration-skill.md` fixture 硬编码 `version: 0.1.0` | phase0 S4.T2 | 🟡 active | 从 submit 响应解析版本号 | 技术债 |

### D62–D66: Phase 1 Plan (容器化 + MCP 池)

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D62** | Per-session tool-sandbox container lifecycle | phase1 plan | 🔴 planned | Sandbox Tiers 未就绪 | Phase 3 |
| **D63** | Tool-sandbox 通用基础镜像 + OCI artifact | phase1 plan | 🔴 planned | 与 D62 | Phase 3 |
| **D64** | T0/T1 runtime 工具容器化 | phase1 plan | 🔴 planned | 与 D62 | Phase 3 |
| **D65** | MCP server 多实例 / 连接池 | phase1 plan | 🟡 active | Memory Engine 增强 | Phase 2 S2 |
| **D66** | hermes 内置工具与 MCP monkey-patch 叠加修复 | phase1 plan | ⏸️ **frozen** | hermes 冻结 (ADR-V2-017) → 由 goose 替代 | Phase 2.5 goose |

### D67–D72: 保留未用

**占位未分配**。若需新增 Deferred 项，请从 D90 起编号（避免与历史 D67-72 规划保留冲突）。

### D73–D80: Phase 1 Event Engine (ADR-V2-001/002/003)

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D73** | Event Room 推迟 (Phase 1 session = event 容器) | ADR-V2-001 | 🔴 planned | Phase 4 | Phase 4 |
| **D74** | EmitEvent gRPC 反向通道 (L1→L4 gRPC server) | ADR-V2-001 | 🔴 planned | Phase 2 | Phase 2+ |
| **D75** | EventStreamBackend 切换到 NATS JetStream (多节点) | ADR-V2-002 | 🔴 planned | Phase 6 | Phase 6 |
| **D76** | subscribe() polling → push-based (NATS/WebSocket) | ADR-V2-002 | 🔴 planned | Phase 6 | Phase 6 |
| **D77** | TopologyAwareClusterer (L2 Ontology Service 输入) | ADR-V2-003 | 🔴 planned | Phase 5 | Phase 5 |
| **D78** | 向量索引 Indexer (event payload embedding) | ADR-V2-003 | 🔴 planned | Phase 2 | Phase 2 S2 |
| **D79** | Pipeline 多 worker 并行处理 | ADR-V2-003 | 🔴 planned | Phase 6 | Phase 6 |
| **D80** | Clusterer 因果图聚类 (parent_event_id → DAG) | ADR-V2-003 | 🔴 planned | Phase 4 | Phase 4 |

### D81–D82: 保留未用

**占位未分配**。

### D83–D89: Phase 1 E2E 暴露（Phase 2 处理）

| ID | 标题 | 引入 | 状态 | 证据 | 去向 |
|----|------|------|------|------|------|
| **D83** | grid-runtime ToolResult chunk 缺 `tool_name` | Phase 1 E2E | 🟡 **active** | checkpoint.json | **Phase 2 S1.T4** |
| **D84** | CLI `session events --follow` SSE 未实现 | Phase 1 E2E | 🟡 **active** | checkpoint.json | **Phase 2 S4.T2** |
| **D85** | `STOP` event `response_text` 空 | Phase 1 E2E | 🟡 **active** | checkpoint.json | **Phase 2 S1.T5** |
| **D86** | claude-code-runtime SDK wrapper 丢 `ToolResultBlock` | Phase 1 E2E | 🟡 **active** | checkpoint.json | **Phase 2 S1.T3** |
| **D87** 🚨 | grid-engine agent loop 多步工作流过早终止 | Phase 1 E2E | ✅ **closed 2026-04-14** | ADR-V2-016 Accepted · commits `bdc4fd5` + `c0f98f9` + `8a738b1` · Multi-model E2E verified | — |
| **D88** 🚨 | hermes-runtime stdio MCP 缺失 | Phase 1 E2E | ⏸️ **frozen / superseded** | ADR-V2-017 Accepted · hermes 冻结，由 Phase 2.5 goose-runtime 完整替代 | Phase 2.5 goose |
| **D89** | CLI `session close` 未实现 | Phase 1 E2E | 🟡 **active** | checkpoint.json | **Phase 2 S4.T1** |

---

## 新增 Deferred 编号规则

**当前最大编号**: D89
**下一个可用**: **D90** (跳过保留段 D67-D72 / D81-D82)

**引入流程**:
1. 在新 Deferred 产生的 plan 文件里以表格形式定义 `| D90 | 标题 | 去向 |`
2. **同步追加到本 ledger** 的相应 section（不要只写在 plan 里）
3. 在 commit message 引用 `Dxx`
4. 在 [状态变更日志](#状态变更日志) 新增一行

---

## 状态变更日志

| 日期 | ID | 变更 | 证据 |
|------|-----|------|------|
| 2026-04-14 | D87 | active → ✅ closed | ADR-V2-016 Accepted, multi-model E2E PASS |
| 2026-04-14 | D88 | active → ⏸️ frozen/superseded | ADR-V2-017 Accepted (hermes 冻结) |
| 2026-04-14 | — | **ledger 创建** | 初始化，收敛 D1–D89 到 single source of truth |
| 2026-04-12 | D1, D2 | active → ✅ closed | ADR-V2-004 S4.T2 4b-lite (4个 commit) |
| 2026-04-12 | D47, D49, D52 | active → ✅ closed | S4.T2 前置修复 |
| 2026-04-12 | D27, D40 | active → 🔄 superseded by D54 | ADR-V2-004 精化 |
| 2026-04-12 | D54 | active → ✅ closed | Phase 0.5 S1 实装 |
| 2026-04-11 | D7 | active → ✅ closed | Phase 1 Event Engine 完成 |

---

## 统计汇总 (2026-04-14)

| 状态 | 数量 | 说明 |
|------|------|------|
| ✅ **closed** | 10 | D1, D2, D4, D7, D47, D49, D52, D54, D87 + legacy |
| 🔄 **superseded** | 2 | D27 → D54; D40 → D54 |
| ⏸️ **frozen** | 2 | D66, D88 (hermes 冻结，由 Phase 2.5 goose 替代) |
| 🟡 **active** (待处理) | 40 | 大部分技术债 + Phase 2 入口 5 项 |
| 🔴 **planned** (推迟到后续 Phase) | 14 | Phase 2.5 / 3 / 4 / 5 / 6 |
| **占位未用** | 16 | D67-D72, D81-D82 编号段 |
| **总计有效** | **68** | D 编号 1–89 扣除占位 |

---

## 附录 A: Legacy-Octo D 编号（pre-EAASP, 独立命名空间）

以下文件各自维护独立的 D 编号空间，与 EAASP 全局空间无关。仅供历史查询：

| 文件 | D 编号 | 状态 |
|------|--------|------|
| `2026-03-02-phase2-9-agent-registry.md` | D1, D2, D3 | ✅ 均已补 |
| `2026-03-04-octo-platform-design.md` | D1, D2, D3 | 大部分 ✅ 已补 |
| `2026-03-04-v1.0-release-sprint-plan.md` | D1–D5 | ✅ 均已补 |
| `2026-03-09-harness-implementation.md` | D1–D6 | 大部分 ✅ 已补 |
| `2026-03-10-deferred-d2-d4-d5.md` | D3, D6, D7 | ⏳ pending |
| `2026-03-10-deferred-d3-d6-d7.md` | D8–D13 | ⏳ pending |
| `2026-03-10-octo-cli-redesign.md` | D1–D5 | 部分 ⏳ |
| `2026-03-11-deferred-completion.md` | D1–D7 | ⏳ |
| `2026-03-11-wave6-production-hardening.md` | D2, D3, D5, D7, D8, D9 | ⏳ 新增 |
| `2026-03-15-phase-m-eval-cli.md` | D1–D3 | ✅ |
| `2026-03-15-phase-n-agent-debug.md` | D1–D4 | ✅ 均已补 (Phase O) |
| `2026-03-22-phase-u-tui-production-hardening.md` | D1–D10 | 设计决策，非 deferred |

**结论**: Legacy-Octo D 编号大部分已在各自 plan 里闭环。若需追踪 Octo 产品线的遗留技术债，单独建立 `docs/plans/LEGACY_OCTO_DEBT.md`。本 ledger 只管 EAASP 全局命名空间。

---

## 附录 B: 引用格式规范

**commit message**:
```
fix(eaasp): D85 — STOP event response_text populated
```

**plan / ADR 文档**:
```markdown
**关联 Deferred**: D83 (S1.T4), D85 (S1.T5)
**Supersedes**: D27 (原 L4→L1 gRPC 占位描述)
```

**本 ledger 更新**:
每次状态变更后追加到 [状态变更日志](#状态变更日志)，并同步对应 section 的状态列。
