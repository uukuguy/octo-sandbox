# OpenFang 架构整合路线图

**项目**: octo-sandbox (含 octo-workbench + octo-platform)
**创建日期**: 2026-02-27
**更新日期**: 2026-02-27 (代码级深度分析完成)
**目标**: 系统性引入 OpenFang 高价值架构设计

---

## 1. OpenFang 项目概览

### 1.1 项目规模 (实测)

- **代码规模**: 137K+ LOC (Rust)
- **Crate 数量**: 14 个，各司其职
- **测试**: 1,767+ 测试通过，Zero clippy warnings
- **二进制**: 32MB 单文件
- **版本**: v0.1.0 (2026-02 首次发布)

**各 Crate 实测 LOC**:

| Crate | LOC | 文件数 | 核心职责 |
|-------|-----|--------|----------|
| openfang-kernel | ~13,300 | 22 | 核心编排，含 39 字段 Kernel struct |
| openfang-runtime | ~32,400 | 51 | Agent Loop + 26 LLM Provider + 工具安全 |
| openfang-types | ~8,000 | 17 | 基础类型定义，所有 crate 的合约 |
| openfang-memory | ~3,500 | 8 | 三合一存储：KV + 语义 + 知识图谱 |
| openfang-api | ~5,000 | 8 | Axum HTTP/WS，50+ 端点，GCRA 限流 |
| openfang-wire | ~1,700 | 4 | OFP P2P 协议：JSON-RPC over TCP |
| openfang-channels | ~4,000 | 40+ | 40 个通信适配器，Router binding 系统 |
| openfang-hands | ~2,500 | 7 | 7 个自治 Agent 包，HAND.toml manifest |
| openfang-skills | ~3,000 | 5 | 60+ 技能库，安全扫描，热重载 |
| openfang-extensions | ~3,500 | 8 | 25 MCP 模板，AES-256-GCM 凭据保管 |
| openfang-desktop | ~1,500 | 7 | Tauri 2.0，内嵌 HTTP 服务器 |
| openfang-cli | ~4,000 | 20+ | 18 屏 Ratatui TUI + 命令行工具 |
| openfang-migrate | ~500 | - | 迁移工具 |

---

## 2. 核心组件深度分析 (代码级)

### 2.1 Kernel 编排器 (openfang-kernel/kernel.rs, 4,990 行)

**实际结构** — 39 个字段，每个管理一个子系统：

```rust
pub struct OpenFangKernel {
    pub config: KernelConfig,
    pub registry: AgentRegistry,              // DashMap 三索引
    pub capabilities: CapabilityManager,      // Capability-based RBAC
    pub event_bus: EventBus,                  // broadcast + per-agent + ring buffer
    pub scheduler: AgentScheduler,            // 滚动小时窗口资源配额
    pub memory: Arc<MemorySubstrate>,         // SQLite 三合一存储
    pub supervisor: Supervisor,               // watch::Channel 广播关机
    pub workflows: WorkflowEngine,            // Sequential/FanOut/Loop/Conditional
    pub triggers: TriggerEngine,              // 9 种事件模式匹配
    pub background: BackgroundExecutor,       // 4 种自治调度模式
    pub audit_log: Arc<AuditLog>,             // Merkle 哈希链
    pub metering: Arc<MeteringEngine>,        // SQLite 时/日/月成本窗口
    pub default_driver: Arc<dyn LlmDriver>,   // 运行时可切换
    pub wasm_sandbox: WasmSandbox,            // wasmtime 41
    pub auth: AuthManager,                    // RBAC 用户管理
    pub model_catalog: RwLock<ModelCatalog>,  // 26 个 Provider 注册
    pub skill_registry: RwLock<SkillRegistry>,
    pub running_tasks: DashMap<AgentId, AbortHandle>,
    pub mcp_connections: Mutex<Vec<McpConnection>>,
    pub mcp_tools: Mutex<Vec<ToolDefinition>>,
    pub a2a_task_store: A2aTaskStore,
    pub web_ctx: WebToolsContext,             // SSRF 防护
    pub browser_ctx: BrowserManager,
    pub media_engine: MediaEngine,
    pub tts_engine: TtsEngine,
    pub pairing: PairingManager,
    pub embedding_driver: Option<Arc<dyn EmbeddingDriver>>,
    pub hand_registry: HandRegistry,
    pub extension_registry: RwLock<IntegrationRegistry>,
    pub extension_health: HealthMonitor,
    pub delivery_tracker: DeliveryTracker,    // LRU 10,000 条
    pub cron_scheduler: CronScheduler,
    pub approval_manager: ApprovalManager,
    pub bindings: Mutex<Vec<AgentBinding>>,
    pub auto_reply_engine: AutoReplyEngine,
    pub hooks: HookRegistry,
    pub process_manager: Arc<ProcessManager>,
    pub peer_registry: Option<PeerRegistry>,
    pub peer_node: Option<Arc<PeerNode>>,
    pub booted_at: Instant,
    self_handle: OnceLock<Weak<Arc<Self>>>,   // 弱引用避免循环
}
```

**启动序列** (boot_with_config, 22 步):
1. Validate & clamp config
2. 创建数据目录
3. 打开 SQLite (WAL mode + 5s busy timeout)
4. 创建 Primary LLM Driver (从环境变量读 API key)
5. 可选 FallbackDriver 链
6. MeteringEngine 初始化
7. Supervisor (watch channel)
8. BackgroundExecutor
9. WASM Sandbox
10. RBAC AuthManager
11. Model Catalog (探测所有 Provider 可用性)
12. Skills Registry
13. Hand Registry
14. Extension Registry (25 bundled MCP 模板)
15. 合并 MCP 配置 (去重)
16. Web Context (搜索 + fetch + SSRF 防护)
17. Auto-detect Embedding Driver
18. Browser Manager
19. Media/TTS Engine
20. Device Pairing
21. 其余子系统 (EventBus, Registry, Triggers, Cron, Approval...)
22. OFP 网络节点 (可选)

---

### 2.2 AgentRegistry (registry.rs, 346 行)

**DashMap 三索引，O(1) 查找**：

```rust
pub struct AgentRegistry {
    agents: DashMap<AgentId, AgentEntry>,      // 主索引: ID → Entry
    name_index: DashMap<String, AgentId>,      // 名称索引
    tag_index: DashMap<String, Vec<AgentId>>,  // 标签索引
}

impl AgentRegistry {
    pub fn register(&self, entry: AgentEntry) -> OpenFangResult<()>
    pub fn get(&self, id: AgentId) -> Option<AgentEntry>
    pub fn find_by_name(&self, name: &str) -> Option<AgentEntry>
    pub fn find_by_tag(&self, tag: &str) -> Vec<AgentId>
    pub fn set_state(&self, id: AgentId, state: AgentState) -> OpenFangResult<()>
    pub fn set_mode(&self, id: AgentId, mode: AgentMode)
    pub fn update_model(&self, id: AgentId, model: String)
    pub fn update_system_prompt(&self, id: AgentId, prompt: String)
    pub fn update_skills(&self, id: AgentId, skills: Vec<String>)
    pub fn add_child(&self, parent: AgentId, child: AgentId)
    pub fn list_all(&self) -> Vec<AgentEntry>
}
```

**Agent 生命周期**: `Created → Running → Suspended | Crashed | Terminated`

**Agent 工作区目录** (spawn 时自动创建):
```
~/.openfang/agents/{agent-id}/
├── SOUL.md        # 核心使命/价值观 (一次性生成)
├── USER.md        # 用户画像 (随对话更新)
├── TOOLS.md       # 环境注记
├── MEMORY.md      # 长期知识库
├── AGENTS.md      # 行为指南
├── BOOTSTRAP.md   # 首次运行协议
├── IDENTITY.md    # 视觉身份 (emoji, 颜色)
├── HEARTBEAT.md   # 自治提醒
├── data/ output/ sessions/ skills/ logs/
```

---

### 2.3 EventBus (event_bus.rs, 149 行)

```rust
const HISTORY_SIZE: usize = 1000;

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    agent_channels: DashMap<AgentId, broadcast::Sender<Event>>,  // 懒初始化
    history: Arc<RwLock<VecDeque<Event>>>,
}

impl EventBus {
    pub async fn publish(&self, event: Event)          // 存 history + 按 target 路由
    pub fn subscribe_agent(&self, id: AgentId) -> broadcast::Receiver<Event>
    pub fn subscribe_all(&self) -> broadcast::Receiver<Event>
    pub async fn history(&self, limit: usize) -> Vec<Event>
}
```

**路由规则**:
- `Agent(id)` → 写入该 agent 专属 channel
- `Broadcast` → 全局 + 所有 agent channel
- `Pattern(p)` → 全局广播 (TriggerEngine 负责匹配)
- `System` → 全局 channel only

**EventPayload 类型**: Message / ToolResult / MemoryUpdate / Lifecycle / Network / System / Custom

---

### 2.4 AgentScheduler (scheduler.rs, 168 行)

**滚动小时窗口资源配额**：

```rust
pub struct UsageTracker {
    total_tokens: u64,
    tool_calls: u64,
    window_start: Instant,  // 超过 3600s 自动重置
}
```

**ResourceQuota 默认值**:
- `max_llm_tokens_per_hour`: 1,000,000
- `max_tool_calls_per_hour`: 60
- `max_cost_per_hour_usd`: $1.00
- `max_memory_bytes`: 256 MB

---

### 2.5 Supervisor (supervisor.rs, 227 行)

**watch::Channel 广播关机 + 健康监控**：

```rust
pub struct Supervisor {
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    restart_count: AtomicU64,
    panic_count: AtomicU64,
    agent_restarts: DashMap<AgentId, u32>,
}

impl Supervisor {
    pub fn shutdown(&self)                            // 广播给所有监听者
    pub fn is_shutting_down(&self) -> bool
    pub fn record_agent_restart(&self, id: AgentId)  // 超过 max_restarts → Err
    pub fn subscribe(&self) -> watch::Receiver<bool>
}
```

---

### 2.6 MeteringEngine (metering.rs, 692 行)

**SQLite 时间窗口成本追踪**：

```rust
impl MeteringEngine {
    pub fn query_hourly(&self, agent_id: AgentId) -> OpenFangResult<f64>
    pub fn query_daily(&self, agent_id: AgentId) -> OpenFangResult<f64>
    pub fn query_monthly(&self, agent_id: AgentId) -> OpenFangResult<f64>
    pub fn query_global_hourly(&self) -> OpenFangResult<f64>
    pub fn check_quota(&self, agent_id: AgentId, quota: &ResourceQuota) -> OpenFangResult<()>
    pub fn check_global_budget(&self, budget: &GlobalBudget) -> OpenFangResult<()>
}
```

---

### 2.7 TriggerEngine (triggers.rs, 511 行)

**9 种事件模式 → 唤醒休眠 Agent**：

```rust
pub enum TriggerPattern {
    Lifecycle,
    AgentSpawned { name_pattern: String },
    AgentTerminated,
    System,
    SystemKeyword { keyword: String },
    MemoryUpdate,
    MemoryKeyPattern { key_pattern: String },
    All,
    ContentMatch { substring: String },
}

pub struct Trigger {
    pub id: TriggerId,
    pub agent_id: AgentId,
    pub pattern: TriggerPattern,
    pub prompt_template: String,   // 支持 {{event}} 变量替换
    pub enabled: bool,
    pub fire_count: u64,
    pub max_fires: u64,            // 0 = 无限
}
```

---

### 2.8 WorkflowEngine (workflow.rs, 1,367 行)

**DAG 多步骤 Pipeline**：

```rust
pub struct WorkflowStep {
    pub name: String,
    pub agent: StepAgent,
    pub prompt_template: String,    // 支持 {{input}}, {{var_name}}
    pub mode: StepMode,
    pub timeout_secs: u64,
    pub error_mode: ErrorMode,
    pub output_var: Option<String>, // 捕获输出供后续步骤使用
}

pub enum StepMode {
    Sequential,
    FanOut,
    Collect,
    Conditional { condition: String },
    Loop { max_iterations: u32, until: String },
}

pub enum ErrorMode {
    Fail,
    Skip,
    Retry { max_retries: u32 },
}
```

---

### 2.9 BackgroundExecutor (background.rs, 457 行)

**4 种自治调度模式**：

```rust
pub enum ScheduleMode {
    Reactive,
    Continuous { check_interval_secs: u64 },
    Periodic { cron: String },
    Proactive { conditions: Vec<String> },
}
```

- `MAX_CONCURRENT_BG_LLM = 5` 全局 semaphore 防速率限制
- Continuous 模式发送 `[AUTONOMOUS TICK]` 自我提示

---

### 2.10 Agent Loop (openfang-runtime/agent_loop.rs)

**完整执行流程** (每轮迭代，最多 50 次):

```
1. 回忆记忆 (向量搜索 → LIKE fallback)
2. 构建 Prompt (system + identity files + 记忆 + BeforePromptBuild hook)
3. Context Guard (工具结果超 75% headroom 时压缩)
4. 调用 LLM (指数退避重试: 3次, 1s→60s, 20% jitter)
5. 处理 tool_use:
   - Loop Guard (hash重复@3warn/@5block, 全局断路器@30)
   - BeforeToolCall hook (可阻断)
   - Taint Tracking (shell注入 + 数据外泄检测)
   - 120s 超时执行
   - AfterToolCall hook
   - 动态截断 (≤30% context window)
6. end_turn → 持久化会话 + 更新记忆
```

**Context Overflow 4 阶段恢复** (context_overflow.rs, 120 行):
1. ≤70% → 无操作
2. 70-90% → 保留最后 10 条 (AutoCompaction)
3. >90% → 保留最后 4 条 (OverflowCompaction)
4. 仍超限 → 工具结果截至 2K (ToolResultTruncation)
5. 仍超限 → 返回错误，建议 /reset

---

### 2.11 LLM Driver 抽象 (26 providers)

**8 类错误分类** (llm_errors.rs, 770 行):

```rust
pub enum LlmErrorCategory {
    RateLimit,        // 429 → 可重试，尊重 Retry-After
    Overloaded,       // 503/529 → 可重试，指数退避
    Timeout,          // 超时 → 可重试
    Billing,          // 402 → 不可重试
    Auth,             // 401/403 → 不可重试
    ContextOverflow,  // context_length_exceeded → 不可重试
    Format,           // 400 → 不可重试
    ModelNotFound,    //  → 不可重试
}
```

26 个 Provider 中约 20 个复用 OpenAI 兼容格式（同一驱动 + base_url 参数覆盖）。

---

### 2.12 Memory Substrate (openfang-memory, 7 张表)

**SQLite Schema v7**:

| 表 | 序列化 | 说明 |
|----|--------|------|
| `agents` | MessagePack (named) | Agent manifest + state |
| `sessions` | MessagePack | 会话消息历史 |
| `memories` | TEXT + f32 BLOB | 语义记忆 + 嵌入向量 |
| `entities` + `relations` | JSON | 知识图谱 |
| `kv_store` | JSON BLOB | Agent 作用域 KV |
| `canonical_sessions` | MessagePack | 跨通道统一记忆 |
| `usage_events` | - | 成本追踪 |

**混合搜索策略**: 有 embedding → cosine similarity；无 embedding → `content LIKE %query%`

**记忆衰减**: 7 天未访问 → `confidence *= (1 - decay_rate)`，最低 0.1

---

### 2.13 Capability 安全模型 (capability.rs, 317 行)

**细粒度能力声明** (glob 模式匹配):

```rust
pub enum Capability {
    FileRead(String),      // glob: "*.log", "/data/**"
    FileWrite(String),
    NetConnect(String),    // 主机: "api.*.com:443"
    ToolInvoke(String),
    ToolAll,               // 危险
    AgentSpawn,
    AgentMessage(String),
    MemoryRead(String),    // 作用域: "self.*"
    MemoryWrite(String),
    ShellExec(String),     // 命令: "git *"
    EconSpend(f64),        // 美元预算
    OfpDiscover,
    OfpConnect(String),
}

// 子 Agent 不能拥有父 Agent 没有的能力
pub fn validate_capability_inheritance(parent: &[Capability], child: &[Capability]) -> Result<()>
```

---

### 2.14 Taint Tracking (taint.rs, 245 行)

**信息流安全**，防止 prompt injection 和数据外泄：

```rust
pub enum TaintLabel {
    ExternalNetwork,   // 来自 HTTP 请求
    UserInput,
    Pii,
    Secret,            // API key, token
    UntrustedAgent,
}

// 预定义 Sink
TaintSink::shell_exec()     // 阻止 ExternalNetwork, UntrustedAgent, UserInput
TaintSink::net_fetch()      // 阻止 Secret, Pii
TaintSink::agent_message()  // 阻止 Secret
```

---

### 2.15 安全系统全景 (16 层)

| # | 系统 | 实现位置 | 机制 |
|---|------|---------|------|
| 1 | WASM 双计量沙盒 | openfang-runtime | wasmtime + fuel metering |
| 2 | Merkle 哈希链审计 | openfang-kernel/kernel.rs | 不可篡改操作链 |
| 3 | 污点追踪 | openfang-types/taint.rs | 5 种标签 + Sink 检查点 |
| 4 | Ed25519 签名 | openfang-types/manifest_signing.rs | 167 行，SHA-256 + 签名 |
| 5 | SSRF 保护 | openfang-runtime (web) | 阻止私有 IP |
| 6 | 密钥零化 | openfang-extensions/vault.rs | Zeroizing<String> |
| 7 | OFP 双向认证 | openfang-wire/peer.rs | HMAC-SHA256 + 恒定时间比较 |
| 8 | 能力门控 | openfang-types/capability.rs | Glob 匹配 + 继承校验 |
| 9 | 安全头 | openfang-api | CSP, HSTS |
| 10 | 循环保护 | openfang-runtime/loop_guard.rs | SHA256 工具调用指纹 |
| 11 | 会话修复 | openfang-runtime/session_repair.rs | 7 阶段消息验证 |
| 12 | 路径遍历防护 | openfang-runtime (file tools) | 规范化 + 符号链接检查 |
| 13 | GCRA 限速 | openfang-api/rate_limiter.rs | Token Bucket |
| 14 | 提示注入扫描 | openfang-skills (verifier) | 危险模式检测 |
| 15 | 子进程沙盒 | openfang-skills/executor.rs | env_clear() 隔离 |
| 16 | 健康端点精简 | openfang-api | 最小公共信息 |

---

### 2.16 Channels (40 适配器)

| 类别 | 适配器 |
|------|--------|
| 核心 | Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Email |
| 企业 | Teams, Mattermost, Google Chat, Webex, Feishu, Zulip |
| 社交 | LINE, Viber, Facebook Messenger, Mastodon, Bluesky, Reddit, LinkedIn |
| 隐私 | Threema, Nostr, Rocket.Chat |

---

### 2.17 Hands (7 自主 Agent)

| Hand | 功能 |
|------|------|
| Clip | YouTube 视频下载 → 短片段 + 字幕 |
| Lead | 每日潜在客户发现 + 丰富 |
| Collector | OSINT 级情报监控 |
| Predictor | 超级预测 (置信区间) |
| Researcher | 深度自主研究 (引用) |
| Twitter | 自主 Twitter 账号管理 |
| Browser | Web 自动化 (购买审批) |

---

### 2.18 Extensions 系统 (openfang-extensions)

**25 个 bundled MCP 模板** (编译时 include_str! 嵌入):

| 类别 | 集成 |
|------|------|
| DevTools | GitHub, GitLab, Linear, Jira, Bitbucket, Sentry |
| Productivity | Google Calendar, Gmail, Notion, Todoist, Google Drive, Dropbox |
| Communication | Slack, Discord, Teams |
| Data | PostgreSQL, SQLite, MongoDB, Redis, Elasticsearch |
| Cloud | AWS, GCP, Azure |
| AI & Search | Brave Search, Exa Search |

**凭据保管**: AES-256-GCM + Argon2 密钥派生 + Zeroizing<String>

**OAuth2 PKCE**: Google/GitHub/Microsoft/Slack，本地随机端口监听，5 分钟超时

---

## 3. octo-sandbox 当前架构对比

### 3.1 项目规模 (Phase 2.3 完成时)

- **Rust 文件**: 53+
- **Crate**: 3 (octo-types, octo-engine, octo-server)
- **前端**: React 19 + TypeScript

### 3.2 已实现 vs OpenFang 差距

| 模块 | octo 状态 | OpenFang 对应 | 差距 |
|------|-----------|--------------|------|
| Agent Loop | ✅ 50轮, 工具执行 | agent_loop.rs | 缺 Loop Guard, Taint, 4-stage overflow |
| Context Budget | ✅ 双轨估算+3级降级 | context_budget.rs | 缺第4阶段截断 |
| LLM Providers | ✅ Anthropic + OpenAI | 26 providers | 差 24 个，但 OpenAI compat 可快速扩展 |
| Memory | ✅ 三层 W/S/P + FTS5 | substrate.rs | 缺知识图谱、记忆衰减、Canonical Sessions |
| Skills | ✅ SKILL.md + 热重载 | openfang-skills | 缺安全扫描、Python/Node runtime |
| MCP | ✅ stdio transport | mcp.rs | 缺 SSE transport、完整重试 |
| API | ✅ REST + WebSocket | openfang-api | 缺 GCRA 限流、140+ 端点 |
| EventBus | ❌ 无 | event_bus.rs | 完全缺失，Phase 2.4 优先 |
| AgentRegistry | ❌ 无 | registry.rs | 单 Agent，Phase 3 核心需求 |
| Supervisor | ❌ 无 | supervisor.rs | 缺优雅关机 + 健康监控 |
| MeteringEngine | ❌ 无 | metering.rs | 缺成本追踪，平台化必需 |
| WorkflowEngine | ❌ 无 | workflow.rs | 多 Agent 编排，Phase 3 |
| TriggerEngine | ❌ 无 | triggers.rs | 事件驱动 Agent 唤醒，Phase 3 |
| BackgroundExecutor | ❌ 无 | background.rs | 自治调度，Phase 3 |
| Loop Guard | ❌ 无 | loop_guard.rs | 防无限循环，Phase 2.4 立即需要 |
| Error Classification | ❌ 无 | llm_errors.rs | 8类错误分类，Phase 2.4 |
| Capability Security | ❌ 无 | capability.rs | 细粒度权限，Phase 3 |
| Taint Tracking | ❌ 无 | taint.rs | 信息流安全，Phase 3 |
| Channels | ❌ 无 | 40 adapters | 长期目标 |
| Hands | ❌ 无 | 7 autonomous | 长期目标 |

---

## 4. 整合路线图

### 4.1 Phase 2.4 — 立即可移植 (代码量小，价值大)

| 模块 | 优先级 | 价值 | LOC | 参考文件 |
|------|--------|------|-----|---------|
| Loop Guard | P0 | ⭐⭐⭐⭐⭐ | ~100 | openfang-runtime/src/loop_guard.rs |
| Error Classification | P0 | ⭐⭐⭐⭐⭐ | 770 | openfang-runtime/src/llm_errors.rs |
| EventBus | P0 | ⭐⭐⭐⭐⭐ | 149 | openfang-kernel/src/event_bus.rs |
| Context Overflow 4-stage | P1 | ⭐⭐⭐⭐⭐ | 120 | openfang-runtime/src/context_overflow.rs |
| Retry Config | P1 | ⭐⭐⭐⭐ | 514 | openfang-runtime/src/retry.rs |
| Ed25519 Manifest Signing | P2 | ⭐⭐⭐ | 167 | openfang-types/src/manifest_signing.rs |

### 4.2 Phase 3 (octo-platform) — 平台化核心

| 模块 | 优先级 | 价值 | LOC | 参考文件 |
|------|--------|------|-----|---------|
| AgentRegistry | P0 | ⭐⭐⭐⭐⭐ | 346 | openfang-kernel/src/registry.rs |
| MeteringEngine | P0 | ⭐⭐⭐⭐⭐ | 692 | openfang-kernel/src/metering.rs |
| RBAC AuthManager | P0 | ⭐⭐⭐⭐⭐ | 316 | openfang-kernel/src/auth.rs |
| Supervisor | P0 | ⭐⭐⭐⭐ | 227 | openfang-kernel/src/supervisor.rs |
| TriggerEngine | P1 | ⭐⭐⭐⭐ | 511 | openfang-kernel/src/triggers.rs |
| WorkflowEngine | P1 | ⭐⭐⭐⭐ | 1,367 | openfang-kernel/src/workflow.rs |
| BackgroundExecutor | P1 | ⭐⭐⭐⭐ | 457 | openfang-kernel/src/background.rs |
| Knowledge Graph | P1 | ⭐⭐⭐⭐ | ~500 | openfang-memory/src/knowledge.rs |
| Capability Security | P2 | ⭐⭐⭐⭐ | 317 | openfang-types/src/capability.rs |
| Taint Tracking | P2 | ⭐⭐⭐ | 245 | openfang-types/src/taint.rs |
| ApprovalManager | P2 | ⭐⭐⭐ | 403 | openfang-kernel/src/approval.rs |
| Multi-Provider (26) | P2 | ⭐⭐⭐ | ~8,000 | openfang-runtime/src/drivers/ |

### 4.3 长期目标

| 模块 | 优先级 | 价值 | 难度 |
|------|--------|------|------|
| Channels (40 适配器) | P3 | ⭐⭐⭐⭐⭐ | 高 |
| Hands (7 自治 Agent) | P3 | ⭐⭐⭐⭐ | 中 |
| Extensions (25 MCP) | P3 | ⭐⭐⭐ | 中 |
| OFP Wire Protocol | P3 | ⭐⭐⭐ | 高 |

---

## 5. 详细实施计划

### 5.1 Phase 2.4: Loop Guard

**目标**: 防止 Agent Loop 陷入无限工具调用

**核心逻辑**:
```
fingerprint = sha256(tool_name + json(params))
- warn at 3 identical calls
- block at 5 identical calls
- global_circuit_breaker at 30 total tool calls
- ping-pong detection: A-B-A alternation >= 3 times
- Verdict: Allow | Warn(msg) | Block(msg) | CircuitBreak(msg)
```

**octo 实施位置**: `crates/octo-engine/src/agent/loop_.rs`

**任务**:
1. [ ] 定义 LoopGuard 结构 + Verdict 枚举
2. [ ] 实现 hash-based 重复检测
3. [ ] 实现 ping-pong 检测
4. [ ] 集成到 agent loop 工具执行前
5. [ ] 在工具结果中注入 warning 信息

---

### 5.2 Phase 2.4: Error Classification

**目标**: 系统化 LLM 错误处理，正确判断可重试性

**8 类错误**: RateLimit (可重试) / Overloaded (可重试) / Timeout (可重试) / Billing (不重试) / Auth (不重试) / ContextOverflow (不重试) / Format (不重试) / ModelNotFound (不重试)

**octo 实施位置**: `crates/octo-engine/src/providers/`

**任务**:
1. [ ] 定义 LlmErrorCategory 枚举
2. [ ] 实现 classify(status_code, body) -> LlmErrorCategory
3. [ ] 按 Provider 添加特殊模式 (Anthropic 529 等)
4. [ ] 集成到 retry.rs 的 should_retry 判断

---

### 5.3 Phase 2.4: EventBus

**目标**: 事件驱动架构基础，为 Phase 3 多 Agent 通信打基础

**参考**: `github.com/openfang/crates/openfang-kernel/src/event_bus.rs`

**octo 实施位置**: `crates/octo-engine/src/event_bus.rs` (新建)

**任务**:
1. [ ] 定义 Event, EventTarget, EventPayload 类型
2. [ ] 实现 EventBus (broadcast + DashMap + VecDeque)
3. [ ] publish(), subscribe_all(), history() 方法
4. [ ] 集成到 Agent Loop (发布工具执行事件)
5. [ ] WebSocket 事件推送到前端
6. [ ] REST API: GET /api/events/history

---

### 5.4 Phase 2.4: Context Overflow 4-stage

**目标**: 比现有 3 级降级更完善的 context 管理

**与 octo 现有的差异**:
- octo 当前: trim → aggressive_trim → error (3 级)
- OpenFang: None → AutoCompaction(10条) → OverflowCompaction(4条) → ToolResultTruncation(2K) → FinalError (5 级)

**octo 实施位置**: `crates/octo-engine/src/context/pruner.rs`

**任务**:
1. [ ] 增加第 4 阶段: 工具结果截至 2K
2. [ ] 引入 OverflowStage 枚举
3. [ ] 在 Debug UI 展示当前阶段

---

### 5.5 Phase 3: AgentRegistry

**参考**: `github.com/openfang/crates/openfang-kernel/src/registry.rs`

**octo 实施位置**: `crates/octo-engine/src/registry.rs` (新建)

**任务**:
1. [ ] 设计 AgentEntry (id, name, manifest, state, mode, tags)
2. [ ] 实现 AgentRegistry (DashMap 三索引: ID/Name/Tag)
3. [ ] Agent 工作区目录结构
4. [ ] 持久化到 SQLite (新建 agents 表)
5. [ ] REST API: CRUD + 状态管理端点
6. [ ] 集成 Supervisor + MeteringEngine

---

### 5.6 Phase 3: WorkflowEngine

**参考**: `github.com/openfang/crates/openfang-kernel/src/workflow.rs`

**octo 实施位置**: `crates/octo-engine/src/workflow.rs` (新建)

**任务**:
1. [ ] 定义 WorkflowStep, StepMode, ErrorMode 类型
2. [ ] 实现 Sequential + FanOut + Collect
3. [ ] 变量捕获 (output_var) + 模板替换 ({{var}})
4. [ ] Conditional + Loop 模式
5. [ ] REST API: 创建/运行/状态查询
6. [ ] 前端: Workflow 编辑器

---

## 6. 整合追踪

### 6.1 CHECKPOINT_PLAN 里程碑

**Phase 2.4 目标**:

| 模块 | 优先级 | 状态 | 参考 |
|------|--------|------|------|
| Loop Guard | P0 | ⏳ | openfang-runtime/src/loop_guard.rs |
| Error Classification | P0 | ⏳ | openfang-runtime/src/llm_errors.rs |
| EventBus | P0 | ⏳ | openfang-kernel/src/event_bus.rs |
| Context Overflow 4-stage | P1 | ⏳ | openfang-runtime/src/context_overflow.rs |
| Retry Config | P1 | ⏳ | openfang-runtime/src/retry.rs |

**Phase 3 目标**:

| 模块 | 优先级 | 状态 | 参考 |
|------|--------|------|------|
| AgentRegistry | P0 | ⏳ | openfang-kernel/src/registry.rs |
| MeteringEngine | P0 | ⏳ | openfang-kernel/src/metering.rs |
| RBAC AuthManager | P0 | ⏳ | openfang-kernel/src/auth.rs |
| Supervisor | P0 | ⏳ | openfang-kernel/src/supervisor.rs |
| TriggerEngine | P1 | ⏳ | openfang-kernel/src/triggers.rs |
| WorkflowEngine | P1 | ⏳ | openfang-kernel/src/workflow.rs |
| BackgroundExecutor | P1 | ⏳ | openfang-kernel/src/background.rs |
| Knowledge Graph | P1 | ⏳ | openfang-memory/src/knowledge.rs |
| Capability Security | P2 | ⏳ | openfang-types/src/capability.rs |
| Taint Tracking | P2 | ⏳ | openfang-types/src/taint.rs |

**长期**:

| 模块 | 优先级 | 状态 |
|------|--------|------|
| Channels (40 适配器) | P3 | ⏳ |
| Hands (7 自治 Agent) | P3 | ⏳ |
| Multi-Provider (26) | P3 | ⏳ |
| OFP Wire Protocol | P3 | ⏳ |

---

### 6.2 Phase 启动检查

每次 /start-phase 时:
1. 读取 CHECKPOINT_PLAN 的 OpenFang 里程碑
2. 识别与当前 Phase 相关的 P0/P1 模块
3. 询问用户是否在本 Phase 引入哪些模块

---

## 7. 关键代码索引

| 模块 | OpenFang 路径 | LOC | octo 路径 |
|------|---------------|-----|-----------|
| Kernel struct | openfang-kernel/src/kernel.rs | 4,990 | - |
| AgentRegistry | openfang-kernel/src/registry.rs | 346 | - |
| EventBus | openfang-kernel/src/event_bus.rs | 149 | - |
| Supervisor | openfang-kernel/src/supervisor.rs | 227 | - |
| Scheduler | openfang-kernel/src/scheduler.rs | 168 | - |
| MeteringEngine | openfang-kernel/src/metering.rs | 692 | - |
| TriggerEngine | openfang-kernel/src/triggers.rs | 511 | - |
| WorkflowEngine | openfang-kernel/src/workflow.rs | 1,367 | - |
| BackgroundExecutor | openfang-kernel/src/background.rs | 457 | - |
| Agent Loop | openfang-runtime/src/agent_loop.rs | - | loop_.rs |
| Loop Guard | openfang-runtime/src/loop_guard.rs | ~100 | - |
| Error Classification | openfang-runtime/src/llm_errors.rs | 770 | - |
| Context Budget | openfang-runtime/src/context_budget.rs | 276 | context/budget.rs |
| Context Overflow | openfang-runtime/src/context_overflow.rs | 120 | context/pruner.rs |
| Retry | openfang-runtime/src/retry.rs | 514 | - |
| LLM Drivers (26) | openfang-runtime/src/drivers/ | ~8,000 | providers/ (2) |
| MCP Client | openfang-runtime/src/mcp.rs | - | mcp/mod.rs (stdio only) |
| Taint Tracking | openfang-types/src/taint.rs | 245 | - |
| Capability | openfang-types/src/capability.rs | 317 | - |
| Manifest Signing | openfang-types/src/manifest_signing.rs | 167 | - |
| Memory Substrate | openfang-memory/src/substrate.rs | - | memory/ |
| Knowledge Graph | openfang-memory/src/knowledge.rs | ~500 | - |
| API Server | openfang-api/src/server.rs | - | server/ |

---

## 8. 总结

OpenFang 是一个**生产级 Agent OS** (137K LOC, 14 crates, 1,767+ 测试)。代码级分析揭示的最高价值借鉴点：

### 对 octo-workbench (当前) — 立即可行

1. **Loop Guard** (~100 行) — 防无限工具调用，当前 octo 完全缺失
2. **Error Classification** (770 行) — 8 类 LLM 错误 + 可重试性判断
3. **EventBus** (149 行) — broadcast + ring buffer，为 Phase 3 打基础
4. **Context Overflow 4-stage** (120 行) — 比现有 3 级更完善

### 对 octo-platform (Phase 3) — 平台化核心

5. **AgentRegistry** (346 行) — DashMap 三索引，多 Agent 基础
6. **Supervisor** (227 行) — watch::Channel 优雅关机
7. **MeteringEngine** (692 行) — 成本追踪，多用户必需
8. **WorkflowEngine** (1,367 行) — 多 Agent 编排，差异化竞争力

### 策略

- **移植模式而非代码**: 理解设计意图，按 octo 需求简化
- **小步快跑**: 每 Phase 检查里程碑，决定引入哪些模块
- **DashMap 优先**: 注册表类数据结构优先 DashMap，替代 RwLock
- **watch::Channel**: 关机信号广播最佳实践
