# octo-sandbox Memory Index

**Project**: octo-sandbox
**Purpose**: Track session work, decisions, and progress for cross-session continuity

---

## [Active Work]

- 11:15 | Phase 1 阶段关闭。代码提交 `2c9ca43`（73 文件, 13,431 行）。CHECKPOINT_PLAN + WORK_LOG + MEMORY_INDEX 更新。MCP memory 归档。
- 12:30 | Phase 2 上下文工程架构 brainstorming 完成。深度分析 6 个参考项目上下文工程实现，设计三区上下文分配模型 + 三级渐进式降级 + 工具结果三层防御 + Token Budget Manager 重构 + 三层记忆集成。设计文档: `docs/design/CONTEXT_ENGINEERING_DESIGN.md`。[claude-mem #2828]
- 12:45 | Phase 2 Batch 1 实施计划创建完成（14 任务）。覆盖上下文工程核心(5) + 5 新工具 + 集成收尾(4)。计划文档: `docs/plans/2026-02-26-phase2-context-engineering.md`。CHECKPOINT_PLAN + WORK_LOG 更新。
- 13:00 | Phase 2 Batch 1 编码完成。14 任务全部执行，6 个提交。新增 context 模块(SystemPromptBuilder + ContextBudgetManager + ContextPruner) + 5 新工具(file_write/file_edit/grep/glob/find) + memory 增强(priority/expire/budget)。cargo build ✅ + tsc ✅。
- 14:30 | Phase 2 Batch 2 编码完成。16 任务全部执行，8 个提交（`a954f17` → `0637bb5`）。新增: Database 模块(SQLite WAL + 5 表 + FTS5), SqliteWorkingMemory(write-through cache), SqliteSessionStore(DashMap + SQLite), SqliteMemoryStore(混合 FTS5+向量检索, 0.7/0.3 融合), FactExtractor(LLM 事实提取), MemoryFlusher(Compact 级别冲刷), 3 个 Memory 工具(store/search/update)。14 新文件 + 18 修改文件。cargo build ✅。
- 2026-02-27 | Phase 2 Batch 3 编码完成。使用 Subagent-Driven Development 模式执行 13 任务计划，12 个提交（`322eaf3` → `49856cc`）。三条特性链: Skill(SkillLoader+SkillRegistry+SkillTool+热重载) + MCP(McpClient trait+StdioMcpClient rmcp 0.16+McpToolBridge+McpManager) + Debug(ToolExecution types+SQLite v2+ToolExecutionRecorder+8 REST API+WS 新事件+3-tab 前端)。26 新文件 + 20 修改文件。代码审查修复: started_at 时间戳 + RwLock 中毒处理。cargo check ✅ + tsc ✅ + vite build ✅。[claude-mem #2832]

---

## [Archived Phases]

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
