# Claude Code OSS vs Octo-Sandbox 代码级深度对比分析

> 基于 claude-code-opensource 反编译源码（515K行 TS/TSX）与 octo-sandbox（135K行 Rust）的代码级逐模块对比。
> 分析日期：2026-04-01
> **修订 v4**: 2026-04-01 — 新增提示词体系对比分析（第十一节）。CC 有 7 个结构化提示词段落，Octo 缺失 System/Actions/Using Tools/Code Style/Output Efficiency 共 5 个关键段落。

---

## 一、规模概览

| 维度 | Claude Code OSS | Octo-Sandbox | 差距 |
|------|----------------|-------------|------|
| **总源码行数** | ~515,000 行 (TS/TSX) | ~135,000 行 (Rust) | 3.8x |
| **核心 query loop** | 1,732 行 (`query.ts`) | 2,173 行 (`harness.rs`) | 基本持平 |
| **Tool 系统** | 50+ 工具 + 792 行接口 + 每工具 10~288 行使用手册 | 30 工具 + 111 行 trait + 每工具 1 行描述 | **工具描述差距巨大** |
| **Context 管理** | 5 层压缩管道 (3 已实现 + 3 stub) | auto_compact + pruner + budget + MemoryFlusher | **核心差距** |
| **Permission 系统** | 6,300+ 行, 7 种模式, 9 类决策原因 | ~1,695 行 security + ApprovalManager + ApprovalGate | 差距显著但 Octo 有审批流水线 |
| **Agent 系统** | coordinator + worker (简单 supervisor) | 10,613 行 (Runtime/Executor/Loop/Catalog) | **Octo 领先** |
| **提示词体系** | 7 段静态 + 13 段动态 (prompts.ts 850+ 行) | CORE_INSTRUCTIONS 64 行 + OUTPUT_GUIDELINES 3 行 | **差距显著** |

---

## 二、Agent Loop 逐项能力对比

> Octo 的 harness 已覆盖 Claude Code loop 68% 的能力，在安全维度上领先。

| # | Claude Code `query.ts` 能力 | Octo `harness.rs` 现状 | 差距 |
|---|---------------------------|----------------------|------|
| **上下文管理** | | | |
| 1 | System prompt 组装 | 有 (Zone A, SystemPromptBuilder) | 持平 |
| 2 | Working memory 注入 | 有 (Zone B + B+ + B++) | **Octo 领先** |
| 3 | Observation masking (>50% budget) | 有 (ObservationMasker) | 持平 |
| 4 | Token budget tracking + degradation | 有 (ContextBudgetManager, 5级降级) | 持平 |
| 5 | Auto compact (truncate) | 有 (pruner.apply) | 持平 |
| 6 | **LLM 摘要压缩** | 枚举已定义 (`CompactionStrategy::Summarize`)，**未实现** | **缺失** |
| 7 | **prompt_too_long 捕获 + reactive compact** | **无**，LLM 错误直接终止 loop | **缺失** |
| 8 | **context collapse (粒度级折叠)** | **无**（CC 中也是 stub） | 双方都无 |
| 9 | snip compact (用户主动裁剪) | **无**（CC 中也是 stub） | 双方都无 |
| 10 | session memory overflow 存储 | 有 (MemoryFlusher，LLM 提取事实) | 持平 |
| 11 | 压缩后 cache 标记更新 | **无** | 缺失 |
| **LLM 调用** | | | |
| 12 | Streaming API 调用 | 有 | 持平 |
| 13 | Retry with backoff | 有 (RetryPolicy, LlmErrorKind 分类) | 持平 |
| 14 | **prompt_too_long 自动压缩重试** | **无**（retry 只处理 rate limit/timeout） | **缺失** |
| 15 | Fallback model 自动切换 | 有 (ProviderChain failover) 但 loop 层没集成 | 半实现 |
| 16 | Token warning state | 有 (TokenBudgetUpdate 事件) | 持平 |
| **工具执行** | | | |
| 17 | 并行工具执行 | 有 (execute_parallel) | 持平 |
| 18 | Approval gate (权限审批) | 有 (ApprovalManager + ApprovalGate) | 持平 |
| 19 | Rate limiting | 有 (ToolRateLimiter, 60s 滑动窗口) | 持平 |
| 20 | **Streaming tool execution (进度回调)** | **无**，工具执行时无进度反馈 | **缺失** |
| 21 | Tool result 大小限制 + 截断 | 有 (tool_result_budget, 动态 soft/hard limit) | 持平 |
| 22 | Tool result 外部存储 | **无** | 缺失 |
| **错误恢复** | | | |
| 23 | Malformed tool call 检测 + 重试 | 有 (detect_malformed_tool_call, 最多 2 次重试) | 持平 |
| 24 | Text 中解析 tool call (fallback) | 有 (parse_tool_calls_from_text) | 持平 |
| 25 | max_tokens auto-continuation | 有 (ContinuationTracker) | 持平 |
| 26 | **prompt_too_long 降级重试** | **无** | **缺失** |
| **安全** | | | |
| 27 | Input 安全检查 | 有 (AIDefence + SafetyPipeline 双层) | 持平 |
| 28 | Output 安全检查 | 有 (SafetyPipeline output check + sanitize) | 持平 |
| 29 | Canary token 注入+轮换 | 有 (canary_guard.rotate 每轮轮换) | **Octo 领先** |
| 30 | Deferred action 检测 | 有 (DeferredActionDetector) | **Octo 领先** |
| **Hooks** | | | |
| 31 | Session lifecycle hooks | 有 (SessionStart/End, PreTask/PostTask) | 持平 |
| 32 | Per-turn hooks | 有 (LoopTurnStart) | 持平 |
| 33 | Context degradation hooks | 有 (ContextDegraded) | 持平 |
| 34 | PreToolUse / PostToolUse 拦截 | 有 PreToolCall/PostToolCall 但**无法修改输入** | 半实现 |
| **遥测** | | | |
| 35 | Telemetry bus 事件 | 有 (TelemetryEvent 多种) | 持平 |
| 36 | Token 用量累计 | 有 (total_input/output_tokens) | 持平 |
| 37 | 按模型分拆 + USD 成本 | **无** | 缺失 |

### 计分总结

| 状态 | 项数 | 占比 |
|------|------|------|
| **持平或 Octo 领先** | 25 | 68% |
| **半实现** | 2 | 5% |
| **缺失** | 8 | 22% |
| **双方都无** | 2 | 5% |

**差距评估: ⭐⭐⭐ (中等差距)**

---

## 三、Context 管理与压缩

### CC-OSS 开源版实际状态（关键发现）

CC 的 5 层压缩管道中，**3 层在开源版是 stub**：

| 层级 | 模块 | CC 开源版状态 | Octo 对应 |
|------|------|-------------|----------|
| L1 | `snipCompact` | **stub** (exports 空函数) | 无 |
| L2 | `microCompact` | **已实现** (531行，时间触发 + cache_edits) | ObservationMasker (部分对应) |
| L3 | `autoCompact` | **已实现** (LLM 摘要 + sessionMemoryCompact) | pruner.rs (truncate only) |
| L4 | `contextCollapse` | **stub** (只有 resetContextCollapse) | 无 |
| L5 | `reactiveCompact` | **stub** (exports 空函数) | 无 |
| 溢出 | `sessionMemoryCompact` | **已实现** (631行，持久化 session memory) | MemoryFlusher (LLM 事实提取) |

**真正需要追赶的只有 autoCompact + microCompact + sessionMemoryCompact 三个已实现的策略，加上 prompt_too_long 恢复路径。** stub 策略（reactive、collapse、snip）Octo 可以自行设计实现，且实现难度不高。

### CC autoCompact 核心机制

1. **触发条件**：`token_usage >= context_window - 20K(摘要预留) - 13K(缓冲)`
2. **优先尝试 sessionMemoryCompact**（如果有持久化记忆，不需要调 LLM）
3. **回退到 compactConversation**：
   - PreCompact hooks（注入自定义摘要指令）
   - 图片→占位符 (`stripImagesFromMessages`)
   - LLM 摘要调用（9 段结构化 prompt：Primary Request, Key Concepts, Files/Code, Errors/fixes, Problem Solving, All User Messages, Pending Tasks, Current Work, Next Step）
   - 摘要自身 prompt_too_long → 截掉最老 API-round 组重试（最多 3 次）
   - **压缩后状态重建**（最关键）：文件恢复(5个), Plan/Skill 重注入, SessionStart hooks, tool specs delta 重注入
   - 创建 boundary marker + postCompactCleanup 清理缓存

### Octo 现有基础

- `pruner.rs` (589行) truncate 策略完整，有 `CompactionStrategy::Summarize/MoveToWorkspace` **枚举骨架但未实现**
- `budget.rs` (416行) 5 级降级 + 双轨 token 估算（actual API + incremental estimation）
- `MemoryFlusher` 用 LLM 提取事实到 WorkingMemory + MemoryStore
- `ObservationMasker` (165行) 在 >50% budget 时遮蔽旧工具输出
- **核心缺失**：LLM 摘要压缩、prompt_too_long 恢复、压缩后状态重建

**差距评估: ⭐⭐⭐⭐ (核心差距，但 CC 也有 3 个 stub，实际差距比表面小)**

---

## 四、Tool 系统对比

### Octo Tool 接口实际状态 (`traits.rs`, 111行):

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput>;
    fn source(&self) -> ToolSource;
    fn risk_level(&self) -> RiskLevel { RiskLevel::LowRisk }
    fn approval(&self) -> ApprovalRequirement { Never }
    fn execution_timeout(&self) -> Duration { Duration::from_secs(30) }
    fn rate_limit(&self) -> u32 { 0 }
    fn sensitive_params(&self) -> Vec<&str> { vec![] }
    fn category(&self) -> &str { "general" }
}
```

### 精确差距

**接口能力：**

| CC Tool 能力 | Octo 现状 | 差距 |
|-------------|----------|------|
| `call()` | `execute()` | 持平 |
| `validateInput()` | **无** | 缺失 |
| `checkPermissions()` | `approval()` + ApprovalManager | 半实现 |
| `isConcurrencySafe()` | **无** | 缺失 |
| `isReadOnly()` / `isDestructive()` | **无** (可从 risk_level 推断) | 缺失 |
| `onProgress` 进度回调 | **无** | **缺失** |
| `maxResultSizeChars` | harness 层 tool_result_budget | 已有但位置不同 |
| `risk_level()` / `approval()` / `rate_limit()` / `execution_timeout()` / `sensitive_params()` / `category()` | **全部已有** | 持平 |

**工具描述质量（关键发现）：**

CC 的每个重要工具都有 **10~288 行的使用手册**（何时用、何时不用、最佳实践、反模式警告、示例），而 Octo 的工具描述是**一句话**。这三层耦合设计（系统提示词指导 + 工具描述手册 + 条件注入）直接决定了 LLM 的工具选择和使用质量。

**工具数量差距集中在 agent 协调领域**（CC 有 10 个 Task/Agent/Team 工具，Octo 只有 1 个 subagent）。但 Octo 在 Memory 工具（9 个）和搜索工具（3 个 vs 2 个）上领先。

详细设计见 `TOOL_SYSTEM_ENHANCEMENT_DESIGN.md`。

**差距评估: ⭐⭐⭐ (中等差距，接口中等 + 描述差距大 + agent 协调工具缺失)**

---

## 五、Permission 与安全系统

### CC Permission 模型 (6,300+ 行)

- 7 种权限模式 (default/plan/acceptEdits/bypass/dontAsk/auto/bubble)
- 8 层规则来源优先级
- `ToolName(pattern)` 通配符规则语法
- 9 类结构化决策原因追踪
- Bash AST 分析 (9 类危险模式)
- bashClassifier 机器学习辅助

### Octo Security (1,695 行 + ApprovalManager)

- **SecurityPolicy**: 3 级 AutonomyLevel + 命令风险评估 + 路径黑名单
- **ApprovalManager**: 3 种策略 (AlwaysApprove/SmartApprove/AlwaysAsk) × 3 级 ApprovalRequirement (Never/AutoApprovable/Always)
- **ApprovalGate**: WebSocket 异步审批（完整实现）
- **AIDefence + SafetyPipeline**: 双层输入/输出安全检查
- **缺失**: 通配符规则语法、多来源规则合并、Bash AST 分析、结构化决策追踪

**差距评估: ⭐⭐⭐ (中等差距，从 ⭐⭐⭐⭐ 下调——Octo 的 ApprovalManager + ApprovalGate 提供了完整审批流水线，只缺规则引擎层)**

---

## 六、其他模块对比

### MCP 集成 — ⭐⭐ (基本持平)

| 维度 | Claude Code | Octo |
|------|------------|------|
| 传输协议 | Stdio, SSE, HTTP, WebSocket | Stdio, SSE (Streamable HTTP) |
| OAuth 支持 | 完整 OAuth + token refresh | 无 |
| 工具发现 | 动态 + 缓存 + 搜索 | 动态发现 |
| 内容截断 | 智能截断 + 外部存储回退 | 基础截断 |

### 多智能体协调 — Octo 底层领先，上层抽象待补 ⭐（基础设施）/ ⭐⭐⭐（上层抽象）

**底层基础设施 Octo 全面领先**：多 tokio task（vs 单进程 AsyncLocalStorage）、mpsc channel（vs 文件邮箱）、DashMap SessionRegistry（vs tmux 窗格）、CollaborationManager + Byzantine 共识（CC 无）、Docker 容器隔离（vs git worktree）、AgentRouter 自动路由（CC 无）、DualAgent Plan/Build 切换（CC 更简单）。

**上层抽象 Octo 缺两个**：团队管理（TeamManager）和结构化任务跟踪（TaskTracker）。但这两个都是在已有 SessionRegistry 上加薄包装，总计 ~350 行。CC 的 Team/Task/SendMessage 体系在其受限的 Node.js 环境中是合理设计，但 Octo 不应照搬——应基于自身更强的基础设施设计更简洁的上层抽象。

详细设计见 `MULTI_AGENT_ORCHESTRATION_DESIGN.md`。

### 会话管理 — ⭐⭐⭐ (差距明显)

Octo 缺少: teleport、fork、rewind、backgrounding、抄本

### 成本追踪 — ⭐⭐⭐ (差距显著)

Octo 缺少: 按模型分拆、实时 USD 成本、cache token 统计、会话成本恢复

### Hook 系统 — ⭐⭐ (中等偏小差距，从 ⭐⭐⭐ 再次下调)

> **修正**：深入读代码后发现 Octo 的 hooks 系统远比预期完善——不是"EventBus 通知型"，而是完整的 3 层分层注册系统 (Builtin SecurityPolicy/AuditLog → PolicyEngine policies.yaml → Declarative hooks.yaml + WASM)，支持 17 种 Hook 事件、5 种 HookAction (Continue/Modify/Abort/Block/Redirect)、FailOpen/FailClosed 失败模式、优先级排序、WASM 插件扩展、OCTO_* 环境变量导出。

CC 有 28 种事件、5 种执行类型。Octo 真正缺的是 **3 个输出能力**（ModifyInput/InjectContext/PermissionDecision）和 **1 个事件**（UserPromptSubmit），以及 `if` 条件过滤。这些都是小改动（~170 行），Octo 在 WASM 插件和 Policy Engine 上反而领先 CC。

详细设计见 `HOOK_SYSTEM_ENHANCEMENT_DESIGN.md`。

---

## 七、Octo 已有优势

| 维度 | Octo 优势 |
|------|----------|
| **多智能体架构** | SessionRegistry + multi-session 远比 CC 文件邮箱先进 |
| **Memory 层次** | L0/L1/L2 + KnowledgeGraph + FTS + MemoryFlusher + Zone B/B+/B++ |
| **安全纵深** | AIDefence + SafetyPipeline 双层 + Canary rotation + Deferred action detection |
| **Agent Loop 健壮性** | Malformed tool call recovery, text tool call parsing, auto-continuation, LoopGuard, E-Stop |
| **审批流水线** | ApprovalManager (3 策略 × 3 级) + ApprovalGate (WebSocket 异步审批) |
| **Hook 系统** | 3 层注册 (Builtin+PolicyEngine+Declarative) + 17 事件 + WASM 插件 + policies.yaml |
| **Web 前端** | React 19 前端 + TUI 双形态 |
| **沙箱** | Docker + WASM + SandboxProfile 3 级 (dev/stg/prod) + SessionSandboxManager |
| **评估框架** | octo-eval 完整 benchmark 体系 |
| **密钥管理** | AES-GCM 加密 + keyring |
| **调度器** | Cron-based scheduler |
| **平台化** | octo-platform-server 多租户 + JWT auth |
| **Memory 提示词** | 6 个 memory 工具的详细使用指导（CC 无等价物） |
| **ReAct 推理策略** | Problem-Solving Strategy 4 步推理（CC 无等价段落） |
| **搜索策略提示词** | 精确查询+关键词重构+多源交叉（CC 无等价段落） |
| **文件处理提示词** | 二进制文件处理+python3 fallback（CC 无等价段落） |

---

## 八、提示词体系对比

### CC-OSS 提示词架构

CC 的系统提示词由 `prompts.ts` (850+ 行) 构建，分为 7 个静态段（prompt 缓存友好）+ 13 个动态段，通过 `SYSTEM_PROMPT_DYNAMIC_BOUNDARY` 分割：

| CC 静态段 | 内容 | Octo 覆盖度 |
|----------|------|-----------|
| **Intro** | Agent 身份 + CYBER_RISK 安全指令 + URL 生成禁令 | 30%（有身份，缺安全指令） |
| **System** | 输出可见性、权限模式说明、`<system-reminder>` tag 语义、prompt injection 防御、上下文压缩说明、hook 说明 | **0%** |
| **Doing Tasks** | 软件工程任务指导 + YAGNI 代码风格（不加多余功能/错误处理/抽象）+ 安全漏洞防范 + 验证要求 | 10%（有泛泛 Guidelines，缺 YAGNI 具体规则） |
| **Actions** | 可逆性分析、爆炸半径评估、确认协议、破坏性操作清单（delete/force-push/reset --hard 等） | **0%** |
| **Using Tools** | 专用工具优先于 Bash（file_read 替代 cat、file_edit 替代 sed）+ 并行调用指导 | **0%** |
| **Tone & Style** | Emoji 限制、代码引用 `file:line` 格式、GitHub `owner/repo#123` 格式 | 10%（有 3 行 Output Format） |
| **Output Efficiency** | 直奔主题、简洁原则、信息层次（决策/进展/错误） | **0%** |

### Octo 独有的提示词优势

CC **没有**等价段落的 Octo 提示词：
- **Memory Management** (27 行) — 6 个 memory 工具的详细使用指导，包括自动提取行为说明
- **Problem-Solving Strategy** (5 行) — ReAct 模式的 4 步推理策略
- **Search Strategy** (5 行) — 精确查询、关键词重构、web_fetch、多源交叉
- **File Handling** (5 行) — 二进制文件处理、python3 fallback

### 差距评估: ⭐⭐⭐⭐ (显著差距)

Octo 的 `CORE_INSTRUCTIONS` 只有 64 行通用指导，CC 有 850+ 行精细分段的行为控制。CC 的提示词经过大量 A/B 测试优化（代码中可见 `@[MODEL LAUNCH]` 和 feature flag 注释），每条规则都有明确的行为纠正目标。

**最关键的 5 个缺失段落**：

1. **System** — 让 LLM 知道要防御 prompt injection、理解权限系统、知道上下文会被压缩
2. **Actions** — 让 LLM 评估操作风险、主动确认危险操作（而非只靠代码层 SecurityPolicy 拦截）
3. **Using Tools** — 让 LLM 使用专用工具而非万事用 bash
4. **Code Style (YAGNI)** — 让 LLM 不做过度设计、不加多余代码
5. **Output Efficiency** — 让 LLM 直奔主题、不废话

详细设计见 `PROMPT_SYSTEM_ENHANCEMENT_DESIGN.md`。

---

## 九、改进方案总览（最终版）

### 设计文档索引（9 份）

| # | 文档 | 覆盖维度 | 代码量 |
|---|------|---------|--------|
| 1 | `CONTEXT_MANAGEMENT_ENHANCEMENT_DESIGN.md` | 上下文压缩管道 + Streaming tool execution | ~1000 行 |
| 2 | `PERMISSION_AND_P1_ENHANCEMENT_DESIGN.md` | 6 层权限引擎 + Context Collapse + Snip + Tool 接口 | ~920 行 |
| 3 | `PROMPT_SYSTEM_ENHANCEMENT_DESIGN.md` | 5 个新提示词段落 + Git status 注入 | ~100 行 |
| 4 | `HOOK_SYSTEM_ENHANCEMENT_DESIGN.md` | Hook 输出能力 (ModifyInput/InjectContext/Permission) | ~170 行 |
| 5 | `TOOL_SYSTEM_ENHANCEMENT_DESIGN.md` | 工具三层耦合 + 7 新工具 + 30 工具描述升级 | ~1690 行 |
| 6 | `MULTI_AGENT_ORCHESTRATION_DESIGN.md` | TeamManager + TaskTracker + 10 LLM 工具 | ~1320 行 |
| 7 | `TUI_EXPERIENCE_ENHANCEMENT_DESIGN.md` | 20 项 TUI 打磨（Unicode/Shimmer/快捷键/Vim） | ~1050 行 |
| 8 | `AUTONOMOUS_MODE_DESIGN.md` | 自主运行模式（Tick 循环 + 企业触发器 + 预算控制） | ~490 行 |
| 9 | 本文档 | 对比分析总览 + 所有维度汇总 + 路线图 | — |

### 完整改进清单（按优先级 + 维度）

#### P0 — 核心能力（直接影响 agent 工作能力）

| 编号 | 改进项 | 维度 | 设计状态 | 代码量 |
|------|--------|------|---------|--------|
| P0-1 | prompt_too_long 恢复 | 上下文 | 已设计 | ~60 行 |
| P0-2 | LLM 摘要压缩 (CompactionPipeline) | 上下文 | 已设计 | ~400 行 |
| P0-3 | 压缩后状态重建 | 上下文 | 已设计 | ~150 行 |
| P0-4 | Reactive compact | 上下文 | 已设计 | ~80 行 |
| P0-5 | 微压缩增强 (ObservationMasker) | 上下文 | 已设计 | ~100 行 |
| P0-6 | Streaming tool execution | 工具 | 已设计 | ~200 行 |
| P0-7 | 提示词 Phase 1 (System/Actions/Using Tools) | 提示词 | 已设计 | ~45 行 |
| | **P0 小计** | | | **~1035 行** |

#### P1 — 安全与协作（决定 agent 自治能力和多 agent 协作）

| 编号 | 改进项 | 维度 | 设计状态 | 代码量 |
|------|--------|------|---------|--------|
| P1-1 | PermissionEngine 6 层规则引擎 | 权限 | 已设计 | ~515 行 |
| P1-2 | Context Collapse 粒度折叠 | 上下文 | 已设计 | ~220 行 |
| P1-3 | Snip Compact 用户裁剪 | 上下文 | 已设计 | ~55 行 |
| P1-4 | Tool 接口增强 (validate/readonly/destructive) | 工具 | 已设计 | ~130 行 |
| P1-5 | 提示词 Phase 2 (Code Style/Efficiency/Format) | 提示词 | 已设计 | ~55 行 |
| P1-6 | 多 Agent 工具 (session_create/message/status/stop) | 编排 | 已设计 | ~520 行 |
| P1-7 | TaskTracker + task_create/update/list | 编排 | 已设计 | ~290 行 |
| P1-8 | TeamManager + team_create/add_member/dissolve | 编排 | 已设计 | ~320 行 |
| P1-9 | 现有 30 工具描述升级为使用手册 | 工具 | 已设计 | ~500 行 |
| P1-10 | 条件注入的 Using Tools 提示词段 | 工具 | 已设计 | ~50 行 |
| | **P1 小计** | | | **~2655 行** |

#### P2 — 增量增强

| 编号 | 改进项 | 维度 | 设计状态 | 代码量 |
|------|--------|------|---------|--------|
| P2-1 | 成本追踪 (CostTracker) | 可观测 | 待设计 | ~150 行 |
| P2-2 | Hook 输出能力增强 | Hook | 已设计 | ~80 行 |
| P2-3 | UserPromptSubmit hook + if 条件 + async | Hook | 已设计 | ~90 行 |
| P2-4 | Bash 命令分析增强 | 权限 | 待设计 | ~150 行 |
| P2-5 | Plan Mode 工具 (enter/exit) | 工具 | 已设计 | ~150 行 |
| P2-6 | tool_search + ask_user 工具 | 工具 | 已设计 | ~180 行 |
| P2-7 | 自主运行模式 Phase 1 (Manual + Sleep + Tick) | 自主 | 已设计 | ~250 行 |
| P2-8 | Session Memory 持续提取 | 记忆 | 待设计 | ~200 行 |
| P2-9 | sessionTranscript 会话抄本 | 可观测 | 待设计 | ~150 行 |
| P2-10 | toolUseSummary 工具摘要 | 上下文 | 待设计 | ~100 行 |
| | **P2 小计** | | | **~1500 行** |

#### P3 — 锦上添花

| 编号 | 改进项 | 维度 | 设计状态 | 代码量 |
|------|--------|------|---------|--------|
| P3-1 | 会话高级功能 (fork/rewind) | 会话 | 待设计 | ~300 行 |
| P3-2 | MCP OAuth | MCP | 待设计 | ~200 行 |
| P3-3 | 自主模式 Phase 2 (Webhook/Cron 触发 + 暂停恢复) | 自主 | 已设计 | ~150 行 |
| P3-4 | 自主模式 Phase 3 (用户感知 + 审计) | 自主 | 已设计 | ~90 行 |
| P3-5 | 内置开发命令 (commit/diff/security-review/autofix-pr) | 工具 | 待设计 | ~300 行 |
| P3-6 | doctor 自诊断 | 运维 | 待设计 | ~100 行 |
| P3-7 | notifier 通知 (Slack/桌面) | 体验 | 待设计 | ~100 行 |
| | **P3 小计** | | | **~1240 行** |

#### TUI — 独立体验打磨（可并行）

| 编号 | 改进项 | 阶段 | 代码量 |
|------|--------|------|--------|
| TUI-Ph1 | Unicode 符号集 + Stalled animation + Spinner verbs + ⎿指示符 + Effort + Shimmer + 快捷键提示 | 立即 | ~290 行 |
| TUI-Ph2 | Glimmer 效果 + Ctrl+R 历史搜索 + 模式循环 + 外部编辑器 + 权限 UI | 中期 | ~380 行 |
| TUI-Ph3 | Vim 模式 + 模型选择器 + 多 Session Spinner Tree | 后期 | ~380 行 |
| | **TUI 小计** | | **~1050 行** |

### 总工作量

| 优先级 | 代码量 | 占比 |
|--------|--------|------|
| **P0** | ~1035 行 | 14% |
| **P1** | ~2655 行 | 35% |
| **P2** | ~1500 行 | 20% |
| **P3** | ~1240 行 | 17% |
| **TUI** | ~1050 行 | 14% |
| **总计** | **~7480 行** | 100% |

### 实施路线图

```
Phase A: P0 核心能力 (~1035行)
  ├─ G1: PTL恢复 + Reactive compact
  ├─ G2: LLM 摘要 + 状态重建
  ├─ G3: 微压缩增强
  ├─ G4: Streaming 进度流
  └─ G5: 提示词 Phase 1
  预期: Agent Loop 覆盖 68% → 92%

Phase B: P1 安全与协作 (~2655行)       TUI Phase 1 (~290行)
  ├─ G1: PermissionEngine 6 层           ├─ Unicode 符号集
  ├─ G2: Collapse + Snip                ├─ Stalled animation
  ├─ G3: Tool 接口增强                  ├─ Spinner verbs
  ├─ G4: 多 Agent 工具 (4个)            └─ 快捷键提示
  ├─ G5: Task/Team 管理 (6个)
  ├─ G6: 工具描述升级 (30个)
  └─ G7: 提示词 Phase 2
  预期: 覆盖 92% → ~97%

Phase C: P2 增量增强 (~1500行)          TUI Phase 2 (~380行)
  ├─ 成本追踪                           ├─ Glimmer 效果
  ├─ Hook 增强                          ├─ Ctrl+R 历史搜索
  ├─ 自主模式 Phase 1                    └─ 权限 UI 增强
  ├─ Session Memory 持续提取
  └─ 会话抄本 + 工具摘要

Phase D: P3 锦上添花 (~1240行)          TUI Phase 3 (~380行)
  ├─ 会话 fork/rewind                   ├─ Vim 模式
  ├─ 自主模式 Phase 2+3                 └─ 模型选择器
  ├─ 内置开发命令
  └─ MCP OAuth
```

---

## 十、关键设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 改进范围 | octo-engine 核心库 | 两个产品 (workbench/platform) 同时受益 |
| P0 实施路径 | 先 Loop 框架后接入压缩层 | PTL 恢复是用户可感知 bug，先修复 |
| 摘要压缩方案 | CompactionPipeline 独立模块 | 保持可测试性，承担完整状态管理 |
| 摘要模型 | 支持配置独立的 compact_model | 允许用便宜模型做摘要 |
| 权限规则语法 | CC 风格 `ToolName(pattern)` | 简洁、经过验证、用户迁移成本低 |
| 权限层级 | 6 层 (Platform/Tenant/Project/User/Session/ToolDefault) | 对齐企业组织结构 |
| 规则合并语义 | deny 向下穿透，allow 不覆盖上层 deny | 企业 RBAC deny-override 模型 |
| PermissionEngine 位置 | 替代 SecurityPolicy，保留 ApprovalManager 作下游 | 最小改动 |
| 多 Agent 方案 | 不照搬 CC (文件邮箱)，在 SessionRegistry 上加薄抽象 | Octo 基础设施更优 |
| 工具-提示词耦合 | 三层设计：系统提示词指导 + 工具描述手册 + 条件注入 | CC 验证过的最佳实践 |
| 自主模式 | 基于 Tick 循环，多触发器 (Manual/Cron/Webhook/MQ) | CC KAIROS 的企业增强版 |
| Hook 增强 | 扩展 HookAction 而非重建，保留 WASM + PolicyEngine | Octo Hook 系统已完善 |
| TUI 打磨 | 全量引进 CC Unicode 符号，极致打磨 Spinner/动画/快捷键 | CLI 产品体验对标 CC |

---

## 十一、核心结论（最终版）

### Octo 的优势（保持和强化）

| 维度 | Octo 优势 | CC 无等价物 |
|------|----------|-----------|
| 多智能体基础设施 | tokio task + mpsc channel + DashMap SessionRegistry + Byzantine 共识 | 单进程 AsyncLocalStorage + 文件邮箱 |
| Memory 体系 | L0/L1/L2 + KnowledgeGraph + FTS + 9 个 Memory 工具 + Zone B/B+/B++ | 只有 auto-memory 目录 |
| 安全纵深 | AIDefence + SafetyPipeline + Canary rotation + Deferred detection | 无 canary/deferred |
| 审批流水线 | ApprovalManager (3 策略 × 3 级) + ApprovalGate (WS 异步审批) | 无等价物 |
| Hook 系统 | 3 层注册 + 17 事件 + WASM 插件 + PolicyEngine | 无 WASM/PolicyEngine |
| 沙箱隔离 | Docker + WASM + SandboxProfile 3 级 + SessionSandboxManager | 依赖 @anthropic-ai/sandbox-runtime |
| 平台化 | 多租户 + JWT auth + Agent Catalog + Router | 纯单用户 CLI |
| 评估框架 | octo-eval 完整 benchmark | 无 |
| TUI | 12 主题 + 呼吸动画 + git 状态集成 | 6 主题、无呼吸动画 |
| 提示词 | Memory Management + ReAct + Search + File Handling 指导 | CC 无等价段落 |

### Octo 需要追赶的（按影响力排序）

| 排名 | 差距 | 当前覆盖 | 改进后 | 设计状态 |
|------|------|---------|--------|---------|
| 1 | **上下文压缩管道** | truncate only | 5 层渐进式 | 已设计 (P0) |
| 2 | **工具-提示词三层耦合** | 1 行描述 | 10~100 行使用手册 + 条件注入 | 已设计 (P1) |
| 3 | **提示词行为控制** | 64 行通用 | 150+ 行精细分段 | 已设计 (P0/P1) |
| 4 | **6 层权限规则引擎** | 3 级粗粒度 | 通配符规则 + 多来源 + 决策追踪 | 已设计 (P1) |
| 5 | **多 Agent 上层抽象** | 底层 SessionRegistry | Team + Task + 10 LLM 工具 | 已设计 (P1) |
| 6 | **自主运行模式** | 无 | Tick 循环 + 企业触发器 + 预算控制 | 已设计 (P2) |
| 7 | **TUI 极致打磨** | 生产就绪 | CC 同级精致度 | 已设计 (TUI) |
| 8 | **Hook 输出能力** | 通知型 | 拦截型 (ModifyInput/InjectContext) | 已设计 (P2) |
| 9 | **持续 Session Memory** | session-end 提取 | 每轮持续提取 | 待设计 (P2) |
| 10 | **成本追踪** | 基础 token | 按模型 + USD + cache token | 待设计 (P2) |

### 一句话总结

> **Octo 在架构层面全面领先 CC-OSS（多 Agent、Memory、安全、沙箱、平台化），在精细工程层面有明确差距（上下文压缩、工具描述、提示词控制、权限规则）。全部改进约 7500 行代码，分 4 个 Phase 实施，可使 Octo 在所有维度上达到或超过 CC-OSS 水平，同时保持自身独有优势。**
