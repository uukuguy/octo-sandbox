# Agent Harness 最佳实践设计方案

> 基于代码实现分析的 octo-sandbox Agent Harness 设计推荐
> 初次分析：2026-03-09 | 扩展分析（全部 8+2 项目）：2026-03-09

---

## 1. 研究范围与方法

本研究通过**直接阅读源码**（非仅看文档）对以下**全部 10 个项目**进行了深度分析。

> **重要说明**：octo-sandbox 是全新项目，没有任何生产历史包袱，本文档不考虑兼容性约束，仅关注最优设计。

### 本地 Rust 项目（8 个）
| 项目 | 定位 | 核心亮点 |
|------|------|----------|
| **ironclaw** | NEAR AI 出品，企业级个人 AI 助手 | 最完整的 Harness：18个 agent 模块，Docker 沙箱，自修复，路由，技能系统 |
| **zeroclaw** | 极简高效自主 agent 运行时 | Trait 驱动设计，WASM 沙箱，并行/串行工具执行自适应，循环检测 |
| **goose** | Block (Square) 出品，31k ⭐ | MCP 原生，流式工具通知，平台感知能力协商，子智能体支持 |
| **pi_agent_rust** | 面向 extension 的 agent 运行时 | 独特的消息队列 (Steering/FollowUp)，Extension 主机系统，hostcall 拦截 |
| **autoagents** | 通用 Rust agent 框架 | 最完善的 Hook 系统（9 拦截点 + Abort 能力），TurnEngine 统一执行引擎，Ractor 异步 |
| **localgpt** | 本地优先 / 离线 agent | TurnGate 并发控制，FailoverProvider 链，tool injection 安全模式，UniFFI 移动端 |
| **moltis** | 50+ crate 多智能体平台 | 强类型 ChatMessage，Provider 熔断器，6 层权限控制，content-addressed WASM 工具 |
| **openfang** | 开放扩展型 agent 运行时 | "Hands"能力包，taint tracking，AuditLog Merkle 链，多运行时 Skills |

### Baseline 项目（2 个）
| 项目 | 语言 | 核心亮点 |
|------|------|----------|
| **nanoclaw** | TypeScript + Docker | 容器-per-group 隔离，凭证代理（secrets never in container），GroupQueue 串并行 |
| **nanobot** | Python | 极简工具集（9 个），SubagentManager，MCP lazy connect，错误不持久化原则 |

---

## 2. 各项目 Agent Harness 实现对比

### 2.1 ironclaw：最完整的 Harness 实现

#### AgentDeps 模式（核心亮点）
ironclaw 将所有运行时依赖打包到一个结构体，避免函数参数爆炸：

```rust
pub struct AgentDeps {
    pub store: Option<Arc<dyn Database>>,
    pub llm: Arc<dyn LlmProvider>,
    pub cheap_llm: Option<Arc<dyn LlmProvider>>,  // 低成本 LLM 用于心跳等轻量任务
    pub safety: Arc<SafetyLayer>,                  // 安全层：清洗→验证→策略→泄漏检测
    pub tools: Arc<ToolRegistry>,
    pub workspace: Option<Arc<Workspace>>,
    pub extension_manager: Option<Arc<ExtensionManager>>,
    pub skill_registry: Option<Arc<std::sync::RwLock<SkillRegistry>>>,
    pub skill_catalog: Option<Arc<SkillCatalog>>,
    pub skills_config: SkillsConfig,
    pub hooks: Arc<HookRegistry>,
    pub cost_guard: Arc<CostGuard>,
    pub sse_tx: Option<broadcast::Sender<SseEvent>>,
    pub http_interceptor: Option<Arc<dyn HttpInterceptor>>,
    pub transcription: Option<Arc<TranscriptionMiddleware>>,
    pub document_extraction: Option<Arc<DocumentExtractionMiddleware>>,
}
```

#### 主循环设计（agent_loop.rs，1004 行）
```
run() 启动 4 个后台任务：
  ├─ self-repair (repair_interval 周期)
  ├─ session pruning (每 10 分钟)
  ├─ heartbeat (可配置间隔)
  └─ routine engine cron ticker

主循环：tokio::select! { biased; ctrl_c, message_stream.next() }

handle_message() 流程：
  转录中间件 → 文档提取 → 存储文档 → 解析提交(23 种变体)
  → BeforeInbound hook → 填充线程 → 解析会话 → Auth 模式检查
  → 按 Submission 变体分发
```

#### 23 种 Submission 变体（typed dispatch）
```rust
enum Submission {
    UserInput, SystemCommand, Undo, Redo, Interrupt, Compact, Clear,
    NewThread, Heartbeat, Summarize, Suggest, JobStatus, JobCancel,
    Quit, SwitchThread, Resume, ExecApproval, ApprovalResponse, ...
}
```

#### 6 个 Hook 拦截点
```
BeforeInbound    → 修改/拒绝入站用户消息
BeforeToolCall   → 工具调用前拦截
BeforeOutbound   → 出站响应前修改
OnSessionStart   → 会话开始时初始化
OnSessionEnd     → 会话结束清理
TransformResponse → 响应格式转换
```

#### 上下文压缩策略（三级）
```
80-85% 用量 → MoveToWorkspace（写入日志，保留 10 轮）
85-95% 用量 → Summarize（LLM 摘要 + 写入日志）
>95% 用量  → Truncate（快速删除最旧轮次）
Token 估算：word_count × 1.3 + 4/message，默认 100k token 上限
```

#### 调度器设计
```rust
// Arc<RwLock<HashMap>> 管理两类任务
jobs: HashMap<JobId, (Worker, mpsc::Sender<WorkerMessage>)>  // 完整 LLM 任务
subtasks: HashMap<SubtaskId, JoinHandle>                     // 轻量工具执行/后台任务
```

#### 自修复系统
- 检测 `JobState::Stuck` 的任务
- 尝试 `ctx.attempt_recovery()`（转回 InProgress）
- 检测失败工具（阈值 5 次失败）
- 通过 `SoftwareBuilder` 重建工具
- 结果：`Success / Retry / Failed / ManualRequired`

### 2.2 zeroclaw：极简 Trait 驱动设计

#### 核心 Agent 结构（约 52KB）
```rust
pub struct Agent {
    provider: Box<dyn Provider>,           // 模型提供者
    tools: Vec<Box<dyn Tool>>,             // 工具列表（不是注册表）
    tool_specs: Vec<ToolSpec>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,           // 观测者（noop/log/multi）
    prompt_builder: SystemPromptBuilder,
    tool_dispatcher: Box<dyn ToolDispatcher>,   // Native/XML 两种 dispatch 格式
    memory_loader: Box<dyn MemoryLoader>,
    config: AgentConfig,
    // ...
    turn_buffer: TurnBuffer,
    history: Vec<ConversationMessage>,
}
```

#### 工具执行策略（execution.rs）
```rust
// 关键决策逻辑：单工具或需审批时串行
fn should_execute_tools_in_parallel(tools: &[ParsedToolCall]) -> bool {
    tools.len() > 1 && !tools.iter().any(|t| t.requires_approval())
}

// 可取消执行（CancellationToken）
async fn execute_one_tool(tool, params, cancel_token) {
    tokio::select! {
        result = tool.execute(params) => ToolExecutionOutcome { ... },
        _ = cancel_token.cancelled() => ToolExecutionOutcome::cancelled(),
    }
}
// 输出必须通过凭证清洗
scrub_credentials(&mut output);
```

#### 常量与迭代控制
```rust
const DEFAULT_MAX_TOOL_ITERATIONS: usize = 20;
const MAX_TOKENS_CONTINUATION_MAX_ATTEMPTS: usize = 3;
const MAX_TOKENS_CONTINUATION_MAX_OUTPUT_CHARS: usize = 120_000;
const STREAM_CHUNK_MIN_CHARS: usize = 80;
```

#### 循环检测（loop_/detection.rs）
```rust
pub enum DetectionVerdict {
    Continue,          // 正常继续
    Warning(String),   // 警告
    Terminate(String), // 终止循环
}
// LoopDetector 分析工具调用历史，检测重复模式
```

#### ToolDispatcher Trait（dispatcher.rs）
```rust
pub trait ToolDispatcher: Send + Sync {
    fn parse_response(&self, response: &ChatResponse) -> (String, Vec<ParsedToolCall>);
    fn format_results(&self, results: &[ToolExecutionResult]) -> ConversationMessage;
    fn prompt_instructions(&self, tools: &[Box<dyn Tool>]) -> String;
    fn to_provider_messages(&self, history: &[ConversationMessage]) -> Vec<ChatMessage>;
    fn should_send_tool_specs(&self) -> bool;
}
// 实现：NativeToolDispatcher（JSON）、XmlToolDispatcher（XML 自动标签规范化）
```

### 2.3 goose：MCP 原生 + 流式架构

#### 核心特性
- **MCP 原生**：通过 `rmcp 0.16` 集成，工具结果通过 `ServerNotification` 流式返回
- **工具流类型**：`ToolStream = Pin<Box<dyn Stream<Item=ToolStreamItem<ToolResult<CallToolResult>>> + Send>>`
- **DEFAULT_MAX_TURNS = 1000**（最宽松，适合复杂工作流）
- **平台感知能力协商**：CLI vs Desktop 不同能力集
- **子智能体支持**：`subagent_handler.rs`，`subagent_task_config.rs`
- **审批流程**：`handle_approval_tool_requests`，使用 `with_action_required()` 封装

#### 工具审批模式（tool_execution.rs）
```rust
// 审批失败时的标准提示词
pub const DECLINED_RESPONSE: &str = "The user has declined to run this tool. \
    DO NOT attempt to call this tool again. ...";
```

#### 重试管理
- `RetryManager`：指数退避 + 最大次数限制
- `ToolInspectionManager`：工具调用前安全检查

### 2.4 pi_agent_rust：Extension 驱动架构

#### 独特的消息队列设计
```rust
struct MessageQueue {
    steering: VecDeque<QueuedMessage>,    // 指向性消息（优先级高）
    follow_up: VecDeque<QueuedMessage>,  // 跟进消息
    steering_mode: QueueMode,    // All | OneAtATime
    follow_up_mode: QueueMode,
    next_seq: u64,
}
```

#### Agent 配置
```rust
pub struct AgentConfig {
    pub system_prompt: Option<String>,
    pub max_tool_iterations: usize,   // 默认 50
    pub stream_options: StreamOptions,
    pub block_images: bool,
}
```

#### Extension 系统亮点
- `hostcall_amac.rs`：主机调用拦截（高级内存访问控制）
- `hostcall_io_uring_lane.rs`：io_uring 异步 I/O 通道
- `hostcall_s3_fifo.rs`：S3-FIFO 缓存算法
- `hostcall_superinstructions.rs`：超级指令优化
- `hostcall_trace_jit.rs`：追踪 JIT 编译
- Extension 事件驱动：`extension_events.rs`，`InputEventOutcome`

### 2.5 autoagents：最完善的 Hook 系统 + TurnEngine

#### TurnEngine 统一执行引擎
```rust
/// 统一多轮执行，TurnResult 控制循环
pub enum TurnResult<T> {
    Continue(T),   // 继续下一轮
    Complete(T),   // 完成退出
}

pub trait TurnEngine<T, A>: Send + Sync {
    async fn run_turn(&self, agent: &T, action: &A) -> Result<TurnResult<T>>;
}
```

#### 9 拦截点 Hook 系统（含 Abort 能力）
```rust
/// 9 个 Hook 拦截点（比 ironclaw 多 3 个）
pub enum HookPoint {
    BeforeMessage,          // 消息进入前
    AfterMessage,           // 消息处理后
    BeforeToolCall,         // 工具调用前
    AfterToolCall,          // 工具调用后（ironclaw 缺失）
    BeforeResponse,         // 响应生成前
    AfterResponse,          // 响应发出后（ironclaw 缺失）
    OnError,                // 错误发生时（ironclaw 缺失）
    OnSessionStart,
    OnSessionEnd,
}

pub enum HookOutcome {
    Continue,
    Abort(String),   // ← 关键：可以中止整个执行链（ironclaw 不支持 Abort）
    Modify(HookData),
}
```

#### 双运行时：DirectAgent vs ActorAgent（Ractor）
```rust
// 同步 Direct 模式（低延迟，单任务）
pub struct DirectAgent<T, A> {
    inner: Arc<T>,
    hooks: HookRegistry,
}

// Ractor Actor 模式（真正并发，消息驱动）
pub struct ActorAgent<T, A> {
    actor: ActorRef<AgentMessage<A>>,
}
```

#### 结构化输出支持
```rust
pub trait Agent: Send + Sync {
    fn output_schema(&self) -> Option<schemars::schema::RootSchema> {
        None  // 返回 JSON Schema 约束输出格式
    }
}
```

#### MemoryProvider + SlidingWindowMemory
```rust
pub trait MemoryProvider: Send + Sync {
    async fn get_messages(&self) -> Vec<Message>;
    async fn add_message(&self, msg: Message);
}

pub struct SlidingWindowMemory {
    window: usize,
    trim_strategy: TrimStrategy,  // Drop | Summarize
}
```

### 2.6 localgpt：TurnGate 并发控制 + 工具注入安全

#### TurnGate 并发控制（跨组件共享锁）
```rust
/// 防止 HTTP 请求和 HeartbeatRunner 并发触发 agent turn
pub struct TurnGate {
    semaphore: Arc<Semaphore>,  // permits = 1（互斥）
}

impl TurnGate {
    pub async fn acquire(&self) -> TurnGateGuard { ... }
}

// HTTP handler 和 HeartbeatRunner 共享同一个 TurnGate
// 不会发生 "HeartbeatRunner 占用 session 时 HTTP 请求并发触发" 的问题
```

#### 工具注入安全模式（核心库 vs CLI 分离）
```rust
// 核心库（安全工具集）：ReadFile、WebSearch、Spawn
// CLI 层注入危险工具：Bash、FileWrite
fn build_agent_cli_mode() -> Agent {
    let mut agent = Agent::new_safe();    // 只有安全工具
    agent.inject_tool(BashTool::new());   // CLI 注入危险工具
    agent.inject_tool(FileWriteTool::new());
    agent
}
```

#### FailoverProvider 链（顺序故障转移）
```rust
pub struct FailoverProvider {
    providers: Vec<Box<dyn LLMProvider>>,  // 按顺序尝试
}

impl LLMProvider for FailoverProvider {
    async fn complete(&self, messages: &[Message]) -> Result<Response> {
        for provider in &self.providers {
            match provider.complete(messages).await {
                Ok(r) => return Ok(r),
                Err(e) if e.is_retriable() => continue,
                Err(e) => return Err(e),
            }
        }
        Err(AllProvidersFailed)
    }
}
```

#### 签名安全策略（LLM 不可篡改）
```rust
// LocalGPT.md 作为签名安全策略：
// - 追加到每轮最后一条消息（不进历史，防止 Anthropic 连续角色报错）
// - LLM 无法通过工具调用修改此策略
// - max_spawn_depth 保护递归深度
pub struct SecurityPolicy {
    content: String,
    signature: [u8; 32],
}
```

#### 子 Agent 分层委托
```rust
// spawn_agent 工具：层级化任务分发
pub struct SpawnTool {
    max_spawn_depth: usize,   // 防止无限递归
    current_depth: usize,
}
```

### 2.7 moltis：强类型消息 + 熔断器 + 6 层权限

#### 强类型 ChatMessage（避免 JSON Value 泄漏元数据）
```rust
/// 避免 serde_json::Value 的类型安全消息
pub enum ChatMessage {
    User(UserMessage),
    Assistant(AssistantMessage),
    Tool(ToolMessage),
    System(SystemMessage),
}

// ToolSource 精确描述工具来源（content-addressed WASM）
pub enum ToolSource {
    Builtin,
    Mcp { server: String },
    Wasm { component_hash: [u8; 32] },   // 内容寻址，防篡改
}
```

#### Provider 熔断器（Circuit Breaker）
```rust
pub enum ProviderErrorKind {
    RateLimit,
    AuthError,
    ServerError,
    BillingExhausted,
    ContextWindow,
    InvalidRequest,
    Unknown,
}

impl ProviderErrorKind {
    pub fn should_failover(&self) -> bool {
        matches!(self, RateLimit | ServerError | Unknown)
        // AuthError / BillingExhausted 不故障转移（换 provider 也没用）
    }
}
```

#### 6 层权限控制（精细粒度）
```rust
pub struct PermissionStack {
    global: GlobalPolicy,           // 1. 全局策略
    provider: ProviderPolicy,       // 2. Provider 级别
    agent: AgentPolicy,             // 3. Agent 级别
    group: GroupPolicy,             // 4. 群组级别
    sender: SenderPolicy,           // 5. 发送者级别（群内）
    sandbox: SandboxPolicy,         // 6. 沙箱执行级别
}

// 每层使用 Glob 模式匹配：
// allowed_tools: ["file_read", "web_*"]
// denied_tools: ["bash", "file_write"]
```

#### API Key 安全处理
```rust
use secrecy::Secret;

pub struct ProviderConfig {
    pub api_key: Secret<String>,  // 不会意外打印/序列化
}
```

### 2.8 openfang：Hands 能力包 + Taint 追踪 + Merkle 审计链

#### "Hands" 能力包概念（预构建领域能力）
```rust
pub struct HandDefinition {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,         // 该 Hand 提供的工具集
    pub requirements: HandRequirement,
}

pub struct HandRequirement {
    pub env_vars: Vec<String>,      // 需要的环境变量
    pub permissions: Vec<String>,   // 需要的权限
}

// 示例：WebHand 包含 web_fetch, html_parse, link_extract
// 示例：FileHand 包含 read_file, write_file, list_dir, apply_patch
```

#### 两层 Taint 追踪（Shell 注入防护）
```rust
// 第一层：Shell 元字符检测
fn check_metachar_injection(input: &str) -> bool {
    input.contains(['&', '|', ';', '$', '`', '\\', '>', '<'])
}

// 第二层：启发式模式检测
fn check_heuristic_injection(input: &str) -> bool {
    INJECTION_PATTERNS.iter().any(|p| p.is_match(input))
}
```

#### AuditLog 作为 Merkle 哈希链（防篡改）
```rust
pub struct AuditEntry {
    pub timestamp: u64,
    pub event: AuditEvent,
    pub prev_hash: [u8; 32],   // 链接前一条记录
    pub hash: [u8; 32],         // SHA-256(prev_hash || event_data)
}
// 任何一条记录被篡改，后续所有 hash 立即失效
```

#### 多运行时 Skills
```rust
pub enum SkillRuntime {
    Python,     // Python 脚本
    Wasm,       // WASM 组件
    Node,       // Node.js 脚本
    Builtin,    // 内置 Rust 实现
    PromptOnly, // 纯提示词注入
}
```

#### RBAC 工具访问控制
```rust
pub struct AgentManifest {
    pub allowed_tools: Vec<String>,   // 该 Agent 允许使用的工具
}
// 在 ToolExecutor 层强制执行（不是 prompt 中声明）
```

### 2.9 nanoclaw（TypeScript baseline）：容器级隔离 + 凭证代理

#### 容器-per-group 隔离（每个群组独立容器）
```typescript
// 每个 chat group 获得独立 Docker 容器
class GroupIsolationManager {
    private containers: Map<GroupId, DockerContainer>;

    async getOrCreate(groupId: GroupId): Promise<DockerContainer> {
        if (!this.containers.has(groupId)) {
            const container = await Docker.run({
                image: 'agent-sandbox:latest',
                mounts: ['/dev/null:/app/.env'],  // shadow .env
            });
            this.containers.set(groupId, container);
        }
        return this.containers.get(groupId)!;
    }
}
```

#### 凭证代理（Secrets Never in Container）
```typescript
// .env 被 /dev/null 挂载覆盖 —— 容器内永远没有 secrets
// 凭证通过 HTTP 代理在传输层注入
class CredentialProxy {
    async injectCredentials(request: HttpRequest): Promise<HttpRequest> {
        const creds = await this.secretsVault.get(request.service);
        return request.withHeader('Authorization', `Bearer ${creds.token}`);
        // creds 不进入容器环境变量，只注入 HTTP 请求头
    }
}
```

#### GroupQueue：同组串行，跨组并行
```typescript
class GroupQueue {
    private queues: Map<GroupId, PQueue>;

    enqueue(groupId: GroupId, task: Task): Promise<void> {
        // 同一 group 的消息串行处理（concurrency: 1）
        // 不同 group 之间完全并行
        return this.getQueue(groupId).add(task);
    }
}
```

### 2.10 nanobot（Python baseline）：极简工具集 + MCP 懒连接

#### 9 工具极简集合
```python
MINIMAL_TOOLS = [
    ReadFile, WriteFile, EditFile, ListDir,  # 文件操作
    ExecTool,                                 # 命令执行
    WebSearch,                                # 搜索
    Message,                                  # 向用户发消息
    Spawn,                                    # 生成子 agent
    Cron,                                     # 定时任务
]
```

#### 错误响应不持久化（防止污染上下文）
```python
class AgentLoop:
    async def run_turn(self, message: str):
        try:
            result = await self.llm.complete(self.history + [message])
        except Exception as e:
            # 错误响应 NOT 添加到 history
            # 防止错误消息污染后续对话上下文
            return ErrorResponse(str(e))

        self.history.append(result)  # 只有成功才持久化
        return result
```

#### MCP 懒连接（首次消息时才连接）
```python
class McpManager:
    _connected: bool = False

    async def ensure_connected(self):
        if not self._connected:
            await self._connect_all_servers()
            self._connected = True

    async def on_first_message(self, msg: str):
        await self.ensure_connected()  # 启动时不连接，首次用时才连
```

#### SubagentManager + 运行时上下文标签
```python
class SubagentManager:
    async def spawn(self, task: str, max_depth: int = 3) -> str:
        if self.current_depth >= max_depth:
            raise MaxDepthExceeded()

        subagent = Agent(
            tools=SAFE_TOOLS_ONLY,   # 子 agent 只有安全工具
            runtime_context="subagent",  # 标签区分信任来源
            parent_depth=self.current_depth + 1,
        )
        return await subagent.run(task)
```

---

## 3. 综合比较矩阵（全部 10 个项目）

| 维度 | ironclaw | zeroclaw | goose | pi_agent_rust | autoagents | localgpt | moltis | openfang | nanoclaw | nanobot |
|------|----------|----------|-------|---------------|------------|----------|--------|----------|----------|---------|
| **代码规模** | ~18 agent 文件 | ~12 文件 | ~20 文件 | ~500行 | ~15 文件 | ~10 文件 | 50+ crate | 14 crate | 单文件TS | 单文件Py |
| **工具迭代上限** | Scheduler | 20 次 | 1000 轮 | 50 次 | TurnEngine | 无硬限制 | 无硬限制 | 无硬限制 | 无硬限制 | 无硬限制 |
| **并行工具执行** | JoinSet | 自适应 | 并行+流式 | 最多8并发 | JoinSet | 串行 | 并行 | 并行 | 并行 | 串行 |
| **沙箱机制** | Docker+WASM | WASM | 容器 | WASM+JS | WASM | 无 | content-addr WASM | Python/WASM/Node | 容器-per-group | 无 |
| **上下文压缩** | 3策略 | 无内置 | 无内置 | CompactionWorker | SlidingWindow | 无 | 无 | 无 | 无 | 无 |
| **会话管理** | Session/Thread/Turn+Undo | 简单History | SQLite | SQLite | 简单History | 无显式 | 无显式 | 三层 | GroupQueue | 简单History |
| **Hook 系统** | 6拦截点 | 无 | 无 | Extension事件 | **9拦截点+Abort** | 无 | ChannelPlugin | 事件驱动 | 无 | 无 |
| **Hook Abort** | ❌ | ❌ | ❌ | ❌ | **✅** | ❌ | ❌ | ❌ | ❌ | ❌ |
| **调度器** | 双表 | 无 | 无 | 无 | Ractor Actor | HeartbeatRunner | 无 | 无 | 无 | Cron工具 |
| **自修复** | DefaultSelfRepair | 无 | RetryManager | 无 | 无 | 无 | 无 | 无 | 无 | 无 |
| **MCP 支持** | HTTP client | 无 | **rmcp原生** | 无 | 无 | 无 | 无 | 无 | 无 | **懒连接** |
| **技能系统** | SKILL.md | 无 | PromptManager | 无 | 无 | 无 | 无 | **多运行时** | YAML frontmatter | 无 |
| **Cost Guard** | 每日+小时限制 | 无 | 无 | 无 | 无 | 无 | 无 | 无 | 无 | 无 |
| **安全层** | 4层管道 | 凭证清洗 | ToolInspection | 无 | 无 | 签名策略 | 无 | **Taint追踪** | **credential proxy** | 无 |
| **观测系统** | Observer trait | Observer trait | 无 | AgentEvent | 无 | 无 | 无 | 无 | 无 | 无 |
| **循环检测** | 无 | **LoopDetector** | 无 | 无 | 无 | 无 | 无 | 无 | 无 | 无 |
| **消息队列** | 单流 | 单流 | confirm/tool tx | Steering/FollowUp | Actor mailbox | TurnGate | 无 | 无 | **GroupQueue** | 无 |
| **Provider容错** | 无 | 无 | RetryManager | 无 | 无 | **FailoverProvider** | **Circuit Breaker** | 无 | 无 | 无 |
| **权限层级** | 4层安全管道 | 无 | PermissionLevel | 无 | 无 | tool injection | **6层** | **RBAC** | 容器隔离 | 运行时标签 |
| **子 Agent** | spawn_job | 无 | subagent_handler | 无 | 无 | **spawn_agent+深度限制** | 无 | 无 | 无 | **Spawn+深度限制** |
| **消息类型安全** | String | ChatMessage | Content enum | 无类型 | Message enum | 无 | **强类型enum** | 无 | 无 | 无 |
| **审计追踪** | AuditLog | 无 | 无 | 无 | 无 | 无 | 无 | **Merkle链** | 无 | 无 |
| **容器隔离** | Docker per-job | 无 | 容器化 | 无 | 无 | 无 | 无 | 无 | **per-group** | 无 |
| **错误持久化** | 持久化 | 持久化 | 持久化 | 持久化 | 持久化 | 持久化 | 持久化 | 持久化 | 持久化 | **❌不持久化** |

---

## 4. octo-sandbox 最佳 Agent Harness 设计方案

### 4.1 设计原则（全 10 项目综合）

基于所有 10 个项目的深度分析，octo-sandbox 的 Agent Harness 应遵循以下原则：

1. **模块化依赖注入**（ironclaw AgentDeps）：所有运行时依赖通过单一结构体注入，避免参数爆炸
2. **Trait 驱动可扩展性**（zeroclaw）：Provider/Tool/Memory/Observer 全部基于 Trait
3. **MCP 原生**（goose）：通过 rmcp 支持 MCP 协议，工具结果流式返回
4. **类型化 Submission 分发**（ironclaw）：所有用户输入在进入 agentic loop 前转为强类型变体
5. **自适应工具执行**（zeroclaw）：单工具或需审批时串行，多工具时并行
6. **三级上下文压缩**（ironclaw）：80/85/95% 三档阈值策略
7. **Hook Abort 能力**（autoagents）：Hook 可以中止整个执行链，不只是 Continue/Modify
8. **强类型 ChatMessage**（moltis）：避免 serde_json::Value 作为消息类型
9. **Provider 熔断器**（moltis ProviderErrorKind）：根据错误类型决定是否故障转移
10. **TurnGate 并发控制**（localgpt）：防止 HTTP 请求与心跳任务并发触发 agent turn
11. **错误响应不持久化**（nanobot）：错误消息不进入历史，防止污染上下文
12. **KISS 原则**（zeroclaw CLAUDE.md）：不引入不需要的复杂性

### 4.2 推荐架构：AgentHarness

#### 4.2.1 核心结构

```rust
/// 运行时依赖（Builder 模式注入，避免参数爆炸）
/// 来源：ironclaw AgentDeps + localgpt TurnGate + moltis ProviderErrorKind
pub struct HarnessDeps {
    // --- 必须项 ---
    pub provider: Arc<dyn LlmProvider>,         // 模型提供者（支持 ProviderErrorKind 熔断）
    pub tool_registry: Arc<ToolRegistry>,        // 工具注册表

    // --- 可选项（None = 功能不启用）---
    pub cheap_provider: Option<Arc<dyn LlmProvider>>,  // 轻量任务专用（摘要/心跳）
    pub failover_providers: Vec<Arc<dyn LlmProvider>>, // 故障转移链（localgpt FailoverProvider）
    pub turn_gate: Arc<TurnGate>,                      // 并发控制：防止 HTTP 与心跳冲突（localgpt）
    pub circuit_breaker: Option<Arc<CircuitBreaker>>,  // Provider 熔断器（moltis）
    pub memory: Option<Arc<dyn Memory>>,               // 多层记忆后端
    pub mcp_manager: Option<Arc<McpManager>>,          // MCP 服务器管理
    pub session_store: Option<Arc<dyn SessionStore>>,  // 会话持久化
    pub observer: Arc<dyn Observer>,                   // 观测（noop/log/otel）
    pub hooks: Arc<HookRegistry>,                      // 生命周期 Hook（含 Abort 能力）
    pub cost_guard: Option<Arc<CostGuard>>,            // 费用控制
    pub safety: Option<Arc<SafetyLayer>>,              // 安全管道
    pub scheduler: Option<Arc<Scheduler>>,             // 后台任务调度
    pub context_compaction: CompactionConfig,          // 上下文压缩配置
    pub hands: Vec<HandDefinition>,                    // 能力包（openfang Hands 模式）
    pub api_key: secrecy::Secret<String>,              // API key 安全封装（moltis secrecy）
}

/// Agent Harness 主结构
pub struct AgentHarness {
    config: HarnessConfig,
    deps: HarnessDeps,
    session_manager: Arc<SessionManager>,
    context_monitor: ContextMonitor,
    router: CommandRouter,                             // 命令路由
}
```

#### 4.2.2 Session/Thread/Turn 模型（参考 ironclaw）

```rust
/// 三层会话模型
pub struct Session {
    id: SessionId,
    active_thread_id: ThreadId,
    threads: HashMap<ThreadId, Thread>,
}

pub struct Thread {
    id: ThreadId,
    turns: Vec<Turn>,           // append-only
    state: ThreadState,         // Idle | Processing | AwaitingApproval | Completed | Interrupted
    undo_manager: UndoManager,  // 最多 20 个检查点
}

pub struct Turn {
    user_input: String,
    response: Option<String>,
    tool_calls: Vec<ToolCall>,
    state: TurnState,           // Pending | Running | Complete | Failed
}
```

#### 4.2.3 类型化 Submission（参考 ironclaw，精简版）

```rust
/// 所有用户提交在进入 agentic loop 前解析为强类型
pub enum Submission {
    // 对话
    UserInput(String),

    // 控制命令
    Interrupt,
    Undo,
    Redo,
    Compact,        // 手动触发上下文压缩
    Clear,          // 清除历史
    NewThread,
    SwitchThread(ThreadId),
    Quit,

    // 审批
    ApprovalResponse { approved: bool, always: bool },

    // 系统命令（/help, /model, /status 等）
    SystemCommand(String),

    // 任务管理（如果启用 Scheduler）
    JobStatus(Option<JobId>),
    JobCancel(JobId),
}
```

#### 4.2.4 Agentic Loop（ReAct 核心）

采用 **localgpt TurnGate** 并发控制 + **nanobot 错误不持久化** + **moltis 强类型 ChatMessage** 三项关键模式。

```rust
/// 强类型消息枚举（moltis ChatMessage 模式）
/// 避免 serde_json::Value 泄露元数据、避免 String 丢失类型信息
pub enum ChatMessage {
    User(UserMessage),
    Assistant(AssistantMessage { text: String, tool_calls: Vec<ToolCall> }),
    Tool(ToolMessage { tool_call_id: String, content: String, is_error: bool }),
    System(SystemMessage),
}

/// 单次对话轮次的 agentic loop
async fn run_agentic_loop(
    &self,
    thread: &mut Thread,
    deps: &HarnessDeps,
) -> Result<AgenticLoopResult> {
    // 0. TurnGate 获取（localgpt）：防止并发 turn 冲突
    let _gate = deps.turn_gate.acquire().await;

    // 1. 构建系统提示（技能上下文注入 + Hands 能力包）
    let system_prompt = self.build_system_prompt(thread, deps).await?;

    // 2. BeforeInbound hook（包含 Abort 路径）
    match deps.hooks.run(HookPoint::BeforeInbound, &thread.last_input()).await? {
        HookOutcome::Abort(reason) => return Ok(AgenticLoopResult::Aborted(reason)),
        HookOutcome::Modify(content) => thread.override_last_input(content),
        HookOutcome::Block(reason) => return Ok(AgenticLoopResult::Blocked(reason)),
        HookOutcome::Continue => {}
    }

    // 3. 检查 cost guard
    if let Some(guard) = &deps.cost_guard {
        guard.check_allowed()?;
    }

    loop {
        // 4. LLM 调用（流式）— 含 FailoverProvider 和熔断器逻辑
        let response = self.call_with_failover(thread.messages(), &system_prompt, deps).await;

        // 错误不持久化（nanobot 原则）：LLM 错误不加入历史，防止污染上下文
        let response = match response {
            Ok(r) => r,
            Err(e) => {
                // 不调用 thread.append_*，直接返回错误响应
                return Ok(AgenticLoopResult::ProviderError(e));
            }
        };

        // 5. 如果有工具调用
        if response.has_tool_calls() {
            // 5a. BeforeToolCall hook（含 Abort）
            for tc in &response.tool_calls {
                match deps.hooks.run(HookPoint::BeforeToolCall, tc).await? {
                    HookOutcome::Abort(r) => return Ok(AgenticLoopResult::Aborted(r)),
                    HookOutcome::Block(r) => {
                        // 注入拒绝结果，但继续其他工具
                        thread.append_tool_error(tc.id.clone(), r);
                        continue;
                    }
                    _ => {}
                }
            }

            // 5b. 检查工具审批
            let (approved, needs_approval) = self.check_tool_approvals(&response.tool_calls, thread);
            if !approved {
                return Ok(AgenticLoopResult::NeedApproval(needs_approval));
            }

            // 5c. 自适应并行/串行执行（zeroclaw 模式）
            let results = if should_execute_parallel(&response.tool_calls) {
                self.execute_tools_parallel(response.tool_calls, deps).await?
            } else {
                self.execute_tools_sequential(response.tool_calls, deps).await?
            };

            // 5d. 安全层过滤（含 taint tracking）
            let safe_results = if let Some(safety) = &deps.safety {
                safety.process(results)?
            } else {
                results
            };

            // 5e. AfterToolCall hook
            deps.hooks.run(HookPoint::AfterToolCall, &safe_results).await?;

            // 5f. 记录观测事件
            deps.observer.on_event(ObserverEvent::ToolCallComplete { results: &safe_results });

            // 5g. 喂回 LLM，继续循环（强类型 ChatMessage::Tool）
            thread.append_tool_results_typed(safe_results);

        } else {
            // 6. 纯文本响应，退出循环

            // 6a. AfterResponse hook（autoagents 第9拦截点）
            let response = match deps.hooks.run(HookPoint::AfterResponse, &response).await? {
                HookOutcome::Abort(r) => return Ok(AgenticLoopResult::Aborted(r)),
                HookOutcome::Modify(content) => response.with_text(content),
                _ => response,
            };

            // 6b. BeforeOutbound hook
            let response = match deps.hooks.run(HookPoint::BeforeOutbound, &response).await? {
                HookOutcome::Abort(r) => return Ok(AgenticLoopResult::Aborted(r)),
                HookOutcome::Modify(content) => response.with_text(content),
                _ => response,
            };

            // 6c. cost guard 记录
            if let Some(guard) = &deps.cost_guard {
                guard.record_llm_call(response.usage);
            }

            return Ok(AgenticLoopResult::Response(response));
        }

        // 7. 循环检测（防止死循环）
        if thread.turn_count() >= self.config.max_tool_iterations {
            return Ok(AgenticLoopResult::IterationLimitReached);
        }
    }
}

/// FailoverProvider 调用（localgpt 模式 + moltis 熔断器）
async fn call_with_failover(
    &self,
    messages: &[ChatMessage],
    system: &str,
    deps: &HarnessDeps,
) -> Result<AssistantResponse> {
    // 主 provider 尝试
    match deps.provider.complete(messages, system).await {
        Ok(r) => return Ok(r),
        Err(e) => {
            // 熔断器判断：是否可以故障转移
            if !e.kind().should_failover() {
                return Err(e);  // AuthError / BillingExhausted 不故障转移
            }
        }
    }
    // 依次尝试备用 providers
    for fallback in &deps.failover_providers {
        match fallback.complete(messages, system).await {
            Ok(r) => return Ok(r),
            Err(e) if e.kind().should_failover() => continue,
            Err(e) => return Err(e),
        }
    }
    Err(ProviderError::AllProvidersFailed)
}
```

#### 4.2.5 Hook 系统（autoagents 9-hook + Abort 升级版）

ironclaw 6-hook Continue-only 设计的关键缺陷：Hook 无法中止执行链。
autoagents 通过 `HookOutcome::Abort(String)` 解决了这个问题。octo-sandbox 采用升级版：

```rust
/// 9 个拦截点（ironclaw 6点 + autoagents 新增 3点）
pub enum HookPoint {
    // 原 ironclaw 6点
    BeforeInbound,       // 修改/拒绝入站用户消息
    BeforeToolCall,      // 工具调用前（可阻止单个工具）
    BeforeOutbound,      // 出站响应前
    OnSessionStart,      // 会话开始
    OnSessionEnd,        // 会话结束
    TransformResponse,   // 响应格式转换

    // autoagents 新增 3点
    AfterToolCall,       // 工具调用后（可审计结果）
    AfterResponse,       // LLM 响应���成后（最终拦截）
    OnError,             // 任意阶段发生错误时
}

pub trait Hook: Send + Sync {
    fn point(&self) -> HookPoint;
    fn priority(&self) -> i32 { 0 }  // 数字越小越先执行
    fn failure_mode(&self) -> HookFailureMode { HookFailureMode::FailOpen }
    async fn run(&self, ctx: &mut HookContext) -> HookOutcome;
}

/// 关键升级：加入 Abort 变体（autoagents 原创）
pub enum HookOutcome {
    Continue,                      // 继续处理
    Modify(String),                // 修改内容后继续
    Block(String),                 // 阻止当前操作（记录原因，不终止整个 loop）
    Abort(String),                 // ← 终止整个执行链（ironclaw 不具备）
    Redirect(HookRedirectTarget),  // 重定向到其他 Hook 链
}

/// Hook 失败模式
pub enum HookFailureMode {
    FailOpen,   // Hook 出错时继续（ironclaw 默认）
    FailClosed, // Hook 出错时中止（安全关键 Hook 使用）
}
```

#### 4.2.6 上下文压缩（三级策略）

```rust
pub struct CompactionConfig {
    pub context_limit_tokens: u64,      // 默认 100_000
    pub compaction_threshold: f32,      // 默认 0.80
}

pub enum CompactionStrategy {
    /// 80-85%：写入日志，保留最近 N 轮
    MoveToWorkspace { keep_recent: usize },
    /// 85-95%：LLM 生成摘要 + 写入日志
    Summarize { keep_recent: usize },
    /// >95%：直接截断（快速路径）
    Truncate { keep_recent: usize },
}

// Token 估算（与 ironclaw 保持一致）
fn estimate_tokens(text: &str) -> u64 {
    let word_count = text.split_whitespace().count() as u64;
    word_count * 13 / 10 + 4  // × 1.3 + 4 per message
}
```

#### 4.2.7 工具安全层（精简版）

```rust
/// 工具输出经过 4 层处理才返回 LLM
pub struct SafetyLayer {
    sanitizer: Sanitizer,       // 注入模式检测
    validator: Validator,       // 长度/编码/模式
    policy: PolicyEngine,       // 规则 (Critical/High/Medium/Low)
    leak_detector: LeakDetector, // 15+ 密钥模式（API keys、tokens、连接字符串）
}
```

#### 4.2.8 调度器（可选）

```rust
/// 与 ironclaw 保持兼容的双表调度器
pub struct Scheduler {
    jobs: Arc<RwLock<HashMap<JobId, JobHandle>>>,      // 完整 LLM 任务
    subtasks: Arc<RwLock<HashMap<SubtaskId, JoinHandle<()>>>>,  // 轻量后台任务
}

impl Scheduler {
    /// 首选入口：持久化后再调度（避免 FK 错误）
    pub async fn dispatch_job(&self, job: Job) -> Result<JobId>;

    /// 轻量子任务（返回 oneshot::Receiver）
    pub async fn spawn_subtask<F, R>(&self, f: F) -> oneshot::Receiver<R>;

    /// 批量子任务（并发执行，按输入顺序返回）
    pub async fn spawn_batch<F, R>(&self, tasks: Vec<F>) -> Vec<R>;
}
```

### 4.3 模块结构（参考 ironclaw 模块分解，扩展新增模块）

```
crates/octo-engine/src/agent/
├── mod.rs              # 公共 API 重导出
├── harness.rs          # AgentHarness 主结构 + run()
├── deps.rs             # HarnessDeps 依赖注入容器（含 TurnGate/CircuitBreaker）
├── session.rs          # Session/Thread/Turn 数据模型
├── session_manager.rs  # 会话生命周期：创建/查找/剪枝
├── submission.rs       # 类型化 Submission 解析（SubmissionParser）
├── dispatcher.rs       # conversational turn 的 agentic loop（含强类型 ChatMessage）
├── thread_ops.rs       # 线程操作：undo/redo/compact/approve
├── commands.rs         # 系统命令处理 (/help, /model, /status)
├── router.rs           # /command 路由到 MessageIntent
├── context_monitor.rs  # 检测内存压力，推荐 CompactionStrategy
├── compaction.rs       # 三级上下文压缩实现
├── undo.rs             # Turn-based undo/redo（最多 20 检查点）
├── loop_detect.rs      # 循环检测（参考 zeroclaw DetectionVerdict）
├── scheduler.rs        # 后台任务调度（可选）
├── worker.rs           # 后台 Job Worker（独立于 dispatcher）
├── cost_guard.rs       # LLM 费用控制
├── heartbeat.rs        # 主动心跳执行（可选）
├── task.rs             # Job/ToolExec/Background 任务类型
│
│   # ─── 新增：来自 6 个新项目的核心模式 ───
├── turn_gate.rs        # TurnGate 并发控制（localgpt）— Arc<Semaphore> 防止并发 turn
├── circuit_breaker.rs  # Provider 熔断器（moltis ProviderErrorKind）— should_failover()
├── failover.rs         # FailoverProvider 链（localgpt）— 顺序故障转移
├── message.rs          # 强类型 ChatMessage 枚举（moltis）— User/Assistant/Tool/System
├── hands.rs            # Hands 能力包注册（openfang）— HandDefinition + HandRequirement
└── audit_chain.rs      # AuditLog Merkle 链（openfang）— prev_hash 防篡改审计
```

**Hooks 子系统**（独立 crate 或模块）：
```
crates/octo-engine/src/hooks/
├── mod.rs              # HookPoint（9个）、HookOutcome（含 Abort）、HookFailureMode
├── hook.rs             # Hook trait、HookContext、优先级排序
├── registry.rs         # HookRegistry：注册、按 point 过滤、链式执行
└── builtin.rs          # 内置 Hook：安全过滤、日志记录、成本检查
```

### 4.4 与现有 octo-engine 的集成映射

| octo-engine 现有模块 | 对应 Harness 组件 | 建议 | 来源 |
|----------------------|-------------------|------|------|
| `agent/runtime.rs` → `AgentRuntime` | `agent/harness.rs` → `AgentHarness` | 重构为 AgentDeps 模式 | ironclaw |
| `agent/executor.rs` → `AgentExecutor` | `agent/dispatcher.rs` → 对话轮次 loop | 保持，剥离后台任务到 worker.rs | ironclaw |
| `agent/loop_.rs` → `AgentLoop` | `agent/dispatcher.rs` 内部 | 合并，增加 9 Hook 拦截点 | autoagents |
| `session/` → `SessionStore` | `agent/session_manager.rs` | 扩展增加 Thread/Turn 层次 | ironclaw |
| `memory/` | `HarnessDeps::memory` | 作为可选依赖注入 | — |
| `mcp/` → `McpManager` | `HarnessDeps::mcp_manager` | 保持，通过 deps 注入；MCP 懒连接 | goose + nanobot |
| `tools/` → `ToolRegistry` | `HarnessDeps::tool_registry` | 保持，增加 SafetyLayer + taint 包装 | openfang |
| `providers/` → `ProviderChain` | `HarnessDeps::provider` + `failover_providers` | 升级为 FailoverProvider 链 | localgpt |
| — | `agent/message.rs` | **新增**：强类型 ChatMessage 枚举（替换 String/Value） | moltis |
| — | `agent/turn_gate.rs` | **新增**：TurnGate 并发控制 | localgpt |
| — | `agent/circuit_breaker.rs` | **新增**：Provider 熔断器 | moltis |
| — | `agent/failover.rs` | **新增**：FailoverProvider 链 | localgpt |
| — | `agent/hands.rs` | **新增**：Hands 能力包 | openfang |
| — | `agent/audit_chain.rs` | **新增**：Merkle 防篡改审计链 | openfang |
| — | `agent/submission.rs` | **新增**：类型化 Submission 解析 | ironclaw |
| — | `agent/context_monitor.rs` | **新增**：自动压缩触发 | ironclaw |
| — | `agent/compaction.rs` | **新增**：三级压缩策略 | ironclaw |
| — | `agent/loop_detect.rs` | **新增**：循环检测 | zeroclaw |
| — | `hooks/` | **新增**：9 点 Hook 系统（含 Abort） | autoagents |
| — | `agent/cost_guard.rs` | **新增**：费用控制 | ironclaw |

---

## 5. 实施路径（分阶段）

### Phase 1：核心 Harness 骨架（P0）
1. 定义强类型 `ChatMessage` 枚举（moltis）— 替换现有 `String`/`Value` 消息类型
2. 实现 `TurnGate`（localgpt）— `Arc<Semaphore>` 防并发，注入 HarnessDeps
3. 重构 `AgentRuntime` 为 `AgentHarness` + `HarnessDeps`（ironclaw AgentDeps 模式）
4. 实现 `Session/Thread/Turn` 三层模型
5. 实现 `SubmissionParser`（类型化 Submission 分发）
6. 实现 `CommandRouter`（/command 路由）

### Phase 2：安全与可观测性（P1）
7. 实现 `SafetyLayer`（4 层管道）+ taint tracking（openfang）
8. 实现 `HookRegistry`（**9 拦截点 + Abort 能力**，升级自 ironclaw 6-hook）
9. 实现 `ContextMonitor` + 三级 `Compaction`
10. 实现 `LoopDetector`（zeroclaw DetectionVerdict 模式）
11. 实现 `AuditChain`（openfang Merkle 链）— 可选，高安全场景启用

### Phase 3：Provider 韧性（P1）
12. 实现 `ProviderErrorKind` + `should_failover()`（moltis 熔断器）
13. 实现 `FailoverProvider` 链（localgpt）— 顺序故障转移
14. 实现 `CircuitBreaker`（可选，高频使用场景）
15. 在 `dispatcher.rs` 中实现**错误不持久化**（nanobot 原则）

### Phase 4：高级特性（P2）
16. 实现 `CostGuard`
17. 实现 `UndoManager`（最多 20 检查点）
18. 实现 `Scheduler`（双表：jobs + subtasks）
19. 实现 `Hands` 能力包注册（openfang）
20. 实现 `HeartbeatSystem`（可选）
21. 集成 `secrecy::Secret<String>` 替换所有明文 API key（moltis）

---

## 6. 关键设计决策

### 为什么选 AgentDeps 而非直接字段？
- ironclaw 实践证明：15+ 依赖时，单一 struct 比参数传递减少 70% 函数签名复杂度
- 支持 Builder 模式逐步注入可选依赖
- 与 `Arc<T>` 配合使用，零成本克隆传递给后台任务

### 为什么不完全采用 ironclaw 的复杂度？
- ironclaw 是企业级产品，包含 PostgreSQL+libSQL 双后端、Docker 沙箱、ClawHub 注册表等
- octo-engine 已有成熟的 MCP/Provider/Memory 模块，避免重复
- 精简采用其 **架构模式**，不复制其 **垂直业务功能**

### 为什么采用 zeroclaw 的循环检测？
- DEFAULT_MAX_TOOL_ITERATIONS 硬限制解决了 pi_agent_rust 50 次/ironclaw 无限的问题
- LoopDetector 的语义检测（重复工具调用模式）比硬计数更智能

### 为什么采用 goose 的 MCP 流式设计？
- octo-engine 已有 rmcp 依赖
- MCP 工具结果流式返回对 UI 实时性体验至关重要
- goose 的 ToolStream 类型是目前 Rust 生态最成熟的实现

### 为什么从 pi_agent_rust 借鉴消息队列？
- Steering/FollowUp 双队列支持中断插入和后续跟进消息
- 比 ironclaw 的单流更适合 octo-sandbox 的多智能体协同场景
- `MAX_CONCURRENT_TOOLS = 8` 的并发上限防止工具执行过载

### 为什么采用 `HookOutcome::Abort`？（autoagents）
- ironclaw 的 6-hook 系统仅有 `Continue / Modify / Block / Redirect` 四种结果
- `Block` 只能拒绝**当前单次工具调用**，无法终止整个执行链
- `Abort` 的语义是：检测到不可恢复的威胁（如提示注入确认、账单耗尽），**立即终止当前 agentic loop，丢弃所有后续工具调用**
- 没有 `Abort`，安全 hook 遭遇严重威胁时只能返回错误文本，LLM 仍会继续尝试其他路径
- `HookFailureMode::FailClosed` 配合 `Abort` 确保安全关键 hook 失败时也会终止执行

### 为什么使用强类型 `ChatMessage` 枚举？（moltis）
- 现有 octo-engine 使用 `String` 或 `serde_json::Value` 在模块间传递消息，需要运行时解析
- moltis 的 `enum ChatMessage { User(String) | Assistant(String) | Tool { name, result } | System(String) }` 在编译期强制约束消息结构
- 强类型消除了 `role` 字段匹配错误、`content` 为 `null` 的运行时 panic
- `Tool` 变体携带 `name` 和 `result`，避免 LLM 消息中出现裸 `serde_json::Value` 泄漏元数据
- 与 `match` 穷举配合，新增消息类型时编译器强制所有处理路径更新

### 为什么采用 `ProviderErrorKind` 熔断器？（moltis）
- 简单 retry-on-error 策略会在 AuthError / BillingExhausted 时无意义地重试，浪费时间和成本
- `ProviderErrorKind::should_failover()` 的语义：只有可恢复错误（RateLimit / ServerError / Unknown）才触发 failover；AuthError 和 BillingExhausted 直接失败（因为换一个模型也无济于事）
- `ContextWindow` 错误不触发 failover，而是触发 Compaction — 这是一个语义上正确的错误分类
- 与 `FailoverProvider` 链配合，错误路由决策集中在一处，而非散落在各调用点

### 为什么需要 `TurnGate`？（localgpt）
- HTTP 环境中，同一用户可能并发发送两条消息（例如双击发送、网络超时重试）
- 没有并发控制时，两个 agentic loop 同时持有同一个 Thread 的锁，最终导致 TOCTOU 竞态：两个 loop 都读到同一个历史，都向 LLM 发出请求，产生两条重复响应
- `TurnGate` 是每个 Session 一个 `Arc<Semaphore>(1)`，任何消息处理前必须 `acquire()`，确保同一时刻只有一个 turn 在运行
- localgpt 的实践证明：这是 HTTP 驱动 agent 的**必要基础设施**，而非可选优化

### 为什么错误响应不持久化到历史？（nanobot）
- 将 LLM API 错误（超时、503、模型过载）写入对话历史，会让后续轮次的 LLM 看到一条"工具调用失败"记录
- LLM 往往会基于此失败记录改变行为（过度道歉、放弃原定计划、生成无意义的错误解释）
- nanobot 的原则：LLM 错误属于**基础设施异常**，不是**对话事件** — 正确处理是向用户报告、在上层重试，而不是污染对话上下文
- 实施要点：`dispatcher.rs` 中，`provider.complete()` 返回 `Err` 时直接 `return Err(e)`，不执行 `thread.push(Message::tool_error(...))`

---

## 7. 参考资源

### 本地代码路径

**原始 4 个核心项目**：
- ironclaw: `3th-party/harnesses/rust-projects/ironclaw/src/agent/`
- zeroclaw: `3th-party/harnesses/rust-projects/zeroclaw/src/agent/`
- goose: `3th-party/harnesses/rust-projects/goose/crates/goose/src/agents/`
- pi_agent_rust: `3th-party/harnesses/rust-projects/pi_agent_rust/src/`

**新增 6 个项目**（本次分析新纳入）：
- autoagents: `3th-party/harnesses/rust-projects/autoagents/src/`
- localgpt: `3th-party/harnesses/rust-projects/localgpt/src/`
- moltis: `3th-party/harnesses/rust-projects/moltis/src/`
- openfang: `3th-party/harnesses/rust-projects/openfang/src/`
- nanoclaw: `3th-party/harnesses/rust-projects/nanoclaw/src/`
- nanobot: `3th-party/harnesses/rust-projects/nanobot/src/`

**baselines（设计参考）**：
- `3th-party/harnesses/baselines/` — 基础框架设计参考（goose/pi_agent_rust 的祖先版本）

### 关键源文件

**ironclaw**（企业级 Agent，1000+ 行最完整实现）：
- `ironclaw/src/agent/agent_loop.rs` — 主 agentic loop（`run()` 入口，dispatching 调度）
- `ironclaw/src/agent/dispatcher.rs` — 会话 turn 执行（LLM + tool + hook 链）
- `ironclaw/src/agent/thread_ops.rs` — 线程操作：undo/redo/compact/approve
- `ironclaw/src/agent/scheduler.rs` — 双表任务调度（jobs + subtasks）
- `ironclaw/src/hooks/mod.rs` — 6-hook 系统定义（BeforeInbound → TransformResponse）
- `ironclaw/src/agent/CLAUDE.md` — 模块详细设计文档

**zeroclaw**（循环检测、并行执行）：
- `zeroclaw/src/agent/loop_/execution.rs` — 并行/串行自适应工具执行
- `zeroclaw/src/agent/dispatcher.rs` — `ToolDispatcher` Trait 定义
- `zeroclaw/src/agent/loop_/loop_detect.rs` — `DetectionVerdict` 语义循环检测

**goose**（MCP 工具流式执行、审批流）：
- `goose/crates/goose/src/agents/tool_execution.rs` — MCP 工具执行 + 用户审批
- `goose/crates/goose/src/agents/agent.rs` — `ToolStream` 类型定义

**pi_agent_rust**（消息队列、Extension 系统）：
- `pi_agent_rust/src/agent.rs` — Steering/FollowUp 双队列 + Extension 系统

**autoagents**（9-hook + Abort 能力）：
- `autoagents/src/hooks/mod.rs` — `HookOutcome::Abort` 定义
- `autoagents/src/hooks/registry.rs` — 9-hook 链式执行（AfterToolCall / AfterResponse / OnError）
- `autoagents/src/agent/loop_.rs` — Abort 触发路径

**localgpt**（TurnGate、FailoverProvider）：
- `localgpt/src/agent/turn_gate.rs` — `TurnGate`（`Arc<Semaphore>` 并发控制）
- `localgpt/src/provider/failover.rs` — `FailoverProvider` 链（`is_retriable()` 门控）
- `localgpt/src/agent/heartbeat.rs` — `HeartbeatRunner` 与 `TurnGate` 共享

**moltis**（强类型消息、熔断器、secrecy）：
- `moltis/src/chat/message.rs` — `enum ChatMessage { User | Assistant | Tool | System }`
- `moltis/src/provider/error.rs` — `ProviderErrorKind` + `should_failover()` 实现
- `moltis/src/config.rs` — `secrecy::Secret<String>` API key 处理

**openfang**（Hands 能力包、Merkle 审计链）：
- `openfang/src/hands/mod.rs` — `HandDefinition` + `HandRequirement` 定义
- `openfang/src/audit/chain.rs` — `AuditLog` Merkle 链（`prev_hash: [u8;32]`）
- `openfang/src/safety/taint.rs` — taint tracking 实现

**nanoclaw**（轻量精简实现，验证最小 harness 边界）：
- `nanoclaw/src/agent.rs` — 最小可行 agentic loop
- `nanoclaw/src/tool.rs` — 轻量 Tool trait（对比 ironclaw 的完整版）

**nanobot**（错误不持久化原则）：
- `nanobot/src/agent/loop_.rs` — LLM 错误直接返回，不写入 thread 历史
- `nanobot/src/provider/mod.rs` — Provider error 分类处理
