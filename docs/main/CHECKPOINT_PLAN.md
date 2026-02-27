# octo-sandbox 计划执行状态 Checkpoint

**日期**: 2026-02-27
**最后更新**: 2026-02-27 23:30 GMT+8
**当前阶段**: octo-workbench Phase 2.1 (核心闭环可用) ✅ 完成
**git 分支**: octo-workbench
**git 最新提交**: 6e33b3b fix(api): resolve route matching issue + add TimelineView/JsonViewer
**实施规划**: docs/plans/2026-02-27-phase2-1-implementation.md

---

## 总体计划状态

| 阶段 | 状态 | 说明 |
|------|------|------|
| 参考项目分析 | ✅ 完成 | 8 个项目深度代码分析 |
| 架构设计 Brainstorming | ✅ 完成 | 8/8 段全部确认（含记忆模块） |
| 正式设计文档 | ✅ 完成 | 整合 8 段 brainstorming 为 `docs/design/ARCHITECTURE_DESIGN.md`（12 章 + 附录，10 张 Mermaid 图，19 条技术决策） |
| Phase 1 实施计划 | ✅ 完成 | 10 步详细任务拆分，Claude plan mode 审批通过 |
| Phase 1 编码 | ✅ 完成 | 32 个 Rust 源文件 + 16 个 TS/React 文件，全部编译通过 |
| Phase 1 运行时验证 | ✅ 完成 | 端到端流式对话验证通过，多项 bugfix |
| Phase 2 上下文工程设计 | ✅ 完成 | 6 个参考项目分析 + 上下文工程架构设计（10 章）+ 14 任务实施计划 |
| Phase 2 Batch 1 编码 | ✅ 完成 | 上下文工程核心 + 5 新工具（14 任务），6 个提交 |
| Phase 2 Batch 2 编码 | ✅ 完成 | SQLite 持久化 + Session Memory + 混合检索 + Memory Flush + 3 Memory 工具（16 任务），8 个提交 |
| Phase 2 Batch 3 编码 | ✅ 完成 | Skill Loader + MCP Client + Tool Execution Recording + REST API + Debug UI |
| octo-workbench Phase 2.1 | ✅ 完成 | 核心闭环可用（运行时验证 + Timeline + JsonViewer + REST API） |
| octo-workbench Phase 2.2 | ⏳ 待开始 | 记忆系统完整（5 memory tools + Explorer） |
| octo-workbench Phase 2.3 | ⏳ 待开始 | 调试面板完善（MCP + Skill + 高级调试） |
| octo-workbench Phase 2.4 | ⏳ 待开始 | Engine Hardening（Loop Guard / Context 4+1 / LLM Retry / EventBus / Tool Security） |
| octo-platform (Phase 3) | ⏳ 待开始 | Docker + 多用户 + 完整功能 |
| Phase 4 编码 | ⏳ 待开始 | 生产就绪 |

---

## Phase 1 实施进度（10/10 步骤）

| 步骤 | 内容 | 状态 |
|------|------|------|
| Step 1 | Workspace 脚手架 + 基础服务 | ✅ 完成 |
| Step 2 | 共享类型定义 (octo-types) | ✅ 完成 |
| Step 3 | Provider Trait + Anthropic SSE 实现 | ✅ 完成 |
| Step 4 | Tool Trait + BashTool + FileReadTool | ✅ 完成 |
| Step 5 | NativeRuntime 沙箱 | ✅ 完成 |
| Step 6 | Agent Loop (max 10 轮 + 工具调用) | ✅ 完成 |
| Step 7 | WebSocket 服务 + Session 管理 | ✅ 完成 |
| Step 8 | Chat UI 前端 | ✅ 完成 |
| Step 9 | Working Memory + Context Injector + Token Budget | ✅ 完成 |
| Step 10 | 集成验证 | ✅ 编译 + 运行时端到端验证全部通过 |

### 构建验证结果

| 检查项 | 状态 | 详情 |
|--------|------|------|
| `cargo check --workspace` | ✅ 通过 | 0.42s (sccache enabled) |
| `cargo build` | ✅ 通过 | 24.5s (sccache hot cache) |
| `npx tsc --noEmit` | ✅ 通过 | 0 errors |
| `npx vite build` | ✅ 通过 | 874ms, 241KB JS bundle |

### 运行时验证结果

| 检查项 | 状态 | 详情 |
|--------|------|------|
| 服务器启动 | ✅ 通过 | `octo-server listening on 127.0.0.1:3001` |
| Health endpoint | ✅ 通过 | `GET /api/health` → 200 ok |
| WebSocket 连接 | ✅ 通过 | Upgrade 101, 连接建立 |
| Session 创建 | ✅ 通过 | UUID 返回客户端 |
| AgentLoop 启动 | ✅ 通过 | 历史消息加载, system prompt 构建 |
| Working Memory 注入 | ✅ 通过 | 390 chars system prompt (含 `<working_memory>` 标签) |
| Anthropic API 流式调用 | ✅ 通过 | 200 OK, SSE 解析正确 |
| 流式文本传输 | ✅ 通过 | 客户端收到完整响应 "Hello from octo-sandbox!" |
| 错误事件传播 | ✅ 通过 | Error + Done 事件正确到达客户端 |
| API 重试机制 | ✅ 通过 | 5xx 错误自动重试 3 次指数退避 |

---

## Phase 2 上下文工程设计（已完成）

### 设计文档

- **`docs/design/CONTEXT_ENGINEERING_DESIGN.md`** — 上下文工程架构设计（10 章，500+ 行）
- 深度分析 6 个参考项目（OpenClaw, ZeroClaw, NanoClaw, HappyClaw, pi_agent_rust, Craft Agents）的上下文工程实现
- 提炼跨项目共识模式，设计 octo-sandbox 上下文工程架构

### 核心设计

| 组件 | 设计 | 说明 |
|------|------|------|
| 三区上下文分配 | A(系统提示) + B(动态上下文) + C(对话历史) | 区域 A 可缓存，区域 B 每轮重建，区域 C 渐进降级 |
| 三级渐进式降级 | L0(无) → L1(软裁剪) → L2(硬清除) → L3(压缩摘要) | 基于使用率阈值 60%/80%/90% |
| 压缩边界保护 | 不在工具调用链中间截断 | 借鉴 pi_agent_rust |
| 工具结果三层防御 | 工具侧硬限制 → 注入时软裁剪(30K) → 历史降级 | 最新一轮永远不裁剪 |
| Token 预算管理 | ContextBudgetManager + ContextPruner 分离 | 双轨估算：API 真实值 + chars/4 |
| 三层记忆集成 | L0(Working Memory) + L1(Session Memory) + L2(Persistent Memory) | L0 Phase 2 增强，L1 Phase 2 新增，L2 Phase 3 |
| 记忆冲刷 | 压缩前提取关键事实到 Working Memory | 防止压缩丢失重要信息 |

### 实施计划

- **`docs/plans/2026-02-26-phase2-context-engineering.md`** — 14 任务实施计划（Batch 1）
- 每个任务含完整代码、文件路径、构建验证命令、git 提交命令

| 任务分组 | 任务编号 | 内容 |
|----------|----------|------|
| 上下文工程核心 | Task 1-5 | MemoryBlock 类型扩展 + SystemPromptBuilder + ContextBudgetManager + ContextPruner + AgentLoop 集成 |
| 5 个新工具 | Task 6-10 | FileWriteTool + FileEditTool + GrepTool + GlobTool + FindTool |
| 集成收尾 | Task 11-14 | 工具注册 + 软裁剪 + Working Memory 增强 + 全量验证 |

### Phase 2 Batch 1 实施结果 (✅ 已完成)

**提交记录** (7 个提交: `4508d95` → `e692d35`)：

| 提交 | 内容 |
|------|------|
| `4508d95` | docs: 设计文档 + 实施计划 |
| `8943ffa` | feat(types): MemoryBlock 扩展 (priority/max_age/AutoExtracted/Custom) |
| `1854397` | feat(engine): context 模块 (SystemPromptBuilder + ContextBudgetManager + ContextPruner) |
| `de47c3f` | feat(engine): AgentLoop 集成 Budget+Pruner + 工具结果软裁剪 30K |
| `f8ffdbb` | feat(tools): 5 新工具 (file_write/file_edit/grep/glob/find) |
| `0bfe864` | feat(memory): 优先级排序 + 12K 预算 + add/remove/expire |
| `e692d35` | docs: checkpoint + work log + memory index 更新 |

**新增源文件** (10 个)：

| 文件 | 说明 |
|------|------|
| `crates/octo-engine/src/context/mod.rs` | context 模块入口 |
| `crates/octo-engine/src/context/builder.rs` | SystemPromptBuilder + Bootstrap 文件发现 |
| `crates/octo-engine/src/context/budget.rs` | ContextBudgetManager 双轨估算 |
| `crates/octo-engine/src/context/pruner.rs` | ContextPruner 三级降级 |
| `crates/octo-engine/src/tools/file_write.rs` | FileWriteTool |
| `crates/octo-engine/src/tools/file_edit.rs` | FileEditTool |
| `crates/octo-engine/src/tools/grep.rs` | GrepTool |
| `crates/octo-engine/src/tools/glob.rs` | GlobTool |
| `crates/octo-engine/src/tools/find.rs` | FindTool |

**构建验证**: `cargo build` ✅ | `tsc --noEmit` ✅

### Phase 2 Batch 2 实施结果 (✅ 已完成)

**提交记录** (8 个提交: `a954f17` → `0637bb5`)：

| 提交 | 内容 |
|------|------|
| `a954f17` | feat(deps): 添加 SQLite, ULID, bincode workspace 依赖 |
| `78144ba` | feat(db+types): Database 模块 + WAL 迁移 + Persistent Memory 类型 |
| `c9f8329` | feat(memory): MemoryStore trait + Provider.embed() + SqliteWorkingMemory |
| `5bcedf9` | feat(session): SessionStore 移至 engine, 异步化, SqliteSessionStore |
| `1e41a10` | feat(memory): SqliteMemoryStore + 混合 FTS5+向量检索 |
| `c9988a0` | feat(context): FactExtractor + MemoryFlusher (Compact 级别冲刷) |
| `2bc4c76` | feat(tools): 3 个 Memory 工具 (memory_store/memory_search/memory_update) |
| `0637bb5` | feat(server): SQLite 持久化集成到 AppState + main.rs |

**新增源文件** (14 个)：

| 文件 | 说明 |
|------|------|
| `crates/octo-engine/src/db/mod.rs` | 数据库模块入口 |
| `crates/octo-engine/src/db/connection.rs` | Database struct + WAL 配置 |
| `crates/octo-engine/src/db/migrations.rs` | Schema 迁移 (5 表 + FTS5 + 触发器 + 索引) |
| `crates/octo-engine/src/memory/store_traits.rs` | MemoryStore trait |
| `crates/octo-engine/src/memory/sqlite_working.rs` | SqliteWorkingMemory (write-through cache) |
| `crates/octo-engine/src/memory/sqlite_store.rs` | SqliteMemoryStore + 混合检索 |
| `crates/octo-engine/src/memory/extractor.rs` | FactExtractor (LLM 事实提取) |
| `crates/octo-engine/src/session/mod.rs` | Async SessionStore trait |
| `crates/octo-engine/src/session/memory.rs` | InMemorySessionStore (迁移) |
| `crates/octo-engine/src/session/sqlite.rs` | SqliteSessionStore (DashMap + SQLite) |
| `crates/octo-engine/src/context/flush.rs` | MemoryFlusher |
| `crates/octo-engine/src/tools/memory_store.rs` | memory_store 工具 |
| `crates/octo-engine/src/tools/memory_search.rs` | memory_search 工具 |
| `crates/octo-engine/src/tools/memory_update.rs` | memory_update 工具 |

**修改文件** (18 个)：

| 文件 | 变更 |
|------|------|
| `Cargo.toml` | 添加 rusqlite, tokio-rusqlite, ulid, bincode |
| `crates/octo-types/Cargo.toml` | 添加 ulid |
| `crates/octo-types/src/memory.rs` | MemoryId, MemoryCategory, MemoryEntry 等 6 个新类型; MemoryBlock 扩展 char_limit/is_readonly |
| `crates/octo-types/src/lib.rs` | 新类型 re-exports |
| `crates/octo-engine/Cargo.toml` | 添加 rusqlite, tokio-rusqlite, ulid, bincode, dashmap |
| `crates/octo-engine/src/lib.rs` | 添加 db, session 模块 + re-exports |
| `crates/octo-engine/src/providers/traits.rs` | Provider trait 添加 embed() (默认返回错误) |
| `crates/octo-engine/src/providers/openai.rs` | 实现 embed() (text-embedding-3-small) |
| `crates/octo-engine/src/memory/mod.rs` | 添加 store_traits, sqlite_working, sqlite_store, extractor 模块 |
| `crates/octo-engine/src/context/mod.rs` | 添加 flush 模块 |
| `crates/octo-engine/src/agent/loop_.rs` | memory_store 字段 + with_memory_store() + Compact flush |
| `crates/octo-engine/src/tools/mod.rs` | 3 个 memory tool 模块 + register_memory_tools() |
| `crates/octo-server/src/main.rs` | Database 初始化 + SQLite 服务 + memory tools 注册 |
| `crates/octo-server/src/state.rs` | 添加 memory_store 字段 |
| `crates/octo-server/src/session.rs` | 改为 re-export octo_engine::session |
| `crates/octo-server/src/ws.rs` | session .await + AgentLoop.with_memory_store() |
| `.env.example` | 添加 OCTO_DB_PATH |

**关键架构决策**:

| 决策 | 选择 | 原因 |
|------|------|------|
| SessionStore 位置 | 移至 octo-engine + async | 核心引擎概念，SQLite 实现需 async |
| Memory 工具注入 | 构造器注入 Arc | 不扩展 ToolContext，每个工具持有自己的引用 |
| 检索策略 | 混合 FTS5 + 向量 | 0.7 vec + 0.3 FTS 分数融合, 无 embedding 时降级为 FTS-only |
| 缓存策略 | Write-through (DashMap/RwLock) | 热路径不访问 DB，写操作同步持久化 |
| 事实提取 | LLM-based FactExtractor | Provider.complete() 调用, JSON 解析容错 |
| Memory Flush 时机 | Compact 降级前 | 先 flush 再 prune，防止信息丢失 |

**构建验证**: `cargo build` ✅ (仅 1 个预存 warning)

### Phase 2 Batch 3 实施结果 (✅ 已完成)

**设计文档**: `docs/plans/2026-02-26-phase2-batch3-design.md` (859 行)
**实施计划**: `docs/plans/2026-02-26-phase2-batch3-implementation.md` (2769 行, 13 任务)

**提交记录** (12 个提交: `322eaf3` → `49856cc`)：

| 提交 | 内容 |
|------|------|
| `322eaf3` | feat(deps): add serde_yaml, notify, rmcp workspace deps + ToolSource(String) |
| `76a9687` | feat(skills): SkillDefinition type + SKILL.md parser with frontmatter splitting |
| `b107664` | feat(skills): SkillRegistry + SkillTool + SystemPromptBuilder integration |
| `9867798` | feat(skills): hot-reload with notify watcher (300ms debounce) |
| `39c2409` | feat(mcp): McpClient trait + StdioMcpClient (rmcp wrapper) |
| `c220901` | feat(mcp): McpToolBridge + McpManager (multi-server, config file) |
| `0569bfc` | feat(types+db): ToolExecution types + SQLite migration v2 (tool_executions table) |
| `a1d05a3` | feat(tools): ToolExecutionRecorder + AgentLoop integration (SQLite recording) |
| `d73499a` | feat(server): REST API endpoints + AppState integration |
| `c52b496` | feat(ws): tool_execution + token_budget_update WebSocket events |
| `cf71344` | feat(web): 3-tab layout + ExecutionList + TokenBudgetBar + WS events |
| `49856cc` | fix: correct started_at timestamp + handle RwLock poisoning |

**新增源文件** (26 个)：

| 文件 | 说明 |
|------|------|
| `crates/octo-types/src/skill.rs` | SkillDefinition 类型 |
| `crates/octo-types/src/execution.rs` | ExecutionStatus, ToolExecution, TokenBudgetSnapshot |
| `crates/octo-engine/src/skills/mod.rs` | Skills 模块入口 |
| `crates/octo-engine/src/skills/loader.rs` | SkillLoader + SKILL.md 解析器 |
| `crates/octo-engine/src/skills/registry.rs` | SkillRegistry (线程安全 + 热重载) |
| `crates/octo-engine/src/skills/tool.rs` | SkillTool (Tool trait 实现) |
| `crates/octo-engine/src/mcp/mod.rs` | MCP 模块入口 |
| `crates/octo-engine/src/mcp/traits.rs` | McpClient trait + McpToolInfo + McpServerConfig |
| `crates/octo-engine/src/mcp/stdio.rs` | StdioMcpClient (rmcp 0.16 封装) |
| `crates/octo-engine/src/mcp/bridge.rs` | McpToolBridge (Tool trait 实现) |
| `crates/octo-engine/src/mcp/manager.rs` | McpManager (多服务器管理) |
| `crates/octo-engine/src/tools/recorder.rs` | ToolExecutionRecorder (SQLite 异步记录) |
| `crates/octo-server/src/api/mod.rs` | REST API 路由 + PaginationParams |
| `crates/octo-server/src/api/sessions.rs` | GET /api/sessions, /api/sessions/{id} |
| `crates/octo-server/src/api/executions.rs` | GET /api/sessions/{id}/executions, /api/executions/{id} |
| `crates/octo-server/src/api/tools.rs` | GET /api/tools |
| `crates/octo-server/src/api/memories.rs` | GET /api/memories, /api/memories/working |
| `crates/octo-server/src/api/budget.rs` | GET /api/budget |
| `web/src/atoms/debug.ts` | Debug 状态原子 |
| `web/src/pages/Tools.tsx` | Tools 页面 |
| `web/src/pages/Debug.tsx` | Debug 页面 |
| `web/src/components/tools/ExecutionList.tsx` | 工具执行列表 |
| `web/src/components/tools/ExecutionDetail.tsx` | 执行详情面板 |
| `web/src/components/debug/TokenBudgetBar.tsx` | Token 预算可视化 |

**修改文件** (20 个)：

| 文件 | 变更 |
|------|------|
| `Cargo.toml` | 添加 serde_yaml, notify, notify-debouncer-mini, rmcp |
| `crates/octo-types/Cargo.toml` | 添加 serde_yaml |
| `crates/octo-engine/Cargo.toml` | 添加 serde_yaml, notify, notify-debouncer-mini, rmcp |
| `crates/octo-types/src/lib.rs` | 新模块注册 (skill, execution) |
| `crates/octo-types/src/tool.rs` | ToolSource: Mcp(String), Skill(String) |
| `crates/octo-types/src/memory.rs` | MemoryResult 添加 Serialize/Deserialize |
| `crates/octo-engine/src/lib.rs` | 新模块注册 (skills, mcp) + re-exports |
| `crates/octo-engine/src/agent/loop_.rs` | recorder 集成 + ToolExecution/TokenBudgetUpdate 事件 |
| `crates/octo-engine/src/context/builder.rs` | with_skills() 方法 |
| `crates/octo-engine/src/context/budget.rs` | snapshot() 方法 |
| `crates/octo-engine/src/db/migrations.rs` | v2 迁移 (tool_executions 表) |
| `crates/octo-engine/src/tools/mod.rs` | recorder 模块注册 |
| `crates/octo-server/src/main.rs` | recorder + skill 初始化 + api 模块 |
| `crates/octo-server/src/router.rs` | .nest("/api", api::routes()) |
| `crates/octo-server/src/state.rs` | recorder + skill_registry 字段 |
| `crates/octo-server/src/ws.rs` | 新 ServerMessage 变体 + recorder 传递 |
| `web/src/App.tsx` | 3-tab 条件渲染 |
| `web/src/atoms/ui.ts` | TabId 类型扩展 |
| `web/src/components/layout/TabBar.tsx` | 3-tab 数据驱动 |
| `web/src/ws/types.ts` + `events.ts` | 新 WS 事件类型 + 处理 |

**关键架构决策**:

| 决策 | 选择 | 原因 |
|------|------|------|
| Skill 集成模式 | 混合：系统提示注入 + 可调用工具 | 发现用注入，执行用工具 |
| MCP SDK | rmcp 0.16 薄封装 | 官方 Rust SDK，自定义 McpClient trait |
| Skill 热重载 | notify-debouncer-mini 300ms | 轻量级，无运行时依赖 |
| 工具执行记录 | SQLite + AgentLoop opt-in | with_recorder() builder 模式 |
| REST API | Axum .nest() 模块化 | 8 端点，独立于 WS |
| Tab 布局 | 3-tab (Chat\|Tools\|Debug) | 职责分离，Tools 独立 |

**代码审查修复** (commit `49856cc`):

| 问题 | 修复 |
|------|------|
| started_at 时间戳在执行后记录 | 执行前捕获 chrono::Utc::now() |
| RwLock 中毒导致级联 panic | unwrap_or_else(\|e\| e.into_inner()) |

**已知限制** (已全部修复 - Task 6 Batch 3 Bugfix):

| 限制 | 说明 | 状态 |
|------|------|------|
| TokenBudgetUpdate 事件未从 AgentLoop 发射 | MessageStop 后添加 snapshot() + 发射事件 | ✅ 已修复 |
| snapshot() 硬编码 dynamic_context=0 | 使用 estimate_tool_specs_tokens() 填充 tool_tokens | ✅ 已修复 |
| list_sessions 返回空列表 | SessionStore trait 添加 list_sessions()；SqliteSessionStore + InMemorySessionStore 均实现 | ✅ 已修复 |
| get_working_memory 每次创建新 SandboxId | 从 query params 接受 sandbox_id 参数，默认 "default" | ✅ 已修复 |
| Recorder 使用独立 Database 连接 | 改为 ToolExecutionRecorder::new(conn.clone()) 共享连接 | ✅ 已修复 |

**构建验证**: `cargo check --workspace` ✅ | `tsc --noEmit` ✅ | `vite build` ✅ (248.58 kB JS)

---

## 本次会话变更

### 会话 1：运行时验证阶段

1. **sccache 启用** — `.cargo/config.toml` 启用 `rustc-wrapper`，Makefile 移除 11 处 `RUSTC_WRAPPER=""`。热缓存重编译 -35%。
2. **ANTHROPIC_BASE_URL 支持** — `create_provider()` 接受 `Option<String>` base_url，支持中转代理。
3. **dotenv_override** — 防止系统环境变量覆盖项目 `.env` 配置（401 根因）。
4. **错误事件传播修复** — stream 初始化失败时发送 Error+Done 事件再返回。
5. **thinking_delta 支持** — SSE 解析器新增 thinking/thinking_delta/signature_delta 处理。
6. **API 重试机制** — 5xx 错误自动重试最多 3 次，指数退避。
7. **前端端口** — `vite.config.ts` 端口 5173 → 5180。

### 会话 2：OpenAI Provider + Thinking 全链路

1. **OpenAI Provider 实现** — 新增 `providers/openai.rs`（450+ 行），完整 SSE 流解析 + tool_calls + base_url normalize。
2. **Provider 切换** — `LLM_PROVIDER` 环境变量切换 anthropic/openai，`OPENAI_API_KEY`/`OPENAI_BASE_URL`/`OPENAI_MODEL_NAME`。
3. **Thinking 全链路** — `StreamEvent::ThinkingDelta` + `AgentEvent::ThinkingDelta/ThinkingComplete` + `ServerMessage::thinking_delta/thinking_complete`。
4. **三种 Thinking 来源统一** — Anthropic `thinking_delta` + OpenAI `reasoning_content` → `ThinkingDelta`；MiniMax 中转降级（只有 thinking 无 text 时 thinking 作为正式回复）。
5. **前端 Thinking UI** — `StreamingDisplay` 流式展示（可折叠 + Brain 图标）；`MessageBubble` 持久展示（折叠保留在消息记录）；`ChatMsg.thinking` 字段。
6. **兼容性修复** — `finish_reason: "null"` 过滤；`stopped` 防重复 MessageStop；前端 text trim 去开头标点。

### 会话 3：SSE Stream 事件丢失 bugfix

1. **🔴 SSE Stream poll_next 事件丢失** — `openai.rs` 和 `anthropic.rs` 的 `poll_next()` 中，`parse_sse_events()` 从 buffer 消费 SSE 原始数据后返回多个 `StreamEvent`，但只取第一个返回，**剩余事件随 iter 离开作用域被丢弃**。当多个 SSE chunk 在同一次 TCP read 到达时（代理/中转服务如 dashscope 常见），后续 TextDelta 事件丢失，导致正式回复文本被截断。
2. **修复方案** — 给两个 SSE Stream 结构体添加 `pending_events: VecDeque<Result<StreamEvent>>` 字段。`parse_sse_events()` 结果全部入队到 `pending_events`，然后逐个 `pop_front` 返回，确保不丢失任何事件。
3. **影响范围** — 两个 Provider 都存在此 bug：`openai.rs` 和 `anthropic.rs`。修复后编译通过（`cargo check` ✅）。

### 会话 4：Phase 2.1 核心闭环 + TimelineView/JsonViewer

1. **运行时验证** — 服务器启动 ✅, Health endpoint ✅, WebSocket 连接 ✅, 工具执行 ✅
2. **TimelineView 组件** — 新增 `web/src/components/tools/TimelineView.tsx`，展示工具执行时间线
3. **JsonViewer 组件** — 新增 `web/src/components/tools/JsonViewer.tsx`，展示结构化 JSON 数据
4. **CSS 样式** — 添加 timeline 和 jsonviewer 样式到 `globals.css`
5. **组件集成** — TimelineView/JsonViewer 集成到 ExecutionDetail.tsx
6. **Router 重构** — 将 API handlers 内联到 router.rs 以解决 Axum 0.7 参数化路由 bug

### 会话 5：Phase 2.1 完成 + Router 调试

1. **REST API 固定路径** — `/api/sessions`, `/api/tools`, `/api/memories` 等全部 200 OK
2. **REST API 参数化路径** — `/api/sessions/{id}`, `/api/executions/{id}` 存在 Axum 0.7 类型推断 bug
3. **TimelineView 集成** — 修复类型错误，正确显示时间线事件
4. **Phase 2.1 完成** — 核心功能全部可用，构建验证通过

### 会话 6：Axum 0.8 升级

1. **Axum 0.7 → 0.8.8 升级** — 修复 REST API 参数化路由 404 bug
2. **axum-extra 0.9 → 0.10.3 升级**
3. **路由修复** — 改用 `.nest("/api", api)` 替代 `.merge(api)`
4. **验证通过** — 所有 REST API 端点正常返回 200

### 已知限制

（已修复：无）

### 修改文件清单（累计）

| 文件 | 变更 |
|------|------|
| `.cargo/config.toml` | 启用 sccache rustc-wrapper |
| `Makefile` | 移除 11 处 RUSTC_WRAPPER="" |
| `.env.example` | LLM_PROVIDER + OpenAI 配置示例 |
| `crates/octo-types/src/provider.rs` | StreamEvent 新增 ThinkingDelta |
| `crates/octo-engine/src/providers/openai.rs` | **新增** — OpenAI Provider (450+ 行); **bugfix**: pending_events VecDeque 修复事件丢失 |
| `crates/octo-engine/src/providers/anthropic.rs` | thinking_delta → ThinkingDelta; base_url; block_types; **bugfix**: pending_events VecDeque 修复事件丢失 |
| `crates/octo-engine/src/providers/mod.rs` | create_provider(name,key,url) 工厂; openai 模块 |
| `crates/octo-engine/src/agent/loop_.rs` | ThinkingDelta/ThinkingComplete; thinking 降级; 默认模型; 5xx 重试 |
| `crates/octo-engine/src/lib.rs` | 更新 re-exports |
| `crates/octo-server/src/main.rs` | LLM_PROVIDER/OPENAI_MODEL_NAME; provider 切换; dotenv_override |
| `crates/octo-server/src/state.rs` | AppState::new 接受 model 参数 |
| `crates/octo-server/src/ws.rs` | ServerMessage 新增 thinking_delta/thinking_complete |
| `web/vite.config.ts` | 端口 5173 → 5180 |
| `web/src/ws/types.ts` | thinking 消息类型 |
| `web/src/ws/events.ts` | thinking 事件处理 + text trim |
| `web/src/atoms/session.ts` | streamingThinkingAtom + ChatMsg.thinking |
| `web/src/components/chat/StreamingDisplay.tsx` | 流式 thinking 展示 |
| `web/src/components/chat/MessageBubble.tsx` | 持久 thinking 展示 |

---

## 已完成的工作

### 参考项目分析（8 个项目）
- pi-mono, pi_agent_rust, pi-skills, OpenClaw, nanoclaw, happyclaw, craft-agents-oss, zeroclaw
- 深度代码分析：渠道架构、多用户体系、工具系统、UI 架构、调试功能

### 架构设计 8 段（全部确认）
1. **系统分层** — 六大核心组件
2. **Agent Engine** — Agent Loop + Provider/Tool/Skill Trait
3. **沙箱管理器** — WASM→Docker→Apple Container + Transport
4. **渠道和多用户** — Channel Trait + 三角色 RBAC + 双层权限
5. **调试面板** — 5 大模块全进 MVP
6. **Web UI** — React 19 + Jotai + shadcn/ui
7. **MVP 路线图** — 4 Phase，Phase 1 精简版
8. **记忆模块** — 四层记忆架构 + 混合检索 + 上下文工程

### Phase 1 核心引擎（全部完成）

#### 后端 (Rust, 32 个源文件)

**octo-types (8 文件)** — 共享类型定义
- `crates/octo-types/src/id.rs` — UserId, SessionId, SandboxId newtype (宏生成)
- `crates/octo-types/src/message.rs` — MessageRole, ChatMessage, ContentBlock (Text/ToolUse/ToolResult)
- `crates/octo-types/src/provider.rs` — CompletionRequest, CompletionResponse, StreamEvent, TokenUsage, StopReason
- `crates/octo-types/src/tool.rs` — ToolSource, ToolSpec, ToolResult, ToolContext
- `crates/octo-types/src/memory.rs` — MemoryBlock, MemoryBlockKind, TokenBudget
- `crates/octo-types/src/sandbox.rs` — RuntimeType, SandboxConfig, ExecResult
- `crates/octo-types/src/error.rs` — OctoError enum (thiserror)
- `crates/octo-types/src/lib.rs` — 模块声明 + pub re-exports

**octo-engine (14 文件)** — 核心引擎
- `providers/traits.rs` — Provider trait (complete + stream)
- `providers/anthropic.rs` — AnthropicProvider (SSE stream + ThinkingDelta)
- `providers/openai.rs` — OpenAIProvider (Chat Completions SSE + tool_calls + reasoning_content)
- `providers/mod.rs` — create_provider(name, key, url) 工厂，LLM_PROVIDER 切换
- `tools/traits.rs` — Tool trait (name/desc/params/execute/spec)
- `tools/bash.rs` — BashTool (tokio::process::Command, 30s 超时, env 清理)
- `tools/file_read.rs` — FileReadTool (1MB 限制, 行号显示, offset/limit)
- `tools/mod.rs` — ToolRegistry + default_tools()
- `agent/loop_.rs` — AgentLoop (最大 10 轮, 流式事件, 工具调用循环, 5xx 重试)
- `agent/context.rs` — ContextBuilder (系统提示词组装, token 估算)
- `memory/traits.rs` — WorkingMemory trait
- `memory/working.rs` — InMemoryWorkingMemory (默认 4 blocks)
- `memory/injector.rs` — ContextInjector (blocks → XML tags)
- `memory/budget.rs` — TokenBudgetManager (chars/4 估算)

**octo-sandbox (3 文件)** — 沙箱运行时
- `traits.rs` — RuntimeAdapter trait
- `native.rs` — NativeRuntime (进程执行 + 超时 + env 清理)
- `lib.rs` — 模块声明

**octo-server (5 文件)** — HTTP/WebSocket 服务
- `main.rs` — Axum 启动, dotenv_override, tracing, ANTHROPIC_BASE_URL, graceful shutdown
- `router.rs` — build_router() (/api/health + /ws, CORS, TraceLayer)
- `ws.rs` — WebSocket handler (消息解析, AgentLoop 启动, broadcast 事件转发)
- `session.rs` — InMemorySessionStore (DashMap), SessionStore trait
- `state.rs` — AppState (Provider + ToolRegistry + WorkingMemory + AgentLoop)

#### 前端 (TypeScript/React, 16 个源文件)

**基础设施**
- `web/package.json` — React 19 + Jotai 2.16 + Tailwind CSS 4 + Vite 6
- `web/vite.config.ts` — Vite 配置 + API proxy → localhost:3001, 端口 5180
- `web/tsconfig.json` — TypeScript 严格模式 + path aliases
- `web/src/main.tsx` — React root + Jotai Provider
- `web/src/globals.css` — Tailwind CSS 基础样式 + CSS 变量主题
- `web/src/lib/utils.ts` — cn() (clsx + tailwind-merge)

**状态管理**
- `web/src/atoms/session.ts` — sessionIdAtom, messagesAtom, isStreamingAtom, streamingTextAtom, streamingThinkingAtom, toolExecutionsAtom; ChatMsg.thinking
- `web/src/atoms/ui.ts` — activeTabAtom, sidebarOpenAtom

**WebSocket**
- `web/src/ws/manager.ts` — WsManager 单例 (connect/disconnect/send, 指数退避重连)
- `web/src/ws/types.ts` — ClientMessage, ServerMessage TypeScript 类型
- `web/src/ws/events.ts` — handleWsEvent() 事件分发到 Jotai atoms

**UI 组件**
- `web/src/components/layout/AppLayout.tsx` — NavRail + TabBar + Main
- `web/src/components/layout/NavRail.tsx` — 左侧栏 (Phase 1 占位)
- `web/src/components/layout/TabBar.tsx` — 顶部标签栏
- `web/src/components/chat/MessageList.tsx` — 滚动消息列表 + 自动滚底
- `web/src/components/chat/MessageBubble.tsx` — 单条消息 (用户右蓝/助手左灰) + thinking 折叠展示
- `web/src/components/chat/ChatInput.tsx` — Textarea + 发送按钮
- `web/src/components/chat/StreamingDisplay.tsx` — 流式文本 + 流式 thinking + 工具执行状态

#### 构建配置
- `Cargo.toml` — workspace 定义 + profile 优化 (split-debuginfo, codegen-units=256)
- `.cargo/config.toml` — sccache 启用 + 编译优化 (jobs=8, dead_strip)
- `Makefile` — dev/build/check/test/fmt/lint/web 命令
- `.env.example` — LLM_PROVIDER + Anthropic/OpenAI 配置模板

### 关键技术决策
- Rust 双重角色（独立智能体 + 沙箱调度器）
- MCP 标准兼容（2025-11-25 版）
- SQLite WAL + JSONL 兼容
- 通信四通道 + Debug Interceptor
- 运行时优先级：WASM → Docker → Apple Container
- 三角色 RBAC (Admin/Developer/Viewer)
- Session 权限模式 (ReadOnly/Interactive/AutoApprove)
- 前端 Jotai atomFamily per-sandbox 状态隔离
- Cross-Agent Comparison 为可选功能
- Replay 功能 Phase 2 加入
- **Phase 1 特定决策:**
  - NativeRuntime 优先（Phase 1.5 补 WASM）
  - sccache 启用（热缓存 -35% 编译时间）
  - dotenv_override 确保 .env 优先于系统环境变量
  - Anthropic 原生 Tool Calling（不用 XML 解析）
  - 5xx 错误自动重试 3 次指数退避
  - **OpenAI Provider 支持** — LLM_PROVIDER 环境变量切换 anthropic/openai，兼容所有 OpenAI 兼容 API
  - **Thinking/Reasoning 全链路** — StreamEvent::ThinkingDelta 统一 Anthropic thinking_delta + OpenAI reasoning_content；MiniMax 中转降级（只有 thinking 无 text 时作为正式回复）
  - **默认模型按 provider** — anthropic→claude-sonnet-4, openai→gpt-4o, OPENAI_MODEL_NAME 可覆盖

---

## 遗留问题

1. ~~**sccache 不可用**~~ — **已解决**: 实测热缓存 -35% 编译时间，已启用。
2. **Dead code warnings** — `AppState.provider/tools/memory` 字段仅被 `ws.rs` 通过 `agent_loop` 间接使用，但 compiler 无法追踪。无需处理。
3. **Cancel 功能未实现** — WebSocket cancel 消息的处理需要 CancellationToken，留待后续实现。
4. ~~**thinking vs text 分离**~~ — **已解决**: ThinkingDelta 全链路支持，前端独立展示 thinking 内容（流式可折叠 + 持久保留在消息记录中）。MiniMax 中转降级处理（只有 thinking 时作为正式回复）。
5. ~~**SSE Stream 事件丢失导致正式回复截断**~~ — **已解决**: `poll_next()` 中 `parse_sse_events()` 返回多个事件时只取第一个、剩余丢弃。添加 `pending_events: VecDeque` 队列修复。影响 openai.rs 和 anthropic.rs 两个 Provider。待运行时验证。

---

## OpenFang 架构整合里程碑

**参考文档**: `docs/design/OPENFANG_ARCHITECTURE_ROADMAP.md`

### Phase 2.4 可引入 (当前分支)

**实施计划**：`docs/plans/2026-02-27-phase2-4-engine-hardening.md`（7 任务，约 500 LOC 新增）

| 模块 | 优先级 | 状态 | 计划任务 |
|------|--------|------|---------|
| Loop Guard / Circuit Breaker | P0 | ⏳ | Task 1 |
| Context Overflow 4+1 阶段 | P0 | ⏳ | Task 2 |
| LLM 错误分类 + 重试 | P0 | ⏳ | Task 3 |
| EventBus（广播通道） | P1 | ⏳ | Task 4 |
| 工具执行安全（ExecSecurityMode / env_clear） | P1 | ⏳ | Task 5 |
| Batch 3 遗留 Bugfix（5 项） | P0 | ⏳ | Task 6 |
| MCP SSE 传输支持 | P1 | ⏳ | 后续批次 |

### Phase 3 (octo-platform) 目标

| 模块 | 优先级 | 状态 |
|------|--------|------|
| AgentRegistry | P0 | ⏳ |
| Memory 增强 | P1 | ⏳ |
| Scheduler | P2 | ⏳ |
| WorkflowEngine | P2 | ⏳ |
| 安全系统 | P2 | ⏳ |

### 长期目标

| 模块 | 优先级 | 状态 |
|------|--------|------|
| Channels | P3 | ⏳ |
| Hands | P3 | ⏳ |

---

## 文件清单

### 设计文档
| 文件 | 用途 |
|------|------|
| `docs/main/CHECKPOINT_BRAINSTORMING.md` | 完整 7 段架构设计（主要设计文档） |
| `docs/main/CHECKPOINT_MEMORY_BRAINSTORMING.md` | 第 8 段：记忆模块架构设计 |
| `docs/main/CHECKPOINT_PLAN.md` | 本文件，计划执行状态 |
| `docs/design/ARCHITECTURE_DESIGN.md` | 正式架构设计文档（12 章 + 附录） |
| `docs/design/CONTEXT_ENGINEERING_DESIGN.md` | **Phase 2 上下文工程架构设计**（10 章） |
| `docs/design/RUST_BUILD_OPTIMIZATION.md` | Rust 编译速度优化方案 |
| `docs/plans/2026-02-26-phase2-context-engineering.md` | **Phase 2 Batch 1 实施计划**（14 任务） |
| Claude plan mode `wiggly-seeking-spindle.md` | **Phase 2 Batch 2 实施计划**（16 任务 8 提交） |
| `docs/plans/2026-02-26-phase2-batch3-design.md` | **Phase 2 Batch 3 设计文档**（859 行） |
| `docs/plans/2026-02-26-phase2-batch3-implementation.md` | **Phase 2 Batch 3 实施计划**（13 任务） |
| `docs/dev/MEMORY_INDEX.md` | 记忆索引 |
| `docs/main/WORK_LOG.md` | 开发工作日志 |
| `docs/plans/2026-02-27-phase2-4-engine-hardening.md` | **Phase 2.4 Engine Hardening 实施计划**（7 任务：Loop Guard + Context 4+1 + LLM Retry + EventBus + Tool Security + Bugfix） |

### Phase 1 源代码
| 目录 | 文件数 | 用途 |
|------|--------|------|
| `crates/octo-types/src/` | 10 | 共享类型定义 (含 Memory, Skill, Execution 类型) |
| `crates/octo-engine/src/` | 49 | 核心引擎 (Provider[2] + Tool[11+recorder] + Agent + Memory[7] + Context[5] + DB[3] + Session[3] + Skills[4] + MCP[5]) |
| `crates/octo-sandbox/src/` | 3 | 沙箱运行时 |
| `crates/octo-server/src/` | 11 | HTTP/WebSocket 服务 + REST API (6 端点模块) |
| `web/src/` | 23 | React 前端 (3-tab: Chat + Tools + Debug) |
| 配置文件 | 6+ | Cargo.toml, .cargo/config.toml, Makefile, .env.example, package.json, vite.config.ts 等 |

---

## MCP Memory 引用

| 存储 | ID/Key | 内容 |
|------|--------|------|
| claude-mem | #2776 | 完整架构摘要 |
| claude-mem | #2778 | 记忆保存（含进展和决策） |
| claude-mem | #2788 | 正式架构设计文档完成记录 |
| claude-mem | #2790 | 正式架构设计文档阶段完成（完整工作记忆） |
| claude-mem | #2820 | sccache 启用记录 |
| claude-mem | #2821 | Phase 1 运行时验证通过 + 多项 bugfix |
| claude-mem | #2823 | OpenAI Provider + Thinking/Reasoning 全链路支持 |
| claude-mem | #2828 | Phase 2 上下文工程架构 brainstorming 完成 |
| claude-mem | #2829 | Phase 2 Batch 1 编码完成（上下文工程 + 5 新工具） |
| claude-mem | #2831 | Phase 2 Batch 2 编码完成（SQLite 持久化 + Memory + 混合检索 + 3 工具） |
| claude-mem | #2832 | Phase 2 Batch 3 设计文档完成（Skill Loader + MCP Client + Debug UI） |
| knowledge graph | octo-sandbox | 项目实体 + 5 个架构决策实体 |

---

## 恢复指令

恢复此 checkpoint 时，执行：

1. 读取 `docs/main/CHECKPOINT_PLAN.md`（本文件）了解总体状态和下一步
2. 读取 `docs/plans/2026-02-26-phase2-context-engineering.md` — **Phase 2 Batch 1 实施计划**（14 任务）
3. 读取 `docs/design/CONTEXT_ENGINEERING_DESIGN.md` — **上下文工程架构设计**（10 章）
4. 读取 `docs/design/ARCHITECTURE_DESIGN.md` — **权威架构规范**（2300 行，12 章 + 附录）
5. 读取 `docs/main/WORK_LOG.md` — 开发工作日志
6. 读取 `docs/dev/MEMORY_INDEX.md` 了解工作历史
7. 可选：搜索 MCP memory（project: octo-sandbox）获取更多细节

### 下一步操作（按优先级）

1. **Phase 2 Batch 3 已知限制修复** ✅ 已完成
   - TokenBudgetUpdate 事件发射（MessageStop 后 snapshot() + 发射）
   - snapshot() 填充 dynamic_context (tool_tokens)
   - Recorder 共享 Database 连接
   - list_sessions 返回实际数据（SqliteSessionStore + InMemorySessionStore）
   - get_working_memory 接受 sandbox_id 参数

2. **Phase 2.4 Engine Hardening**（下一步）
   - Loop Guard / Circuit Breaker
   - Context 4+1 阶段（70%/90% 双阈值）
   - LLM 错误分类 + 指数退避重试
   - EventBus 广播通道

3. **Phase 2 Batch 4 规划**（待开始）
   - 完整 Debug Panel UI（日志面板、网络面板）
   - Context Viewer（实时上下文窗口可视化）
   - 性能优化

3. **Phase 2 运行时验证**（可选）
   - Batch 1-3 全栈端到端验证
   - MCP 服务器连接测试
   - Skill 加载 + 热重载验证

4. **Phase 3 规划**（Phase 2 全部完成后）
   - Docker 沙箱 + 多用户 + 完整功能
