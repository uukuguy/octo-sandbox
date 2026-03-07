# octo-sandbox Memory Index

**Project**: octo-sandbox
**Purpose**: Track session work, decisions, and progress for cross-session continuity

---

## [Active Work]

- 16:10 | ADR/DDD organization complete: Converted all ADR file headers (ADR-002 to ADR-008) to English format, removed test ADR-030, mem-save completed
- 16:30 | RuView ADR/DDD analysis: Created README.md for adr/ and ddd/ in English, documented current structure vs target structure, Agent usage mechanisms, mem-save completed
- 11:05 | Checkpoint saved: Phase A+B+C complete (19/24), ready for Phase D

---

## v1.0 Release Sprint - Phase C 前端控制台 [COMPLETED 2026-03-04]

- 18:30 | 补充方案设计完成
  - Phase 2: Architecture - Skills + Runtime
    - Agent Skills 标准实现 (Progressive Disclosure)
    - SkillRuntime (Python/WASM/Node.js)
  - Phase 3: Auth - API Key + RBAC
  - Phase 4: Observability - 结构化日志 + Metering
  - 文档: docs/plans/2026-03-04-v1.0-enhancement-plan.md
  - 预计工作量: ~1230 LOC

---

## v1.0 Release Sprint - Phase C 前端控制台 [COMPLETED 2026-03-04]

- 16:30 | Phase C (6 tasks) 完成: C1-C6 完成/已存在
  - C1: TabBar 扩展（Tasks, Schedule 标签）
  - C2: Tasks 页面（任务提交、列表、详情、删除）
  - C3: Schedule 页面（Cron 任务 CRUD、手动触发、执行历史）
  - C4: Tools 页面（已存在 MCP+Tools tab，Built-in Tools/Skills 需 API）
  - C5: Memory 页面（已存在 Working/Session/Persistent 内存）
  - C6: Debug 页面（已存在 Token Budget + Tool Stats）
  - Deferred 剩余: D1 (observability), D3 (auth)

---

## v1.0 Release Sprint - Phase A 稳定地基 [COMPLETED 2026-03-04]

- 11:30 | Phase A (6 tasks) 完成: A1-A6 全部完成
  - A1: stop_primary 改为 drop tx（不再发送 Cancel 消息）
  - A2: ToolRegistry 改为 Arc<StdMutex<>> 共享引用（支持 MCP 热插拔）
  - A3: scheduler run_now 改为真实执行（调用 execute_task）
  - A4: WorkingMemory 每 session 独立实例（防止数据污染）
  - A5: graceful shutdown 添加 MCP shutdown_all
  - A6: 确认 RetryPolicy 已实现（max_retries=3, base_delay=1s）
  - Deferred 项 D1/D2/D3 已通过 A2 解决
  - cargo check 零错误，149 测试通过

---

## v1.0 Release Sprint - Phase B 后端能力 [COMPLETED 2026-03-04]

- 15:15 | Phase B (6 tasks) 完成: B1-B6 全部完成
  - B1: 并行工具执行 (already done)
  - B2: Background Tasks REST API (POST/GET/DELETE /api/tasks)
  - B3: 增强 /health 端点 (status, uptime, provider, mcp_servers, version)
  - B4: LoopTurnStarted 事件 (turns.total 指标修复)
  - B5: JSON 日志格式 (OCTO_LOG_FORMAT=json)
  - B6: 移除 Option<McpManager> (简化 API)
  - cargo check 零错误，200 测试通过

---

## v1.0 Release Sprint - Deferred Code 修复 [COMPLETED 2026-03-04]

- 15:45 | 代码级 Deferred 修复: D2, D4, D5 已解决
  - D2: 删除 legacy new_legacy 构造函数 (runtime.rs)
  - D4: ws.rs 3处 .unwrap() 改为 if let Ok 处理
  - D5: tools.rs Mutex lock 改为错误处理
  - Deferred 剩余: D1 (observability), D3 (auth middleware)

---

## Phase 2.11 - AgentRegistry + 上下文工程重构 [COMPLETED 2026-03-03]

- 05:00 | Phase 2.11 完成: AgentRegistry + AgentRunner + Zone A/B 上下文重构 + SQLite 持久化 + REST API
  - AgentRegistry: DashMap 三索引 + SQLite 持久化 (7 Tasks, cargo check 0 errors, 149 tests pass)
  - AgentManifest: role/goal/backstory + system_prompt 优先级构建
  - AgentRunner: per-agent ToolRegistry 过滤 + start/stop/pause/resume
  - Zone A/B: working memory 注入首条 Human Message，system prompt 静态
  - REST API: /api/v1/agents CRUD + lifecycle 8 端点

---

## Phase 2.9 - MCP SSE Transport [COMPLETED 2026-03-03]

- 00:10 | Phase 2.9 MCP SSE Transport 完成: SseMcpClient + add_server_v2() + transport/url API
- 00:00 | Phase 2.9 开始实施 (验证已完成的工作)

---

## Phase 2.10 - Knowledge Graph [COMPLETED 2026-03-02]

- 22:00 | Memory 知识图谱完成: Entity/Relation + Graph + FTS5 + 持久化

---

## Phase 2.9-2.11 设计方案 [2026-03-02]

---

## Phase 2.8 - Agent 增强 + Secret Manager [COMPLETED 2026-03-02]

- 17:00 | Phase 2.8 complete: 10/10 tasks, 149 tests pass
- 16:30 | Phase 2.8 进度: 9/10 tasks completed (Task 9 pending)
- 14:40 | Phase 2.8 checkpoint saved - ready for execution
- 14:30 | Phase 2.8 设计完成

---

## [Active Work]

- 21:30 | octo-platform P1-6 设计 + 实施计划完成
  - 设计: docs/plans/2026-03-04-p1-6-web-platform-design.md
  - 实施: docs/plans/2026-03-04-p1-6-web-platform-implementation.md (11 tasks)
  - React 19 + Vite + TailwindCSS 4 + Jotai
  - 登录页 + Dashboard + Chat + Sessions 完整用户工作空间
- 12:30 | v1.0 Release Sprint Phase B checkpoint: A1-A6 complete, B1 verified implemented, B2 attempted (Axum issue - use scheduler API)
- 10:30 | README 重写完成：英文(README.md) + 中文(README.zh.md)，企业级定位，沙箱安全可控，无对标竞品，已提交 5682a72
- 10:00 | 项目名 octo-sandbox 确认保留；GitHub About/Topics 方案确定；v1.0 sprint 待执行 (Phase A-D, 17 tasks)
- 04:30 | Phase 2.11 设计完成（完整 brainstorming）：AgentManifest 三段身份 + AgentRunner + Zone A/B 分离 + SQLite 持久化，计划文档更新（1223行，7 Tasks），待实施
- 00:10 | Phase 2.9 MCP SSE Transport 完成 (已验证之前会话的实现)
- 22:00 | Phase 2.10 Knowledge Graph 完成
- 17:00 | Phase 2.8 - Agent 增强 + Secret Manager completed

---

## [Active Work] Phase 2.7 - Metrics + Audit [2026-03-01]

- 19:30 | Phase 2.7 Metrics + Audit 设计完成
  - 实施计划: docs/plans/2026-03-01-phase2-7-metrics-audit.md (8 tasks)
  - 设计文档: docs/design/PHASE_2_7_METRICS_AUDIT_DESIGN.md
  - Metrics: Counter/Gauge/Histogram, Prometheus 风格
  - Audit: SQLite 存储, Middleware 自动记录
  - REST API: /api/v1/metrics, /api/v1/audit
  - 估算: ~880 LOC
- 19:30 | checkpoint saved - ready for execution

---

## [Active Work] Phase 2.5 - 核心基础设施 [2026-03-01]

- 15:30 | Phase 2.5.4 Scheduler 完成 (10/10 tasks)
  - DB Migration v5, Scheduler 数据结构, Storage trait+impl
  - CronParser, Scheduler 核心, REST API (7 endpoints)
  - 启用配置: scheduler.enabled=true
- 15:30 | Phase 2.5.3 用户隔离 完成 (代码已实现)
  - Session: create_session_with_user, get_session_for_user, list_sessions_for_user
  - Memory: user_id 参数传入 compile
  - MCP: list_servers_for_user, get_server_for_user
  - Scheduler: list_tasks, run_now 支持 user_id 过滤
- 16:00 | Phase 2.6 Provider Chain 设计完成
  - 实施计划: docs/plans/2026-03-01-phase2-6-provider-chain.md (8 tasks)
- 16:00 | checkpoint saved - ready for execution
  - 设计文档: docs/design/PHASE_2_6_PROVIDER_CHAIN_DESIGN.md
  - LlmInstance, ProviderChain, ChainProvider
  - 自动/手动/混合故障切换
  - REST API 6 endpoints
  - 估算: ~630 LOC
  - AuthMode: None / ApiKey / Full
  - ApiKey: key 管理、过期时间、用户绑定
  - Permission: Read / Write / Admin
  - AuthConfig: 认证配置 + 密钥验证
  - UserContext: 用户上下文 + get_user_context 中间件
- 14:31 | Phase 2.5.4 Scheduler 设计完成
  - 设计文档: docs/design/PHASE_2_5_4_SCHEDULER_DESIGN.md
  - 实施计划: docs/plans/2026-03-01-phase2-5-4-scheduler.md (10 tasks)
- 12:30 | Phase 2.5.1 Sandbox System 完成 (7/7 tasks)
  - RuntimeAdapter trait + types (SandboxType, SandboxConfig, ExecResult, SandboxId)
  - SubprocessAdapter: 直接进程执行
  - WasmAdapter: WASM 沙箱 (wasmtime, feature-gated)
  - DockerAdapter: 容器沙箱 (bollard, feature-gated)
  - SandboxRouter: 工具→沙箱路由 (Shell→Docker, Compute→Wasm, FileSystem→Docker, Network→Wasm)
  - Bash tool 集成: 可选沙箱执行
  - 82 tests passing
- 09:40 | Phase 2.5 设计文档更新 (docs/design/PHASE_2_5_DESIGN.md)
  - 拆分为 4 个子阶段: 2.5(核心) / 2.6(Provider+Scheduler) / 2.7(可观测性) / 2.8(Agent增强)
  - **Phase 2.5**: 沙箱 + 认证 + 用户隔离 (~1800 LOC)
  - **Phase 2.6**: Provider 多实例 + Scheduler (~800 LOC)
  - **Phase 2.7**: Metrics + 审计 (~500 LOC)
  - **Phase 2.8**: Agent Loop + Secret (~400 LOC)
  - 参考项目标注: openfang (auth/sandbox/scheduler/metrics/audit), openclaw (agent_loop)
- 09:35 | Phase 2.5 设计文档更新
- 09:30 | Phase 2.5 设计文档完成

- 12:30 | octo-workbench v1.0 完成 + 4 个企业级增强模块
  - LoopGuard 增强: 结果感知、乒乓检测、轮询处理、警告升级 (14 tests)
  - 安全策略: AutonomyLevel、命令白名单、路径黑名单、ActionTracker (8 tests)
  - 消息队列: Steering/FollowUp、QueueMode (6 tests)
  - Extension 系统: 完整生命周期、拦截器、ExtensionManager (6 tests)
  - 总计: 34 新测试全部通过
- 12:00 | 开始企业级增强实施 (Phase 1-4)
- 08:00 | octo-workbench v1.0 设计文档完成

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

## Phase 2.4 - Engine Hardening [COMPLETED 2026-02-27]

- 19:30 | Phase 2.4 完成，所有 7 任务交付，构建验证通过 [claude-mem #2886]
  - cargo check: 0 errors ✅ | tsc: 0 errors ✅ | vite build: 265.66kB ✅
- 19:00 | Task 5-7 完成: BashTool 安全 + Batch3 Bugfix 验证 + 文档更新
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

## MCP SSE Transport [COMPLETED 2026-02-27]

- 20:10 | MCP SSE Transport 完成: SseMcpClient + add_server_v2() + transport/url API 字段 [claude-mem #2887]
  - 5/5 任务完成，5 commits (7d3c878 → 59a4d1d)
  - cargo check: 0 errors ✅ | tsc: 0 errors ✅ | vite build: 265.66kB ✅
- 19:55 | 计划制定完成 (docs/plans/2026-02-27-mcp-sse-transport.md)

---

## [Active Work]

- 04:00 | octo-workbench v1.0 方案设计完成
  - 方案: 33 测试案例, 4 阶段 (A-D), 12 天
  - MCP: 6 servers (filesystem, fetch, sqlite, github, notion, brave-search)
  - Skills: 6 skills (code-debugger, git-helper, readme-writer, test-generator, code-review, file-organizer)
  - 文档: docs/plans/2026-03-01-octo-workbench-v1-0-tasks.md
- 00:30 | OpenAI Thinking 修复: 添加多字段支持 (reasoning_content, thinking, reasoning)
  - 问题: provider=openai 时 Thinking 不显示，只解析 reasoning_content 字段
  - 修复: openai.rs 增加 thinking_fields 数组遍历匹配 [claude-mem #2998]
- 21:00 | 统一配置系统: 实现 /api/config 端点，前端从后端获取运行时配置
- 21:00 | 修复 provider 特定环境变量: 根据 LLM_PROVIDER 读取对应 MODEL_NAME
- 21:00 | 修复 dotenv 加载顺序: dotenv_override() 必须在 Config::load() 之前
- 21:00 | 模型参数 fail-fast: 未设置时 panic 而非静默使用默认值
- 16:45 | 对话上下文 Bug 修复完成 (cargo check + tsc 全通过)
  - loop_.rs: 所有退出路径保证写入 Assistant 消息，防止连续两个 User 消息
  - ws.rs: session 复用改用 get_session() 保留原 sandbox_id
  - Memory.tsx: 搜索过滤字段从 block.content 修正为 block.value + block.label
- 20:15 | MCP SSE Transport 阶段归档完成
- 20:45 | 竞争力分析完成: 7项目代码级对比 (docs/design/COMPETITIVE_ANALYSIS.md)
  - 对比项目: OpenFang(137K), Craft-Agents(145K), pi_agent_rust(278K), OpenClaw(289K), ZeroClaw(37K), HappyClaw(18K)
  - 核心优势: 6级Context降级精细度领先、Debug面板可观测性最好、代码密度高
  - 关键差距: 沙箱隔离(NativeRuntime)、定时任务(空白)、企业安全(零)、工具数(12 vs 54)
  - v1.0 方案A(单用户): 需补齐~5,150 LOC; 方案B(企业级): 额外15-20K LOC

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
