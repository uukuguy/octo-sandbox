# OpenFang 架构整合路线图

**项目**: octo-sandbox (含 octo-workbench + octo-platform)
**创建日期**: 2026-02-27
**目标**: 系统性引入 OpenFang 高价值架构设计

---

## 1. OpenFang 项目概览

### 1.1 项目规模

- **代码规模**: 137K+ LOC (Rust)
- **测试**: 1,767+ 测试通过
- **警告**: Zero clippy warnings
- **二进制**: 32MB 单文件
- **版本**: v0.1.0 (2026-02 首次发布)

### 1.2 14 个 Crate 模块

| Crate | 功能 | 核心组件 | 代码路径 |
|-------|------|----------|----------|
| openfang-kernel | 核心编排引擎 | Kernel, AgentRegistry, EventBus, Scheduler, WorkflowEngine, Triggers | `crates/openfang-kernel/src/` |
| openfang-runtime | Agent 运行时 | AgentLoop, MCP, LLM Drivers (27 providers), Sandbox | `crates/openfang-runtime/src/` |
| openfang-api | REST API | 140+ 端点, Axum, WebSocket, SSE | `crates/openfang-api/src/` |
| openfang-memory | 记忆系统 | Structured, Semantic, Knowledge Graph | `crates/openfang-memory/src/` |
| openfang-types | 类型定义 | Agent, Tool, Memory, Config | `crates/openfang-types/src/` |
| openfang-skills | Skill 插件 | 60+ Skills, SKILL.md 解析 | `crates/openfang-skills/src/` |
| openfang-channels | 消息通道 | 40 适配器 (Telegram, Discord, Slack, etc.) | `crates/openfang-channels/src/` |
| openfang-hands | 自主 Agent | 7 Hands (Clip, Lead, Researcher, etc.) | `crates/openfang-hands/src/` |
| openfang-extensions | 扩展集成 | 25 MCP 模板, AES-256-GCM Vault | `crates/openfang-extensions/src/` |
| openfang-wire | P2P 协议 | OFP 协议, HMAC-SHA256 | `crates/openfang-wire/src/` |
| openfang-cli | CLI 工具 | Daemon, TUI Dashboard | `crates/openfang-cli/src/` |
| openfang-desktop | 桌面应用 | Tauri 2.0 | `crates/openfang-desktop/src/` |
| openfang-migrate | 迁移工具 | OpenClaw, LangChain 迁移 | `crates/openfang-migrate/src/` |

---

## 2. 核心组件深度分析

### 2.1 Kernel (openfang-kernel)

**职责**: 核心编排引擎，协调所有子系统

**核心组件**:

```rust
// github.com/openfang/crates/openfang-kernel/src/kernel.rs
pub struct OpenFangKernel {
    pub config: KernelConfig,
    pub registry: AgentRegistry,           // Agent 生命周期管理
    pub capabilities: CapabilityManager,   // RBAC 权限
    pub event_bus: EventBus,               // 事件发布订阅
    pub scheduler: AgentScheduler,         // Cron 定时调度
    pub memory: Arc<MemorySubstrate>,       // 记忆系统
    pub workflows: WorkflowEngine,          // 工作流引擎
    pub triggers: TriggerEngine,            // 事件触发器
    pub background: BackgroundExecutor,     // 后台执行器
    pub audit_log: Arc<AuditLog>,           // 审计日志
    pub metering: Arc<MeteringEngine>,     // 计费系统
    pub auth: AuthManager,                  // RBAC 认证
    // ... 30+ 组件
}
```

**关键模块**:

| 模块 | 文件 | 功能 |
|------|------|------|
| AgentRegistry | `registry.rs` | 多索引 Agent 管理 |
| EventBus | `event_bus.rs` | 发布订阅 + 历史缓冲 |
| Scheduler | `scheduler.rs` | Cron 定时任务 |
| WorkflowEngine | `workflow.rs` | 多步骤工作流 |
| Triggers | `triggers.rs` | 事件触发器 |

---

### 2.2 AgentRegistry 详解

**文件**: `crates/openfang-kernel/src/registry.rs`

```rust
pub struct AgentRegistry {
    agents: DashMap<AgentId, AgentEntry>,     // 主索引: ID -> Entry
    name_index: DashMap<String, AgentId>,     // 名称索引
    tag_index: DashMap<String, Vec<AgentId>>, // 标签索引
}

// 核心操作
impl AgentRegistry {
    pub fn register(&self, entry: AgentEntry) -> OpenFangResult<()>
    pub fn get(&self, id: AgentId) -> Option<AgentEntry>
    pub fn find_by_name(&self, name: &str) -> Option<AgentEntry>
    pub fn set_state(&self, id: AgentId, state: AgentState) -> OpenFangResult<()>
    pub fn find_by_tag(&self, tag: &str) -> Vec<AgentId>
    pub fn list_all(&self) -> Vec<AgentEntry>
}
```

**设计亮点**:
- DashMap 并发安全
- 多索引支持 (ID, Name, Tag)
- 实时状态更新

---

### 2.3 EventBus 详解

**文件**: `crates/openfang-kernel/src/event_bus.rs`

```rust
const HISTORY_SIZE: usize = 1000;

pub struct EventBus {
    sender: broadcast::Sender<Event>,                    // 全局广播
    agent_channels: DashMap<AgentId, broadcast::Sender<Event>>, // per-agent 通道
    history: Arc<RwLock<VecDeque<Event>>>,              // 历史 Ring Buffer
}

impl EventBus {
    pub async fn publish(&self, event: Event) {
        // 1. 存入历史
        // 2. 按目标路由 (Agent/Broadcast/System)
    }

    pub fn subscribe_agent(&self, agent_id: AgentId) -> broadcast::Receiver<Event>
    pub async fn history(&self, limit: usize) -> Vec<Event>
}
```

**设计亮点**:
- broadcast 通道高效
- 历史缓冲支持回溯
- 多目标路由

---

### 2.4 Agent Loop (openfang-runtime)

**文件**: `crates/openfang-runtime/src/agent_loop.rs`

```rust
const MAX_ITERATIONS: u32 = 50;
const TOOL_TIMEOUT_SECS: u64 = 120;
const DEFAULT_CONTEXT_WINDOW: usize = 200_000;

pub async fn run_agent_loop(
    manifest: &AgentManifest,
    user_message: &str,
    session: &mut Session,
    memory: &MemorySubstrate,
    driver: Arc<dyn LlmDriver>,
    available_tools: &[ToolDefinition],
    kernel: Option<Arc<dyn KernelHandle>>,
) -> OpenFangResult<AgentLoopResult>
```

**核心流程**:
1. Recall: 检索相关记忆
2. Build Prompt: 组装系统提示 + Skills + 记忆
3. Call LLM: 带工具调用
4. Execute Tools: 沙盒执行
5. Loop: 最多 50 次迭代
6. Save: 保存会话到记忆

---

### 2.5 MCP Client (openfang-runtime)

**文件**: `crates/openfang-runtime/src/mcp.rs`

```rust
// 配置
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    pub timeout_secs: u64,
    pub env: Vec<String>,
}

pub enum McpTransport {
    Stdio { command: String, args: Vec<String> },
    Sse { url: String },
}

// 连接
pub struct McpConnection {
    config: McpServerConfig,
    tools: Vec<ToolDefinition>,
    transport: McpTransportHandle,
    next_id: u64,
}

enum McpTransportHandle {
    Stdio { child, stdin, stdout },
    Sse { client, url },
}
```

**JSON-RPC 2.0 处理**:
```rust
struct JsonRpcRequest {
    jsonrpc: &'static str,  // "2.0"
    id: u64,
    method: String,
    params: Option<serde_json::Value>,
}
```

---

### 2.6 Memory Substrate (openfang-memory)

**文件**: `crates/openfang-memory/src/substrate.rs`

```rust
pub struct MemorySubstrate {
    conn: Arc<Mutex<Connection>>,
    structured: StructuredStore,   // SQLite
    semantic: SemanticStore,      // FTS5 / 向量
    knowledge: KnowledgeStore,    // 知识图谱
    sessions: SessionStore,      // 会话管理
    consolidation: ConsolidationEngine, // 记忆整合
    usage: UsageStore,            // 使用统计
}
```

**三层存储**:

| 层 | 存储 | 查询方式 |
|----|------|----------|
| Structured | SQLite | SQL |
| Semantic | FTS5/向量 | 相似度 |
| Knowledge | 图数据库 | 图遍历 |

**SQLite 配置**:
```rust
conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
```

---

### 2.7 API Server (openfang-api)

**文件**: `crates/openfang-api/src/server.rs`

```rust
pub async fn build_router(
    kernel: Arc<OpenFangKernel>,
    listen_addr: SocketAddr,
) -> (Router<()>, Arc<AppState>)
```

**140+ 端点分类**:

| 类别 | 数量 | 示例 |
|------|------|------|
| Agent | 20+ | `/api/agents`, `/api/agents/{id}/message` |
| Workflow | 10+ | `/api/workflows`, `/api/workflows/{id}/run` |
| Trigger | 5+ | `/api/triggers` |
| Channel | 15+ | `/api/channels`, `/api/channels/{name}/send` |
| Skill | 10+ | `/api/skills`, `/api/skills/install` |
| Hand | 20+ | `/api/hands`, `/api/hands/{id}/activate` |
| MCP | 5+ | `/api/mcp/servers` |
| Budget | 5+ | `/api/budget`, `/api/budget/agents` |

---

### 2.8 安全系统 (16 层)

| # | 系统 | 实现 |
|---|------|------|
| 1 | WASM 双计量沙盒 | wasmtime + fuel metering |
| 2 | Merkle 哈希链审计 | 加密操作链 |
| 3 | 污点追踪 | 信息流标签 |
| 4 | Ed25519 签名 | Agent 身份 |
| 5 | SSRF 保护 | 阻止私有 IP |
| 6 | 密钥零化 | 内存自动清除 |
| 7 | OFP 双向认证 | HMAC-SHA256 |
| 8 | 能力门控 | RBAC |
| 9 | 安全头 | CSP, HSTS |
| 10 | 循环保护 | SHA256 工具调用指纹 |
| 11 | 会话修复 | 7 阶段消息验证 |
| 12 | 路径遍历防护 | 规范化 + 符号链接 |
| 13 | GCRA 限速 | Token Bucket |
| 14 | 提示注入扫描 | 覆盖/危险模式 |
| 15 | 子进程沙盒 | env_clear() |
| 16 | 健康端点精简 | 最小公共信息 |

---

### 2.9 Channels (40 适配器)

**文件**: `crates/openfang-channels/src/lib.rs`

| 类别 | 适配器 |
|------|--------|
| 核心 | Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Email |
| 企业 | Teams, Mattermost, Google Chat, Webex, Feishu, Zulip |
| 社交 | LINE, Viber, Facebook Messenger, Mastodon, Bluesky, Reddit, LinkedIn |
| 隐私 | Threema, Nostr, Rocket.Chat |

---

### 2.10 Hands (7 自主 Agent)

**文件**: `crates/openfang-hands/src/lib.rs`

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

## 3. octo-sandbox 当前架构

### 3.1 项目规模

- **Rust 文件**: ~30+
- **Crate**: 3 (octo-sandbox, octo-engine, octo-types)
- **前端**: React + TypeScript

### 3.2 当前模块

| 模块 | 功能 | 状态 |
|------|------|------|
| Agent Loop | 对话 + 工具执行 | ✅ |
| Providers | Anthropic, OpenAI | ✅ |
| Memory | Working + Session + Persistent | ✅ |
| Skills | Skill 加载 + 热重载 | ✅ |
| MCP | stdio transport, Manager | ✅ |
| API | REST + WebSocket | ✅ |
| Debug UI | Timeline, LogViewer | ✅ |
| MCP Workbench | Server 管理 | ✅ |

---

## 4. 整合路线图

### 4.1 模块分类

#### 🟢 Phase 2.4 可引入 (当前分支)

| 模块 | 优先级 | 价值 | 难度 | 工作量 |
|------|--------|------|------|--------|
| MCP Client 完善 | P0 | ⭐⭐⭐⭐⭐ | 中 | 2-3 天 |
| EventBus | P1 | ⭐⭐⭐⭐ | 低 | 1 天 |
| 配置管理 | P1 | ⭐⭐⭐⭐ | 低 | 1 天 |
| API 设计模式 | P2 | ⭐⭐⭐ | 低 | 参考 |

#### 🟡 Phase 3 (octo-platform) 目标

| 模块 | 优先级 | 价值 | 难度 | 工作量 |
|------|--------|------|------|--------|
| AgentRegistry | P0 | ⭐⭐⭐⭐⭐ | 中 | 3-5 天 |
| Memory 增强 | P1 | ⭐⭐⭐⭐⭐ | 中 | 3 天 |
| Scheduler | P2 | ⭐⭐⭐⭐ | 中 | 2-3 天 |
| WorkflowEngine | P2 | ⭐⭐⭐⭐ | 中 | 3-5 天 |
| 安全系统 | P2 | ⭐⭐⭐⭐ | 高 | 5+ 天 |

#### 🔵 长期目标

| 模块 | 优先级 | 价值 | 难度 |
|------|--------|------|------|
| Channels | P3 | ⭐⭐⭐⭐⭐ | 高 |
| Hands | P3 | ⭐⭐⭐⭐ | 高 |
| Multi-Provider | P3 | ⭐⭐⭐ | 高 |

---

### 4.2 详细实施计划

#### Phase 2.4: MCP Client 完善

**目标**: 完整 MCP stdio + SSE 传输支持

**参考代码**:
```
github.com/openfang/crates/openfang-runtime/src/mcp.rs
- McpServerConfig: 服务器配置
- McpTransport: stdio + SSE
- McpConnection: 连接生命周期
- JSON-RPC 2.0 处理
```

**任务拆分**:
1. [ ] 增强 stdio transport (参考 OpenFang 实现)
2. [ ] 添加 SSE transport 支持
3. [ ] 实现工具发现 protocol
4. [ ] 添加超时和重试机制

---

#### Phase 2.4/3: EventBus 事件系统

**目标**: 统一事件驱动架构

**参考代码**:
```
github.com/openfang/crates/openfang-kernel/src/event_bus.rs
- broadcast::Sender 发布订阅
- 历史 Ring Buffer
- per-agent 通道
```

**任务拆分**:
1. [ ] 定义事件类型
2. [ ] 实现 EventBus 结构
3. [ ] 集成到 Agent Loop
4. [ ] 添加历史查询 API

---

#### Phase 3: AgentRegistry 多代理

**目标**: 支持多代理注册和管理

**参考代码**:
```
github.com/openfang/crates/openfang-kernel/src/registry.rs
- DashMap 并发存储
- 多索引 (ID, Name, Tag)
- Agent 生命周期
```

**任务拆分**:
1. [ ] 设计 AgentEntry 结构
2. [ ] 实现 AgentRegistry
3. [ ] 添加多索引支持
4. [ ] 集成到 API 层

---

#### Phase 3: Memory 增强

**目标**: 知识图谱集成

**参考代码**:
```
github.com/openfang/crates/openfang-memory/src/
- substrate.rs: 统一入口
- knowledge.rs: 知识图谱
- semantic.rs: 语义搜索
```

**任务拆分**:
1. [ ] 扩展 Structured Store
2. [ ] 添加语义搜索 (FTS5)
3. [ ] 实现知识图谱
4. [ ] 记忆整合引擎

---

## 5. 整合机制

### 5.1 CHECKPOINT_PLAN 里程碑

在 `docs/main/CHECKPOINT_PLAN.md` 中维护:

```markdown
## OpenFang 整合里程碑

| 模块 | 目标 Phase | 优先级 | 状态 |
|-----|-----------|--------|------|
| MCP Client | Phase 2.4 | P0 | ⏳ |
| EventBus | Phase 2.4/3 | P1 | ⏳ |
| 配置管理 | Phase 2.4 | P1 | ⏳ |
| AgentRegistry | Phase 3 | P0 | ⏳ |
| Memory 增强 | Phase 3 | P1 | ⏳ |
| Scheduler | Phase 3 | P2 | ⏳ |
| WorkflowEngine | Phase 3 | P2 | ⏳ |
| 安全系统 | Phase 3 | P2 | ⏳ |
| Channels | 长期 | P3 | ⏳ |
| Hands | 长期 | P3 | ⏳ |
```

---

### 5.2 Phase 启动检查

每次 `/start-phase` 时自动检查里程碑:
1. 读取 CHECKPOINT_PLAN 的 OpenFang 里程碑
2. 识别与当前 Phase 相关的模块
3. 询问用户是否需要先研究/引入

---

### 5.3 记忆索引追踪

在 `docs/dev/MEMORY_INDEX.md` 维护待办:
```markdown
## [OpenFang 整合待办]
- [ ] MCP Client 完善 (Phase 2.4)
- [ ] EventBus (Phase 2.4/3)
- [ ] ...
```

---

## 6. 关键代码索引

| 模块 | OpenFang 路径 | octo-sandbox 当前路径 |
|------|---------------|---------------------|
| Kernel | `kernel.rs` | - |
| AgentRegistry | `registry.rs` | - |
| EventBus | `event_bus.rs` | - |
| AgentLoop | `agent_loop.rs` | `loop_.rs` |
| MCP | `mcp.rs` | `mcp/mod.rs` |
| Memory | `memory/substrate.rs` | `memory/` |
| API | `api/server.rs` | `server/` |

---

## 7. 总结

OpenFang 是一个**成熟的 Agent OS** (137K LOC, 14 crates)，其架构设计对 octo-sandbox 有极高参考价值:

1. **短期**: 完善 MCP Client + EventBus (Phase 2.4)
2. **中期**: AgentRegistry + Memory 增强 (Phase 3)
3. **长期**: Channels + Hands (未来)

通过 CHECKPOINT_PLAN 里程碑 + Phase 启动检查 + MEMORY_INDEX 追踪，确保整合工作持续推进。
