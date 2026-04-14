# EAASP v2.0 Deferred Items 总账

> **Single Source of Truth** — 本文件是所有 Deferred 项的唯一权威登记处。
> 新增 / 关闭 / 迁移 D 编号都必须同步更新本文件，并在 commit message 引用 `Dxx`。

**最后更新**: 2026-04-14 (S1 batch A closed: D83/D85/D86 → ✅ closed, D90 → 🟡 P1-defer 新增)
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

## EAASP v2.0 真正需修清单（2026-04-14 重分类）

> 针对 v2.0 架构**逐项重审**结果。40 项原 active → **12 项真需修 + 26 项降级归档**。

### 🔥 P0 — Phase 2 plan 已排期（5 项 D 编号 + 2 项非 D 任务）

| ID | 标题 | 处理位置 | 影响 |
|----|------|----------|------|
| **D83** | grid-runtime ToolResult chunk 缺 `tool_name` | ✅ **S1.T4 closed 2026-04-14** @ `bdc5b8b` | runtime 侧工具识别（已修；衍生 D90 follow-up） |
| **D85** | `STOP` event `response_text` 空串 | ✅ **S1.T5 closed 2026-04-14** @ `bdc5b8b`+`d0e6cb0` | 上层 CLI 显示不出最终回答（已修 Rust+Python 双侧） |
| **D86** | claude-code-runtime SDK wrapper 丢 `ToolResultBlock` | ✅ **S1.T3 closed 2026-04-14** @ `d0e6cb0` | POST_TOOL_USE hook 空链路（已修） |
| **D84** | CLI `session events --follow` SSE 未实现 | **S4.T2** | CLI UX |
| **D89** | CLI `session close` 未实现 | **S4.T1** | CLI UX |
| (非 D) | S1.T6 ErrorClassifier | ✅ **closed 2026-04-14** @ `4001de2` | 解锁 S1.T7 + S3.T1 |
| (非 D) | S1.T7 withRetry | ✅ **closed 2026-04-14** @ `8b532cb` | Runtime 容错（graduated retry + jitter + FailoverReason routing） |

### 🟡 P1 — 功能缺口必补（4 项，新挂到 S2/S3）

| ID | 标题 | 建议挂靠 | 必须原因 |
|----|------|----------|----------|
| **D50** | `ScopedHookBody::Prompt` executor loop 未实装 | **S3 新增 T5** | SKILL 里 Prompt hook 类型功能上不存在 |
| **D53** | D49 helper 写了但 runtime 没调用 | **S3 新增 T5** | scoped-hook executor 真空 |
| **D51** | Hook stdin envelope schema 未 ADR 化 | **S3 T5 前置，新增 ADR-V2-006** | D50/D53 实施前必须先定义契约 |
| **D78** | Event payload embedding 向量索引 | **S2.T1 扩展** | 与 semantic 检索共 HNSW 架构 |

### 🟢 P2 — S2 顺带完成（2 项）

| ID | 标题 | 建议挂靠 |
|----|------|----------|
| **D12** | L2 connection-per-call → store 级长连接 | **S2.T1 前置** (hnswlib 接入时必然改) |
| **D60** | verify-v2-mvp assertion 11 hybrid search 降级 | **S2.T5 收尾** (升级为硬断言) |

### 🔵 P3 — 可选加速（1 项）

| ID | 标题 | 建议 |
|----|------|------|
| **D74** | EmitEvent gRPC 反向通道 (L1→L4) | Phase 2 完成后视情况，若 event clustering 需要再上 |

**P0+P1+P2+P3 合计 12 项（扣除非 D 编号的 S1.T6/T7）**

### 最近完成（2026-04-14）

| ID | 标题 | 证据 |
|----|------|------|
| **D87** | grid-engine agent loop 多步工作流早终止 | ✅ ADR-V2-016 · `bdc4fd5`+`c0f98f9`+`8a738b1` · Multi-model E2E |
| **D88** | hermes-runtime stdio MCP 缺失 | ⏸️ ADR-V2-017 · 由 Phase 2.5 goose-runtime 替代 |
| **S1.T6** | ErrorClassifier (hermes pattern in Rust) | ✅ `4001de2` · 14 FailoverReason variants + RecoveryActions + 36 tests |
| **D86** | claude-code-runtime SDK wrapper ToolResultBlock 丢失 | ✅ S1.T3 · `d0e6cb0` · `_tool_result_chunk` helper + UserMessage branch + 6 tests |
| **D83** | grid-runtime ToolResult chunk 缺 tool_name | ✅ S1.T4 · `bdc5b8b` · enum field + 10+ pattern-match sites (衍生 D90 WS follow-up) |
| **D85** | STOP event response_text 空 | ✅ S1.T5 · `bdc5b8b`+`d0e6cb0` · event_to_chunk(Completed) extract text + Python accumulator |
| **S1.T7** | Graduated retry with backoff | ✅ `8b532cb` · RetryPolicy::graduated() + ±15% jitter + FailoverReason::recovery_actions 路由 |

---

## D 编号详细登记（EAASP 命名空间）

**状态图例**（2026-04-14 重分类后）：
- ✅ **closed** — 已完成
- 🔄 **superseded** — 被另一 D 编号或 ADR 取代
- ⏸️ **frozen** — 对应模块冻结（如 hermes）
- 🔥 **P0-active** — Phase 2 plan 已排期
- 🟡 **P1-active** — 真功能缺口，已挂到 S2/S3
- 🟢 **P2-active** — S2 顺带
- 🔵 **P3-active** — 可选加速
- 🧹 **tech-debt** — 纯代码整洁度，不影响功能，Phase 2 后批量清
- 📦 **long-term** — Phase 4/5/6 长期路线，当前视野移除
- 🔴 **phase3-gated** — 依赖 Phase 3 身份/租户模型
- 🤔 **revisit-after-S2** — 需 S2 context engineering 决策后再判断

### D1–D15: Phase 0 S3 产生（L2/L3 服务基础设施）

| ID | 标题 | 引入 | 状态 | 证据 / 去向 |
|----|------|------|------|------|
| **D1** | grid-runtime harness 接入 `payload.policy_context` (P1) | phase0 S3.T3 | ✅ closed | ADR-V2-004 S4.T2 4b-lite |
| **D2** | grid-runtime harness 接入 `payload.memory_refs` (P3) | phase0 S3.T3 | ✅ closed | ADR-V2-004 `build_memory_preamble` |
| **D3** | harness 接入 `payload.user_preferences` (P5) + `trim_for_budget()` | phase0 S3.T3 | 🤔 revisit-after-S2 | 大 context 时代是否还需要？等 S2 决策 |
| **D4** | harness 接入 `payload.event_context` (P2) | phase0 S3.T3 | ✅ closed | Phase 1 ADR-V2-002 |
| **D5** | grpc_integration 测试迁移到 v2 telemetry envelope | phase0 S3.T3 | 🤔 revisit-after-S2 | EmitTelemetry Terminate 语义需先定 |
| **D6** | certifier 补充 SessionPayload P1–P5 字段断言 | phase0 S3.T3 | 🤔 revisit-after-S2 | 等 D3/D5 决策后一并 |
| **D7** | EmitEvent 真实实现 | phase0 S3.T3 | ✅ closed | Phase 1 ADR-V2-001 |
| **D8** | `access_scope` 真实 RBAC 执行 | phase0 S3.T1 | 🔴 phase3-gated | Phase 3 身份与租户模型 |
| **D9** | `skill_usage` 返回真实遥测 | phase0 S3.T1 | 🔴 phase3-gated | L3 telemetry ingest + L2 聚合 |
| **D10** | S3.T1 MCP REST facade 升级为真 rmcp ServerHandler | phase0 S3.T1 | 🧹 tech-debt | L2/L3/L4 统一切换契机 |
| **D11** | skill-registry `scope` 过滤在 `LIMIT` 之后 | phase0 S3.T1 | 🧹 tech-debt | migration + 索引 |
| **D12** | L2 memory-engine connection-per-call 延迟浪费 | phase0 S3.T2 | 🟢 **P2-active** | **S2.T1 前置**（hnswlib 接入必改） |
| **D13** | L2 `archive()` 创建 "archive of archive"，FTS 仍可搜 | phase0 S3.T2 | 🧹 tech-debt | 归档检索语义明确后 |
| **D14** | L2 `index._row_to_memory` 跨模块访问私有符号 | phase0 S3.T2 | 🧹 tech-debt | 重构为公共符号 |
| **D15** | L2 memory-engine 缺 `[tool.ruff]` / `[tool.mypy]` | phase0 S3.T2 | 🧹 tech-debt | 统一 lint 配置 |

### D16–D26: Phase 0 S3.T3 (L3 governance) — 全部 tech-debt 或运维

| ID | 标题 | 状态 | 备注 |
|----|------|------|------|
| **D16** | L3 policy_engine.deploy() 在 commit 前读 `created_at` | 🧹 tech-debt | SQLite RETURNING 子句 |
| **D17** | L3 validate_session() `hook["hook_id"]` KeyError 风险 | 🧹 tech-debt | 增加守卫 |
| **D18** | L3 validate_session() 对 `session_id` path param 不校验 | 🧹 tech-debt | 与 D29 合并 |
| **D19** | L3 switch_mode() 接受任意 hook_id 静默创建 override | 🧹 tech-debt | warn 或 404 |
| **D20** | `_sanitize_errors()` 仅在 L3 定义，L2 也需要 | 🧹 tech-debt | 抽到 `eaasp_common` |
| **D21** | L3 `managed_settings_versions` / `telemetry_events` 无保留策略 | 📦 long-term | 运维侧 TTL 策略 |
| **D22** | L3 无全局 FastAPI exception handler | 🧹 tech-debt | 与 D28 合并 |
| **D23** | L3 无 loguru/logging 初始化 | 🧹 tech-debt | 与 D31 合并 |
| **D24** | IDE Pyright missing-import 假阳性 | 🧹 tech-debt | DevEx, pyrightconfig.json |
| **D25** | L3 无并发部署 E2E (HTTP 栈) | 📦 long-term | 运维侧 load test |
| **D26** | L3 tests 用 `time.sleep(1.1)` 防撞秒 | 🧹 tech-debt | 单调 tiebreaker 列 |

### D27–D45: Phase 0 S3.T4+ (L4 + CLI v2)

| ID | 标题 | 状态 | 备注 |
|----|------|------|------|
| **D27** | L4 session_orchestrator `Initialize`/`Send` 占位 | 🔄 superseded by D54 | ADR-V2-004 精化 |
| **D28** | L4 无全局 exception handler (D22 复现) | 🧹 tech-debt | 与 D22 合并 |
| **D29** | L4 `/v1/sessions/{id}/*` path param 未校验 | 🧹 tech-debt | 与 D18 合并 |
| **D30** | L2/L3 `busy_timeout=5000` 未统一 | 🧹 tech-debt | `eaasp_common.connect()` |
| **D31** | L4 无 loguru 初始化 | 🧹 tech-debt | 与 D23 合并 |
| **D32** | L4 无并发 `create_session` 压力测试 | 📦 long-term | 运维侧 load test |
| **D33** | L4 SESSION_CREATED 事件 payload 重复存储 | 🧹 tech-debt | 改引用式 |
| **D34** | L4 无 Intent → skill_id NLU 解析 | 🔴 phase3-gated | Phase 3+ NLU 或 L5 portal |
| **D35** | L4 无 WebSocket / SSE event streaming | 🔥 P0-active | **合并到 D84 S4.T2** |
| **D36** | L4 event window 无 cursor (>10k 事件触发) | 📦 long-term | Phase 3+ |
| **D37** | L4 `context_assembly` 硬编码 `allow_trim_p4=False` | 🤔 revisit-after-S2 | 与 D3 关联 |
| **D38** | L4 `L2Client.search_memory` 未传 `user_id` | 🔴 phase3-gated | 跨租户隔离，Phase 3 RBAC |
| **D39** | L4 `PolicyContext.policy_version` 用 `str(int)` 而非哈希 | 🧹 tech-debt | evidence chain 时顺带 |
| **D40** | L4 `sessions.status` 三态机未实现 | 🔄 superseded by D54 | — |
| **D41** | eaasp-cli-v2 `session list` 无后端 endpoint | 🔴 phase3-gated | 多租户同步 |
| **D42** | cli-v2 test_client 未覆盖 5xx exit_code=4 | 🧹 tech-debt | 补测 |
| **D43** | cli-v2 pyproject `respx>=0.21` 未使用 | 🧹 tech-debt | 删除 dep |
| **D44** | cli-v2 `cmd_session.show` 硬编码 `limit=100` | 🧹 tech-debt | S4 时顺带暴露 flag |
| **D45** | cli-v2 响应 shape 假设 → default exit 1 | 🧹 tech-debt | response-shape guard |

### D46–D53: Phase 0 S4.T1 (Skill + Hook 扩展)

| ID | 标题 | 状态 | 备注 |
|----|------|------|------|
| **D46** | Skill `access_scope` 无 RBAC / 命名空间校验 | 🔴 phase3-gated | Phase 3 policy backend |
| **D47** | mock-scada.py argparse stub | ✅ closed | `tools/mock-scada/` (2026-04-12) |
| **D48** | `ScopedHookBody` 无 `matcher` / `tool_filter` 字段 | 🧹 tech-debt | hook schema v2.1 |
| **D49** | `${SKILL_DIR}` 变量替换 helper | ✅ closed | `substitute_hook_vars` (2026-04-12) |
| **D50** | `ScopedHookBody::Prompt` executor loop 未实装 | 🟡 **P1-active** | **S3 新 T5 hook executor** |
| **D51** | Hook stdin envelope schema 未 ADR 化 | 🟡 **P1-active** | **S3 T5 前置 ADR-V2-006** |
| **D52** | SKILL.md prose 与 L2 MCP tool schema 一致性 | ✅ closed | 逐字对照验证 (2026-04-12) |
| **D53** | D49 helper 写了但 runtime 没调用 | 🟡 **P1-active** | **S3 新 T5 hook executor** |

### D54–D61: Phase 0 S4.T2 (4b-lite + E2E verify)

| ID | 标题 | 状态 | 备注 |
|----|------|------|------|
| **D54** | L4→L1 真 gRPC binding | ✅ closed | Phase 0.5 S1 |
| **D55** | proto3 submessage presence 应统一用 `HasField` | 🧹 tech-debt | has_field 辅助 |
| **D56** | `verify-v2-mvp.sh` 只清 SQLite | 📦 long-term | 持久化后端变化时 |
| **D57** | `harness_payload_integration.rs` 复制 `build_memory_preamble` | 🧹 tech-debt | pub fn 升级 |
| **D58** | `test_initialize_injects_memory_refs_preamble` 未走 Send 全路径 | 🧹 tech-debt | SdkWrapper 替身 |
| **D59** | `Makefile::mcp-orch-start` 硬编码 `--port 8082` | 🧹 tech-debt | 改为 18082 |
| **D60** | verify-v2-mvp assertion 11 hybrid search 降级 | 🟢 **P2-active** | **S2.T5 收尾升级为硬断言** |
| **D61** | `threshold-calibration-skill.md` fixture 硬编码 `version` | 🧹 tech-debt | 解析 submit 响应 |

### D62–D66: Phase 1 Plan (容器化 + MCP 池)

| ID | 标题 | 状态 | 备注 |
|----|------|------|------|
| **D62** | Per-session tool-sandbox container lifecycle | 🔴 phase3-gated | Sandbox Tiers 未就绪 |
| **D63** | Tool-sandbox 通用基础镜像 + OCI artifact | 🔴 phase3-gated | 与 D62 |
| **D64** | T0/T1 runtime 工具容器化 | 🔴 phase3-gated | 与 D62 |
| **D65** | MCP server 多实例 / 连接池 | 🧹 tech-debt | Phase 2 S2 或 Phase 3 顺带 |
| **D66** | hermes 内置工具与 MCP monkey-patch | ⏸️ frozen | ADR-V2-017 hermes 冻结 → goose 替代 |

### D67–D72: 保留未用

**占位未分配**。若需新增 Deferred 项，请从 D90 起编号（避免与历史 D67-72 规划保留冲突）。

### D73–D80: Phase 1 Event Engine (ADR-V2-001/002/003)

| ID | 标题 | 状态 | 备注 |
|----|------|------|------|
| **D73** | Event Room 推迟 | 📦 long-term | Phase 4 |
| **D74** | EmitEvent gRPC 反向通道 (L1→L4 gRPC server) | 🔵 **P3-active** | Phase 2 可选加速，视 event clustering 需要 |
| **D75** | EventStreamBackend 切换到 NATS JetStream | 📦 long-term | Phase 6 多节点 |
| **D76** | subscribe() polling → push-based | 📦 long-term | Phase 6 |
| **D77** | TopologyAwareClusterer (L2 Ontology Service 输入) | 📦 long-term | Phase 5 |
| **D78** | 向量索引 Indexer (event payload embedding) | 🟡 **P1-active** | **S2.T1 扩展**（与 semantic 共 HNSW 架构） |
| **D79** | Pipeline 多 worker 并行处理 | 📦 long-term | Phase 6 |
| **D80** | Clusterer 因果图聚类 (parent_event_id → DAG) | 📦 long-term | Phase 4 |

### D81–D82: 保留未用

**占位未分配**。

### D83–D90: Phase 1 E2E 暴露（Phase 2 处理）+ Phase 2 衍生

| ID | 标题 | 状态 | 处理位置 |
|----|------|------|----------|
| **D83** | grid-runtime ToolResult chunk 缺 `tool_name` | ✅ closed 2026-04-14 | S1.T4 @ `bdc5b8b` (衍生 D90) |
| **D84** | CLI `session events --follow` SSE 未实现 (合并 D35) | 🔥 P0-active | **S4.T2** |
| **D85** | `STOP` event `response_text` 空 | ✅ closed 2026-04-14 | S1.T5 @ `bdc5b8b`+`d0e6cb0` |
| **D86** | claude-code-runtime SDK wrapper 丢 `ToolResultBlock` | ✅ closed 2026-04-14 | S1.T3 @ `d0e6cb0` |
| **D87** 🚨 | grid-engine agent loop 多步工作流过早终止 | ✅ closed 2026-04-14 | ADR-V2-016 · `bdc4fd5`/`c0f98f9`/`8a738b1` · Multi-model E2E |
| **D88** 🚨 | hermes-runtime stdio MCP 缺失 | ⏸️ frozen / superseded | ADR-V2-017 → Phase 2.5 goose |
| **D89** | CLI `session close` 未实现 | 🔥 P0-active | **S4.T1** |
| **D90** | `ServerMessage::ToolResult` WS schema 缺 `tool_name` 字段（grid-server + grid-platform） | 🟡 P1-defer | 下游 TS 类型级联改造，衍生自 D83。前置条件：frontend workbench/platform UI 决定是否需要工具名显示。目前 CLI / L4 gRPC 已有 tool_name；仅 WS 端丢失 |
| **D91** | HNSW 软删 tombstone rebuild 策略 | 🟡 P1-defer | `mark_deleted` 标签单调累积；达到 N% 删除率后索引膨胀/搜索延迟劣化。需要触发阈值（建议 30%）+ 后台 compaction 任务。衍生自 S2.T1 → **Phase 2.5** |
| **D92** | MockEmbedding 64-bit seed 碰撞审查 | 🔵 P3-defer | SHA-256(text)[:8] 生日碰撞约 2^32。测试场景可接受；若被误用于 staging，两条不同文本可能撞同向量。加宽到完整 32-byte digest 或明确标注 "tests-only"。衍生自 S2.T1 → **Phase 3 GA 前** |
| **D93** | `embed_batch` 顺序实现 | 🟡 P1-defer | `OllamaEmbedding` / `MockEmbedding` 均 `for text in texts: await embed(text)` N 次。Ollama/TEI 均支持真正 batched POST。衍生自 S2.T1 → **S2.T4 或 Phase 2.5** hybrid-search perf pass |
| **D94** | MemoryStore 单例 + 共享连接重构（D12 收尾） | 🟡 P1-defer | S2.T1 仅完成 schema 迁移 + pack/unpack helper；`MemoryFileStore`/`AnchorStore`/`HybridIndex` 仍 per-call `connect()`。全 store 单例化需求较大，与 Phase 2.5 runtime ecosystem 工作合并推进 |
| **D95** | FTS 命中的语义分数从 DB `embedding_vec` 回填 | 🔵 P2-defer | S2.T2 union 阶段只对同时出现在 HNSW 结果中的 FTS 命中打 `semantic_score`；若 HNSW add 静默失败，FTS 命中永远 `sem_score=0`。可从 DB BLOB unpack + 与 query_vec 计算 cosine 回填。衍生自 S2.T2 → **S2.T4 或 Phase 2.5** |
| **D96** | 用户自定义 memory_id 含 `:v` 子串导致 HNSW key 解析丢失 | 🔵 P3-defer | HNSW key 格式 `{memory_id}:v{N}`，`split(":v")` 在 memory_id 含 `:v` 时产生 3 段被静默跳过。建议 (a) `MemoryFileIn.memory_id` 校验禁止 `:v`，或 (b) `rsplit(":v", 1)`。默认自动生成 `mem_{uuid}` 不受影响，仅用户传入自定 id 的边角场景。衍生自 S2.T2 reviewer M1 → **Phase 3 前** |
| **D97** | `weights=(0.0, 0.0)` 退化情形缺少构造期告警 | 🔵 P3-defer | 所有候选 `score==0`，插入序生效无信息。运维场景罕见，但建议 `HybridIndex.__init__` 下发 `logger.warning("Both weights zero; results will be unordered")`。衍生自 S2.T2 reviewer M2 → **Phase 2.5** |
| **D98** | `HybridIndex.search()` 每次重建 HNSWVectorIndex | 🟡 P1-defer | 每次 search 重新 `_try_load_sync()` 读磁盘 ~10ms；小索引可接受，QPS 上升后变成 perf 热点。承继 T1 同类问题。应随 D94 MemoryStore 单例化一起改为进程级缓存。衍生自 S2.T2 reviewer N3 → **Phase 2.5** |
| **D99** | MCP dispatcher 参数类型强制转换抛原生 `ValueError`/`TypeError` 而非 `ToolError("invalid_arg")` | 🔵 P3-defer | `mcp_tools.py::_memory_list` / `_memory_search` 对 `limit`/`offset`/`top_k` 用 `int(args.get(...))` 直接转换；非法类型/字符串会抛原生异常，绕过 `_require` 的 `ToolError` 包装。预期下游 MCP SDK / REST body parsing 应先做 JSON-schema 校验，但 dispatcher 层应有兜底。不是 S2.T3 引入的回归，是承继既有模式。衍生自 S2.T3 reviewer → **Phase 2.5 dispatcher harden sweep** |

---

## 新增 Deferred 编号规则

**当前最大编号**: D99
**下一个可用**: **D100** (跳过保留段 D67-D72 / D81-D82)

**引入流程**:
1. 在新 Deferred 产生的 plan 文件里以表格形式定义 `| D90 | 标题 | 去向 |`
2. **同步追加到本 ledger** 的相应 section（不要只写在 plan 里）
3. 在 commit message 引用 `Dxx`
4. 在 [状态变更日志](#状态变更日志) 新增一行

---

## 状态变更日志

| 日期 | ID | 变更 | 证据 |
|------|-----|------|------|
| 2026-04-14 | D3, D5, D6, D37 | active → 🤔 revisit-after-S2 | 需 S2 context engineering 决策后判断 |
| 2026-04-14 | D8, D9, D34, D38, D41, D46, D62, D63, D64 | active → 🔴 phase3-gated | 依赖 Phase 3 身份/租户模型 |
| 2026-04-14 | D21, D25, D32, D36, D56, D73, D75, D76, D77, D79, D80 | active → 📦 long-term | Phase 4/5/6 路线 |
| 2026-04-14 | D10/11/13/14/15/16/17/18/19/20/22/23/24/26/28/29/30/31/33/39/42/43/44/45/48/55/57/58/59/61/65 | active → 🧹 tech-debt | 纯技术债，Phase 2 后批量清 |
| 2026-04-14 | D12, D60 | active → 🟢 P2-active | S2 顺带完成 |
| 2026-04-14 | D50, D51, D53, D78 | active → 🟡 P1-active | 功能缺口必补，挂到 S2/S3 |
| 2026-04-14 | D35 | active → 🔥 合并到 D84 | SSE event streaming 与 CLI --follow 合并 |
| 2026-04-14 | D74 | active → 🔵 P3-active | Phase 2 可选加速 |
| 2026-04-14 | — | **重分类** | 40 active → 12 真需修 + 26 降级归档 |
| 2026-04-14 | D87 | active → ✅ closed | ADR-V2-016, multi-model E2E PASS |
| 2026-04-14 | D88 | active → ⏸️ frozen/superseded | ADR-V2-017 (hermes 冻结) |
| 2026-04-14 | D83 | active → ✅ closed | S1.T4 @ `bdc5b8b` (衍生 D90) |
| 2026-04-14 | D85 | active → ✅ closed | S1.T5 @ `bdc5b8b`+`d0e6cb0` |
| 2026-04-14 | D86 | active → ✅ closed | S1.T3 @ `d0e6cb0` |
| 2026-04-14 | D90 | **新增** 🟡 P1-defer | ServerMessage WS schema tool_name 衍生自 D83，前置 frontend UI 决策 |
| 2026-04-14 | D91 | **新增** 🟡 P1-defer | HNSW tombstone rebuild，S2.T1 review 提出，→ Phase 2.5 |
| 2026-04-14 | D92 | **新增** 🔵 P3-defer | MockEmbedding 碰撞审查，S2.T1 review 提出，→ Phase 3 GA 前 |
| 2026-04-14 | D93 | **新增** 🟡 P1-defer | embed_batch 顺序实现，S2.T1 review 提出，→ S2.T4 或 Phase 2.5 |
| 2026-04-14 | D94 | **新增** 🟡 P1-defer | MemoryStore 单例 refactor（D12 收尾），S2.T1 review 提出，→ Phase 2.5 |
| 2026-04-15 | D95 | **新增** 🔵 P2-defer | FTS 命中的 semantic_score 从 DB `embedding_vec` 回填，S2.T2 衍生 → S2.T4 或 Phase 2.5 |
| 2026-04-15 | D96 | **新增** 🔵 P3-defer | 用户自定 memory_id 含 `:v` 子串 HNSW key 解析丢失（reviewer M1），→ Phase 3 前 |
| 2026-04-15 | D97 | **新增** 🔵 P3-defer | `weights=(0,0)` 退化情形缺构造期告警（reviewer M2），→ Phase 2.5 |
| 2026-04-15 | D98 | **新增** 🟡 P1-defer | HybridIndex 每次 search 重建 HNSW（reviewer N3，承继 T1）→ Phase 2.5 |
| 2026-04-15 | D99 | **新增** 🔵 P3-defer | MCP dispatcher 参数类型转换抛原生异常（S2.T3 reviewer Major）→ Phase 2.5 |
| 2026-04-14 | — | **ledger 创建** | 收敛 D1–D89 到 single source of truth |
| 2026-04-12 | D1, D2 | active → ✅ closed | ADR-V2-004 S4.T2 4b-lite |
| 2026-04-12 | D47, D49, D52 | active → ✅ closed | S4.T2 前置修复 |
| 2026-04-12 | D27, D40 | active → 🔄 superseded by D54 | ADR-V2-004 精化 |
| 2026-04-12 | D54 | active → ✅ closed | Phase 0.5 S1 实装 |
| 2026-04-11 | D7 | active → ✅ closed | Phase 1 Event Engine |

---

## 统计汇总 (2026-04-14 重分类后 — EAASP v2.0 对齐)

**真正需处理的 D 项 = 11 项**（P0 + P1 + P2 + P3 剩余 + D90 新增）

| 状态 | 数量 | D 编号 | 含义 |
|------|------|--------|------|
| ✅ **closed** | 12 | D1, D2, D4, D7, D47, D49, D52, D54, D83, D85, D86, D87 | 已完成（2026-04-14 新增 D83/D85/D86） |
| 🔄 **superseded** | 2 | D27→D54, D40→D54 | 被其他 D 或 ADR 取代 |
| ⏸️ **frozen** | 2 | D66, D88 | hermes 冻结，Phase 2.5 goose 替代 |
| 🔥 **P0-active** | 2 | D84 (含D35), D89 | **Phase 2 plan S4 排期** |
| 🟡 **P1-active** | 4 | D50, D51, D53, D78 | **挂到 S2/S3 新任务必做** |
| 🟡 **P1-defer** | 4 | D90, D93, D94, D98 | 前置 frontend UI / Phase 2.5 refactor 合并 |
| 🔵 **P2-defer** | 1 | D95 | FTS semantic_score 回填，S2.T4 或 Phase 2.5 |
| 🔵 **P3-defer** | 4 | D96, D97, D92, D99 | 边角场景 / 告警优化，Phase 3 GA 前 |
| 🟢 **P2-active** | 2 | D12, D60 | S2 顺带完成 |
| 🔵 **P3-active** | 1 | D74 | Phase 2 可选加速 |
| 🤔 **revisit-after-S2** | 4 | D3, D5, D6, D37 | 等 S2 context engineering 决策 |
| 🔴 **phase3-gated** | 9 | D8, D9, D34, D38, D41, D46, D62, D63, D64 | Phase 3 身份/租户模型 |
| 📦 **long-term** | 11 | D21, D25, D32, D36, D56, D73, D75, D76, D77, D79, D80 | Phase 4/5/6 |
| 🧹 **tech-debt** | 18 | D10, D11, D13, D14, D15, D16, D17, D18, D19, D20, D22, D23, D24, D26, D28, D29, D30, D31, D33, D39, D42, D43, D44, D45, D48, D55, D57, D58, D59, D61, D65 | Phase 2 后批量清 |
| **占位未用** | — | D67-D72, D81-D82 | 不计入 |
| **合计** | **68** | D1–D89 去重（81 表格行含 D66/D88 各出现 2 次） | |

### 给开发者的一句话指引

| 角色 | 真正要关心的 |
|------|--------------|
| **当前 Phase 2 推进** | 13 项（P0×6 + P1×4 + P2×2 + P3×1）— 全部已挂到具体 Stage 任务 |
| **Phase 2 结束 end-phase** | P0/P1/P2 完成 + 启动 tech-debt batch cleanup |
| **Phase 3 规划时** | 查 🔴 phase3-gated + 🤔 revisit 四项 |
| **Phase 4+ 长期规划** | 查 📦 long-term |

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
