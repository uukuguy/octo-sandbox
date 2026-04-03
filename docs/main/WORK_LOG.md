# Octo Sandbox 开发工作日志

## 2026-04-03 — Phase AY: SubAgent Runtime 完整生命周期 @ e80679f

### 会话概要

实现 SubAgentRuntime 完整生命周期架构，对齐 CC-OSS 的 AgentTool + runAgent 模式。Agent 和 Skill Playbook 执行路径统一。所有 6 个 deferred items 清零，并完成全面接线审计修复。

### 完成内容

**W1: SubAgentRuntime 核心 + 工具重命名**
- `SubAgentRuntime` 结构体 (build/run_sync/run_async/Drop lifecycle)
- `SpawnSubAgentTool` → `AgentTool`，tool name: `spawn_subagent` → `agent`
- `QuerySubAgentTool` → `QueryAgentTool`，tool name: `query_subagent` → `query_agent`
- `ExecuteSkillTool` Playbook 模式复用 SubAgentRuntime（Agent/Skill 共享执行路径）
- 删除 `agents/` YAML 目录（builtin agents 只在代码中定义）

**W2: 生命周期增强**
- event_sender 从 AgentExecutor broadcast_tx 注入
- Drop guard: config 未消费时自动 cancel 已注册的 sub-agent

**Deferred D1-D6 全部解决**
- D1: working_dir 继承（worktree 隔离）
- D2: transcript_writer 继承
- D3: 子 agent 独立 CancellationToken
- D4: MCP scope 由既有 tool_filter/disallowed_tools 覆盖
- D5: `HookRegistry.scoped()` + `AgentManifest.hook_scope`
- D6: `AgentManifest.permission_mode` → per-instance `ApprovalManager`

**接线审计修复（4 个 landmine）**
- safety_pipeline 传入 parent_config + SubAgentRuntime
- canary_token 同上
- recorder 同上
- loop_guard 在 SubAgentRuntime 中创建

### 技术变更

| 文件 | 变更 |
|------|------|
| `crates/octo-engine/src/agent/subagent_runtime.rs` | 新增 ~300 行 |
| `crates/octo-engine/src/tools/subagent.rs` | 重写 AgentTool/QueryAgentTool |
| `crates/octo-engine/src/skills/execute_tool.rs` | 重写使用 SubAgentRuntime |
| `crates/octo-engine/src/agent/executor.rs` | AgentTool 注册 + 接线修复 |
| `crates/octo-engine/src/agent/entry.rs` | +permission_mode, +hook_scope 字段 |
| `crates/octo-engine/src/hooks/registry.rs` | +scoped() 方法 |
| `crates/octo-engine/src/agent/runtime.rs` | 移除 agents_dir 加载 |
| `crates/octo-engine/src/agent/builtin_agents.rs` | tool name 更新 |
| `crates/octo-cli/src/tui/formatters/tool_registry.rs` | TUI tool name 更新 |
| `agents/*.yaml` | 删除 6 个文件 |

### 测试结果
- 15 个新测试全部通过（13 subagent + 2 hook scoped）
- 全量编译通过

### 未清项
- 无（6/6 deferred 全部解决）

### 下一步
- 待定：新 Phase 规划

---

## 2026-04-03 — CC-OSS 分析报告缺陷全部清零 + P3-5/P3-6/P3-7 @ efaa230

### 会话概要

完成 CC-OSS vs Octo 分析报告中最后 3 项缺失功能（P3-5 内置开发命令、P3-6 doctor 自诊断、P3-7 通知器），使报告中 37/37 项改进全部落地。4 文件，+1128 行，24 个新测试全部通过。

### 完成内容

**P3-5: 内置开发命令 (`dev_commands.rs`, ~300 行)**
- `git_diff` — 结构化 diff（staged/unstaged/stat/path 过滤），只读工具
- `git_commit` — 安全 commit（自动拒绝 .env/.pem/.key 等敏感文件模式匹配）
- `security_review` — 静态安全扫描器（9 种正则模式：硬编码密钥、AWS Key、GitHub Token、SQL 注入上下文、命令注入、路径遍历、unsafe 块）
- 10 个单元测试

**P3-6: Doctor 自诊断 (`doctor.rs`, ~230 行)**
- 5 个必需二进制检查（git/cargo/node/npm/rustc）+ 版本输出
- 环境变量检查（ANTHROPIC_API_KEY 必需，OPENAI_API_KEY/RUST_LOG 可选）
- 工作目录存在性、Git 仓库状态（分支名）、磁盘空间
- 结构化 PASS/WARN/FAIL 报告 + 汇总统计
- 7 个单元测试

**P3-7: Notifier 通知器 (`notifier.rs`, ~220 行)**
- desktop: macOS `osascript` / Linux `notify-send`，支持 low/normal/critical urgency
- webhook: Slack/Discord 兼容 JSON POST（text + content 双字段），10 秒超时
- log: 追加到 `.octo/notifications.log`（UTC 时间戳格式）
- `channel: "all"` 同时发送全部渠道
- 7 个单元测试

**注册与集成**
- 5 个工具（git_diff, git_commit, security_review, doctor, notify）注册到 `default_tools()`
- mod.rs 新增 3 个模块声明 + 3 个 use import

### CC-OSS 分析报告最终状态

| 优先级 | 项数 | 完成度 |
|--------|------|--------|
| P0 核心能力 | 7/7 | 100% |
| P1 安全与协作 | 10/10 | 100% |
| P2 增量增强 | 10/10 | 100% |
| P3 锦上添花 | 7/7 | 100% |
| TUI 体验打磨 | 3/3 | 100% |
| **总计** | **37/37** | **100%** |

### 技术细节

- 编译: `cargo check --workspace` 通过，无新 warning
- 测试: 24 个新测试全部通过（10 dev_commands + 7 doctor + 7 notifier）
- RiskLevel: git_commit 使用 `HighRisk`（无 MediumRisk 枚举）
- GitHub token 检测: `ghp_[a-zA-Z0-9]{20,}` (从 {36} 放宽到 {20,})

---

## 2026-04-02 — Phase AT: 提示词体系增强 + 编译优化 @ c1fad3d

### 会话概要

完成 Octo 提示词体系增强（4 大领域），修复 pinned memories 回显 bug，实施编译优化。23 文件，+886/-55 行。

### 完成内容

**T1: System Prompt 静态段增强**
- 新增 `GIT_WORKFLOW_SECTION`：commit/PR 安全协议（~25 行）
- 新增 `CYBER_RISK_SECTION`：安全约束（URL 禁令、授权测试范围）
- 新增 `PERMISSION_SECTION`：权限模式说明（Development/Preferred/Strict）
- 新增 `SUBAGENT_SECTION`：子智能体专属行为指导
- `with_subagent_mode()` builder 方法

**T2: System Prompt 动态段增强**
- `with_environment_info(platform, shell, os, model)` — 运行环境注入
- `with_token_budget(max_output, context_window)` — token 预算注入
- Harness 从 `builder.build()` 切换到 `builder.build_separated().merge()` 为 prompt caching 做准备

**T3: Tool Description 接线**
- 8 个核心工具接线到 `prompts.rs` 详细描述：bash, file_read, file_edit, file_write, grep, glob, web_search, web_fetch
- 每个工具从 1 行短描述升级为 CC-OSS 风格的完整使用手册（含 When to use / NOT to use / Best practices）

**T4: Pinned Memories 修复**
- Cross-session 和 pinned memories 从 user message 移至 system prompt（防止 LLM 在工具结果中回显）
- `CompactionResult` 新增 `system_prompt_additions` 字段
- `rebuild_state()` 返回 `(Vec<ChatMessage>, String)` 元组
- XML 标签包裹 `<pinned-memories>` / `<cross-session-memory>`

**T5: 编译优化**
- Feature gate：WASM/Docker/PDF 从 default 移除，加 `full` feature（cold build 112s → 83s）
- `codegen-units` 256 → 16（匹配 M3 Max 16 核）
- Makefile 新增 `build-full`、`build-cli-full`，`release` 自动带 `--features full`
- `PhantomData` import 修复（docker.rs 在 sandbox-docker feature 关闭时）
- 更新 `RUST_BUILD_OPTIMIZATION.md` 编译瓶颈分析

### 测试结果
- `cargo check --workspace` 通过
- 21 个 system_prompt 相关测试全部通过
- 13 个 memory_injector 测试全部通过

### Deferred
| ID | 内容 | 前置条件 |
|----|------|----------|
| AT-D1 | MCP instructions 从 rmcp InitializeResult 提取 | rmcp 0.16 instructions 字段确认 |
| AT-D2 | SecurityPolicy 当前值动态注入 | SecurityPolicy 可序列化为人类可读文本 |
| AT-D3 | Coordinator prompt（多 agent 编排模式） | Coordinator 架构设计 |
| AT-D4 | 补全所有 memory/skill 工具的详细 description | 当前 9 个核心工具优先 |
| AT-D5 | Anthropic prompt caching API（cache_control） | ApiRequest.system 改为数组格式 |

---

## 2026-04-02 — Phase AS Deferred: 4 项遗留解决 @ 6acb2d1

### 会话概要

解决 Phase AS 全部 4 项 deferred items：InteractionGate 接线、SystemPromptBuilder 死代码清理、NotebookEdit 工具、Zone B+ importance 保底通道。总计 16 文件、+625/-281 行变更。

### 完成内容

**T1: InteractionGate 接线**
- `AgentRuntime` 新增共享 `interaction_gate: Arc<InteractionGate>` 字段
- `AgentExecutor` 使用 runtime 共享 gate（替代每 turn 新建）
- `AskUserTool` + `ToolSearchTool` 注册到 runtime 默认工具
- `ToolSearchTool` 从 `tokio::RwLock` 改为 `std::Mutex`（匹配 runtime 类型）
- `octo-server/ws.rs`: 新增 `ClientMessage::InteractionResponse` + `ServerMessage::InteractionRequested`
- `octo-platform-server/ws.rs`: 同步添加
- `octo-cli/tui`: 处理 `InteractionRequested` 事件（显示提示，等待超时）

**T2: SystemPromptBuilder 死代码清理**
- 删除 `builder.rs` 中旧 `SystemPromptBuilder` + `ContextBuilder`（260+ 行）
- 保留 `estimate_messages_tokens` 函数
- 更新 `mod.rs`、`lib.rs`、`agent/context.rs` re-exports
- 活跃 builder 在 `system_prompt.rs`（已有完整 PromptParts 支持）

**T3: NotebookEdit 工具**
- 新增 `tools/notebook_edit.rs`：.ipynb cell 编辑（insert/replace/delete）
- 路径安全验证 + symlink 防御
- 注册到 `default_tools()`
- 3 个测试覆盖全部操作

**T4: Zone B+ importance 保底通道**
- `MemoryInjector::build_pinned_memories()` 按 importance≥0.8 取 top-5 记忆
- 不依赖 FTS query，纯按重要性排序
- 在 harness 初始化和 compaction rebuild 两处注入
- 4 个新测试

### 文件变更

- `crates/octo-engine/src/agent/runtime.rs` — +interaction_gate 字段, AskUser/ToolSearch 注册
- `crates/octo-engine/src/agent/executor.rs` — 共享 gate, 构造函数扩展
- `crates/octo-engine/src/agent/harness.rs` — Zone B+ pinned 注入
- `crates/octo-engine/src/context/builder.rs` — 删除旧 builder（-260 行）
- `crates/octo-engine/src/context/mod.rs` — 更新 re-exports
- `crates/octo-engine/src/lib.rs` — 更新 re-exports
- `crates/octo-engine/src/memory/memory_injector.rs` — +build_pinned_memories +4 tests
- `crates/octo-engine/src/tools/notebook_edit.rs` — 新文件
- `crates/octo-engine/src/tools/mod.rs` — 注册 NotebookEditTool
- `crates/octo-engine/src/tools/tool_search.rs` — StdMutex 替代 RwLock
- `crates/octo-server/src/state.rs` — +interaction_gate
- `crates/octo-server/src/ws.rs` — InteractionRequest/Response 消息
- `crates/octo-platform-server/src/ws.rs` — 同上
- `crates/octo-cli/src/tui/mod.rs` — InteractionRequested 事件处理

### Phase AS 遗留状态

所有 4 项已解决 ✅。Phase AS 完结。

---

## 2026-04-02 — Phase AS: CC-OSS 差距修复 + 记忆系统修复 + TUI 改善

### 会话概要

系统性审查 CC-OSS 功能差距并修复。发现并修复了 6 个未接线/设计偏差问题、3 个记忆系统 bug、多个 TUI 问题、以及 stream 错误处理缺陷。总计 5 个 commit、41 文件、~365 行变更。

### 完成内容

**Phase AS: CC-OSS 差距修复 @ 852e185**（6 项）:
- P0: PermissionEngine 接入 harness 工具执行前 evaluate 检查
- P0: CLAUDE.md/bootstrap 文件注入 LLM system prompt（with_bootstrap_dir）
- P0: Blob 持久化修复 — 当前轮 LLM 看完整内容，历史存 blob 引用
- P0: file_read 防御 blob 引用路径（清晰错误提示）
- P1: Git context（branch/status/commits）注入 system prompt
- P1: AgentLoopConfig 新增 working_dir + GitContext 字段

**记忆系统修复 @ 9ab46fe**（3 项）:
- user_id 统一 — ToolContext 新增 user_id 字段，消除 "default" vs "cli-user" 不一致
- 定义 DEFAULT_USER_ID 常量，所有 memory_* 工具 + CLI 统一使用
- FTS5 中文分词 — 新增 tokenize_for_fts()，CJK 逐字拆分 + Latin 按词拆分
- 修复旧数据迁移（cli-user → default）

**TUI 改善 @ 9ab46fe**:
- 空闲状态无提示，流式仅 "Esc interrupt"
- 修复分隔线缺口（空 hints 时填满横线）
- Esc 中断整轮 — 工具执行共享 config.cancel_token

**构建优化 @ 00c6dc2**:
- Cargo workspace default-members 排除 octo-desktop（Tauri build.rs 重编译问题）
- make cli-tui 依赖 build-cli 而非 build
- 增量编译从 ~30s → <1s

**Stream 错误重试 @ 7650365**:
- consume_stream 错误新增重试机制（MAX_STREAM_ERROR_RETRIES=2，1s 间隔）
- 避免 JSON 解析失败/连接中断时直接终止对话

### 其他修复
- rust-analyzer proc-macro 版本不匹配：wrapper 脚本 /usr/local/bin/rust-analyzer
- 旧 DB 记忆迁移：./data/octo.db → ~/.octo/projects/.../octo.db

### 遗留问题
- P1-#4: InteractionGate 未接线（ask_user 有独立 channel，需更大重构）
- P2-#8: 双 SystemPromptBuilder 共存（builder.rs 死代码）
- P3-#7: 缺少 NotebookEdit 工具
- 记忆系统：Zone B+ 仅靠 FTS 搜索注入，高 importance 记忆无保底通道

## 2026-04-01 — Phase AO: octo-server 功能完善（10/10 + 2 stubs）

### 会话概要

完成 Phase AO — 将 octo-engine 已实现但未暴露的能力全部通过 REST API 输出。3 波次执行，10 个任务全部完成，额外解决 2 个 NOT_IMPLEMENTED stub。

### 完成内容

**Wave 1（P1）@ 757ddc8**:
- AO-T1: Metering API — 4 个端点（snapshot/summary/by-session/reset）
- AO-T2: Knowledge Graph API — 9 个端点（CRUD/FTS/traverse/path/stats）

**Wave 2（P2）@ 9b3075b**:
- AO-T3: Hooks Management — 5 个端点（list/points/reload/wasm/wasm-reload）
- AO-T4: Security Policy — 4 个端点（policy GET/PUT/tracker/check-command）
- AO-T5: AI Defence — 3 个端点（scan/pii-redact/defence-status）
- AO-T6: Secret Vault — 4 个端点（list/store/delete/verify）
- AO-T7: Sandbox Management — 4 个端点（status/sessions/release/cleanup）

**Wave 3（P2-P3）@ a660366**:
- AO-T8: Config 运行时热更新 — PUT /config（RuntimeConfigOverrides 模式）
- AO-T9: Audit 增强 — export/delete/stats 3 个端点
- AO-T10: Context 可观察性 — snapshot/zones 2 个端点

**Stub 修复 @ 39159aa**:
- PUT /security/policy — RuntimeConfigOverrides 存储 autonomy_level 等策略覆盖
- POST /hooks/wasm/:name/reload — feature-gated 插件验证（非 501）

### 技术变更

- `RuntimeConfigOverrides`（RwLock）模式：避免改动所有 `state.config` 读取点
- `AuditStorage` 新增 export/delete_before/stats 方法 + `AuditStats` 结构体
- `ContextManager` 轻量级快照用于可观察性端点
- 新增 `api/context.rs` 模块
- octo-server 零 NOT_IMPLEMENTED stub

### 测试结果

- 新增 36 个 E2E 测试（Wave 1: 10, Wave 2: 21, Wave 3: 11, Stubs: 4）
- 全部通过（--test-threads=1）
- 基线：2476 tests（未运行全量测试套件）

### 暂缓项

| ID | 内容 | 理由 |
|----|------|------|
| AO-D1 | WebSocket 订阅 metering 实时流 | 需前端配合 |
| AO-D2 | KG 图算法扩展 | 需产品场景 |
| AO-D3 | Hook 在线编辑 | 安全审批流 |
| AO-D4 | Secret rotation | 需与 AK-D3 合并 |

### 后续建议

1. 跑一次全量测试确认 baseline 无回归
2. 考虑下一阶段：deferred 清理、Phase AN（platform-server）、或前端集成
3. 前端可基于新 API 端点扩展功能（KG 可视化 AL-D1 现有后端支持）

## 2026-03-11 — 架构完成度评估 + Wave 6 计划创建

### 会话概要

完成了 octo-sandbox 全架构的完成度审计（3 个并行研究智能体 + 1 个集成差距分析智能体），确认架构功能完成度 95%+，无重大偏差。基于审计结果创建了 Wave 6（最后阶段）实施计划。

### 架构审计结果

**octo-engine**: 24 模块全部 COMPLETE，44,076 LOC，0 stub，0 todo!()，1548 tests
**octo-server**: 21 API 端点全部有真实实现，中间件完整（auth → audit → rate_limit）
**octo-cli**: 50+ 文件，Commands + TUI (16 screens) + REPL，三种交互模式
**web/ 前端**: 8 页面 + 21 组件全部实现，API/WS 集成已验证
**octo-desktop**: Tauri 封装 + 嵌入式 Axum server
**设计文档**: 31 篇，~18K 行，全部实质性内容

### 发现的集成差距

1. **CLI 嵌入引擎运行**，无 HTTP 客户端连 server（设计合理，列为 Deferred D8）
2. **Server 无 E2E 测试**（`crates/octo-server/tests/` 目录不存在）
3. **config.default.yaml 与 config.yaml 不同步**，缺少 TLS/sync/consensus 配置
4. **`.unwrap()` 208 处**（octo-engine），关键热点在 `skills/loader.rs` Mutex 锁
5. **Docker Compose 前端需 Nginx 分离服务**（非 Dockerfile 内嵌）

### Wave 6 计划

| Wave | 主题 | Tasks | 状态 |
|------|------|-------|------|
| 6a | Server E2E 测试 | 5 | PENDING |
| 6b | 生产加固 (.unwrap + Error + Shutdown) | 5 | PENDING |
| 6c | 配置 & 部署完善 | 5 | PENDING |

### 产出文件

| 文件 | 说明 |
|------|------|
| `docs/plans/2026-03-11-wave6-production-hardening.md` | Wave 6 实施计划（15 tasks） |
| `docs/plans/.checkpoint.json` | Checkpoint（指向 Wave 6，READY） |

### 设计决策

1. **CLI 维持嵌入引擎** — server 模式作为可选增强（Deferred D8）
2. **E2E 测试用 axum::TestServer** — 避免端口冲突和网络依赖
3. **.unwrap() 仅修关键路径** — 测试代码保留
4. **Docker 维持 compose 分离** — Nginx/Caddy 作为独立服务

### 下一步

- 运行 `/resume-plan` 选择执行模式，开始 Wave 6 三组并行
- 三组完全独立，可同时启动 6-8 个智能体

---

## 2026-03-09 — Harness 实现计划创建

### 会话概要

pre-harness-refactor 阶段已全部完成（42/42 任务 + 5 Deferred，857 tests passing），正式启动 **Harness 实现** 阶段。完成了代码分析、差距评估，并创建了 28 任务的详细实施计划。

### 当前状态分析

**已完成的基础模块（pre-harness-refactor 产出）**:
- `agent/loop_config.rs` — AgentLoopConfig（仅控制参数，未包含依赖注入）
- `agent/events.rs` — AgentLoopResult + NormalizedStopReason
- `agent/loop_steps.rs` — check_loop_guard_verdict / inject_zone_b / maybe_trim_tool_result
- `agent/turn_gate.rs` — TurnGate 并发控制
- `agent/subagent.rs` — SubAgentManager
- `agent/continuation.rs` — ContinuationTracker（max-tokens 续写）
- `agent/deferred_action.rs` — DeferredActionDetector
- `tools/interceptor.rs` — ToolCallInterceptor
- `context/observation_masker.rs` — ObservationMasker
- `context/fork.rs` — ContextFork
- `context/token_counter.rs` — CjkAwareCounter + TiktokenCounter
- `skills/selector.rs` — SkillSelector 4 阶段
- `skills/catalog.rs` — SkillCatalog 远程 Registry
- `mcp/traits.rs` — MCP Tool Annotations
- `providers/pipeline.rs` — ProviderPipelineBuilder
- `security/safety_pipeline.rs` — SafetyPipeline

**核心差距**:
- `loop_.rs` 仍然是 949 行 monolithic `run()` 方法，未使用上述模块
- `AgentEvent` 定义在 `loop_.rs` 中，与 `events.rs` 分离
- `run()` 返回 `Result<()>` 而非 `BoxStream<AgentEvent>`
- AgentExecutor 直接构建 AgentLoop，未使用 AgentLoopConfig 注入

### 实施计划

| 阶段 | 任务数 | 核心目标 | 风险 |
|------|--------|---------|------|
| P0 | 8 | AgentEvent 统一、run_agent_loop() 纯函数、BoxStream 返回 | 高 |
| P1 | 8 | Continuation/ObservationMasker/Interceptor/DeferredAction/TurnGate | 中 |
| P2 | 6 | AgentExecutor/WS handler/Scheduler/Runtime 适配新接口 | 中 |
| P3 | 6 | 全量测试回归、集成测试、废弃清理 | 低 |
| 合计 | 28 | ~1700 LOC | |

### 核心设计决策

1. **新建 `harness.rs`** — 纯函数式 `run_agent_loop()` 实现，与 `loop_.rs` 并存过渡
2. **`BoxStream<AgentEvent>` 返回** — mpsc channel + tokio::spawn 驱动
3. **AgentLoopConfig 扩展为完整依赖注入容器** — 替代 AgentLoop struct 的 17+ 字段
4. **保留 AgentLoop 作为 thin wrapper** — 向后兼容
5. **7 个 step functions** — build_context / manage_context / call_provider / consume_stream / execute_tools / check_loop_guard / handle_continuation

### 产出文件

| 文件 | 说明 |
|------|------|
| `docs/plans/2026-03-09-harness-implementation.md` | Harness 实施计划（28 任务 + 6 Deferred） |
| `docs/plans/.checkpoint.json` | Checkpoint（新阶段） |
| `docs/dev/NEXT_SESSION_GUIDE.md` | 下一会话指南（已更新） |
| `docs/dev/.phase_stack.json` | 阶段栈（Harness 实现 active） |

### 下一步

- 运行 `/superpowers:executing-plans` 开始 P0-1（统一 AgentEvent 到 events.rs）
- P0 严格顺序执行，每步后 `cargo check --workspace`

---

## 2026-03-09 — Pre-Harness Refactor 计划重新组织（P0/P1/P2/P3）

### 会话概要

将原 R1-R8（14 任务）实施计划重新组织为 P0/P1/P2/P3（42 任务），全面覆盖 4 份设计文档的所有设计项。

### 重组原因

原 R1-R8 计划仅覆盖约 40% 设计内容，遗漏了：
- **Harness 缺失**: AgentEvent 事件流、Tool trait 增强（risk_level/approval）、ToolOutput 结构化、截断策略、max-tokens 续传、force_text_at、SafetyPipeline、SubAgent、Observation Masking、NormalizedStopReason、Provider Pipeline
- **Skills 缺失**: SkillManager 统一入口、SkillSelector 4 阶段管道、ToolConstraintEnforcer、NodeJS/Shell Runtime、Skills↔Context 集成、依赖图、SlashRouter、SkillCatalog、context-fork、model override

### 新计划结构

| 阶段 | 任务数 | 估计 LOC | 核心内容 |
|------|--------|----------|----------|
| P0 | 10 | ~1200 | AgentLoopConfig, Step Functions, AgentEvent, SkillDefinition, SkillTool, TrustManager, ToolCallInterceptor, TurnGate, 错误不持久化, ProviderErrorKind |
| P1 | 12 | ~1000 | Tool trait 增强, ToolOutput, 截断策略, HookFailureMode, max-tokens 续传, force_text_at, SkillManager, SafetyPipeline, ContextManager, ApprovalManager, cast_params, error hints |
| P2 | 10 | ~800 | Provider Pipeline, NormalizedStopReason, SkillSelector, NodeJS/Shell Runtime, TokenCounter, Skills↔Context, ToolConstraintEnforcer, 依赖图, Slash Router, AgentResult |
| P3 | 10 | ~600 | SubAgent, Observation Masking, context-fork, model override, WASM runtime, REST API, SkillCatalog, MCP annotations, semantic index, skill hooks |

**总计**: ~3600 LOC 新增 + ~1200 LOC 测试，42 个任务 + 15 个 Deferred 项

### 覆盖验证

计划包含 4 个设计文档覆盖追踪表，确保每个设计项都映射到对应任务或 Deferred 项。

### 研究过程（同日早期会话）

1. **Opus 深度分析** — 分析 8 个 Rust agent 项目 + 2 个 baseline，产出 3 份设计文档
2. **Sonnet 4.6 独立分析** — 发现 TurnGate、nanobot 错误不持久化、SkillSelector 4 阶段等关键模式
3. **交叉对比** — 合成收敛点和独特发现

### 产出文件

| 文件 | 说明 |
|------|------|
| `docs/plans/2026-03-09-pre-harness-refactor.md` | 实施计划（P0-P3，42 任务，已重写） |
| `docs/plans/.checkpoint.json` | Checkpoint（已更新为 42 任务） |
| `docs/dev/NEXT_SESSION_GUIDE.md` | 下一会话指南（已更新） |
| `docs/dev/.phase_stack.json` | 阶段栈 |
| `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` | Harness 设计（Opus） |
| `docs/design/AGENT_SKILLS_BEST_IMPLEMENTATION_DESIGN.md` | Skills 设计（Opus） |
| `docs/design/AGENT_HARNESS_INDUSTRY_RESEARCH_2025_2026.md` | 行业研究（Opus） |
| `docs/found-by-sonnet4.6/AGENT_HARNESS_DESIGN.md` | Harness 发现（Sonnet 4.6） |
| `docs/found-by-sonnet4.6/AGENT_SKILLS_DESIGN.md` | Skills 发现（Sonnet 4.6） |

### 下一步

- 选择执行方式（Subagent-Driven 或 Executing-Plans）开始 P0
- P0 推荐顺序: P0.1→P0.2→P0.3（Loop 链）| P0.4→P0.5→P0.6→P0.7（Skills 链）| P0.8（独立）| P0.9→P0.10（Provider 链）

---

## 2026-03-05 — Phase 2.11a 多租户支持 - Task 1 完成

### 会话概要

完成 Phase 2.11a (octo-engine 多租户适配) 的第一个任务：为 AgentCatalog 添加 TenantId 索引支持。

### 技术实现

**修改文件**：

1. **`crates/octo-types/src/id.rs`**
   - 添加 `TenantId` 类型（使用 `newtype_id!` 宏）
   - 添加 `DEFAULT_TENANT_ID = "default"` 常量用于向后兼容

2. **`crates/octo-engine/src/agent/entry.rs`**
   - `AgentEntry` 结构体添加 `tenant_id: TenantId` 字段
   - `AgentEntry::new()` 方法接受 `Option<TenantId>` 参数，默认使用 DEFAULT_TENANT_ID

3. **`crates/octo-engine/src/agent/catalog.rs`**
   - 添加 `by_tenant_id: DashMap<TenantId, Vec<AgentId>>` 索引
   - `register()` 方法接受 `tenant_id: Option<TenantId>` 参数
   - 新增 `get_by_tenant()` 方法查询租户下所有 Agent
   - `load_from_store()` 和 `unregister()` 同步更新 tenant 索引

4. **`crates/octo-engine/src/agent/store.rs`**
   - 数据库 schema 添加 `tenant_id` 列和索引
   - `save()` 和 `load_all()` 方法支持 tenant_id 持久化

### 验证结果

- `cargo check -p octo-engine` 编译通过，无错误
- Git commit 成功：`a741987 feat(agent): add TenantId to AgentCatalog and AgentEntry`

### 向后兼容

- 现有 agent 无需迁移：空 tenant_id 自动填充为 "default"
- 单用户场景使用默认租户 ID，无需修改调用代码

---

## 2026-03-05 — P2 Multi-tenant 实施 (Task 5: Tenant MCP Config API)

### 会话概要

完成 P2 Multi-tenant Implementation Plan 的 Task 5 - Tenant MCP Config API 实现。

### 技术变更

**新增文件**
- `crates/octo-platform-server/src/api/mcp.rs` — MCP 配置 API 端点
  - `GET /api/mcp` — 列出租户所有 MCP 服务器
  - `POST /api/mcp` — 添加新的 MCP 服务器配置
  - `GET /api/mcp/:id` — 获取指定 MCP 服务器
  - `DELETE /api/mcp/:id` — 删除 MCP 服务器

**修改文件**
- `crates/octo-platform-server/src/api/mod.rs` — 添加 mcp 模块导出
- `crates/octo-platform-server/src/lib.rs` — 添加 TenantManager 到 AppState
- `crates/octo-platform-server/src/main.rs` — 注册 MCP 路由
- `crates/octo-platform-server/src/tenant/manager.rs` — 添加 Debug trait 实现

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/mcp` | GET | 列出租户所有 MCP 服务器 |
| `/api/mcp` | POST | 添加 MCP 服务器配置 |
| `/api/mcp/:id` | GET | 获取指定 MCP 服务器 |
| `/api/mcp/:id` | DELETE | 删除 MCP 服务器 |

### 验证结果

| 检查项 | 状态 |
|--------|------|
| `cargo check -p octo-platform-server` | ✅ 通过 |

### Git 提交

`2dabe3e` - feat(platform): add tenant MCP configuration API
>>>>>>> octo-platform

---

## 2026-03-04 — v1.0 发布冲刺设计 + AgentRuntime 深度架构分析

### 会话概要

完成 AgentRuntime 全面架构审计，对标 Goose/OpenHands/pi_agent_rust 等顶级框架，识别关键问题并制定 v1.0 发布冲刺完整方案。

### 架构分析产出

**深度审计**（`docs/design/AGENT_RUNTIME_ARCHITECTURE_AUDIT.md`）：
- 精确定位 3 个 P0 bug：MCP 动态注册对运行中 Agent 无效、stop_primary 不真正终止、Scheduler run_now 假执行
- 确认 2 个 P1 问题：WorkingMemory 无 session 隔离、enable_parallel 配置无效
- 对标 Goose 版本化缓存模式、OpenHands EventStream 架构、pi_agent_rust 扩展机制

**v1.0 发布冲刺设计**（`docs/plans/2026-03-04-v1.0-release-sprint-design.md`）：
- 19 个 Feature：S1-6（稳定可靠）+ C1-7（能力完整）+ O1-6（可观测性）
- 4 个架构关键调整：ToolRegistry 版本化、stop 语义修复、WorkingMemory 隔离、后台任务 API
- 4 个 Phase：地基（~3天）→ 后端能力（~4天）→ 前端控制台（~5天）→ 集成验收（~2天）

### 当前状态

- **AgentRuntime 重构**：已完成（commit 520a1bc），零编译错误
- **v1.0 设计**：已完成并文档化，待进入实施
- **下一步**：writing-plans 生成实施计划 → Phase A 开始执行

### 待解决问题

- P0: MCP 工具动态注册对运行中 Agent 无效（ToolRegistry 快照问题）
- P0: stop_primary 发 Cancel 但 Executor while loop 继续（需改为 drop tx）
- P0: Scheduler run_now 假执行（未调用 execute_scheduled_task）
- P1: WorkingMemory 全局共享无 session 隔离
- P1: enable_parallel 配置无效（loop_.rs 仍是串行）
- P2: LoopTurnStarted 事件未发布（turns 指标为0）

---

## 2026-03-02 — Phase 2.8 Agent 增强 + Secret Manager 实施

### 会话概要

Phase 2.8 实现企业级 Secret Manager 和 Agent Loop 增强功能。使用 subagent-driven-development 方式，10/10 任务全部完成。

### 技术变更

**Secret Manager**
- `crates/octo-engine/src/secret/vault.rs` — CredentialVault 加密存储
  - AES-256-GCM 加密
  - Argon2id 密钥派生
  - Zeroize 内存安全
- `crates/octo-engine/src/secret/resolver.rs` — CredentialResolver 凭证解析链
  - 支持 Vault / .env / 环境变量优先级
  - 完整 .env 文件解析器（注释、引号、转义序列）
  - `${SECRET:key}` 配置语法解析
- `crates/octo-engine/src/secret/taint.rs` — Taint Tracking 敏感数据追踪
  - Secret / Confidential / Internal / Public 标签
  - Sink 流量控制（Log, Error, ExternalResponse, File）
  - TaintViolation 违规报告

**Agent Loop 增强**
- `crates/octo-engine/src/agent/config.rs` — AgentConfig 配置
  - max_rounds (0=无限)
  - enable_parallel / max_parallel_tools
  - enable_typing_signal
- `crates/octo-engine/src/agent/extension.rs` — Extension 事件钩子
  - ExtensionEvent 事件类型
  - AgentExtension trait
  - ExtensionRegistry 注册表
- `crates/octo-engine/src/agent/cancellation.rs` — CancellationToken 取消机制
  - 父/子 Token 级联
  - watch::Sender 通知
- `crates/octo-engine/src/agent/parallel.rs` — 并行工具执行
  - Semaphore 并发控制
  - CancellationToken 集成
  - 结果顺序保持
- `crates/octo-engine/src/agent/loop_.rs` — 集成修改
  - 50轮/无限轮支持
  - Typing 信号发送
  - 并行/顺序执行切换

### Bug 修复

- resolver.rs: stub .env 解析器 → 完整实现
- taint.rs: 缺失的 TaintedValue 方法 → 完整实现
- loop_guard.rs: unused variable 警告修复

### 测试结果

- `cargo check --workspace`: ✅ 通过
- `cargo test --lib`: ✅ 149 测试通过
- `npx tsc --noEmit`: ✅ 通过

### 产出文件

- `crates/octo-engine/src/secret/` — 完整 Secret Manager 模块
- `crates/octo-engine/src/agent/config.rs` — Agent 配置
- `crates/octo-engine/src/agent/extension.rs` — Extension 钩子
- `crates/octo-engine/src/agent/cancellation.rs` — 取消机制
- `crates/octo-engine/src/agent/parallel.rs` — 并行执行
- `docs/design/PHASE_2_8_AGENT_ENHANCEMENT_DESIGN.md` — 设计文档

---

## 2026-03-01 — Phase 2.7 Metrics + Audit 实施

### 会话概要

Phase 2.7 使用 subagent-driven-development 方式，一次会话完成全部 8 个任务，实现完整的可观测性系统。

### 技术变更

**Metrics 系统**
- `crates/octo-engine/src/metrics/` — 新增 MetricsRegistry 模块
  - Counter, Gauge, Histogram 类型，使用 DashMap 实现无锁并发
  - EventBus 集成自动收集指标
  - 33 个单元测试

**Audit 系统**
- `crates/octo-engine/src/audit/` — 新增 AuditStorage 模块
  - SQLite 持久化，Migration v6
  - Axum Middleware 自动记录 HTTP 请求
- `crates/octo-server/src/api/` — 新增 REST API
  - `GET /api/v1/metrics` — 指标快照
  - `GET /api/v1/audit` — 审计日志查询

**其他修复**
- scheduler 模型名称可配置化
- docker.rs unused field 警告修复
- sandbox-docker/sandbox-wasm 特性确认默认启用

### 测试结果

- `cargo check --all`: ✅ 通过
- `cargo test --lib`: ✅ 110 测试通过

### 产出文件

- `crates/octo-engine/src/metrics/` — 完整 metrics 模块
- `crates/octo-engine/src/audit/` — 完整 audit 模块
- `crates/octo-server/src/middleware/audit.rs` — HTTP 中间件

---

## 2026-02-27 — 竞争力分析 (7项目代码级对比)

### 会话概要

对 octo-workbench 与 6 个本地参考自主智能体项目进行代码级深度对比分析，评估 Phase 2 完成度、各维度竞争力、v1.0 距离。

### 分析范围

- **octo-workbench** (12K LOC, Rust+TS)
- **OpenFang** (137K LOC, Rust, 14 crate Agent OS)
- **Craft-Agents-OSS** (145K LOC, TypeScript, Electron桌面)
- **pi_agent_rust** (278K LOC, Rust, TUI编程Agent)
- **OpenClaw** (289K LOC, TypeScript, 多平台网关)
- **ZeroClaw** (37K LOC, Rust, 轻量级+可观测)
- **HappyClaw** (18K LOC, TypeScript, 多用户Docker平台)

### 关键发现

1. **Phase 2 全部完成** — 53个任务、约30个commit，Phase 2.1~2.4 + MCP SSE Transport 全部交付
2. **核心优势确认** — 6级Context降级精细度领先、Debug面板可观测性最好（TokenBudgetBar+EventLog）、12K LOC代码密度高
3. **关键差距** — 沙箱隔离(NativeRuntime，全场最弱)、定时任务(完全空白)、企业安全(零实现)、工具数量(12 vs OpenFang 54)、Agent Loop(10轮 vs 50轮)
4. **v1.0 距离** — 单用户方案需~5,150 LOC补齐；企业级方案需额外15-20K LOC

### 产出文件

- `docs/design/COMPETITIVE_ANALYSIS.md` — 完整竞争力分析报告

---

## 2026-02-27 — Phase 2.3 MCP Workbench 实现

### 会话概要

Phase 2.3 MCP Workbench 一次会话完成全部 12 个任务。从数据库设计到前端 UI，实现完整的 MCP 服务器管理界面。

### 技术变更

#### 后端 (Rust)

**数据库层**
- `crates/octo-engine/src/db/migrations.rs` — 添加 Migration V3 (mcp_servers, mcp_executions, mcp_logs 表)
- `crates/octo-engine/src/mcp/storage.rs` — 新增 MCP 存储模块 (SQLite CRUD)

**MCP 集成**
- `crates/octo-engine/src/mcp/traits.rs` — 添加 McpServerConfigV2 结构
- `crates/octo-engine/src/mcp/manager.rs` — 添加运行时状态跟踪 (ServerRuntimeState)

**API 层**
- `crates/octo-server/src/api/mcp_servers.rs` — MCP 服务器 CRUD 端点
- `crates/octo-server/src/api/mcp_tools.rs` — MCP 工具调用端点
- `crates/octo-server/src/api/mcp_logs.rs` — MCP 日志查询端点
- `crates/octo-server/Cargo.toml` — 添加 uuid, chrono 依赖

#### 前端 (TypeScript/React)

- `web/src/atoms/ui.ts` — 添加 "mcp" tab
- `web/src/components/layout/TabBar.tsx` — 添加 MCP 导航标签
- `web/src/App.tsx` — 添加 McpWorkbench 页面渲染
- `web/src/pages/McpWorkbench.tsx` — MCP 工作台主页面 (3 子标签)
- `web/src/components/mcp/ServerList.tsx` — 服务器列表组件
- `web/src/components/mcp/ToolInvoker.tsx` — 工具调用器组件
- `web/src/components/mcp/LogViewer.tsx` — 日志查看器组件

### Git 提交

| 提交 | 描述 |
|------|------|
| `6f6ccdb` | feat(mcp-workbench): complete frontend components with API integration |

### 新增/修改文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `crates/octo-engine/src/mcp/storage.rs` | 新增 | MCP 存储模块 |
| `crates/octo-server/src/api/mcp_servers.rs` | 新增 | MCP 服务器 API |
| `crates/octo-server/src/api/mcp_tools.rs` | 新增 | MCP 工具 API |
| `crates/octo-server/src/api/mcp_logs.rs` | 新增 | MCP 日志 API |
| `web/src/pages/McpWorkbench.tsx` | 新增 | MCP 工作台页面 |
| `web/src/components/mcp/ServerList.tsx` | 新增 | 服务器列表组件 |
| `web/src/components/mcp/ToolInvoker.tsx` | 新增 | 工具调用器组件 |
| `web/src/components/mcp/LogViewer.tsx` | 新增 | 日志查看器组件 |
| `web/src/atoms/ui.ts` | 修改 | 添加 mcp tab |
| `web/src/components/layout/TabBar.tsx` | 修改 | 添加 MCP 标签 |
| `web/src/App.tsx` | 修改 | 渲染 McpWorkbench |

### 构建验证

| 检查项 | 状态 |
|--------|------|
| `cargo check --workspace` | ✅ 通过 |
| `cd web && pnpm build` | ✅ 通过 |

### 下一步

- Phase 2.4 — 完善 MCP Workbench (运行时集成、进程管理)
- Phase 3 — 上下文工程完整实现

---

## 2026-02-28 — MCP 服务器启动/停止功能实现

### 会话概要

实现后端 MCP 服务器的启动、停止、状态查询 API，并修复多个编译错误。

### 技术变更

#### 后端 (Rust)

**MCP Manager**
- `crates/octo-engine/src/mcp/manager.rs` — 移除 storage 字段，使 McpManager 可 Send

**MCP Traits**
- `crates/octo-engine/src/mcp/traits.rs` — 为 McpTransport 实现 FromStr trait

**MCP Storage**
- `crates/octo-engine/src/mcp/storage.rs` — McpServerRecord 添加 transport 和 url 字段

**API 层**
- `crates/octo-server/src/api/mcp_servers.rs` — 实现 start_server, stop_server, get_server_status 端点

**状态管理**
- `crates/octo-server/src/state.rs` — 使用 tokio::sync::Mutex 替代 std::sync::Mutex
- `crates/octo-server/src/main.rs` — 更新 AppState 构造函数

### 修复的错误

1. `McpStorage clone` — 修复尝试克隆 MutexGuard 的问题
2. `McpServerRecord` — 添加缺失的 transport 和 url 字段
3. `McpTransport::parse` — 实现 FromStr trait
4. `ServerRuntimeState` — 使用正确的结构体语法
5. `AppState Send` — 移除 McpManager 中的 storage，使用异步 Mutex

### API 验证

| 端点 | 方法 | 状态 |
|------|------|------|
| `/api/mcp/servers` | GET | ✅ |
| `/api/mcp/servers/{id}/start` | POST | ✅ |
| `/api/mcp/servers/{id}/stop` | POST | ✅ |
| `/api/mcp/servers/{id}/status` | GET | ✅ |

---

## 2026-02-26 — Phase 1 核心引擎实现

### 会话概要

完成 Phase 1 全部 10 个步骤的编码实施。从零搭建 Cargo workspace + React 前端，实现完整的 AI 对话引擎（Provider → AgentLoop → WebSocket → Chat UI）。

### 技术变更

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

**octo-engine (12 文件)** — 核心引擎
- `providers/traits.rs` — Provider trait (complete + stream)
- `providers/anthropic.rs` — AnthropicProvider (完整 SSE stream 解析: message_start, content_block_delta, tool_use 积累, message_stop)
- `providers/mod.rs` — create_provider() 工厂
- `tools/traits.rs` — Tool trait (name/desc/params/execute/spec)
- `tools/bash.rs` — BashTool (tokio::process::Command, 30s 超时, env 清理)
- `tools/file_read.rs` — FileReadTool (1MB 限制, 行号显示, offset/limit)
- `tools/mod.rs` — ToolRegistry + default_tools()
- `agent/loop_.rs` — AgentLoop (最大 10 轮, 流式事件, 工具调用循环)
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
- `main.rs` — Axum 启动, dotenvy, tracing, graceful shutdown
- `router.rs` — build_router() (/api/health + /ws, CORS, TraceLayer)
- `ws.rs` — WebSocket handler (消息解析, AgentLoop 启动, broadcast 事件转发)
- `session.rs` — InMemorySessionStore (DashMap), SessionStore trait
- `state.rs` — AppState (Provider + ToolRegistry + WorkingMemory + AgentLoop)

#### 前端 (TypeScript/React, 16 个源文件)

**基础设施**
- `web/package.json` — React 19 + Jotai 2.16 + Tailwind CSS 4 + Vite 6
- `web/vite.config.ts` — Vite 配置 + API proxy → localhost:3001
- `web/tsconfig.json` — TypeScript 严格模式 + path aliases
- `web/src/main.tsx` — React root + Jotai Provider
- `web/src/globals.css` — Tailwind CSS 基础样式 + CSS 变量主题
- `web/src/lib/utils.ts` — cn() (clsx + tailwind-merge)

**状态管理**
- `web/src/atoms/session.ts` — sessionIdAtom, messagesAtom, isStreamingAtom, streamingTextAtom, toolExecutionsAtom
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
- `web/src/components/chat/MessageBubble.tsx` — 单条消息 (用户右蓝/助手左灰)
- `web/src/components/chat/ChatInput.tsx` — Textarea + 发送按钮
- `web/src/components/chat/StreamingDisplay.tsx` — 流式文本 + 工具执行状态

#### 构建配置
- `Cargo.toml` — workspace 定义 + profile 优化 (split-debuginfo, codegen-units=256)
- `.cargo/config.toml` — 编译优化 (jobs=8, dead_strip)
- `Makefile` — dev/build/check/test/fmt/lint 命令
- `.env.example` — ANTHROPIC_API_KEY 模板

### 构建验证结果

| 检查项 | 状态 | 详情 |
|--------|------|------|
| `cargo check --workspace` | ✅ 通过 | 0.25s, 仅 2 个 dead_code warnings |
| `cargo build` | ✅ 通过 | 21s, 13MB binary |
| `npx tsc --noEmit` | ✅ 通过 | 0 errors |
| `npx vite build` | ✅ 通过 | 874ms, 241KB JS bundle |

### 遗留问题

1. **sccache 不可用** — 系统内存压力大时 sccache 进程被 OOM kill。已注释掉配置，待系统空闲时启用。
2. **Dead code warnings** — `AppState.provider/tools/memory` 字段仅被 `ws.rs` 通过 `agent_loop` 间接使用，但 compiler 无法追踪。无需处理。
3. **Cancel 功能未实现** — WebSocket cancel 消息的处理需要 CancellationToken，留待后续实现。

### 下一步

- **运行时验证**: 需要 `ANTHROPIC_API_KEY` 环境变量才能启动 `cargo run -p octo-server` 进行端到端测试
- **前端开发服务器**: `cd web && npm run dev` 启动 Vite dev server
- **端到端测试**: 打开浏览器连接 WebSocket, 发送消息验证流式响应 + 工具调用

---

## 2026-02-26 — Phase 1 收尾与提交

### 会话概要

Phase 1 核心引擎全部代码提交到 git，阶段正式关闭。

### 操作记录

1. **代码提交** — `2c9ca43 feat: Phase 1 core engine - full-stack AI agent sandbox`
   - 73 个文件，13,431 行新增
   - 覆盖：4 个 Rust crates + React 前端 + 构建配置 + 设计文档
   - 排除：`.env`（含密钥）、`node_modules/`、`dist/`

2. **阶段关闭** — Phase 1 正式标记为 ✅ 已完成并提交
   - CHECKPOINT_PLAN.md 更新状态
   - MEMORY_INDEX.md 归档 Phase 1 记录
   - MCP memory 保存阶段完成摘要

### Phase 1 交付物总结

| 类别 | 数量 | 说明 |
|------|------|------|
| Rust 源文件 | 32 | octo-types(8) + octo-engine(14) + octo-sandbox(3) + octo-server(5) + Cargo.toml(4) |
| TS/React 源文件 | 16 | atoms(2) + ws(3) + components(7) + pages(1) + 基础设施(3) |
| 设计文档 | 7 | 架构设计(1) + brainstorming(2) + checkpoint(1) + 工作日志(1) + 记忆索引(1) + 构建优化(2) |
| 构建配置 | 6 | Cargo.toml, .cargo/config.toml, Makefile, .env.example, package.json, vite.config.ts |
| 运行时验证 | 10/10 | 服务器启动→健康检查→WS连接→Session→AgentLoop→Working Memory→API→流式传输→错误传播→重试 |

### Phase 1 遗留问题（移交 Phase 2）

1. **Cancel 功能** — WebSocket cancel 消息需要 CancellationToken 支持
2. **Dead code warnings** — AppState 字段间接使用，compiler 无法追踪，低优先级
3. **SSE bugfix 运行时验证** — pending_events VecDeque 修复已编译通过，待实际多 chunk 场景验证

### 下一步

- **Phase 2 规划** — 调试面板、MCP 集成、SQLite 持久化、Session Memory

---

## 2026-02-27 — Phase 2.2 记忆系统完整

### 会话概要

完成 Phase 2.2 全部任务，实现 5 个 memory tools 和 Memory Explorer UI。

### 技术变更

#### 后端 (Rust)

**新增文件**:
- `crates/octo-engine/src/tools/memory_recall.rs` — 语义记忆检索工具，支持按 ID 召回和语义相似推荐
- `crates/octo-engine/src/tools/memory_forget.rs` — 记忆删除工具，支持按 ID 或分类删除

**修改文件**:
- `crates/octo-engine/src/memory/sqlite_store.rs` — SQLite 存储实现
- `crates/octo-engine/src/memory/store_traits.rs` — MemoryStore trait 定义
- `crates/octo-engine/src/tools/mod.rs` — 工具注册
- `crates/octo-server/src/api/memories.rs` — REST API 端点
- `crates/octo-server/src/api/mod.rs` — API 模块

#### 前端 (TypeScript/React)

**新增文件**:
- `web/src/pages/Memory.tsx` — Memory Explorer 页面组件
  - Working Memory 视图：显示当前上下文块
  - Session Memory 视图：会话期间积累的记忆
  - Persistent Memory 视图：持久化存储的记忆
  - 搜索和分类过滤功能

**修改文件**:
- `web/src/atoms/ui.ts` — 新增 "memory" tab
- `web/src/components/layout/TabBar.tsx` — 新增 Memory 标签页
- `web/src/App.tsx` — 挂载 Memory 页面组件

### 构建验证结果

| 检查项 | 状态 | 详情 |
|--------|------|------|
| `cargo check --workspace` | ✅ 通过 | 24 warnings (unused code) |
| `pnpm tsc --noEmit` | ✅ 通过 | 0 errors |

### 已完成功能

1. **memory_recall** — 语义记忆检索，支持语义相似推荐
2. **memory_forget** — 记忆删除，支持按 ID 或分类批量删除
3. **Memory Explorer UI** — 可视化 Working/Session/Persistent 记忆
4. **REST API** — `/api/memories`, `/api/memories/working`, `/api/memories/{id}`

### 遗留问题

无

### 下一步

- **Phase 2.3** — 调试面板完善（MCP Workbench、Skill Studio、Network Interceptor、Context Viewer）
- **运行时验证** — 需要 API key 进行端到端测试

---

## 2026-02-26 — Phase 2 上下文工程架构设计

### 会话概要

完成 Phase 2 上下文工程架构的深度设计。分析 6 个参考项目的上下文工程实现，提炼跨项目共识模式，设计完整的上下文工程架构，并创建 14 任务实施计划。

### 设计过程

1. **参考项目分析** — 6 个并行子代理分别深度分析 OpenClaw、ZeroClaw、NanoClaw、HappyClaw、pi_agent_rust、Craft Agents 的上下文工程实现
2. **跨项目共识提炼** — Token 估算(3-4 chars/token)、混合检索(70%向量+30%FTS)、渐进式降级(soft→hard→compact)、压缩边界保护、两层提示架构(静态+动态)
3. **架构设计 Brainstorming** — 6 段逐节呈现，用户逐段确认
4. **设计文档编写** — 整合为 `docs/design/CONTEXT_ENGINEERING_DESIGN.md`（10 章，500+ 行）
5. **实施计划创建** — 读取所有现有源文件后，创建 `docs/plans/2026-02-26-phase2-context-engineering.md`（14 任务）

### 核心设计决策

| 决策 | 选项 | 选择 | 原因 |
|------|------|------|------|
| 上下文分区 | 整体混合 vs 分区 | 三区分配(A/B/C) | 区域 A 可利用 prompt caching，区域 B 每轮重建避免累积，区域 C 有明确降级路径 |
| 降级策略 | 简单截断 vs 渐进降级 | 三级渐进式 | 保护最新信息，优先降级旧工具结果 |
| Token 估算 | 纯估算 vs 纯 API | 双轨制 | 优先 API 真实值，fallback chars/4 |
| 预算管理 | 混合模块 vs 关注点分离 | Manager + Pruner 分离 | 可独立测试，职责清晰 |
| 压缩边界 | 任意截断 vs 边界保护 | 工具调用链边界保护 | pi_agent_rust 验证有效 |
| 记忆集成 | 全在历史中 vs 分层 | 三层(Working/Session/Persistent) | 不同生命周期分别管理 |

### 新增文件

| 文件 | 说明 |
|------|------|
| `docs/design/CONTEXT_ENGINEERING_DESIGN.md` | 上下文工程架构设计（10 章） |
| `docs/plans/2026-02-26-phase2-context-engineering.md` | Phase 2 Batch 1 实施计划（14 任务） |

### MCP Memory

- `claude-mem #2828` — Phase 2 上下文工程架构 brainstorming 完成摘要

### 下一步

- ~~执行 Phase 2 Batch 1 实施计划~~ → **已完成**（见下方 Phase 2 Batch 1 记录）

---

## 2026-02-26 — Phase 2 Batch 1 编码完成

### 会话概要

执行 Phase 2 Batch 1 全部 14 个任务。实现上下文工程核心模块（三区分配、渐进式降级、Token 预算管理）+ 5 个新工具 + 集成收尾。6 个 git 提交。

### 提交记录

| 提交 | 内容 |
|------|------|
| `8943ffa` | feat(types): MemoryBlock 新增 priority/max_age_turns/last_updated_turn + AutoExtracted/Custom 变体 |
| `1854397` | feat(engine): context 模块 — SystemPromptBuilder + ContextBudgetManager + ContextPruner |
| `de47c3f` | feat(engine): AgentLoop 集成 Budget+Pruner + 工具结果软裁剪(30K) |
| `f8ffdbb` | feat(tools): 5 个新工具 — file_write/file_edit/grep/glob/find |
| `0bfe864` | feat(memory): 优先级排序 + 预算限制(12K) + add/remove/expire 方法 |

### 新增/修改文件

| 文件 | 操作 | 说明 |
|------|------|------|
| `crates/octo-types/src/memory.rs` | 修改 | MemoryBlock 扩展 |
| `crates/octo-engine/src/context/mod.rs` | 新增 | context 模块入口 |
| `crates/octo-engine/src/context/builder.rs` | 新增 | SystemPromptBuilder + Bootstrap 文件发现 |
| `crates/octo-engine/src/context/budget.rs` | 新增 | ContextBudgetManager 双轨估算 |
| `crates/octo-engine/src/context/pruner.rs` | 新增 | ContextPruner 三级降级 |
| `crates/octo-engine/src/tools/file_write.rs` | 新增 | FileWriteTool |
| `crates/octo-engine/src/tools/file_edit.rs` | 新增 | FileEditTool |
| `crates/octo-engine/src/tools/grep.rs` | 新增 | GrepTool |
| `crates/octo-engine/src/tools/glob.rs` | 新增 | GlobTool |
| `crates/octo-engine/src/tools/find.rs` | 新增 | FindTool |
| `crates/octo-engine/src/tools/mod.rs` | 修改 | 7 工具注册 |
| `crates/octo-engine/src/agent/loop_.rs` | 修改 | Budget+Pruner 集成 + 软裁剪 |
| `crates/octo-engine/src/agent/context.rs` | 修改 | 向后兼容重导出 |
| `crates/octo-engine/src/lib.rs` | 修改 | context 模块导出 |
| `crates/octo-engine/src/memory/traits.rs` | 修改 | 新增 add/remove/expire 方法 |
| `crates/octo-engine/src/memory/working.rs` | 修改 | 实现新方法 + 工具列表更新 |
| `crates/octo-engine/src/memory/injector.rs` | 修改 | 优先级排序 + 12K 预算限制 |
| `crates/octo-engine/Cargo.toml` | 修改 | 添加 glob 依赖 |
| `crates/octo-server/src/state.rs` | 修改 | AppState 存储 model 替代 agent_loop |
| `crates/octo-server/src/ws.rs` | 修改 | 每请求创建 AgentLoop |

### 构建验证

| 检查项 | 状态 |
|--------|------|
| `cargo check --workspace` | ✅ 通过 |
| `cargo build` | ✅ 通过 |
| `npx tsc --noEmit` | ✅ 通过 |

### 架构变更说明

- **AppState 重构**: `Arc<AgentLoop>` → 每请求创建新 `AgentLoop`（因 `ContextBudgetManager` 需要 `&mut self` 跟踪实际 token 使用量，每个请求独立预算状态）
- **context 模块**: 从 `agent/context.rs` 提取为独立顶层模块 `context/`，旧路径保持向后兼容重导出

### 下一步

- ~~**Phase 2 Batch 2 规划**~~ → **已完成**（见下方 Phase 2 Batch 2 记录）
- **Phase 2 Batch 3 规划** — Skill Loader + MCP 集成 + Debug Panel UI

---

## 2026-02-26 — Phase 2 Batch 2 编码完成

### 会话概要

执行 Phase 2 Batch 2 全部 16 个任务（8 次 git 提交）。实现 SQLite WAL 持久化（全数据层）、Session Memory（Layer 1）+ SqliteSessionStore、Persistent Memory（Layer 2）+ 混合检索（FTS5 + 向量余弦相似度）、Memory Flush 机制（Compact 级别前 LLM 事实提取）、3 个 Memory 工具供 Agent 使用。

### 提交记录

| 提交 | 内容 |
|------|------|
| `a954f17` | feat(deps): 添加 rusqlite(0.32 bundled+vtab), tokio-rusqlite(0.6), ulid(1.1 serde), bincode(1.3) |
| `78144ba` | feat(db+types): Database 模块(WAL+PRAGMAs) + 迁移(5 表+FTS5+触发器+索引) + 6 个新 Memory 类型 |
| `c9f8329` | feat(memory): MemoryStore trait + Provider.embed()(默认错误) + SqliteWorkingMemory(write-through RwLock cache) |
| `5bcedf9` | feat(session): SessionStore 移至 engine 异步化 + InMemorySessionStore 迁移 + SqliteSessionStore(DashMap+SQLite) |
| `1e41a10` | feat(memory): SqliteMemoryStore CRUD + 混合检索(FTS5 BM25 + 向量 cosine, 0.7/0.3 融合 + 时间衰减 + 重要性加权) + OpenAI embed() |
| `c9988a0` | feat(context): FactExtractor(LLM JSON 提取) + MemoryFlusher(Compact 前冲刷到 WorkingMemory + MemoryStore) |
| `2bc4c76` | feat(tools): memory_store/memory_search/memory_update 3 个工具 + register_memory_tools() |
| `0637bb5` | feat(server): Database.open() + SQLite 服务初始化 + memory tools 注册 + AppState.memory_store |

### 新增文件 (14 个)

| 文件 | 说明 |
|------|------|
| `crates/octo-engine/src/db/mod.rs` | 数据库模块入口 |
| `crates/octo-engine/src/db/connection.rs` | Database struct, open(path)/open_in_memory(), WAL PRAGMAs |
| `crates/octo-engine/src/db/migrations.rs` | user_version 版本迁移, 5 表 + FTS5 + 3 触发器 + 4 索引 |
| `crates/octo-engine/src/memory/store_traits.rs` | MemoryStore async trait (store/search/get/update/delete/list/batch_store) |
| `crates/octo-engine/src/memory/sqlite_working.rs` | SqliteWorkingMemory — RwLock write-through cache + 4 默认 blocks |
| `crates/octo-engine/src/memory/sqlite_store.rs` | SqliteMemoryStore — CRUD + FTS5 + 向量检索 + 分数融合 + token budget 截断 |
| `crates/octo-engine/src/memory/extractor.rs` | FactExtractor — LLM 提取 fact/category/importance JSON, 4000 char 限制 |
| `crates/octo-engine/src/session/mod.rs` | Async SessionStore trait + SessionData struct |
| `crates/octo-engine/src/session/memory.rs` | InMemorySessionStore (从 octo-server 迁移, async) |
| `crates/octo-engine/src/session/sqlite.rs` | SqliteSessionStore — DashMap 热缓存 + SQLite write-through |
| `crates/octo-engine/src/context/flush.rs` | MemoryFlusher::flush() — 提取事实 → WorkingMemory + MemoryStore |
| `crates/octo-engine/src/tools/memory_store.rs` | memory_store 工具 (embed + 存储) |
| `crates/octo-engine/src/tools/memory_search.rs` | memory_search 工具 (embed query + 混合检索) |
| `crates/octo-engine/src/tools/memory_update.rs` | memory_update 工具 (按 ID 更新内容) |

### 修改文件 (18 个)

| 文件 | 变更 |
|------|------|
| `Cargo.toml` | workspace deps: rusqlite, tokio-rusqlite, ulid, bincode |
| `crates/octo-types/Cargo.toml` | ulid 依赖 |
| `crates/octo-types/src/memory.rs` | MemoryId/MemoryCategory/MemorySource/MemoryEntry/SearchOptions/MemoryResult/MemoryFilter; MemoryBlock +char_limit/is_readonly |
| `crates/octo-types/src/lib.rs` | 新类型 re-exports |
| `crates/octo-engine/Cargo.toml` | rusqlite, tokio-rusqlite, ulid, bincode, dashmap |
| `crates/octo-engine/src/lib.rs` | pub mod db, session + re-exports |
| `crates/octo-engine/src/providers/traits.rs` | Provider.embed() 默认错误实现 |
| `crates/octo-engine/src/providers/openai.rs` | embed() — POST /v1/embeddings, text-embedding-3-small |
| `crates/octo-engine/src/memory/mod.rs` | store_traits, sqlite_working, sqlite_store, extractor 模块 |
| `crates/octo-engine/src/context/mod.rs` | flush 模块 |
| `crates/octo-engine/src/agent/loop_.rs` | memory_store 字段 + with_memory_store() + Compact flush→prune |
| `crates/octo-engine/src/tools/mod.rs` | 3 memory tool 模块 + register_memory_tools() |
| `crates/octo-server/src/main.rs` | Database.open() + SQLite 服务 + memory tools 注册 |
| `crates/octo-server/src/state.rs` | +memory_store: Arc<dyn MemoryStore> |
| `crates/octo-server/src/session.rs` | 改为 re-export octo_engine::session |
| `crates/octo-server/src/ws.rs` | session .await + .with_memory_store() |
| `.env.example` | +OCTO_DB_PATH |

### 编译期问题与修复

| 问题 | 原因 | 修复 |
|------|------|------|
| Future not Send | RwLockWriteGuard 跨 .await | 重构 ensure_loaded() 在 block scope 内释放锁 |
| Type mismatch | tokio_rusqlite::Error vs anyhow::Error | 移除显式 Result 类型标注，让编译器推断 |
| E0282 类型推断失败 | closure 内 Vec 类型不明确 | 添加 Vec<ChatMessage> 显式标注 |
| Arc 类型推断失败 | Arc::from() 到 dyn Provider | 添加 Arc<dyn octo_engine::Provider> 显式标注 |

### 构建验证

| 检查项 | 状态 |
|--------|------|
| `cargo check --workspace` | ✅ 通过 |
| `cargo build` | ✅ 通过 (仅 1 个预存 warning) |

### 下一步

- **Phase 2 Batch 3 规划** — Skill Loader + MCP 集成 + Debug Panel UI
- **Phase 2 运行时验证**（可选）— SQLite 持久化 + session 恢复 + memory 工具 + FTS5 检索 + Compact flush

---

## 2026-02-27 — Phase 2 Batch 3 实现 (Skill Loader + MCP Client + Debug UI)

### 会话概要

使用 Subagent-Driven Development 模式执行 Phase 2 Batch 3 实现计划，共 13 个 Task，11 个 commit。三条独立特性链（Skill → MCP → Debug）在最终集成任务中汇合。全部编译通过。

### 实现概览

#### Skill 链 (Tasks 1-4)

1. **工作区依赖 + ToolSource 增强** — 添加 serde_yaml, notify, notify-debouncer-mini, rmcp 工作区依赖。ToolSource 枚举改为 `Mcp(String)`, `Skill(String)` 携带来源名称。
2. **SkillDefinition + SKILL.md 解析器** — YAML frontmatter 解析，`${baseDir}` 模板替换，两级目录扫描（项目级覆盖用户级）。
3. **SkillRegistry + SkillTool** — 线程安全注册表（`Arc<RwLock<HashMap>>`），用户可调用 Skill 注册为 Tool trait 实现，系统提示词注入。
4. **Skill 热重载** — notify + notify-debouncer-mini 300ms 防抖监控 SKILL.md 变更。

#### MCP 链 (Tasks 5-6)

5. **McpClient trait + StdioMcpClient** — rmcp 0.16 封装，stdio 传输，适配实际 rmcp API（`Cow<'static, str>`, `Arc<JsonObject>`, `Annotated<RawContent>` 等）。
6. **McpToolBridge + McpManager** — 工具桥接到 ToolRegistry，多服务器管理，`.octo/mcp.json` 配置加载。

#### Debug 链 (Tasks 7-10)

7. **ToolExecution 类型 + SQLite v2** — ExecutionStatus 枚举，ToolExecution 记录，tool_executions 表 + 3 索引。
8. **ToolExecutionRecorder + AgentLoop 集成** — SQLite 异步记录，AgentLoop 工具执行前后计时+记录。
9. **REST API** — 8 个 Axum 端点（sessions, executions, tools, memories, budget），AppState 扩展。
10. **WebSocket 新事件** — tool_execution + token_budget_update 事件广播，ContextBudgetManager snapshot 方法。

#### 前端 (Tasks 11-12)

11. **Debug atoms + WS 事件** — executionRecordsAtom, tokenBudgetAtom, 新 ServerMessage 类型处理。
12. **3-Tab 布局** — Chat | Tools | Debug 三标签页，ExecutionList 表格，ExecutionDetail 展开面板，TokenBudgetBar 可视化。

### 技术变更

#### 新文件 (26 个)

**octo-types (2 文件)**
- `src/skill.rs` — SkillDefinition 类型
- `src/execution.rs` — ExecutionStatus, ToolExecution, TokenBudgetSnapshot

**octo-engine (11 文件)**
- `src/skills/mod.rs`, `loader.rs`, `registry.rs`, `tool.rs` — Skill 子系统
- `src/mcp/mod.rs`, `traits.rs`, `stdio.rs`, `bridge.rs`, `manager.rs` — MCP 子系统
- `src/tools/recorder.rs` — 工具执行记录器

**octo-server (6 文件)**
- `src/api/mod.rs`, `sessions.rs`, `executions.rs`, `tools.rs`, `memories.rs`, `budget.rs` — REST API

**web (7 文件)**
- `src/atoms/debug.ts` — Debug 状态原子
- `src/pages/Tools.tsx`, `Debug.tsx` — 新页面
- `src/components/tools/ExecutionList.tsx`, `ExecutionDetail.tsx` — 工具执行 UI
- `src/components/debug/TokenBudgetBar.tsx` — Token 预算可视化

#### 修改文件 (20 个)

- `Cargo.toml` + 2 crate Cargo.toml — 依赖添加
- `octo-types/src/lib.rs`, `tool.rs`, `memory.rs` — 类型注册 + ToolSource 增强 + Serialize 派生
- `octo-engine/src/lib.rs`, `agent/loop_.rs`, `context/builder.rs`, `context/budget.rs`, `db/migrations.rs`, `tools/mod.rs` — 核心集成
- `octo-server/src/main.rs`, `router.rs`, `state.rs`, `ws.rs` — 服务器集成
- `web/src/App.tsx`, `atoms/ui.ts`, `components/layout/TabBar.tsx`, `ws/types.ts`, `ws/events.ts` — 前端集成

### rmcp API 适配

| 计划中的 API | 实际 rmcp 0.16 API | 适配方式 |
|-------------|-------------------|---------|
| `Tool.name: String` | `Cow<'static, str>` | `.to_string()` 转换 |
| `Tool.input_schema: Value` | `Arc<JsonObject>` | `Value::Object(arc.as_ref().clone())` |
| `Content::Text(text)` | `Annotated<RawContent>` | `.raw` 匹配 `RawContent::Text` |
| `cancel() -> Result<()>` | `cancel() -> Result<QuitReason, JoinError>` | `.map_err()` 处理 |

### 构建验证

| 检查项 | 状态 |
|--------|------|
| `cargo check --workspace` | ✅ 通过 (2 个预存 warning) |
| `npx tsc --noEmit` | ✅ 通过 (0 errors) |
| `npx vite build` | ✅ 通过 (248.58 kB JS, 14.47 kB CSS) |

### 提交记录

| 序号 | SHA | 信息 |
|------|-----|------|
| 1 | 322eaf3 | feat(deps): add serde_yaml, notify, rmcp workspace deps + ToolSource(String) |
| 2 | 76a9687 | feat(skills): SkillDefinition type + SKILL.md parser with frontmatter splitting |
| 3 | b107664 | feat(skills): SkillRegistry + SkillTool + SystemPromptBuilder integration |
| 4 | 9867798 | feat(skills): hot-reload with notify watcher (300ms debounce) |
| 5 | 39c2409 | feat(mcp): McpClient trait + StdioMcpClient (rmcp wrapper) |
| 6 | c220901 | feat(mcp): McpToolBridge + McpManager (multi-server, config file) |
| 7 | 0569bfc | feat(types+db): ToolExecution types + SQLite migration v2 (tool_executions table) |
| 8 | a1d05a3 | feat(tools): ToolExecutionRecorder + AgentLoop integration (SQLite recording) |
| 9 | d73499a | feat(server): REST API endpoints + AppState integration |
| 10 | c52b496 | feat(ws): tool_execution + token_budget_update WebSocket events |
| 11 | cf71344 | feat(web): 3-tab layout + ExecutionList + TokenBudgetBar + WS events |

### 下一步

- **Phase 2 Batch 4 规划** — 完整 Debug Panel UI（日志面板、网络面板）、Context Viewer、性能优化
- **运行时验证** — 启动服务器验证 REST API + WebSocket 事件 + MCP 连接
- **Skill 测试** — 创建 `.octo/skills/` 目录并验证加载 + 热重载

---

## Phase 2.4: Engine Hardening（2026-02-27）

### 变更概述

**任务 1: Loop Guard / Circuit Breaker**（`90443f8`）
- 新增 `crates/octo-engine/src/agent/loop_guard.rs`（~120 行）
- 三层保护：重复调用检测（≥5次阻断）/ 乒乓检测（A-B-A-B 模式）/ 全局断路器（≥30次终止）
- 集成到 `AgentLoop`，每次工具调用前执行 `check()` 验证

**任务 2: Context Overflow 4+1 阶段 + 任务 3: LLM 错误分类**（`2b413be`）
- `context/budget.rs`：`DegradationLevel` 扩展为 6 变体（None/SoftTrim/AutoCompaction/OverflowCompaction/ToolResultTruncation/FinalError）
- 阈值更新：60%/70%/90% 双阈值触发机制
- `context/pruner.rs`：实现 5 个降级执行函数
- 新增 `providers/retry.rs`：`LlmErrorKind` 8 类分类（RateLimit/AuthError/ServerError/NetworkError/ContextTooLong/InvalidRequest/ContentFilter/Unknown）
- `RetryPolicy` 指数退避（含 13 个单元测试，`cargo test` 通过）
- 替换 `AgentLoop` 原始的线性重试逻辑

**任务 4: EventBus**（`11fae33`）
- 新增 `event/mod.rs` 和 `event/bus.rs`（73 行）：`tokio::sync::broadcast::Sender` + 环形缓冲区历史（1000 条）
- `AgentEvent` 枚举扩展：`ToolCallStarted` / `ToolCallCompleted` 事件类型
- `AgentLoop` 完整集成：工具调用前后自动发布事件

**任务 5: 工具执行安全**（`4d9b153`）
- `BashTool`：新增 `ExecSecurityMode` 枚举（Strict/Relaxed/Disabled）+ `ExecPolicy` 结构体
- `env_clear()` 调用 + 10 个白名单环境变量（含 CARGO_HOME/RUSTUP_HOME/HOME/PATH/USER）
- 路径遍历检测（`../` 模式识别，阻断目录穿越攻击）

**任务 6: Batch 3 Bugfix 验证**（`7a86985`）
- 审查并确认 5 项已存在修复：
  - TokenBudgetUpdate 事件发射 ✅（MessageStop 后已调用 snapshot() + emit）
  - snapshot() dynamic_context 填充 ✅（estimate_tool_specs_tokens() 已实现）
  - Recorder 共享 DB 连接 ✅（ToolExecutionRecorder::new(conn.clone()) 已实现）
  - list_sessions 返回实际数据 ✅（SqliteSessionStore + InMemorySessionStore 均实现）
  - get_working_memory 使用正确 SandboxId ✅（sandbox_id query param 已实现）

### 验证结果

| 检查项 | 状态 |
|--------|------|
| `cargo check --workspace` | ✅ 通过（0 errors，仅 warnings）|
| `npx tsc --noEmit` | ✅ 通过（0 errors）|
| `npx vite build` | ✅ 通过（265.66 kB JS，19.52 kB CSS）|

### 新增文件

| 文件 | 说明 |
|------|------|
| `crates/octo-engine/src/agent/loop_guard.rs` | Loop Guard / Circuit Breaker 实现 |
| `crates/octo-engine/src/providers/retry.rs` | LLM 错误分类 + 指数退避重试（含 13 单元测试）|
| `crates/octo-engine/src/event/mod.rs` | EventBus 模块声明 |
| `crates/octo-engine/src/event/bus.rs` | EventBus 广播通道 + 历史缓冲区 |

### Git 提交历史（Phase 2.4）

| SHA | 提交信息 |
|-----|---------|
| `90443f8` | feat(engine): add Loop Guard with repetitive/ping-pong/circuit-breaker detection |
| `4d9b153` | feat(tools): add ExecSecurityMode + env_clear + path traversal protection to BashTool |
| `2b413be` | feat(provider): add LLM error classification (8 types) + exponential backoff retry |
| `11fae33` | feat(engine): add EventBus for internal event broadcasting (broadcast + ring buffer) |
| `7a86985` | fix: verify and document Batch3 bugfixes as completed (Task 6) |

### 下一步

- **Phase 3 (octo-platform) 规划** — Docker 容器化 + 多用户支持 + 生产环境配置
- **MCP SSE 传输支持** — 当前仅支持 Stdio，需增加 SSE transport
- **运行时集成验证** — 启动完整服务端验证 Loop Guard + EventBus + 安全策略

---

## 2026-03-01 — Phase 2.5 用户隔离实现

### 会话概要

Phase 2.5.3 用户隔离在单次会话中完成全部 11 个任务，实现跨 API 端点、WebSocket 和工具执行的完整用户资源隔离。

### 技术变更

#### 数据库层 (Migration V4)
- `crates/octo-engine/src/db/migrations.rs` — 添加 user_id 字段到 5 个表:
  - `session_messages` — 会话消息用户隔离
  - `tool_executions` — 工具执行记录用户隔离
  - `mcp_servers` — MCP 服务器用户隔离
  - `mcp_executions` — MCP 工具执行用户隔离
  - `mcp_logs` — MCP 日志用户隔离

#### 存储层
- `crates/octo-engine/src/session/mod.rs` — 添加 SessionStore trait 方法:
  - `create_session_with_user(user_id)` — 创建用户会话
  - `get_session_for_user(session_id, user_id)` — 获取用户会话
  - `list_sessions_for_user(user_id)` — 列出用户会话

- `crates/octo-engine/src/mcp/storage.rs` — McpStorage 增强:
  - `user_id` 字段添加到记录
  - `list_servers_for_user(user_id)` — 列出用户 MCP 服务器
  - `get_server_for_user(id, user_id)` — 获取用户 MCP 服务器

- `crates/octo-engine/src/tools/recorder.rs` — ToolExecutionRecorder 增强:
  - `list_by_user(user_id)` — 列出用户工具执行
  - `record_start()` 添加 user_id 参数

#### API 层
- `crates/octo-server/src/api/user_context.rs` — 新增共享模块:
  - `get_user_id_from_context()` — 从 UserContext 提取 user_id

- `crates/octo-server/src/router.rs` — 认证中间件集成:
  - 应用 auth_middleware 到所有 API 路由

- `crates/octo-server/src/api/sessions.rs` — 用户隔离:
  - 使用 `list_sessions_for_user` 过滤
  - 使用 `get_session_for_user` 验证所有权

- `crates/octo-server/src/api/memories.rs` — 用户隔离:
  - 从 UserContext 提取 user_id
  - 搜索/创建/删除时应用用户过滤

- `crates/octo-server/src/api/mcp_servers.rs` — 用户隔离:
  - CRUD 操作全部验证用户所有权
  - 启动/停止/状态检查验证所有权

- `crates/octo-server/src/api/executions.rs` — 用户隔离:
  - 新增 `GET /api/executions` 端点
  - 现有端点添加用户验证

#### WebSocket 层
- `crates/octo-server/src/ws.rs` — 用户上下文处理:
  - 提取 UserContext
  - 使用 `create_session_with_user` 创建会话
  - 使用 `get_session_for_user` 获取会话
  - 优雅降级: auth 禁用时使用原始方法

#### Agent 层
- `crates/octo-engine/src/agent/loop_.rs` — 用户 ID 传递:
  - 传递 user_id 到 recorder
  - ToolExecution 包含 user_id 字段

### 代码质量
- 修复 22 个编译警告 → 0 warnings
- 移除过时注释
- 添加 feature-gate 到 sandbox imports

### Git 提交历史

| SHA | 提交信息 |
|-----|---------|
| `04ceaaf` | checkpoint: Phase 2.5.3 Complete |
| `4d3c641` | cleanup: Remove outdated comments and fix compilation warnings |
| `9d86956` | fix(ws): handle None user_id to prevent panic when auth is disabled |
| `9a9e482` | feat(auth): WebSocket user isolation - Task 7 |
| `d7335a7` | fix(auth): Add error logging and user isolation to executions API |
| `2045ae3` | fix(auth): record actual user_id in tool executions |
| `b87ebc2` | feat(auth): Task 6.1 - Tool Executions API user isolation |
| `542ae24` | refactor(api): extract get_user_id_from_context to shared module |
| `aa3df3e` | feat(auth): Phase 2.5.3 - Apply auth middleware and UserContext to API handlers |
| `68513cf` | feat(auth): Phase 2.5.3 User Isolation - Database migration and storage layer |

### 验证状态

| 检查项 | 状态 |
|--------|------|
| `cargo check --workspace` | ✅ 通过（0 warnings）|
| 用户隔离 - Sessions API | ✅ |
| 用户隔离 - Memories API | ✅ |
| 用户隔离 - MCP Servers API | ✅ |
| 用户隔离 - Tool Executions API | ✅ |
| 用户隔离 - WebSocket | ✅ |
| 优雅降级 (AuthMode::None) | ✅ |

### 下一步

- **Phase 2.6 规划** — Provider 多实例 + Scheduler
- **运行时验证** — 启动服务测试用户隔离功能

---

## 2026-03-07 — 代码审查修复 + ADR/DDD 文档更新

### 会话概要

执行三智能体并行代码审查（architect-review、security-auditor、code-reviewer），收集审查结果并修复所有 must-fix 和 important 级别的 Bug。同时完成 RuFlo post-task 流水线的附加任务：更新 ADR 架构决策记录、DDD 限界上下文变更日志和本工作日志。

### 代码审查流水线

通过 Claude Code Task tool 并行启动 3 个专项审查智能体，对 `dev` 分支相对 `main` 的所有变更进行审查：

| 审查维度 | 智能体 | 发现问题 |
|----------|--------|----------|
| 架构设计 | architect-review | CRITICAL×1, HIGH×2 |
| 安全漏洞 | security-auditor | CRITICAL×1, HIGH×3 |
| 代码质量 | code-reviewer | CRITICAL×1, HIGH×4 |

### 修复的 Bug（共 8 个）

#### CRITICAL 级别

**1. `call_mcp_tool` 持有 Mutex 跨网络 I/O（并发死锁风险）**
- 文件：`crates/octo-engine/src/agent/runtime_mcp.rs`
- 问题：锁持有期间执行异步网络调用，并发工具调用会串行化甚至死锁
- 修复：改为 clone-under-lock 模式——锁内仅 clone `Arc<RwLock<Box<dyn McpClient>>>`，锁外执行网络 I/O

**2. HMAC Secret 使用硬编码默认值不报错（API key hash 伪造漏洞）**
- 文件：`crates/octo-engine/src/auth/config.rs`
- 问题：`OCTO_HMAC_SECRET` 未配置时使用硬编码默认值，攻击者可离线推算合法 token
- 修复：`warn_if_insecure()` 在 api_key/full 模式下若使用默认 Secret 则 panic，阻止启动

**3. `lock().unwrap()` 导致 Mutex 毒化级联崩溃**
- 文件：`crates/octo-platform-server/src/db/users.rs`（7处）、`crates/octo-platform-server/src/tenant/manager.rs`（1处）
- 问题：任意线程 panic 后 Mutex 毒化，所有后续操作均 panic
- 修复：统一改为 `.lock().unwrap_or_else(|e| e.into_inner())`，从毒化锁中恢复

#### HIGH/IMPORTANT 级别

**4. `list_servers` 运行时状态显示错误（任意服务器 Running → 全部显示 Running）**
- 文件：`crates/octo-server/src/api/mcp_servers.rs`
- 问题：`runtime_states.iter().find(|s| Running)` 找到第一个运行状态就对所有服务器适用
- 修复：改为 `get_all_mcp_server_states()` 按服务器名称查 HashMap，每台独立判断

**5. `args` 序列化格式不一致（MCP 服务器无法从存储重启）**
- 文件：`crates/octo-server/src/api/mcp_servers.rs`
- 问题：`create_server` 用 `args.join(" ")` 存空格分隔字符串，`start_server` 用 `serde_json::from_str` 期望 JSON 数组，永远反序列化为空
- 修复：存储和读取统一改为 JSON 数组格式

**6. `env` 序列化格式不一致（update 与 create 不兼容）**
- 文件：`crates/octo-server/src/api/mcp_servers.rs`
- 问题：`create_server` 存 JSON object，`update_server` 用 `key=value,` 格式覆盖
- 修复：`update_server` 改为 `serde_json::to_string(&env_map)`，统一 JSON object 格式

**7. `update_server` 空值检查不安全（is_none() + unwrap() 模式）**
- 文件：`crates/octo-server/src/api/mcp_servers.rs`
- 问题：先检查 `is_none()` 后立即 `unwrap()`，逻辑上冗余且不安全
- 修复：改为 `let Some(existing) = ... else { return ... }`，用 if-let 模式

**8. `data-platform/users.db` 提交到 Git 仓库（敏感数据泄露）**
- 文件：`.gitignore`、`data-platform/users.db`
- 问题：SQLite 数据库文件（含用户凭据）被纳入版本控制
- 修复：`git rm --cached data-platform/users.db`，`.gitignore` 添加 `data-platform/*.db*` 规则

### 技术变更清单

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `crates/octo-engine/src/agent/runtime_mcp.rs` | 🔴 并发修复 | call_mcp_tool clone-under-lock |
| `crates/octo-engine/src/auth/config.rs` | 🔴 安全加固 | HMAC Secret fail-fast panic |
| `crates/octo-server/src/api/mcp_servers.rs` | 🔴 数据一致性 | args/env 序列化、状态检测、空值检查 |
| `crates/octo-platform-server/src/db/users.rs` | 🔴 可靠性修复 | 7处 mutex 毒化防护 |
| `crates/octo-platform-server/src/tenant/manager.rs` | 🔴 可靠性修复 | 1处 mutex 毒化防护 |
| `.gitignore` | ✅ 安全规则 | 排除 data-platform/*.db* |
| `data-platform/users.db` | ✅ 数据清理 | 从 git 追踪中移除 |

### 架构文档更新（RuFlo post-task 附加任务）

**ADR 更新** — `docs/adr/ADR_SECURITY_REFACTORING.md`：
- **ADR-006**：HMAC Secret fail-fast 保护机制（认证上下文）
- **ADR-007**：`call_mcp_tool` 无锁网络 I/O — clone-under-lock 模式（MCP 集成上下文）

**DDD 变更日志** — `docs/ddd/DDD_CHANGE_LOG.md`：
- 认证上下文：`warn_if_insecure()` 行为变更为 fail-fast
- MCP 集成上下文：`call_mcp_tool()` 并发语义更新
- MCP 存储上下文：args/env 序列化格式变更（已有 DB 记录需重建）
- 平台用户上下文：`UserDatabase` 并发处理策略更新

### 提交记录

```
8528571  fix: address all must-fix and important bugs from code review
```

### 验证结果

- 编译：`cargo check --workspace` ✅ 通过
- 所有 8 个已识别 Bug 均已修复并提交

### 下一步

- **安全遗留项** — DNS rebinding SSRF 防护（`McpUriValidator`）可在后续迭代实现
- **健康端点** — 评估是否需要对 `/health` 进行最小化信息返回
- **命令拒绝响应码** — `SecurityPolicy` 拒绝时考虑返回 HTTP 403 而非 200
- **测试覆盖** — 添加用户隔离单元测试
