# RuFlo 设计分析：octo-sandbox 应当引入的设计

> 基于对 RuFlo v3.5（3th-party/ruflo/）全面分析，对比 octo-sandbox 现有架构，
> 识别出值得引入的设计模式和架构决策。

---

## 一、分析概要

| 维度 | RuFlo v3.5 | octo-sandbox 现状 | 差距评估 |
|------|-----------|-------------------|---------|
| **事件系统** | Event Sourcing + CQRS | Broadcast EventBus（仅通知） | 重大差距 |
| **内存架构** | HNSW 向量索引 + 语义搜索 | 4 层但无向量搜索 | 重大差距 |
| **插件系统** | 声明式 YAML + IPFS 注册表 | Extension trait（代码级） | 中等差距 |
| **Hook 体系** | 17 Hook + 12 Worker + 自学习 | 3 个 Hook 点（无学习） | 重大差距 |
| **Agent 路由** | MoE + Q-Learning 智能路由 | 无路由（单 Agent） | 重大差距 |
| **共识协议** | Raft/Byzantine/Gossip/CRDT | 无（单实例） | 功能缺失 |
| **安全防护** | AIDefence（注入检测+PII） | SecurityPolicy（路径+命令） | 中等差距 |
| **DDD 实践** | 完整 Bounded Context | 模块化但非严格 DDD | 轻度差距 |
| **自学习** | ReasoningBank + SONA + EWC++ | 无 | 功能缺失 |
| **拓扑管理** | 层级/网格/环/星多拓扑 | 无（单 Agent） | 功能缺失 |

---

## 二、P0 级推荐引入（高价值、架构级提升）

### 2.1 Event Sourcing 事件溯源

**RuFlo 设计**：
- 所有状态变更捕获为不可变事件（`AgentSpawnedEvent`, `TaskCreatedEvent` 等）
- EventStore 持久化完整事件流
- Projection 从事件流派生读模型（`AgentStateProjection`, `TaskHistoryProjection`）
- StateReconstructor 从事件历史重建任意时刻状态

**octo-sandbox 现状**：
- `EventBus` 仅是 broadcast channel + ring buffer（1000 条历史）
- 事件是"通知"而非"事实记录"，无法回放重建状态
- 无 Projection，读写共用同一模型

**引入方案**：
```
octo-engine/src/event/
├── event_store.rs       # 事件持久化（SQLite 表：events）
├── projection.rs        # 投影引擎（从事件流生成读模型）
├── state_reconstructor.rs # 状态重建（指定时间点）
└── mod.rs               # 现有 EventBus 扩展
```

**预期收益**：
- 完整审计追踪（who changed what, when）
- 时间旅行调试（回放任意时刻的 Agent 状态）
- 读写模型分离，独立优化查询性能
- 为未来多 Agent 协调打下基础

---

### 2.2 HNSW 向量索引 + 语义记忆搜索

**RuFlo 设计**：
- AgentDB 使用 HNSW（Hierarchical Navigable Small World）索引
- 参数：M=16, efConstruction=200, cosine 距离
- O(log n) 搜索复杂度，比暴力搜索快 150x-12,500x
- 混合查询路由：语义查询 → HNSW，结构查询 → SQLite
- 支持 Int8 量化（3.92x 内存压缩）

**octo-sandbox 现状**：
- KnowledgeGraph 有 FTS（全文搜索）但无向量搜索
- MemoryStore 仅支持关键字和标签过滤
- 无 embedding 生成能力

**引入方案**：
```
octo-engine/src/memory/
├── vector_index.rs      # HNSW 索引实现（可用 hnsw_rs crate）
├── embedding.rs         # Embedding 生成（ONNX Runtime 或 API 调用）
├── hybrid_query.rs      # 混合查询路由（语义 + 结构）
└── 现有模块保持不变
```

**预期收益**：
- Agent 可语义搜索历史对话和知识（"找到类似的安全问题解决方案"）
- 大幅提升记忆检索精度和速度
- 支撑未来的模式学习和推荐

---

### 2.3 通用 Hook 体系 + 生命周期管理

**RuFlo 设计**：
- 17 个 Hook 点覆盖完整生命周期：
  - `PreToolUse` / `PostToolUse` — 工具调用前后
  - `PreTask` / `PostTask` — 任务执行前后
  - `PreEdit` / `PostEdit` — 文件编辑前后
  - `PreCommand` / `PostCommand` — 命令执行前后
  - `SessionStart` / `SessionEnd` — 会话生命周期
  - `PreCompact` — 上下文压缩前
- 12 个后台 Worker（ultralearn, optimize, audit, deepdive 等）
- Hook 可触发学习存储（将执行模式写入 ReasoningBank）
- 配置驱动（settings.json 中声明 matcher + command）

**octo-sandbox 现状**：
- Extension trait 仅 3 个 Hook：`on_agent_start`, `on_agent_end`, `on_tool_call`
- 无配置驱动，需要编写 Rust 代码实现 Extension
- 无后台 Worker 机制

**引入方案**：
```rust
// octo-engine/src/hooks/mod.rs
pub enum HookPoint {
    PreToolUse, PostToolUse,
    PreTask, PostTask,
    SessionStart, SessionEnd,
    ContextDegraded,
    LoopTurnStart, LoopTurnEnd,
}

pub trait HookHandler: Send + Sync {
    fn matches(&self, point: HookPoint, context: &HookContext) -> bool;
    async fn execute(&self, context: &mut HookContext) -> Result<HookAction>;
}

pub struct HookRegistry {
    handlers: HashMap<HookPoint, Vec<Arc<dyn HookHandler>>>,
}
```

**预期收益**：
- 可观测性大幅提升（每个环节都可插入监控/审计）
- 支持配置驱动的行为定制（无需改代码）
- 为自学习系统提供数据采集点

---

### 2.4 智能 Agent 路由（多 Agent 协调基础）

**RuFlo 设计**：
- MoE（Mixture of Experts）8 专家路由
- Q-Learning 强化学习选择最优 Agent
- 基于任务复杂度、领域、可用性动态选择
- 60+ 专业化 Agent 类型

**octo-sandbox 现状**：
- 单 Agent 模式，AgentCatalog 有状态机但无路由逻辑
- AgentExecutor 直接创建，无选择策略

**引入方案**：
```
octo-engine/src/agent/
├── router.rs            # Agent 路由器（基于能力匹配）
├── capability.rs        # 能力声明与匹配
├── catalog.rs           # 现有，扩展能力注册
└── mod.rs
```

**最小可行版本**：
- 每个 Agent 声明 capabilities（代码生成、测试、安全审计等）
- 路由器根据任务描述匹配最佳 Agent
- 先实现关键字匹配，后期可升级为语义匹配

---

## 三、P1 级推荐引入（中等价值、增强型提升）

### 3.1 声明式 Agent 定义

**RuFlo 设计**：
- Agent 以 YAML frontmatter + Markdown 定义（非代码）
- 包含：name, type, capabilities, hooks, priority, color
- 热加载无需重启
- 非技术人员也能编写 Agent

**octo-sandbox 引入建议**：
- 扩展现有 SkillLoader（YAML manifests）支持 Agent 定义
- 在 `config/agents/` 目录下放置 Agent YAML 定义
- AgentCatalog 从 YAML 加载 Agent 模板

```yaml
# config/agents/code-reviewer.yaml
name: code-reviewer
type: reviewer
capabilities: [code_review, security_audit]
system_prompt_template: prompts/code-reviewer.md
max_concurrent_tasks: 3
priority: high
```

---

### 3.2 Fluent Query Builder（记忆查询 DSL）

**RuFlo 设计**：
```typescript
await memory.query(
  query()
    .semantic('security patterns')
    .inNamespace('security')
    .withTags(['critical'])
    .threshold(0.8)
    .limit(10)
    .build()
)
```

**octo-sandbox 引入建议**：
```rust
// 为 MemoryStore 添加 Builder 模式
let results = memory.query()
    .semantic("安全模式")
    .namespace("security")
    .tags(&["critical"])
    .threshold(0.8)
    .limit(10)
    .execute()
    .await?;
```

**收益**：提升记忆搜索的易用性和表达力。

---

### 3.3 AIDefence 安全防护层

**RuFlo 设计**：
- Prompt 注入检测
- PII（个人信息）扫描
- Jailbreak 尝试识别
- 25 级自适应缓解
- 行为分析

**octo-sandbox 现状**：
- SecurityPolicy 仅覆盖路径验证和命令风险评级
- 无 LLM 输入/输出安全检查

**引入建议**：
```
octo-engine/src/security/
├── policy.rs            # 现有
├── ai_defence.rs        # 新增：LLM 交互安全检查
│   ├── injection_detector  # Prompt 注入检测
│   ├── pii_scanner         # PII 信息扫描
│   └── output_validator    # 输出安全验证
└── mod.rs
```

---

### 3.4 优先级消息队列

**RuFlo 设计**：
- 4 级优先级（urgent/high/normal/low）
- 每级使用 Circular Buffer Deque
- O(1) 入队/出队

**octo-sandbox 引入建议**：
- 替换当前 EventBus 的 broadcast channel
- 支持事件优先级，确保关键事件（安全告警、取消信号）优先处理

---

## 四、P2 级推荐引入（长期价值、面向未来）

### 4.1 自学习系统（ReasoningBank）

**RuFlo 设计**：
- 4 步学习循环：RETRIEVE → JUDGE → DISTILL → CONSOLIDATE
- 模式存储带奖励/成功指标
- EWC++（弹性权重巩固）防止灾难性遗忘
- SONA（自优化神经架构）

**建议**：作为 octo-sandbox 中期路线图目标。需要先完成 P0 的事件溯源和向量索引。

---

### 4.2 多 Agent 拓扑与共识

**RuFlo 设计**：
- 层级（Queen-Worker）/ 网格 / 环 / 星 四种拓扑
- Raft / Byzantine / Gossip / CRDT 四种共识
- 拓扑感知的注意力机制

**建议**：octo-platform 多租户多 Agent 场景下引入。当前单用户单 Agent 的 octo-workbench 暂不需要。

---

### 4.3 IPFS 插件注册表

**RuFlo 设计**：
- 去中心化 IPFS 存储（Pinata 网关）
- 插件发现、安装、发布完整生态

**建议**：等 octo-sandbox 插件生态成熟后再考虑。当前 Extension trait 机制足够。

---

## 五、实施优先级总结

| 优先级 | 设计 | 预计工作量 | 依赖 | 影响范围 |
|--------|------|-----------|------|---------|
| **P0-1** | Event Sourcing | 3-5 天 | 无 | event/, agent/, session/ |
| **P0-2** | HNSW 向量索引 | 5-7 天 | 无 | memory/ |
| **P0-3** | 通用 Hook 体系 | 3-4 天 | 无 | 新增 hooks/ 模块 |
| **P0-4** | Agent 路由 | 2-3 天 | 无 | agent/ |
| **P1-1** | 声明式 Agent 定义 | 2 天 | P0-4 | agent/, skills/ |
| **P1-2** | Fluent Query Builder | 1-2 天 | P0-2 | memory/ |
| **P1-3** | AIDefence 安全层 | 3-4 天 | 无 | security/ |
| **P1-4** | 优先级消息队列 | 1 天 | 无 | event/ |
| **P2-1** | 自学习系统 | 10+ 天 | P0-1,2,3 | 新增模块 |
| **P2-2** | 多 Agent 拓扑 | 10+ 天 | P0-4, P2-1 | 新增模块 |

---

## 六、RuFlo 设计中 octo-sandbox 已具备的优势

以下是 octo-sandbox 现有设计中**不需要改变**的部分（已优于或等同于 RuFlo）：

| 设计 | octo-sandbox 优势 |
|------|-------------------|
| **Tool trait + Registry** | Rust trait 抽象比 TS 更类型安全，性能更好 |
| **4 层内存架构** | Working/Session/Persistent/KnowledgeGraph 结构清晰 |
| **Provider Chain** | 失败转移 + 负载均衡已实现，RuFlo 仅有适配器 |
| **Sandbox 隔离** | Subprocess/WASM/Docker 三种运行时，RuFlo 无此能力 |
| **Context Engineering** | BudgetManager + Pruner + DegradationLevel 是独特优势 |
| **类型安全** | Rust 编译期保证远超 TypeScript 运行时检查 |
| **性能** | 原生编译 vs Node.js，数量级差距 |

---

## 七、结论

RuFlo 最值得引入的核心设计理念可以概括为：

> **"可观测的事件流 + 可搜索的语义记忆 + 可扩展的生命周期钩子 + 可路由的多 Agent 协调"**

这四个能力形成闭环：Hook 采集数据 → Event Sourcing 持久化 → HNSW 建立语义索引 → Agent Router 利用历史模式做智能决策。octo-sandbox 引入这些设计后，将从"单 Agent 工作台"进化为"可学习的智能 Agent 平台"。

---

## 八、RuView 实践案例：项目级多智能体编排体系

> 以下基于 3th-party/RuView/ 项目的深度分析。RuView 是 RuFlo 的典型应用案例，
> 展示了如何在**实际项目**中配置多智能体协作体系。对 octo-platform 多智能体编排有直接指导意义。

### 8.1 RuView 编排体系全景

RuView 通过 **6 个配置层** 构建完整的多智能体协作：

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: CLAUDE.md                                 │
│  项目指令 — 定义全局规则、架构约束、行为边界          │
├─────────────────────────────────────────────────────┤
│  Layer 2: .claude/settings.json                     │
│  Hook 配置 — 8 个生命周期钩子 + 权限矩阵            │
├─────────────────────────────────────────────────────┤
│  Layer 3: .claude/agents/ (130+ 定义)               │
│  Agent 定义 — YAML 元数据 + Markdown 系统提示词      │
├─────────────────────────────────────────────────────┤
│  Layer 4: .claude/skills/ + .claude/commands/       │
│  技能与命令 — 30+ skills, 40+ commands               │
├─────────────────────────────────────────────────────┤
│  Layer 5: .claude-flow/ + .swarm/                   │
│  运行时状态 — 守护进程、swarm 数据库、指标追踪       │
├─────────────────────────────────────────────────────┤
│  Layer 6: docs/adr/ + docs/ddd/                     │
│  架构知识库 — 44 个 ADR + 7 个 DDD 领域模型          │
│  作为 Agent 的约束和指导                              │
└─────────────────────────────────────────────────────┘
```

### 8.2 Hook 生命周期（8 个钩子点）

RuView 在 `.claude/settings.json` 中配置了完整的 Hook 链：

| Hook 事件 | 处理器 | 作用 |
|-----------|--------|------|
| **PreToolUse** (Bash) | `hook-handler.cjs pre-bash` | 命令安全验证 |
| **PostToolUse** (Write/Edit) | `hook-handler.cjs post-edit` | 编辑结果学习 |
| **UserPromptSubmit** | `hook-handler.cjs route` | **智能任务路由**（最关键） |
| **SessionStart** | `hook-handler.cjs session-restore` + `auto-memory-hook.mjs import` | 恢复会话状态 + 导入记忆 |
| **SessionEnd** | `hook-handler.cjs session-end` | 持久化会话状态 |
| **Stop** | `auto-memory-hook.mjs sync` | 同步记忆到持久存储 |
| **PreCompact** | `hook-handler.cjs compact-*` | 上下文压缩前保存关键信息 |
| **SubagentStart** | `hook-handler.cjs status` | 子 Agent 启动时注入状态 |

**核心机制 — UserPromptSubmit 路由**：
```
用户提交任务 → Hook 拦截 → router.js 分析关键词/语义
→ 返回 { agent: "coder", confidence: 75%, alternatives: [...] }
→ 显示在状态栏供用户参考
```

### 8.3 Swarm 数据库 Schema（模式学习核心）

RuView 的 `.swarm/schema.sql` 定义了完整的学习型数据模型：

```sql
-- 1. 记忆条目（5种类型：semantic/episodic/procedural/working/pattern）
memory_entries (
    id, key, namespace, content, type,
    embedding BLOB,          -- 768维 HNSW 向量
    tags, metadata, owner_id,
    access_count, status     -- 访问计数驱动热度排序
)

-- 2. 模式学习（带置信度衰减）
patterns (
    id, name, pattern_type,  -- task-routing/error-recovery/optimization/...
    condition, action,
    confidence REAL,         -- 0-1 置信度
    success_count, failure_count,
    decay_rate REAL,         -- 时间衰减率（默认 0.01）
    half_life_days INTEGER,  -- 半衰期（默认 30 天）
    embedding BLOB           -- 768维向量，支持语义搜索相似模式
)

-- 3. SONA 学习轨迹
trajectories (
    id, session_id, status,
    verdict,                 -- success/failure/partial
    total_steps, total_reward REAL
)
trajectory_steps (
    trajectory_id, step_number,
    action, observation, reward REAL
)

-- 4. 向量索引元数据
vector_indexes (
    dimensions: 768,
    metric: cosine,
    hnsw_m: 16, hnsw_ef_construction: 200
)
```

**关键设计**：模式（patterns）带有置信度衰减 — 长期不匹配的模式自动降权，最近成功的模式权重更高。这是 Agent 路由准确性持续提升的核心机制。

### 8.4 Agent 定义规范（130+ Agent，23 个类别）

RuView 的 Agent 按领域分类：

| 类别 | Agent 数 | 代表性 Agent | 用途 |
|------|---------|-------------|------|
| **core/** | 5 | coder, reviewer, tester, planner, researcher | 基础开发 |
| **swarm/** | 3 | hierarchical-coordinator, mesh-coordinator, adaptive-coordinator | 协调策略 |
| **v3/** | 18 | security-architect, adr-architect, ddd-domain-expert, memory-specialist | V3 专业化 |
| **consensus/** | 7 | raft-manager, byzantine-coordinator, gossip-coordinator, crdt-synchronizer | 共识协议 |
| **github/** | 13 | pr-manager, code-review-swarm, issue-tracker, release-manager | GitHub 集成 |
| **sparc/** | 5 | specification, pseudocode, architecture, refinement | SPARC 方法论 |
| **optimization/** | 5 | performance-benchmarker, topology-optimizer, load-balancing | 性能优化 |
| **templates/** | 10 | 各种角色模板 | 快速启动 |

**Agent 定义格式**：
```yaml
---
name: coder
type: developer
capabilities: [code_generation, refactoring, self_learning, context_enhancement]
priority: high
hooks:
  pre: |
    # 搜索 ReasoningBank 获取相似历史模式
    npx claude-flow@v3alpha memory search --query "$TASK" --limit 5 --use-hnsw
  post: |
    # 存储学习模式，带奖励信号
    npx claude-flow@v3alpha hooks intelligence --action pattern-store \
      --consolidate-ewc true
---
# Markdown 系统提示词...
```

### 8.5 后台 Worker 体系

RuView 配置了 7 个后台 Worker 持续优化系统：

| Worker | 间隔 | 优先级 | 作用 |
|--------|------|--------|------|
| **map** | 15min | normal | 代码库索引映射 |
| **audit** | 10min | critical | 安全扫描分析 |
| **optimize** | 15min | high | 性能优化建议 |
| **consolidate** | 30min | low | 记忆巩固（EWC++ 防遗忘） |
| **testgaps** | 20min | normal | 测试覆盖率分析 |
| **predict** | 10min | low | 预测性预加载（禁用） |
| **document** | 60min | low | 自动文档生成（禁用） |

### 8.6 ADR/DDD 作为 Agent 指导系统

这是 RuView 最具创新性的设计 — **文档即约束**：

**ADR（架构决策记录）的 Agent 指导作用**：
- 44 个 ADR 编码了所有已做的架构决策
- Agent 执行任务前可搜索相关 ADR，避免违反已有决策
- ADR 之间互相引用形成知识图谱
- 状态追踪（Proposed/Accepted/Superseded）标识决策成熟度

> 引自 RuView ADR README：
> "When an AI agent works on this codebase, ADRs give it the constraints
> and rationale it needs to make changes that align with the existing
> architecture. Without them, AI-generated code tends to drift."

**DDD 领域模型的 Agent 指导作用**：
- 7 个领域模型定义了系统的 Bounded Context 边界
- **通用语言表（Ubiquitous Language）** — Agent 必须使用的领域术语
- **聚合根和值对象** — 告诉 Agent 数据应属于哪个模块
- **反腐层** — 告诉 Agent 上下文之间如何安全通信

**ADR + DDD 协同**：
```
ADR 提供 "为什么"（决策约束和基本原理）
DDD 提供 "是什么"（结构边界和术语定义）
→ 共同指导 Agent "怎么做"（在正确的边界内用正确的术语实现）
```

### 8.7 完整编排流程

```
用户提交任务
    ↓
SessionStart Hook → 恢复会话状态 + 导入记忆
    ↓
UserPromptSubmit Hook → 路由器分析任务
    ↓ 返回推荐 Agent + 置信度
Agent Pre-Hook → 搜索 ReasoningBank 历史模式（HNSW）
    ↓ 加载相似成功/失败案例
Agent 执行任务
    ↓ 参考 ADR 约束 + DDD 边界
    ↓ PreToolUse Hook → 验证命令安全性
    ↓ PostToolUse Hook → 记录编辑结果
Agent Post-Hook → 计算奖励 → 存储模式 → 训练 SONA
    ↓
后台 Worker → consolidate（巩固记忆）→ audit（安全检查）
    ↓
SessionEnd Hook → 持久化状态 → 同步记忆
```

---

## 九、octo-platform 多智能体编排引入方案

基于 RuView 的完整实践，为 octo-platform 设计以下引入方案：

### 9.1 Phase 1：基础编排框架（P0，2-3 周）

**目标**：建立多 Agent 运行基础

| 组件 | 实现内容 | 对应 RuView |
|------|---------|------------|
| **AgentRouter** | 能力匹配路由器 | `hook-handler.cjs route` |
| **AgentManifest** | YAML Agent 定义加载 | `.claude/agents/*.md` |
| **HookRegistry** | 生命周期钩子注册 | `.claude/settings.json` hooks |
| **TaskOrchestrator** | 任务分解与分配 | swarm task management |

```rust
// crates/octo-engine/src/orchestration/mod.rs（新模块）
pub mod router;      // Agent 路由
pub mod manifest;    // Agent 定义加载
pub mod hooks;       // Hook 注册与执行
pub mod orchestrator; // 任务编排
```

### 9.2 Phase 2：学习型记忆（P0-P1，3-4 周）

**目标**：Agent 从历史中学习

| 组件 | 实现内容 | 对应 RuView |
|------|---------|------------|
| **VectorIndex** | HNSW 向量索引 | `.swarm/schema.sql` vector_indexes |
| **PatternStore** | 模式存储 + 置信度衰减 | `.swarm/schema.sql` patterns |
| **EventStore** | 事件溯源持久化 | Event Sourcing 设计 |
| **HybridQuery** | 语义 + 结构混合查询 | AgentDB hybrid backend |

### 9.3 Phase 3：ADR/DDD 指导系统（P1，1-2 周）

**目标**：文档作为 Agent 约束

| 组件 | 实现内容 | 对应 RuView |
|------|---------|------------|
| **ADR 索引** | 自动扫描 docs/adr/ 建立索引 | `docs/adr/README.md` |
| **DDD 边界检查** | Agent 编辑时验证 Bounded Context | `docs/ddd/*-domain-model.md` |
| **约束注入** | 相关 ADR/DDD 自动注入 Agent 上下文 | ADR 搜索 + 上下文拼接 |

### 9.4 Phase 4：后台优化（P2，持续迭代）

**目标**：自动化持续优化

| 组件 | 实现内容 | 对应 RuView |
|------|---------|------------|
| **BackgroundWorker** | 定时任务框架 | daemon-state.json workers |
| **PatternConsolidator** | 模式巩固 + 衰减 | consolidate worker |
| **SecurityAuditor** | 自动安全扫描 | audit worker |

---

## 十、最终结论

### RuFlo 框架层面的设计价值（第一轮分析）

> **"可观测的事件流 + 可搜索的语义记忆 + 可扩展的生命周期钩子 + 可路由的多 Agent 协调"**

### RuView 项目层面的实践价值（第二轮分析，本次补充）

> **"声明式 Agent 定义 + 配置驱动的 Hook 编排 + 模式学习与置信度衰减 + ADR/DDD 文档即约束"**

两个层面结合，形成完整的多智能体编排能力图谱：

```
Framework Level (RuFlo)          Project Level (RuView)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Event Sourcing                   Hook 生命周期配置
HNSW 向量索引                    Swarm DB 模式学习
通用 Hook 引擎                   8 个具体 Hook 点
Agent Router                     130+ Agent 定义
CQRS 读写分离                    后台 Worker 持续优化
Plugin 扩展机制                  ADR/DDD 约束注入
共识协议                         层级-网格混合拓扑
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
         ↓ octo-sandbox 应当引入 ↓
    框架能力 + 项目级最佳实践
```

octo-platform 引入这套体系后，将具备：**自学习的多 Agent 编排 + 文档驱动的架构治理 + 持续优化的后台工人**。
