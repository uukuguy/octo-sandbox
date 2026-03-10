# Octo-Sandbox Agent Harness 最佳实现方案

> 研究日期: 2026-03-09
> 数据来源: 8 个 Rust 项目源码分析 + 2 个 Baseline 项目 + Tavily/DeepWiki 行业研究
> 研究方法: RuFlo 5 智能体并行分析 + 综合比较

---

## 一、研究背景与目标

### 研究范围

分析了以下 10 个项目的 Agent Harness 实现：

**Rust 项目（8 个）**:

| 项目 | 代码量 | 测试数 | Crate 数 | 核心特点 |
|------|--------|--------|----------|---------|
| **Goose** (Block) | ~122K | ~952 | 8 | MCP-first 架构，25+ Provider，31.2k stars |
| **IronClaw** (NEAR AI) | ~155K | ~2,609 | 1 | 安全纵深防御，WASM/Docker 沙箱，SafetyLayer 四层 |
| **ZeroClaw** | ~259K | ~5,089 | 4 | 极致性能优化，硬件外设支持，Loop Detection |
| **Moltis** | ~225K | ~3,528 | 54 | 最细粒度 crate 拆分，完善 MCP，本地 LLM |
| **AutoAgents** | ~66K | 147 | 5 | 派生宏驱动，10 Provider，WASM 工具 |
| **OpenFang** | ~154K | 187 | 14 | Agent OS 定位，OFP/A2A 协议，14 段 prompt |
| **pi_agent_rust** | ~575K | 374 | 1 | 最广 Provider (10+企业级)，JS/WASM/Native 三扩展 |
| **LocalGPT** | ~61K | 61 | 10 | 安全优先 Tool 分级，跨平台 (iOS/Android)，ClaudeCLI Provider |

**Baseline 项目（2 个）**:

| 项目 | 语言 | 核心特点 |
|------|------|---------|
| **nanobot** | Python | 自建完整 Agent Loop，LiteLLM 15+ Provider，Memory Consolidation |
| **nanoclaw** | TypeScript | 委托 Claude SDK，容器隔离，Credential Proxy |

### 研究目标

为 octo-sandbox 给出**最佳 Agent Harness 实现方案**，要求：
1. Rust 实现
2. 包含现代顶级实践
3. 保持轻量、可扩展
4. 不受现有实现兼容性约束

---

## 实现状态

> 更新日期: 2026-03-09
> 实现阶段: P0-P3 全部完成 + D1/D4/D5-stage2 已补齐
> 测试状态: 865 tests passing

### 总体实现进度

| 阶段 | 名称 | 状态 | 提交 | 说明 |
|------|------|------|------|------|
| **P0** | 核心接口切换 | ✅ 已完成 | `fe60703` | AgentEvent 统一、AgentLoopConfig 扩展、run_agent_loop() 纯函数、BoxStream 返回 |
| **P1** | 模块集成 | ✅ 已完成 | `5ac4c3e` | ContinuationTracker、ObservationMasker、ToolCallInterceptor、DeferredActionDetector、TurnGate |
| **P2** | 消费者适配 | ✅ 已完成 | `73c6534` | AgentExecutor、WS handler、Scheduler 适配新接口；向后兼容层；lib.rs re-export |
| **P3** | 端到端验证与清理 | ✅ 已完成 | `eb40fd3` | 全量测试回归 865 pass、AgentEvent 序列化测试、Clippy clean |
| **D1/D4/D5** | Deferred Items | ✅ 已完成 | `bbf0af9` | final_messages 返回、SubAgent 工具、Scheduler 迁移 |

### 核心设计建议实现状态

| 设计建议 | 来源灵感 | 状态 | 实现说明 |
|---------|---------|------|---------|
| `run_agent_loop()` 纯函数式入口 | ZeroClaw/Moltis | ✅ 已实现 | `harness.rs` — 所有依赖通过 `AgentLoopConfig` 注入 |
| `BoxStream<AgentEvent>` 返回值 | Goose | ✅ 已实现 | mpsc channel + tokio_stream::ReceiverStream |
| `AgentLoopConfig` 依赖注入容器 | 设计方案 §3.2 | ✅ 已实现 | `loop_config.rs` — 完整 builder pattern |
| `NormalizedStopReason` 统一枚举 | ZeroClaw | ✅ 已实现 | `events.rs` — EndTurn/ToolCall/MaxTokens/等 |
| `ContinuationTracker` max-tokens 续写 | ZeroClaw | ✅ 已实现 | `continuation.rs` — 集成到 harness 主循环 |
| `ObservationMasker` 旧轮次遮蔽 | JetBrains Research | ✅ 已实现 | `context/observation_masker.rs` — 在 context 管理步骤中调用 |
| `ToolCallInterceptor` 工具拦截 | IronClaw | ✅ 已实现 | `tools/interceptor.rs` — 执行前检查 skill 约束 |
| `DeferredActionDetector` | ZeroClaw | ✅ 已实现 | `agent/deferred_action.rs` — 检测 "I'll do X" 模式 |
| `TurnGate` 并发 turn 防护 | AutoAgents | ✅ 已实现 | `agent/turn_gate.rs` — 集成到 AgentExecutor |
| `LoopGuard` SHA-256 重复检测 | octo-sandbox 原创 | ✅ 保持 | 已是最佳实现，原封保留 |
| 4+1 阶段上下文降级 | octo-sandbox 原创 | ✅ 保持 | 迁移到 harness 管线中 |
| `AgentLoopResult.final_messages` (D1) | 设计方案 | ✅ 已实现 | Stream 完成时通过 Completed 事件返回最终消息 |
| SubAgent 工具 (D4) | Goose/Moltis | ✅ 已实现 | `tools/subagent.rs` — spawn_subagent + query_subagent 两个内置工具 |
| Scheduler 迁移 (D5) | 设计方案 | 🟡 部分实现 | Stage 2 完成：RuntimeScheduler/Scheduler 已迁移；AgentLoop wrapper 保留用于向后兼容 |
| `ApprovalManager` 交互式审批 (D2) | IronClaw | ⏳ 待实现 | 需要前端 WS 双向通信支持 |
| `SmartRouting` 智能路由 (D3) | IronClaw | ⏳ 待实现 | 需要 query complexity 分类器 |
| Event recording/replay (D6) | 设计方案 | ⏳ 待实现 | 架构已支持（基于 BoxStream），需设计存储层 |
| Provider 装饰器链 Pipeline Builder | IronClaw | ⏳ 待实现 | 当前使用 ProviderChain failover |
| Tool Approval 三级审批 | IronClaw | ⏳ 待实现 | 类型已定义（ApprovalRequirement），流程未集成 |
| 精确 Token 计数 tiktoken-rs | Goose | ⏳ 待实现 | 当前仍使用 chars/4 估算 |

### 实现适配说明

以下设计建议在实现时做了调整：

1. **AgentLoopConfig 字段为 Option** — 设计文档建议所有字段为必填，实现中将 provider/tools/memory 等设为 `Option`，允许渐进式构建和测试
2. **messages 作为独立参数** — 设计建议将 messages 放入 config，实现中 `run_agent_loop(config, messages)` 分离传入，语义更清晰
3. **AgentLoop wrapper 保留** — 设计建议最终删除 AgentLoop，实现中标记为 `#[deprecated]` 但保留，确保向后兼容
4. **SubAgent 通过内置工具实现** — 设计建议用 SubAgentManager 组件，实现中以 `spawn_subagent` / `query_subagent` 两个注册到 ToolRegistry 的工具实现，复用现有工具执行管线
5. **事件通过 mpsc channel 发送** — 设计建议直接返回 Stream，实现中用 mpsc channel + ReceiverStream 包装，支持 backpressure

### 关键实现文件

| 文件 | 说明 |
|------|------|
| `crates/octo-engine/src/agent/harness.rs` | 核心纯函数式 agent loop |
| `crates/octo-engine/src/agent/loop_config.rs` | AgentLoopConfig 依赖注入容器 |
| `crates/octo-engine/src/agent/events.rs` | AgentEvent 统一事件定义 + AgentLoopResult |
| `crates/octo-engine/src/agent/continuation.rs` | ContinuationTracker max-tokens 续写 |
| `crates/octo-engine/src/agent/deferred_action.rs` | DeferredActionDetector |
| `crates/octo-engine/src/agent/turn_gate.rs` | TurnGate 并发防护 |
| `crates/octo-engine/src/context/observation_masker.rs` | ObservationMasker |
| `crates/octo-engine/src/tools/interceptor.rs` | ToolCallInterceptor |
| `crates/octo-engine/src/tools/subagent.rs` | SubAgent 内置工具 |

---

## 二、各维度横向对比与最佳实践提炼

### 2.1 Agent Loop

#### 横向对比

| 项目 | Loop 设计 | 流式 | 并行 Tool | 取消 | Loop Guard | 上下文恢复 |
|------|----------|------|-----------|------|-----------|-----------|
| **Goose** | Stream-based (BoxStream\<AgentEvent\>) | ✅ 完整 | ✅ | CancellationToken | ✅ RepetitionInspector | ✅ auto-compact |
| **IronClaw** | Event loop + Dispatcher | ❌ | ✅ JoinSet | ✅ | ✅ | ✅ 三级压缩 |
| **ZeroClaw** | 纯函数式 run_tool_call_loop() | ✅ | ✅ 串/并自动 | ✅ | ✅ detection.rs | ✅ fact extraction |
| **Moltis** | 纯函数式 run_agent_loop() | ✅ | 串行 | ❌ | ❌ | ✅ retry |
| **OpenFang** | 独立函数 17+ 参数 | ✅ | 串行 | ❌ | ✅ circuit breaker | ✅ 多阶段恢复 |
| **pi_agent_rust** | Agent::run_loop() | ✅ | ✅ 8 并行 | AbortSignal | ❌ | ❌ |
| **octo-sandbox** | AgentLoop::run() 909 行 | ✅ | ✅ Semaphore | AtomicBool | ✅ LoopGuard | ✅ 4+1 阶段 |

#### 最佳实践提炼

**冠军设计: ZeroClaw 纯函数式 + Goose Stream 输出 + octo-sandbox LoopGuard**

1. **纯函数式入口**（ZeroClaw/Moltis）：`run_agent_loop()` 作为自由函数，所有依赖通过参数/config struct 传入，易测试、无隐式状态
2. **Stream 输出**（Goose）：返回 `BoxStream<AgentEvent>` 而非 `Result<()>`，调用方获得实时事件流
3. **LoopGuard**（octo-sandbox）：SHA-256 哈希检测重复调用、ping-pong 模式检测、backoff schedule — 已是最佳实现
4. **并行 Tool 执行**（pi_agent_rust）：`MAX_CONCURRENT_TOOLS = 8`，Semaphore 控制并发
5. **结构化取消**（Goose）：CancellationToken 支持父子传播
6. **max-tokens 续写**（ZeroClaw）：检测 `MaxTokens` 停止原因后自动续写（最多 3 次）
7. **Deferred Action 检测**（ZeroClaw）：检测 "I'll do X" 模式，防止 Agent 承诺但不执行
8. **force_text_at**（IronClaw）：最后一次迭代强制 text-only 响应，确保优雅终止

---

### 2.2 Tool 系统

#### 横向对比

| 项目 | Trait 设计 | 注册方式 | 安全分级 | 输出截断 | Approval |
|------|-----------|---------|----------|---------|---------|
| **Goose** | 无自建（MCP-only） | ExtensionManager | PermissionInspector | large_response_handler | SmartApprove |
| **IronClaw** | Tool trait + requires_approval() | ToolRegistry 静态 | ✅ SafetyLayer 四层 | ✅ sanitize | 三级审批 |
| **ZeroClaw** | Tool trait 极简 | Vec 参数传入 | credential scrub | sanitize_tool_result | ✅ |
| **LocalGPT** | Tool trait + safe/dangerous split | extend_tools() | ✅ 架构级分离 | ❌ | ❌ |
| **AutoAgents** | ToolT + ToolRuntime 分离 | #[tool] 派生宏 | ❌ | ❌ | ❌ |
| **pi_agent_rust** | Tool trait + is_read_only() | 内置列表 | ✅ | ✅ 2000行/50KB | on_update 回调 |
| **octo-sandbox** | Tool trait + source() | ToolRegistry HashMap | ToolContext | ✅ 30K字符 | ❌ |

#### 最佳实践提炼

**冠军设计: IronClaw 安全标记 + pi_agent_rust 输出截断 + AutoAgents 派生宏**

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;  // JSON Schema
    fn source(&self) -> ToolSource;         // BuiltIn / Mcp / Skill / Extension
    fn risk_level(&self) -> RiskLevel;      // ReadOnly / LowRisk / HighRisk / Destructive
    fn approval(&self) -> ApprovalRequirement; // Never / AutoApprovable / Always

    async fn execute(
        &self,
        args: Value,
        ctx: &ToolContext,
        progress: Option<&dyn ToolProgress>,  // 实时进度回调
    ) -> Result<ToolOutput>;
}

pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    pub artifacts: Vec<Artifact>,      // 结构化附件（文件、图片等）
    pub metadata: Option<Value>,       // 执行元数据
}
```

关键改进：
1. **risk_level()** — 取代简单的 `is_read_only()`，四级风险分类
2. **approval()** — 三级审批需求声明（Never/AutoApprovable/Always）
3. **ToolProgress trait** — 执行过程中流式报告进度
4. **ToolOutput 结构化** — 不止 String，支持 artifacts 和 metadata
5. **输出截断** — 在 harness 层统一处理（pi_agent_rust 的 2000行/50KB 上限）
6. **参数类型转换** — 借鉴 nanobot 的 `cast_params()`，自动处理 LLM 返回类型不匹配
7. **错误引导** — 借鉴 nanobot，错误后追加提示引导 LLM 换策略

---

### 2.3 Provider 抽象

#### 横向对比

| 项目 | Provider 数 | Chain/Failover | Token 计数 | 成本追踪 | 流式 |
|------|-----------|---------------|-----------|---------|------|
| **Goose** | 25+ | ❌ | ✅ token_counter | ✅ ProviderUsage | ✅ |
| **IronClaw** | 6 | ✅ 装饰器链 6 层 | 估算 | ✅ CostGuard 预算 | ❌ |
| **ZeroClaw** | 12 | ✅ ReliableProvider | ❌ | ✅ Cost Enforcement | ✅ |
| **Moltis** | 9 | ✅ provider_chain | ❌ | ✅ Usage | ✅ |
| **pi_agent_rust** | 10+企业 | ❌ | ❌ | ❌ | ✅ |
| **octo-sandbox** | 2 (Anthropic/OpenAI) | ✅ ProviderChain | chars/4 估算 | ✅ Metering | ✅ |

#### 最佳实践提炼

**冠军设计: IronClaw 装饰器链 + Goose Provider 广度 + ZeroClaw NormalizedStopReason**

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn model_info(&self) -> &ModelInfo;  // context_window, pricing, capabilities

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream(&self, request: CompletionRequest) -> Result<BoxStream<StreamEvent>>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

// IronClaw 装饰器链模式
// Raw -> Retry -> SmartRouting -> Failover -> CircuitBreaker -> Cache -> Recording
pub struct ProviderPipeline {
    inner: Box<dyn Provider>,
    retry: RetryPolicy,
    routing: Option<SmartRouter>,  // 简单查询走廉价模型
    failover: Vec<Box<dyn Provider>>,
    circuit_breaker: CircuitBreaker,
    cache: Option<ResponseCache>,
    recorder: Option<UsageRecorder>,
}

// ZeroClaw 统一停止原因
pub enum StopReason {
    EndTurn,
    ToolCall,
    MaxTokens,           // 可自动续写
    ContextWindowExceeded, // 需要压缩
    SafetyBlocked,
    Cancelled,
}
```

关键改进：
1. **装饰器链**（IronClaw）— 将横切关注点（重试、路由、failover、熔断、缓存、记录）分层组合
2. **SmartRouting**（IronClaw）— 简单查询自动路由到廉价模型，降低成本
3. **NormalizedStopReason**（ZeroClaw）— 统一各 Provider 的停止原因枚举，消除字符串匹配
4. **CostGuard**（IronClaw）— 每日预算和小时费率限制，防止成本失控
5. **ModelInfo**（Goose）— 每个模型声明 context_window、pricing、capabilities

---

### 2.4 上下文管理

#### 横向对比

| 项目 | System Prompt | Token 估算 | 压缩策略 | Memory Flush | Observation Masking |
|------|--------------|-----------|---------|-------------|-------------------|
| **Goose** | PromptManager 多段 | ✅ token_counter | 80% 阈值 auto-compact | ❌ | ❌ |
| **IronClaw** | Identity files | word×1.3 | 三级（Move/Summarize/Truncate） | ❌ | ❌ |
| **ZeroClaw** | build_context() | ❌ | auto_compact_history() | ✅ extract_facts | ❌ |
| **OpenFang** | 14 段有序组装 | ❌ | 多阶段恢复管线 | ✅ memory flush | ❌ |
| **octo-sandbox** | Zone A/B 分区 | chars/4 | 4+1 阶段降级 | ✅ MemoryFlusher | ❌ |

#### 最佳实践提炼

**冠军设计: octo-sandbox 4+1 降级 + IronClaw 三级策略命名 + JetBrains Observation Masking**

octo-sandbox 当前的 4+1 阶段降级已经是最成熟的设计，建议优化：

1. **精确 Token 计数** — 替换 chars/4，引入 tiktoken-rs 或至少区分中英文估算
2. **Observation Masking**（JetBrains Research）— 选择性遮蔽旧轮次的 tool output，保留 action/reasoning
3. **Auto-Compact with Summary** — 当前 AutoCompaction 仅做占位符替换，应生成 LLM 摘要
4. **合并 SystemPromptBuilder** — 消除 `builder.rs` 和 `system_prompt.rs` 两套并存

---

### 2.5 Memory 系统

#### 横向对比

| 项目 | 工作记忆 | 长期记忆 | 知识图谱 | 向量搜索 | Consolidation |
|------|---------|---------|---------|---------|-------------|
| **Goose** | 对话历史 | MCP Memory server | ❌ | ❌ | auto-compact |
| **IronClaw** | Session | Workspace 文件 | ❌ | pgvector | ❌ |
| **ZeroClaw** | Session | SQLite/PG/Qdrant | ❌ | ✅ 多后端 | ✅ decay |
| **Moltis** | Session | SQLite + embedding | ❌ | ✅ + reranking | ✅ |
| **LocalGPT** | Session | MEMORY.md + FTS5 | ❌ | ✅ fastembed | ✅ flush-before-truncate |
| **nanobot** | MessageBus | MEMORY.md + HISTORY.md | ❌ | ❌ | ✅ LLM 摘要 |
| **octo-sandbox** | WorkingMemory L0 | MemoryStore L2 | ✅ | ✅ (暴力搜索) | ✅ MemoryFlusher |

#### 最佳实践提炼

**冠军设计: octo-sandbox 多层架构 + Moltis reranking + nanobot Consolidation 机制**

octo-sandbox 的三层 + KnowledgeGraph 设计已经是最完整的，需要补齐：

1. **L1 SessionMemory 实际实现** — 填补 L0 和 L2 之间的空白
2. **HNSW 向量索引** — 替换暴力搜索（已有 feature flag，需完善实际集成）
3. **Reranking**（Moltis）— 向量召回后用交叉编码器重排序
4. **Memory Decay**（ZeroClaw）— 记忆时间衰减机制
5. **expire_blocks 实际调用** — 当前定义了但 AgentLoop 中未调用
6. **Memory Tools 增强** — 参考 nanobot 的 LLM 驱动 consolidation 机制

---

### 2.6 MCP 集成

#### 横向对比

| 项目 | MCP 支持 | stdio | SSE/HTTP | Tool 桥接 | Auth |
|------|---------|-------|----------|----------|------|
| **Goose** | 核心架构 | ✅ | ✅ | 无需（MCP-native） | ❌ |
| **IronClaw** | 仅 HTTP | ❌ | ✅ | ToolRegistry 注册 | ❌ |
| **ZeroClaw** | ❌ | - | - | - | - |
| **Moltis** | 完善 | ✅ | ✅ | McpToolBridge mcp__prefix | ✅ auth.rs |
| **LocalGPT** | 完善 | ✅ | ✅ | McpTool wrapper | ❌ |
| **octo-sandbox** | 完善 | ✅ | ✅ | McpToolBridge | ❌ |

#### 最佳实践提炼

**冠军设计: Moltis mcp__prefix 命名 + LocalGPT 容错连接 + Goose 动态发现**

octo-sandbox 的 MCP 集成已经完善，改进方向：

1. **OAuth 2.1 认证**（MCP 2025 规范更新）
2. **Streamable HTTP** — 替代旧的 SSE 模式
3. **Tool Annotations** — 解析 MCP 2025 的 read-only/destructive 标注
4. **容错连接**（LocalGPT）— 单个 MCP server 失败不影响其他
5. **Health Monitoring** — 自动健康检查和重连

---

### 2.7 扩展性

#### 横向对比

| 项目 | Hook 系统 | Plugin | WASM | Event Bus | Sub-Agent |
|------|----------|--------|------|----------|----------|
| **Goose** | ❌ | Extension (MCP) | ❌ | ❌ | ✅ SubAgent 容器隔离 |
| **IronClaw** | ✅ 6 HookPoint | ✅ | ✅ wasmtime | ✅ Observer | ❌ |
| **ZeroClaw** | ✅ HookRunner | ✅ plugins/ | ✅ wasmi+wasmtime | ✅ Observer | ❌ |
| **Moltis** | ✅ HookRegistry | ✅ plugins/ | ✅ wasmtime 36 | ❌ | ✅ spawn_agent |
| **pi_agent_rust** | ❌ | ✅ JS/WASM/Native | ✅ | AgentEvent | ❌ |
| **octo-sandbox** | ✅ 10 HookPoint | ✅ Extension | ✅ wasmtime | ✅ TelemetryBus | ❌ |

#### 最佳实践提炼

**冠军设计: octo-sandbox 10 HookPoint + IronClaw 5 Action + Goose SubAgent**

octo-sandbox 的 Hook 系统（10 HookPoint + 5 Action）已经很好，改进方向：

1. **合并 Extension 和 Hook 系统** — 消除两套并行的生命周期钩子
2. **SubAgent 支持**（Goose/Moltis）— 独立上下文中执行子任务
3. **Hook 持久化** — 当前 handlers 仅内存注册，重启丢失

---

### 2.8 安全

#### 横向对比

| 项目 | 审批系统 | 沙箱 | Credential | 输入检测 | 输出检测 |
|------|---------|------|-----------|---------|---------|
| **Goose** | SmartApprove + Permission | ❌ | ❌ | SecurityInspector | RepetitionInspector |
| **IronClaw** | 三级审批 | WASM + Docker | 零暴露 + canary | SafetyLayer 4层 | LeakDetector |
| **ZeroClaw** | ✅ | ❌ | scrub | Canary Guard | ❌ |
| **LocalGPT** | ❌ | Landlock/Seatbelt | ❌ | ❌ | ❌ |
| **nanoclaw** | ❌ | 容器隔离 | Credential Proxy | ❌ | ❌ |
| **octo-sandbox** | ❌ | Subprocess/WASM/Docker | SecretManager AES-GCM | AIDefence 3层 | ✅ |

#### 最佳实践提炼

**冠军设计: IronClaw SafetyLayer + nanoclaw Credential Proxy + octo-sandbox AIDefence**

1. **Tool Approval 系统** — 三级审批（Never/AutoApprovable/Always）
2. **Credential Proxy**（nanoclaw）— 容器内只有 placeholder，真实密钥通过代理注入
3. **Canary Token**（ZeroClaw）— 在 system prompt 中注入 canary，检测 prompt exfiltration

---

## 三、Octo-Sandbox 最佳 Agent Harness 实现方案

### 3.1 总体架构

```
                    ┌─────────────────────────────────────┐
                    │          AgentRuntime               │
                    │  (全局容器：Provider, Tools, MCP,   │
                    │   Memory, Security, Hooks, Skills)  │
                    └────────────┬────────────────────────┘
                                 │ spawn
                    ┌────────────▼────────────────────────┐
                    │         AgentExecutor               │
                    │  (持久化 Agent 实例，mpsc 接收消息) │
                    │  - 维护会话历史                     │
                    │  - 每条消息创建 AgentLoop            │
                    │  - SubAgent 管理                     │
                    └────────────┬────────────────────────┘
                                 │ run
                    ┌────────────▼────────────────────────┐
                    │     run_agent_loop() 纯函数         │
                    │  输入: AgentLoopConfig               │
                    │  输出: BoxStream<AgentEvent>         │
                    │                                     │
                    │  ┌─ build_context()                 │
                    │  ├─ call_provider()                 │
                    │  ├─ parse_response()                │
                    │  ├─ execute_tools()                 │
                    │  ├─ check_loop_guard()              │
                    │  ├─ manage_context()                │
                    │  └─ emit_events()                   │
                    └─────────────────────────────────────┘
```

### 3.2 Agent Loop 重构方案 ✅ 已实现

**核心改变：从 909 行 `run()` 方法拆分为纯函数式设计**

```rust
// ===== 核心入口 =====

/// 纯函数式 Agent Loop —— 所有依赖通过 config 传入，返回事件流
pub fn run_agent_loop(
    config: AgentLoopConfig,
) -> BoxStream<'static, AgentEvent> {
    // 返回 Stream，调用方可异步迭代事件
}

/// Agent Loop 配置 —— 替代 17+ 参数
pub struct AgentLoopConfig {
    // 依赖注入
    pub provider: Arc<dyn Provider>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub memory: Arc<dyn MemorySystem>,
    pub hooks: Arc<HookRegistry>,
    pub security: Arc<SecurityPolicy>,

    // 会话状态
    pub messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub working_memory: WorkingMemory,

    // 控制参数
    pub max_iterations: u32,            // 默认 30
    pub max_concurrent_tools: usize,    // 默认 8
    pub tool_timeout: Duration,         // 默认 120s
    pub cancel_token: CancellationToken,

    // 上下文管理
    pub context_budget: ContextBudgetConfig,
    pub loop_guard: LoopGuardConfig,
}

/// Agent 事件流 —— 实时推送所有事件
pub enum AgentEvent {
    // LLM 流式输出
    TextDelta(String),
    TextComplete(String),
    ThinkingDelta(String),
    ThinkingComplete(String),

    // Tool 执行
    ToolStart { id: String, name: String, args: Value },
    ToolProgress { id: String, progress: ToolProgress },
    ToolResult { id: String, result: ToolOutput },

    // 上下文管理
    ContextDegraded { level: DegradationLevel, usage_pct: f32 },
    MemoryFlushed { facts_count: usize },
    HistoryCompacted { before: usize, after: usize },

    // 安全
    ApprovalRequired { tool_name: String, args: Value, tx: oneshot::Sender<bool> },
    SecurityBlocked { reason: String },

    // 元信息
    IterationStart { round: u32 },
    IterationEnd { round: u32, stop_reason: StopReason },
    TokenUsage(TokenUsageSnapshot),
    Done(AgentResult),
    Error(AgentError),
}

/// 结构化返回结果
pub struct AgentResult {
    pub final_text: String,
    pub rounds: u32,
    pub tool_calls: u32,
    pub token_usage: TokenUsage,
    pub duration: Duration,
    pub stop_reason: StopReason,
}
```

**Loop 内部拆分为独立步骤函数：**

```rust
// 每个步骤都是独立的、可测试的函数

/// 步骤 1: 构建上下文（Zone A + Zone B）
fn build_context(config: &AgentLoopConfig) -> ContextBundle { ... }

/// 步骤 2: 检查上下文预算，必要时降级
async fn manage_context_budget(
    messages: &mut Vec<ChatMessage>,
    budget: &ContextBudgetManager,
    memory: &dyn MemorySystem,
) -> Option<DegradationLevel> { ... }

/// 步骤 3: 调用 LLM（流式）
async fn call_provider(
    provider: &dyn Provider,
    messages: &[ChatMessage],
    tools: &[ToolSpec],
    event_tx: &Sender<AgentEvent>,
) -> Result<ProviderResponse> { ... }

/// 步骤 4: 解析响应，提取 tool calls
fn parse_response(response: &ProviderResponse) -> ParsedResponse { ... }

/// 步骤 5: 执行 tools（并行，带 approval）
async fn execute_tools(
    tool_calls: Vec<ToolCall>,
    tools: &[Arc<dyn Tool>],
    ctx: &ToolContext,
    config: &ToolExecutionConfig,
    event_tx: &Sender<AgentEvent>,
) -> Vec<ToolOutput> { ... }

/// 步骤 6: LoopGuard 检查
fn check_loop_guard(
    guard: &mut LoopGuard,
    tool_calls: &[ToolCall],
    results: &[ToolOutput],
    round: u32,
) -> LoopGuardDecision { ... }

/// 步骤 7: 处理 max-tokens 续写
async fn handle_max_tokens_continuation(
    provider: &dyn Provider,
    messages: &mut Vec<ChatMessage>,
    max_retries: u32,
) -> Result<Option<String>> { ... }
```

### 3.3 Tool 系统重构方案 🟡 部分实现

```rust
/// 增强的 Tool trait
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    fn source(&self) -> ToolSource;
    fn risk_level(&self) -> RiskLevel;
    fn approval(&self) -> ApprovalRequirement;

    async fn execute(
        &self,
        args: Value,
        ctx: &ToolContext,
        progress: Option<&dyn ToolProgress>,
    ) -> Result<ToolOutput>;

    // 默认实现
    fn spec(&self) -> ToolSpec { /* 从上述方法组装 */ }
}

/// 风险等级（MCP Tool Annotations 对齐）
pub enum RiskLevel {
    ReadOnly,     // 只读操作（file_read, grep, glob）
    LowRisk,      // 低风险写操作（memory_store）
    HighRisk,     // 高风险写操作（file_write, file_edit）
    Destructive,  // 破坏性操作（bash rm, git push --force）
}

/// 审批需求
pub enum ApprovalRequirement {
    Never,           // 无需审批（read-only tools）
    AutoApprovable,  // 可自动审批（匹配安全策略时跳过）
    Always,          // 必须审批（destructive operations）
}

/// 结构化工具输出
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    pub artifacts: Vec<Artifact>,
    pub metadata: Option<Value>,
    pub truncated: bool,        // 是否被截断
    pub duration: Duration,
}

/// 工具进度报告
#[async_trait]
pub trait ToolProgress: Send + Sync {
    async fn report(&self, message: &str, percent: Option<f32>);
}

/// 工具执行配置
pub struct ToolExecutionConfig {
    pub max_concurrent: usize,           // 默认 8
    pub timeout: Duration,               // 默认 120s
    pub max_output_bytes: usize,         // 默认 50KB
    pub max_output_lines: usize,         // 默认 2000
    pub truncation_strategy: TruncationStrategy, // Head67Tail27 | HeadOnly | TailOnly
    pub auto_cast_params: bool,          // 自动类型转换
    pub error_hint: bool,                // 错误后追加引导提示
}
```

### 3.4 Provider 装饰器链方案 ⏳ 待实现

```rust
/// Provider 管线构建器（IronClaw 装饰器链模式）
pub struct ProviderPipelineBuilder {
    inner: Box<dyn Provider>,
}

impl ProviderPipelineBuilder {
    pub fn new(provider: impl Provider + 'static) -> Self { ... }

    /// 添加重试层
    pub fn with_retry(self, policy: RetryPolicy) -> Self { ... }

    /// 添加智能路由（简单查询 → 廉价模型）
    pub fn with_smart_routing(self, cheap_model: impl Provider + 'static) -> Self { ... }

    /// 添加故障转移
    pub fn with_failover(self, fallbacks: Vec<Box<dyn Provider>>) -> Self { ... }

    /// 添加熔断器
    pub fn with_circuit_breaker(self, config: CircuitBreakerConfig) -> Self { ... }

    /// 添加响应缓存
    pub fn with_cache(self, cache: impl ResponseCache + 'static) -> Self { ... }

    /// 添加成本控制
    pub fn with_cost_guard(self, budget: CostBudget) -> Self { ... }

    /// 添加使用量记录
    pub fn with_recorder(self, recorder: impl UsageRecorder + 'static) -> Self { ... }

    /// 构建最终 Provider
    pub fn build(self) -> Box<dyn Provider> { ... }
}

// 使用示例
let provider = ProviderPipelineBuilder::new(AnthropicProvider::new(config))
    .with_retry(RetryPolicy::exponential(3, Duration::from_secs(1)))
    .with_failover(vec![Box::new(OpenAIProvider::new(fallback_config))])
    .with_circuit_breaker(CircuitBreakerConfig::default())
    .with_cost_guard(CostBudget::daily(10.0))
    .with_recorder(SqliteUsageRecorder::new(db))
    .build();
```

### 3.5 上下文管理优化方案 🟡 部分实现

```rust
/// 上下文管理器（统一的 Zone A/B 构建 + 预算管理 + 降级策略）
pub struct ContextManager {
    budget: ContextBudgetManager,
    pruner: ContextPruner,
    flusher: MemoryFlusher,
    token_counter: Box<dyn TokenCounter>,  // 精确计数
}

/// Token 计数器 trait（替换 chars/4）
pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &str) -> usize;
    fn count_messages(&self, messages: &[ChatMessage]) -> usize;
}

/// Tiktoken 实现
pub struct TiktokenCounter { /* cl100k_base / o200k_base */ }

/// 估算实现（无外部依赖的 fallback）
pub struct EstimateCounter {
    // 区分中英文：英文 chars/4，中文 chars/1.5
}

/// Observation Masking（JetBrains Research）
pub struct ObservationMasker;

impl ObservationMasker {
    /// 遮蔽旧轮次的 tool output，保留 action 和 reasoning
    pub fn mask(
        messages: &mut Vec<ChatMessage>,
        keep_recent_n: usize,  // 保留最近 N 轮完整输出
    ) { ... }
}
```

### 3.6 SubAgent 支持方案 ✅ 已实现

```rust
/// SubAgent 编排（借鉴 Goose 三阶段分离）
pub struct SubAgentManager {
    runtime: Arc<AgentRuntime>,
    active_agents: DashMap<String, SubAgentHandle>,
    max_concurrent: usize,
}

impl SubAgentManager {
    /// 创建子 Agent，拥有独立上下文
    pub async fn spawn(
        &self,
        task: SubAgentTask,
        config: SubAgentConfig,
    ) -> Result<SubAgentHandle> { ... }

    /// 等待子 Agent 完成并返回结果
    pub async fn wait(&self, handle: &SubAgentHandle) -> Result<SubAgentResult> { ... }
}

pub struct SubAgentTask {
    pub description: String,
    pub context: Vec<ChatMessage>,  // 传递给子 Agent 的上下文
    pub tools: Option<Vec<String>>, // 可用工具白名单
    pub max_iterations: u32,
}

pub struct SubAgentConfig {
    pub model: Option<String>,      // 可用不同模型
    pub isolated: bool,             // 是否完全隔离上下文
    pub sandbox: Option<SandboxConfig>, // 容器隔离
}
```

### 3.7 安全增强方案 ⏳ 待实现

```rust
/// 统一安全管线（IronClaw SafetyLayer 模式）
pub struct SafetyPipeline {
    layers: Vec<Box<dyn SafetyLayer>>,
}

#[async_trait]
pub trait SafetyLayer: Send + Sync {
    async fn check_input(&self, message: &ChatMessage) -> SafetyDecision;
    async fn check_output(&self, response: &str) -> SafetyDecision;
    async fn check_tool_result(&self, tool: &str, result: &str) -> SafetyDecision;
}

pub enum SafetyDecision {
    Allow,
    Sanitize(String),  // 替换后的安全内容
    Block(String),      // 阻止原因
    Warn(String),       // 警告但放行
}

// 具体层实现
pub struct InjectionDetector;     // Prompt injection 检测
pub struct PiiScanner;            // PII 扫描
pub struct CredentialScrubber;    // 凭证清理
pub struct CanaryGuard;           // Canary token 检测 prompt exfiltration
pub struct OutputValidator;       // 输出安全验证

/// Tool Approval 管理器
pub struct ApprovalManager {
    policy: ApprovalPolicy,
    pending: DashMap<String, oneshot::Sender<bool>>,
}

pub enum ApprovalPolicy {
    AlwaysApprove,                    // 开发模式
    SmartApprove(SmartApproveRules),  // 基于规则自动审批
    AlwaysAsk,                        // 生产模式
}
```

---

## 四、实施优先级

### P0 — 核心重构（立即） ✅ 已完成

| 编号 | 改进项 | 来源灵感 | 预计工作量 |
|------|-------|---------|-----------|
| P0-1 | AgentLoop::run() 拆分为纯函数式步骤 | ZeroClaw/Moltis | 大 |
| P0-2 | AgentEvent Stream 返回值（替代 Result<()>） | Goose | 中 |
| P0-3 | AgentResult 结构化返回 | pi_agent_rust | 小 |
| P0-4 | tool_timeout_secs 实际启用 | 所有项目 | 小 |
| P0-5 | 并行 Tool 执行时调用 Hooks | IronClaw | 小 |
| P0-6 | 合并 Extension 和 Hook 系统 | - | 中 |

### P1 — 安全与可靠性（短期） 🟡 部分实现

| 编号 | 改进项 | 来源灵感 | 预计工作量 |
|------|-------|---------|-----------|
| P1-1 | Tool Approval 系统（三级审批） | IronClaw | 中 |
| P1-2 | Tool RiskLevel + Annotations | MCP 规范 / IronClaw | 小 |
| P1-3 | ToolOutput 结构化（artifacts, metadata） | pi_agent_rust | 中 |
| P1-4 | 工具输出截断统一处理（50KB/2000行） | pi_agent_rust | 小 |
| P1-5 | 参数自动类型转换 cast_params | nanobot | 小 |
| P1-6 | 错误后引导提示 | nanobot | 小 |
| P1-7 | max-tokens 自动续写 | ZeroClaw | 中 |
| P1-8 | Canary Token（prompt exfiltration 检测） | ZeroClaw | 小 |

### P2 — Provider 增强（中期） 🟡 部分实现

| 编号 | 改进项 | 来源灵感 | 预计工作量 |
|------|-------|---------|-----------|
| P2-1 | Provider 装饰器链（Pipeline Builder） | IronClaw | 大 |
| P2-2 | NormalizedStopReason 统一枚举 | ZeroClaw | 小 |
| P2-3 | SmartRouting（简单查询→廉价模型） | IronClaw | 中 |
| P2-4 | CostGuard 预算控制 | IronClaw/ZeroClaw | 中 |
| P2-5 | stream() failover 支持 | - | 中 |
| P2-6 | 精确 Token 计数（tiktoken-rs） | Goose | 中 |

### P3 — 高级功能（长期） 🟡 部分实现

| 编号 | 改进项 | 来源灵感 | 预计工作量 |
|------|-------|---------|-----------|
| P3-1 | SubAgent 支持 | Goose / Moltis | 大 |
| P3-2 | Observation Masking | JetBrains Research | 中 |
| P3-3 | Auto-Compact with LLM Summary | Goose / nanobot | 中 |
| P3-4 | Memory Decay 机制 | ZeroClaw | 小 |
| P3-5 | L1 SessionMemory 实现 | - | 中 |
| P3-6 | HNSW 向量索引完善 | Moltis | 中 |
| P3-7 | MCP OAuth 2.1 认证 | MCP 规范 | 中 |
| P3-8 | MCP Streamable HTTP | MCP 规范 | 中 |
| P3-9 | Deferred Action 检测 | ZeroClaw | 小 |
| P3-10 | ToolProgress 实时进度报告 | pi_agent_rust / MCP | 中 |

---

## 五、设计原则总结

### 从 10 个项目中提炼的核心原则

1. **Agent Loop 是带 tools 的 while 循环** — 不要过度工程化核心循环，差异化在于安全层和上下文管理
2. **纯函数式优于状态方法** — `run_agent_loop(config)` 比 `self.run()` 更易测试、更灵活
3. **Stream 输出优于 Result** — 调用方需要实时事件，不是最终结果
4. **安全是分层的** — 审批、风险分级、凭证清理、注入检测各司其职
5. **Tool 应声明安全属性** — risk_level、approval 让 harness 做出智能决策
6. **Provider 横切关注点用装饰器** — 重试、路由、failover、熔断、缓存是独立关注点
7. **上下文管理是渐进式的** — 从 soft trim 到 summary 到 truncate，逐步降级
8. **Memory 在裁剪前先 Flush** — 防止信息丢失是生产环境的硬需求
9. **Bet on Protocols, Not Frameworks** — MCP/A2A 协议层持久，框架会重建
10. **保持轻量** — octo-engine 应该是可嵌入的库，不是庞大的平台

### octo-sandbox 的独特优势（保持并强化）

- **4+1 阶段上下文降级** — 所有项目中最成熟的渐进式降级策略
- **LoopGuard** — SHA-256 哈希 + ping-pong 检测 + backoff schedule，业界顶级
- **AIDefence 三层检查** — 输入安全、输出安全、工具结果注入检测
- **Zone A/B 分区** — 参考 Claude Code 的前沿实践
- **TelemetryBus + Event Sourcing** — 完整的事件溯源模式

### 需要放弃的设计

- **Extension 系统**（已有 Hook 系统覆盖相同能力，Extension 未集成是负债）
- **双 SystemPromptBuilder**（合并为一个统一实现）
- **chars/4 token 估算**（至少区分中英文，理想用 tiktoken）

---

## 六、参考项目索引

| 项目 | 关键文件 | 最值得学习的设计 |
|------|---------|----------------|
| **Goose** | `agents/agent.rs` | Stream-based Loop, SubAgent 容器隔离, 25+ Provider |
| **IronClaw** | `agent/agent_loop.rs`, `tools/tool.rs` | SafetyLayer 四层, 装饰器 Provider 链, 三级审批 |
| **ZeroClaw** | `agent/loop_.rs`, `providers/traits.rs` | 纯函数式 Loop, NormalizedStopReason, Canary Guard, CJK 处理 |
| **Moltis** | `agents/runner.rs`, `mcp/tool_bridge.rs` | 54 crate 精细拆分, WASM 组件模型 MCP 工具, 本地 LLM |
| **OpenFang** | `agent_loop.rs`, `prompt_builder.rs` | 14 段 prompt 有序组装, 上下文溢出恢复管线, OFP/A2A 协议 |
| **pi_agent_rust** | `agent.rs`, `tools.rs` | 8 并行 Tool, JS/WASM/Native 三扩展, 企业级 Provider |
| **LocalGPT** | `agent/mod.rs`, `agent/tools/mod.rs` | safe/dangerous Tool 分离, ClaudeCLI Provider, 跨平台 |
| **AutoAgents** | `agent/executor/turn_engine.rs` | #[tool] 派生宏, ToolT/ToolRuntime 分离 |
| **nanobot** | `agent/loop.py`, `agent/tools/base.py` | cast_params, 错误引导, Memory Consolidation |
| **nanoclaw** | `container-runner.ts`, `credential-proxy.ts` | Credential Proxy, 容器隔离, IPC 文件系统 |
