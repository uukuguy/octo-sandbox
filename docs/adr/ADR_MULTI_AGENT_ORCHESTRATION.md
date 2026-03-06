# ADR：多智能体编排架构决策记录

**项目**：octo-sandbox
**版本**：v1.0
**日期**：2026-03-06
**状态**：已提议

---

## 目录

- [ADR-006：三层架构职责边界（Engine/Workbench/Platform）](#adr-006三层架构职责边界)
- [ADR-007：octo-engine 引入通用 Hook 引擎](#adr-007octo-engine-引入通用-hook-引擎)
- [ADR-008：octo-engine 引入 Event Sourcing](#adr-008octo-engine-引入-event-sourcing)
- [ADR-009：octo-engine 引入 HNSW 向量索引](#adr-009octo-engine-引入-hnsw-向量索引)
- [ADR-010：octo-engine 引入 Agent 路由器](#adr-010octo-engine-引入-agent-路由器)
- [ADR-011：octo-platform 引入多 Agent 拓扑与编排](#adr-011octo-platform-引入多-agent-拓扑与编排)
- [ADR-012：ADR/DDD 文档作为 Agent 约束系统](#adr-012adrddd-文档作为-agent-约束系统)

---

## ADR-006：三层架构职责边界（Engine/Workbench/Platform）

### 状态

**已提议** — 2026-03-06

### 上下文

octo-sandbox 是 mono-repo，包含三个产品层级：
- `octo-engine`：核心引擎（共享库）
- `octo-workbench`（`octo-server` + `web/`）：单用户单 Agent 工作台
- `octo-platform`（`octo-platform-server` + `web-platform/`）：多租户多 Agent 平台

类比关系：
```
CC (Claude Code)    ←→  octo-engine（核心能力层）
RuFlo (框架)        ←→  octo-engine 的编排模块（框架能力层）
RuView (应用)       ←→  octo-workbench / octo-platform（产品配置层）
```

当前问题是三个层级之间的职责边界不够清晰，尤其在引入多智能体编排能力后，
需要明确哪些能力属于 engine，哪些属于 workbench，哪些属于 platform。

### 决策

采用以下三层职责划分：

#### octo-engine（核心引擎 — 能力提供者）

**定位**：提供所有 Agent 能力的底层原语，不包含任何产品逻辑。

| 模块 | 职责 | 新增/现有 |
|------|------|----------|
| `agent/` | AgentRuntime, AgentExecutor, AgentLoop, **AgentRouter**, **Capability** | 现有 + 新增 |
| `memory/` | 4 层内存, **VectorIndex (HNSW)**, **HybridQuery**, **Embedding** | 现有 + 新增 |
| `event/` | EventBus, **EventStore**, **Projection**, **StateReconstructor** | 现有 + 新增 |
| `hooks/` | **HookRegistry**, **HookPoint**, **HookHandler trait** | 新增 |
| `orchestration/` | **TaskOrchestrator**, **AgentManifestLoader** | 新增 |
| `tools/` | ToolRegistry, Tool trait, 内置工具 | 现有 |
| `mcp/` | McpManager, McpClient, McpToolBridge | 现有 |
| `providers/` | Provider trait, ProviderChain | 现有 |
| `context/` | SystemPromptBuilder, BudgetManager, Pruner | 现有 |
| `security/` | SecurityPolicy, **AIDefence** | 现有 + 新增 |
| `session/` | SessionStore | 现有 |
| `skills/` | SkillLoader, SkillRegistry | 现有 |

**关键原则**：
- 所有新增模块都是**通用能力**，不含 workbench/platform 特定逻辑
- Hook 引擎只提供注册和执行机制，具体 Hook 由产品层配置
- Agent 路由只提供匹配算法，Agent 定义由产品层加载
- 向量索引只提供索引和搜索 API，数据由产品层灌入

#### octo-workbench（单用户工作台 — 简化配置）

**定位**：面向开发者的单用户单 Agent 工作台，追求简单易用。

| 能力 | 使用 engine 的 | 配置方式 |
|------|---------------|---------|
| 单 Agent 模式 | AgentRuntime + AgentExecutor | 默认 Agent，无需路由 |
| 基础 Hook | HookRegistry | 少量 Hook（tool_call, session） |
| 简单记忆 | 4 层内存（不启用 HNSW） | 关键字 + 标签搜索 |
| 事件通知 | EventBus（不启用 EventStore） | 实时流推送到前端 |
| MCP 管理 | McpManager | UI 界面配置 |
| 技能系统 | SkillLoader | YAML 技能定义 |

**不引入**：多 Agent 路由、拓扑管理、共识协议、模式学习

#### octo-platform（多租户平台 — 完整编排）

**定位**：面向团队/企业的多租户多 Agent 平台，追求智能和可扩展。

| 能力 | 使用 engine 的 | 配置方式 |
|------|---------------|---------|
| **多 Agent 路由** | AgentRouter + Capability | 声明式 Agent 定义（YAML） |
| **完整 Hook 链** | HookRegistry | 8+ Hook 点，配置驱动 |
| **语义记忆** | VectorIndex + HybridQuery | HNSW 索引 + 混合查询 |
| **事件溯源** | EventStore + Projection | 完整审计追踪 + 状态回放 |
| **任务编排** | TaskOrchestrator | 任务分解 + Agent 分配 |
| **模式学习** | PatternStore（新增） | 置信度衰减 + 奖励信号 |
| **ADR/DDD 约束** | ContextBuilder 扩展 | 自动注入相关约束到 Agent 上下文 |
| **后台 Worker** | Scheduler（现有扩展） | 定时优化、审计、巩固 |
| **多租户隔离** | TenantContext + JWT | 租户级 Agent 池 + 记忆隔离 |

### 后果

**收益**：
- engine 保持通用，两个产品按需使用
- workbench 保持简洁不被编排复杂度拖累
- platform 获得完整的多智能体能力
- 新增模块对 workbench 零影响（feature gate 控制）

**风险**：
- engine 模块增多，需要更严格的接口设计
- platform 对 engine 的依赖面扩大

**缓解**：
- 新增模块以 Cargo feature flag 控制（`feature = "orchestration"`）
- engine 内部模块间通过 trait 解耦

### 关联 ADR

- ADR-005（AgentRuntime 模块化拆分）—— 为本 ADR 的前置条件
- ADR-007 ~ ADR-012 —— 本 ADR 的子决策

---

## ADR-007：octo-engine 引入通用 Hook 引擎

### 状态

**已提议** — 2026-03-06

### 上下文

RuView 实践表明，8 个生命周期 Hook 点（PreToolUse, PostToolUse, UserPromptSubmit,
SessionStart, SessionEnd, Stop, PreCompact, SubagentStart）是多智能体协作的数据采集基础。
当前 octo-engine 的 Extension trait 仅有 3 个 Hook 点（on_agent_start/end/tool_call），
且需要编写 Rust 代码，无法通过配置驱动。

### 决策

在 `octo-engine` 中新增 `hooks/` 模块：

```rust
pub enum HookPoint {
    PreToolUse,      // 工具调用前（安全验证、参数增强）
    PostToolUse,     // 工具调用后（结果记录、模式学习）
    PreTask,         // Agent 任务开始前（上下文准备、约束注入）
    PostTask,        // Agent 任务完成后（奖励计算、模式存储）
    SessionStart,    // 会话开始（状态恢复、记忆加载）
    SessionEnd,      // 会话结束（状态持久化、记忆同步）
    ContextDegraded, // 上下文预算不足（保存关键信息）
    LoopTurnStart,   // 对话轮次开始
    LoopTurnEnd,     // 对话轮次结束
}

#[async_trait]
pub trait HookHandler: Send + Sync {
    fn name(&self) -> &str;
    fn matches(&self, point: HookPoint, context: &HookContext) -> bool;
    async fn execute(&self, context: &mut HookContext) -> Result<HookAction>;
}

pub struct HookRegistry {
    handlers: HashMap<HookPoint, Vec<Arc<dyn HookHandler>>>,
}
```

**集成点**：
- `AgentLoop::run()` 在 Zone A/B/C 之间插入 Hook 调用
- `ToolRegistry::execute()` 在工具调用前后触发 Hook
- `SessionStore` 在会话开始/结束时触发 Hook

### 后果

- Extension trait 保留（向后兼容），但推荐迁移到 HookRegistry
- workbench 可注册少量 Hook（如审计日志）
- platform 可注册完整 Hook 链（路由、学习、约束注入）

### 关联 ADR

- ADR-006（三层架构）—— Hook 引擎属于 engine 层
- RuView `.claude/settings.json` hooks 配置 —— 设计参考

---

## ADR-008：octo-engine 引入 Event Sourcing

### 状态

**已提议** — 2026-03-06

### 上下文

当前 `EventBus` 是 broadcast channel + ring buffer（1000 条），事件是"通知"而非"事实记录"。
RuFlo v3 的 Event Sourcing 设计（EventStore + Projection + StateReconstructor）
支持完整的审计追踪和状态回放，是多 Agent 协调的数据基础。

### 决策

扩展现有 `event/` 模块：

```rust
// event/store.rs — 事件持久化
pub struct EventStore {
    db: Arc<Database>,  // 复用现有 tokio-rusqlite
}

impl EventStore {
    pub async fn append(&self, event: OctoEvent) -> Result<EventId>;
    pub async fn read_stream(&self, stream_id: &str, from: u64) -> Result<Vec<StoredEvent>>;
    pub async fn read_all(&self, from: u64, limit: usize) -> Result<Vec<StoredEvent>>;
}

// event/projection.rs — 读模型投影
pub trait Projection: Send + Sync {
    fn handle(&mut self, event: &StoredEvent) -> Result<()>;
}

// 内置投影
pub struct AgentStateProjection;      // Agent 当前状态
pub struct TaskHistoryProjection;     // 任务执行历史
pub struct ToolUsageProjection;       // 工具使用统计
```

**SQLite 表设计**：
```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    stream_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,   -- JSON
    metadata TEXT,           -- JSON（agent_id, session_id 等）
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_events_stream ON events(stream_id, id);
```

### 后果

- workbench：可选启用（`feature = "event-sourcing"`），默认不启用
- platform：默认启用，支撑审计、回放、模式分析
- 现有 EventBus 保持不变，EventStore 是额外的持久化层

### 关联 ADR

- ADR-006（三层架构）—— EventStore 属于 engine 层
- ADR-012（ADR/DDD 约束）—— 事件流是约束验证的数据源

---

## ADR-009：octo-engine 引入 HNSW 向量索引

### 状态

**已提议** — 2026-03-06

### 上下文

当前 `memory/` 模块有 FTS 全文搜索但无向量语义搜索。
RuView 的 swarm DB 使用 768 维 HNSW 向量（M=16, efConstruction=200, cosine 距离），
搜索比暴力方式快 150x-12,500x。这是语义记忆检索和模式学习的基础。

### 决策

在 `memory/` 模块新增向量搜索能力：

```rust
// memory/vector_index.rs
pub struct HnswIndex {
    // 使用 hnsw_rs 或 usearch crate
    index: /* ... */,
    config: HnswConfig,
}

pub struct HnswConfig {
    pub dimensions: usize,      // 384 或 768
    pub m: usize,               // 默认 16
    pub ef_construction: usize, // 默认 200
    pub ef_search: usize,       // 默认 100
    pub metric: DistanceMetric, // Cosine | Euclidean | DotProduct
}

impl HnswIndex {
    pub fn insert(&mut self, id: &str, vector: &[f32]) -> Result<()>;
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>>;
    pub fn delete(&mut self, id: &str) -> Result<()>;
}

// memory/hybrid_query.rs — 混合查询路由
pub struct HybridQueryEngine {
    sqlite: Arc<SqliteMemoryStore>,
    vector: Arc<RwLock<HnswIndex>>,
}

impl HybridQueryEngine {
    pub async fn query(&self, q: MemoryQuery) -> Result<Vec<MemoryEntry>> {
        match q.query_type() {
            QueryType::Semantic => self.vector_search(q).await,
            QueryType::Structured => self.sqlite_search(q).await,
            QueryType::Hybrid => self.merged_search(q).await,
        }
    }
}
```

**Embedding 生成**：
- 方案 A：通过 LLM Provider API 生成（简单但有延迟）
- 方案 B：本地 ONNX Runtime（快但增加二进制大小）
- 推荐：Provider API 优先，ONNX 作为可选 feature

### 后果

- 现有 MemoryStore trait 保持不变
- HybridQueryEngine 是新的高层 API，封装结构 + 语义查询
- workbench：可选（默认关闭）
- platform：默认启用

### 关联 ADR

- ADR-006（三层架构）—— VectorIndex 属于 engine 层
- RuView `.swarm/schema.sql` vector_indexes —— Schema 参考

---

## ADR-010：octo-engine 引入 Agent 路由器

### 状态

**已提议** — 2026-03-06

### 上下文

当前 octo-engine 是单 Agent 模式，`AgentCatalog` 有注册功能但无选择逻辑。
RuView 的 `hook-handler.cjs route` 实现了基于关键词和语义的 Agent 路由，
返回 `{ agent, confidence, alternatives }`，是多 Agent 协调的入口。

### 决策

```rust
// agent/capability.rs
pub struct AgentCapability {
    pub name: String,
    pub capabilities: Vec<String>,       // ["code_generation", "security_audit"]
    pub priority: Priority,              // Low, Normal, High, Critical
    pub max_concurrent_tasks: usize,
    pub system_prompt_template: String,
}

// agent/router.rs
pub struct AgentRouter {
    catalog: Arc<AgentCatalog>,
    // 未来可接入 VectorIndex 做语义匹配
}

pub struct RouteResult {
    pub agent_id: AgentId,
    pub confidence: f64,
    pub reason: String,
    pub alternatives: Vec<(AgentId, f64)>,
}

impl AgentRouter {
    /// 基于任务描述路由到最佳 Agent
    pub fn route(&self, task_description: &str) -> Result<RouteResult>;
}
```

**路由策略演进**：
1. V1：关键词匹配（MVP，无额外依赖）
2. V2：TF-IDF 加权匹配（轻量级语义）
3. V3：HNSW 向量匹配（依赖 ADR-009 的向量索引）

### 后果

- workbench：不启用路由（单 Agent 直接执行）
- platform：启用路由，支持多 Agent 协调
- AgentCatalog 扩展 capability 字段，向后兼容

### 关联 ADR

- ADR-006（三层架构）—— Router 属于 engine 层
- ADR-009（HNSW）—— V3 路由策略的前置依赖

---

## ADR-011：octo-platform 引入多 Agent 拓扑与编排

### 状态

**已提议** — 2026-03-06

### 上下文

RuView 使用 hierarchical-mesh 拓扑支持最多 15 个并发 Agent。
octo-platform 作为多租户平台，需要支持租户级的多 Agent 协调。
这是 **platform 专有** 的能力，不放入 engine。

### 决策

在 `octo-platform-server` 中实现：

```rust
// octo-platform-server/src/orchestration/
pub mod topology;     // 拓扑管理（hierarchical, mesh, adaptive）
pub mod coordinator;  // 协调器（任务分配、结果汇总）
pub mod consensus;    // 共识协议（Raft 优先，未来扩展 Byzantine）
pub mod pool;         // Agent 池管理（扩缩容、健康检查）
```

**使用 engine 的**：
- `AgentRouter` — Agent 选择
- `HookRegistry` — 编排过程中的 Hook
- `EventStore` — 编排事件持久化
- `TaskOrchestrator` — 任务分解

**platform 额外提供**：
- 拓扑感知的消息路由
- 租户级 Agent 池隔离
- 协调器状态管理

### 后果

- engine 不会被拓扑/共识逻辑污染
- platform 独立迭代编排能力
- workbench 完全不受影响

### 关联 ADR

- ADR-006（三层架构）—— 明确这是 platform 层职责
- ADR-010（Agent 路由）—— 基础能力来自 engine

---

## ADR-012：ADR/DDD 文档作为 Agent 约束系统

### 状态

**已提议** — 2026-03-06

### 上下文

RuView 的 44 个 ADR + 7 个 DDD 领域模型不仅是文档，更是 Agent 的**行为约束**。
Agent 执行任务前搜索相关 ADR 了解决策约束，搜索 DDD 了解边界定义，
避免 "AI-generated code tends to drift — reinventing patterns, contradicting earlier decisions"。

当前 octo-sandbox 有 5 个 ADR（安全相关）和 1 份 DDD 分析报告，但尚未用于 Agent 约束。

### 决策

1. **ADR 索引化**：在 `docs/adr/` 下维护 `README.md` 索引表（按 RuView 模式）
2. **DDD 约束注入**：`SystemPromptBuilder` 在构建上下文时，
   自动搜索与当前任务相关的 ADR/DDD 片段并注入 Agent 系统提示词
3. **ADR 命名规范**：采用 `ADR-{NNN}-{kebab-case-title}.md` 格式
4. **ADR 状态追踪**：每个 ADR 标注 Proposed / Accepted / Superseded

**实现路径**：
```rust
// context/constraint_injector.rs
pub struct ConstraintInjector {
    adr_index: Vec<AdrEntry>,     // 从 docs/adr/ 扫描
    ddd_index: Vec<DddContext>,   // 从 docs/ddd/ 扫描
}

impl ConstraintInjector {
    /// 根据任务描述找到相关约束
    pub fn find_constraints(&self, task: &str) -> Vec<Constraint>;

    /// 将约束格式化为系统提示词片段
    pub fn format_for_prompt(&self, constraints: &[Constraint]) -> String;
}
```

### 后果

- ADR/DDD 不再只是文档，成为 Agent 行为的**主动约束**
- 新的架构决策自动被后续 Agent 遵守
- 需要保持 ADR/DDD 的更新纪律（过时的约束比没有约束更危险）

### 关联 ADR

- ADR-006（三层架构）—— 约束注入属于 engine 层
- RuView `docs/adr/README.md` —— 索引格式参考
- 现有 DDD_DOMAIN_ANALYSIS.md —— 已有的领域模型基础
