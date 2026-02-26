# octo-sandbox Memory Index

**Project**: octo-sandbox
**Purpose**: Track session work, decisions, and progress for cross-session continuity

---

## [Active Work]

- 18:45 | 🔴 SSE Stream poll_next 事件丢失 bugfix。openai.rs 和 anthropic.rs 的 poll_next() 中 parse_sse_events() 从 buffer 消费 SSE 数据后返回多个 StreamEvent，但只取第一个返回、剩余随 iter 出作用域被丢弃。多 SSE chunk 同一 TCP read 到达时（代理/中转服务常见），后续 TextDelta 丢失导致正式回复截断。修复: 添加 pending_events: VecDeque<Result<StreamEvent>> 字段，所有解析结果入队后逐个出队返回。影响两个 Provider。cargo check ✅。待运行时验证。
- 17:30 | OpenAI Provider + Thinking/Reasoning 全链路支持。(1) 新增 openai.rs 实现 Provider trait（SSE流解析+tool_calls+base_url normalize）; (2) LLM_PROVIDER 环境变量切换 anthropic/openai; (3) StreamEvent/AgentEvent/ServerMessage 全链路新增 ThinkingDelta; (4) Anthropic thinking_delta + OpenAI reasoning_content 统一为 ThinkingDelta; (5) MiniMax 中转降级—只有 thinking 无 text 时 thinking 作为正式回复; (6) 前端 StreamingDisplay 流式 thinking + MessageBubble 持久 thinking 折叠展示; (7) 兼容性修复: finish_reason "null" 过滤, stopped 防重复 MessageStop, text trim 去开头标点。已验证: Anthropic(MiniMax中转) ✅, OpenRouter ✅, Qwen(dashscope) ✅。[claude-mem #2823]
- 09:10 | Phase 1 运行时端到端验证通过。修复: (1) dotenv_override 防止系统环境变量覆盖 .env (401根因); (2) 错误事件传播—stream失败时发送Error+Done; (3) thinking_delta支持—中转代理MiniMax模型兼容; (4) 5xx自动重试3次指数退避; (5) ANTHROPIC_BASE_URL支持; (6) 前端端口5173→5180。E2E验证: WS→Session→AgentLoop→API 200→流式响应 ✅。[claude-mem #2821]
- 08:20 | 启用 sccache 编译缓存。实测：冷编译无sccache 37.5s vs 有sccache 45.3s（+21%）；热缓存重编译无sccache 37.5s vs 有sccache 24.5s（-35%）。修改 .cargo/config.toml 启用 rustc-wrapper + Makefile 移除 11 处 RUSTC_WRAPPER=""。[claude-mem #2820]
- 02:40 | 正式架构设计文档创建完成。整合 8 段 brainstorming 为 `docs/design/ARCHITECTURE_DESIGN.md`（2300行）。文档结构：12 章（项目概述/系统架构/Agent Engine/记忆系统/沙箱管理器/渠道多用户/调试面板/Web UI/数据模型/接口定义/技术决策/MVP路线图）+ 附录（参考路径/术语表/外部链接）。包含 10 张 Mermaid 图。汇总 19 条技术决策。7 个核心 Rust Trait + 支撑类型。完整 SQLite Schema（10张表）。CHECKPOINT_PLAN.md 已更新状态。[claude-mem #2788 #2790]
- 02:30 | 记忆模块 Brainstorming（第八段）完成。深度分析 14+ 项目（mem0/Letta/OpenViking/openclaw/zeroclaw/happyclaw/pi_agent_rust/craft-agents 等），综合设计四层记忆架构（Working/Session/Persistent/Archive）+ 混合检索（向量0.7+FTS0.3）+ LLM事实提取 + 压缩前刷写 + 上下文工程反退化。关键决策: SQLite WAL统一存储, 5类记忆分类, Agent自编辑Working Memory, 记忆Token预算15%上限, Phase 1内存记忆→Phase 2 SQLite持久化→Phase 3向量检索。文档: docs/main/CHECKPOINT_MEMORY_BRAINSTORMING.md。Brainstorming 8/8 段全部完成。
- 00:45 | Checkpoint Plan 保存完成。docs/main/CHECKPOINT_PLAN.md 创建，记录总体计划状态和恢复指令。[claude-mem #2779]
- 00:30 | 架构设计 Brainstorming 7/7 段全部完成。本次完成第四段(渠道/多用户)、第五段(调试面板)、第六段(Web UI)、第七段(MVP路线图)。关键决策: 三角色RBAC + ReadOnly/Interactive/AutoApprove权限模式, 五大调试模块全进MVP, React 19+Jotai+shadcn/ui, Phase 1精简(含WASM无认证)。下一步: 正式设计文档 + Phase 1 实施计划 + 项目脚手架。[claude-mem #2776 #2778] [memory: octo-sandbox, octo-sandbox-channel-system, octo-sandbox-auth-rbac, octo-sandbox-debug-panel, octo-sandbox-web-ui, octo-sandbox-mvp-roadmap]

---

## [Archived Phases]

(none yet)
