# octo-sandbox Memory Index

**Project**: octo-sandbox
**Purpose**: Track session work, decisions, and progress for cross-session continuity

---

## Phase 2.3 - MCP Workbench [COMPLETED 2026-02-27]

- 15:00 | Phase 2.3 MCP Workbench 完成！12/12 任务全部完成
  - Backend: DB migration v3, MCP storage, Manager 扩展, 3 API 模块
  - Frontend: MCP tab + McpWorkbench + ServerList/ToolInvoker/LogViewer
  - API 集成完成，带 mock 数据降级
- 12:31 | Phase 2.3 开始: 启动 MCP Workbench 设计
- 12:40 | MCP Workbench 需求确认: 动态添加 MCP Server、分级日志、持久化
- 12:50 | MCP Workbench 设计方案完成 (docs/design/MCP_WORKBENCH_DESIGN.md)
- 12:50 | 实施计划完成: 12 个任务 (docs/plans/...implementation.md)

---

## Phase 2.2 - 记忆系统完整 [COMPLETED 2026-02-27]

- 11:35 | Phase 2.2 开始实施：memory_recall + memory_forget tools + Memory Explorer UI
- 11:45 | Phase 2.2 完成：实现 memory_recall 语义检索、memory_forget 删除工具、Memory Explorer 前端页面（Working/Session/Persistent 视图）

---

## [Active Work]

- 18:45 | ARCHITECTURE_DESIGN.md v1.1 完成 + 三文档一致性更新 [claude-mem #2885]
  - 关键修正：双场景沙箱定位（场景A工具执行安全=Phase 2，场景B CC/OC圈养=Phase 3）
  - 新增 §5.0 双场景沙箱 + §5.5 工具执行安全策略（ExecSecurityMode/env_clear/WASM Fuel+Epoch/SSRF/路径遍历）
  - 新增 §3.2.1 Loop Guard/Circuit Breaker + §3.7.1 Context Overflow 4+1 阶段
  - 技术决策 S-05~S-08，Phase 2.4 OpenFang P0 模块表，Phase 3 参考索引表
  - CONTEXT_ENGINEERING_DESIGN.md: DegradationLevel 4→6 变体，阈值修正为 60%/70%/90%
  - MCP_WORKBENCH_DESIGN.md: 新增 Phase 2.4 SSE Transport 计划说明
- 17:30 | OpenFang 架构研究完成！
  - 创建完整路线图: docs/design/OPENFANG_ARCHITECTURE_ROADMAP.md
  - 14 crate 模块分析 (Kernel, Runtime, Memory, API, Channels, Hands...)
  - 整合里程碑已添加到 CHECKPOINT_PLAN.md
  - 参考文档已创建: docs/plans/2026-02-27-openfang-architecture-research.md
- 17:00 | OpenFang 架构研究阶段开始
  - 研究 openfang-kernel: Kernel, AgentRegistry, EventBus, Scheduler
  - 研究 openfang-runtime: AgentLoop, MCP Client, 27 LLM Providers
  - 研究 openfang-memory: 三层存储 (Structured + Semantic + Knowledge)
  - 研究 openfang-api: 140+ Axum 端点设计
  - 对比分析完成，制定引入计划
  - 产出: docs/plans/2026-02-27-openfang-architecture-research.md
- 15:00 | Phase 2.3 MCP Workbench completed

---

## [Archived Phases]

### Phase 2.1 - 调试面板基础 (2026-02-27, ✅ 已完成)

**提交**: `b4fb4e9 docs: checkpoint Phase 2 Batch 3 complete`
**交付**: 调试面板基础（Timeline + JsonViewer + Tool Execution Inspector）
**验证**: 编译 ✅

**关键里程碑**:
- 02:00 | Phase 2 Batch 3 编码完成，13 任务，12 提交
- 02:27 | 代码审查修复: started_at 时间戳 + RwLock 中毒处理

### Phase 1 核心引擎 (2026-02-26, ✅ 已完成并提交)

**提交**: `2c9ca43 feat: Phase 1 core engine - full-stack AI agent sandbox`
**交付**: 32 Rust 源文件 + 16 TS/React 文件 + 7 设计文档 + 6 构建配置
**验证**: 编译 ✅ + 运行时 E2E 10/10 ✅

**关键里程碑**:
- 00:30 | 架构设计 Brainstorming 8/8 段完成 [claude-mem #2776 #2778]
- 02:40 | 正式架构设计文档 (2300行, 12章) [claude-mem #2788 #2790]
- 08:20 | sccache 启用 (-35% 热缓存) [claude-mem #2820]
- 09:10 | 运行时 E2E 验证通过 + 多项 bugfix [claude-mem #2821]
- 17:30 | OpenAI Provider + Thinking 全链路 [claude-mem #2823]
- 18:45 | SSE Stream 事件丢失 bugfix (pending_events VecDeque)
- 11:15 | 阶段关闭，代码提交

**遗留问题移交 Phase 2**:
1. Cancel 功能 (CancellationToken)
2. Dead code warnings (低优先级)
3. SSE bugfix 运行时验证 (多 chunk 场景)
