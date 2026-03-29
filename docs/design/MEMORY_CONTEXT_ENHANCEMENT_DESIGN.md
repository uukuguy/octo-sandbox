# Phase AG — 记忆和上下文机制增强设计文档

**日期**: 2026-03-29
**阶段**: Phase AG
**状态**: 设计完成，待实施
**基线**: 2476 tests, commit df54865, DB migration v11

---

## 1. 设计背景与目标

### 1.1 问题陈述

octo-sandbox 拥有完整的记忆基础设施（L0 Working Memory、L2 Persistent Store、Knowledge Graph、向量索引、混合查询引擎），但存在**五个关键断裂点**导致记忆系统形同虚设：

1. **会话结束不提取** — `SessionEndMemoryHook` 从未被 runtime 调用，L2 永远为空
2. **新会话不注入** — `MemoryInjector` 从未被调用，即使 L2 有数据也不使用
3. **搜索只有 FTS** — `VectorIndex`/`EmbeddingClient`/`HybridQueryEngine` 未接入实际搜索
4. **Zone B 只注入一次** — `ContextInjector.compile()` 在 loop 开始前调用，之后不再更新
5. **压缩只能截断** — `ContextPruner` 的 `Summarize` 策略返回 `NeedsSummarize` 但无人处理

同时，从认知科学三层记忆模型的视角审视，octo-sandbox **完全缺失情景记忆（Episodic）和程序记忆（Procedural）**：

| 记忆类型 | 认知对应 | 存储内容 | octo 现状 |
|---------|---------|---------|-----------|
| 语义记忆 | "我知道什么" | 事实、偏好、知识 | ⚠️ 基础设施就绪但未接线 |
| 情景记忆 | "我经历过什么" | 事件、对话摘要、时间线 | ❌ 完全缺失 |
| 程序记忆 | "我会怎么做" | 工作流模式、最佳实践 | ❌ 完全缺失 |

### 1.2 业界参考

本设计综合了以下业界最前沿的记忆系统实践：

#### ChatGPT Memory（OpenAI, 2025）
- **核心方案**: 不用 RAG，四层直接注入
  - Session Metadata → Saved Memories（长期事实）→ Recent Chat Summaries → Current Session
- **关键洞察**: "预计算摘要 + 直接注入" 比 RAG 更快更有效
- **记忆触发**: 模型自己决定什么值得存（通过内置 memory 工具）
- 参考: https://llmrefs.com/blog/reverse-engineering-chatgpt-memory

#### Letta/MemGPT（Letta, 2025-2026）
- **核心方案**: LLM-as-OS，agent 自己管理记忆
  - Core Memory（始终在上下文，可编辑）→ Archival Memory（向量存储）→ Recall Memory（对话历史）
- **关键洞察**: 记忆是 agent 可编辑的状态，不是只读检索
- **记忆工具**: `memory_replace`, `memory_insert`, `memory_rethink`, `archival_memory_search`
- 参考: https://docs.letta.com/advanced/memory-management/

#### Mem0（2025）
- **核心方案**: 语义 + 图谱双路记忆
- **Memory Candidate Selector**: 从对话中筛选"原子事实"
- **LoCoMo benchmark**: 比 OpenAI Memory 高 26%（66.9% vs 52.9%）
- 参考: https://arxiv.org/abs/2504.19413

#### A-Mem（NeurIPS 2025）
- **核心方案**: Zettelkasten 方法，记忆自动建立双向链接
- **关键洞察**: 互联的知识网络比扁平列表更有效
- 参考: https://openreview.net/forum?id=FiM0M8gcct

#### 程序记忆前沿研究
- **PRAXIS**: 从经验中存储行动后果，按状态匹配检索
- **MACLA** (AAMAS 2026): 分层程序记忆，贝叶斯可靠性跟踪
- **ACE Framework**: Generator → Reflector → Curator 三角循环
- 参考: https://arxiv.org/html/2511.22074, https://arxiv.org/html/2512.18950v1

### 1.3 设计原则

1. **接线优先** — 先把已实现的组件接上，投入最小价值最大
2. **ChatGPT 式简洁** — 预计算摘要 + 直接注入，不做复杂 RAG
3. **Agent 主动管理** — 借鉴 Letta，让 agent 自己决定记什么（通过工具）
4. **渐进式增强** — Tier 分层，核心功能先上，高级功能后续 Phase
5. **向后兼容** — 扩展现有类型和接口，不破坏已有功能

---

## 2. 架构设计

### 2.1 目标架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                    Agent Loop (harness.rs)                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Zone A (Static): SystemPromptBuilder                            │
│  ├── 现有: manifest + bootstrap + skills + core instructions      │
│  └── 【新增】记忆管理指令（告诉 agent 主动使用记忆工具）           │
│                                                                   │
│  Zone B (Dynamic): ContextInjector                               │
│  ├── 现有: WorkingMemory L0 blocks (UserProfile, TaskContext)     │
│  ├── 【新增】Cross-Session Memory 注入（MemoryInjector）          │
│  ├── 【新增】Recent Session Summaries 注入（最近 N 个会话摘要）    │
│  └── 【新增】周期性刷新（每 N 轮重新 compile）                     │
│                                                                   │
│  Zone C (Conversation): 对话历史                                  │
│  ├── 现有: 渐进式降级 (SoftTrim → AutoCompaction → Overflow)      │
│  ├── 【新增】ObservationMasker 接入（隐藏旧 tool output）         │
│  └── 【新增】Summarize 策略实现（压缩时 LLM 摘要）                │
│                                                                   │
│  Session Lifecycle:                                               │
│  ├── 【新增】Session Start → MemoryInjector 注入跨会话记忆        │
│  └── 【新增】Session End → SessionEndMemoryHook 提取 + 摘要生成   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                    Memory Subsystem                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  L0 Working Memory (InMemoryWorkingMemory / SqliteWorkingMemory) │
│  ├── 现有: UserProfile, TaskContext, AutoExtracted, Custom blocks  │
│  └── 【新增】Agent 可通过工具编辑 blocks (Letta 模式)             │
│                                                                   │
│  L2 Persistent Memory (SqliteMemoryStore)                        │
│  ├── 现有: MemoryEntry + FTS + 向量搜索 + RRF 融合               │
│  ├── 【新增】session_id 字段（关联来源会话）                       │
│  ├── 【新增】memory_type 字段（semantic/episodic/procedural）     │
│  ├── 【新增】时间范围查询支持                                      │
│  └── 【新增】事件结构化存储（EventExtractor 产出）                 │
│                                                                   │
│  Session Summaries (新增表)                                       │
│  ├── session_id, summary_text, event_count, created_at            │
│  └── 最近 N 个摘要注入 Zone B                                     │
│                                                                   │
│  Knowledge Graph (现有, 增强)                                     │
│  └── 现有 Entity + Relation + FTS（本 Phase 不改动 KG）           │
│                                                                   │
│  Agent Memory Tools:                                              │
│  ├── 现有: memory_search, memory_recall, memory_forget, memory_store │
│  ├── 现有: kg_add_entity, kg_add_relation, kg_search, kg_traverse  │
│  ├── 【新增】memory_edit — 编辑 Working Memory blocks             │
│  └── 【新增】memory_timeline — 按时间/范围/session 查询记忆        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 记忆类型扩展

在 `octo-types/src/memory.rs` 中扩展：

```rust
/// 记忆类型（认知科学三层模型）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// 语义记忆 — 事实、偏好、知识
    Semantic,
    /// 情景记忆 — 事件、对话摘要、时间线
    Episodic,
    /// 程序记忆 — 工作流模式、最佳实践（Phase AH）
    Procedural,
}

/// MemoryEntry 新增字段
pub struct MemoryEntry {
    // ... 现有字段 ...

    /// 记忆类型
    pub memory_type: MemoryType,  // 新增，默认 Semantic
    /// 来源会话 ID
    pub session_id: Option<String>,  // 新增
    /// 事件结构化数据（仅 Episodic 类型使用）
    pub event_data: Option<EventData>,  // 新增
}

/// 事件结构化数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    /// 事件类型: register, create, delete, deploy, configure, ...
    pub event_type: String,
    /// 事件目标: "MoltBook website", "database schema", ...
    pub target: String,
    /// 事件结果: success, failure, partial
    pub outcome: String,
    /// 关键数据: {"username": "octo-agent", "email": "..."}
    pub artifacts: serde_json::Value,
    /// 使用的工具链
    pub tool_chain: Vec<String>,
}
```

### 2.3 查询接口扩展

```rust
/// SearchOptions 新增字段
pub struct SearchOptions {
    // ... 现有字段 ...

    /// 时间范围过滤 (start_timestamp, end_timestamp)
    pub time_range: Option<(i64, i64)>,  // 新增
    /// 按来源会话过滤
    pub session_id: Option<String>,  // 新增
    /// 按记忆类型过滤
    pub memory_types: Option<Vec<MemoryType>>,  // 新增
    /// 排序方式
    pub sort_by: SortField,  // 新增，默认 Relevance
}

#[derive(Debug, Clone, Default)]
pub enum SortField {
    #[default]
    Relevance,
    CreatedAt,
    UpdatedAt,
    Importance,
}

/// MemoryFilter 新增字段
pub struct MemoryFilter {
    // ... 现有字段 ...

    pub time_range: Option<(i64, i64)>,  // 新增
    pub session_id: Option<String>,  // 新增
    pub memory_types: Option<Vec<MemoryType>>,  // 新增
}
```

### 2.4 数据库 Migration v12

```sql
-- Migration v12: Memory enhancement

-- 1. 新增字段到 memories 表
ALTER TABLE memories ADD COLUMN memory_type TEXT NOT NULL DEFAULT 'semantic';
ALTER TABLE memories ADD COLUMN session_id TEXT;
ALTER TABLE memories ADD COLUMN event_data TEXT;  -- JSON

-- 2. 新增索引
CREATE INDEX idx_memories_created_at ON memories(created_at);
CREATE INDEX idx_memories_session_id ON memories(session_id);
CREATE INDEX idx_memories_memory_type ON memories(memory_type);
CREATE INDEX idx_memories_type_time ON memories(memory_type, created_at);

-- 3. 会话摘要表
CREATE TABLE IF NOT EXISTS session_summaries (
    session_id TEXT PRIMARY KEY,
    summary TEXT NOT NULL,
    event_count INTEGER NOT NULL DEFAULT 0,
    key_topics TEXT,        -- JSON array of topic strings
    memory_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_session_summaries_created ON session_summaries(created_at);
```

### 2.5 事件提取器设计

```rust
/// 从 tool call chain 中提取结构化事件
pub struct EventExtractor;

impl EventExtractor {
    /// 从消息中提取事件（LLM 辅助）
    pub async fn extract_events(
        provider: &dyn Provider,
        messages: &[ChatMessage],
        model: &str,
    ) -> Result<Vec<EventData>> {
        // 1. 筛选包含 ToolUse + ToolResult 的消息对
        // 2. 构建提取 prompt:
        //    "从以下工具调用链中提取结构化事件。
        //     每个事件包含: event_type, target, outcome, artifacts, tool_chain。
        //     只提取有明确结果的操作（忽略纯查询）。"
        // 3. 调用 LLM 提取
        // 4. 解析 JSON 响应
    }
}
```

### 2.6 会话摘要生成

```rust
/// 会话结束时生成摘要
pub struct SessionSummarizer;

impl SessionSummarizer {
    /// 生成会话摘要
    pub async fn summarize(
        provider: &dyn Provider,
        messages: &[ChatMessage],
        model: &str,
    ) -> Result<SessionSummary> {
        // Prompt: "请用 2-3 句话总结这次对话的主要内容和结果。
        //          包括：做了什么、得到什么结果、有什么关键决策。"
        // 返回: SessionSummary { text, key_topics, event_count }
    }
}
```

### 2.7 跨会话注入流程（ChatGPT 式）

在 `harness.rs` 中，loop 开始前：

```rust
// --- Zone B: Inject working memory ---
// (现有代码)

// --- 【新增】Zone B+: Cross-session memory injection ---
if let Some(ref store) = config.memory_store {
    // 1. 注入最近 N 个会话摘要
    let summaries = SessionSummaryStore::recent(store, 5).await;
    if !summaries.is_empty() {
        let summary_block = format_session_summaries(&summaries);
        loop_steps::inject_after_zone_b(&mut messages, &summary_block);
    }

    // 2. 注入相关的跨会话记忆（基于首条用户消息语义搜索）
    let injector = MemoryInjector::with_defaults();
    let first_user_msg = messages.iter()
        .find(|m| m.is_user())
        .map(|m| m.text_content())
        .unwrap_or_default();
    let cross_session = injector
        .build_memory_context(store, &config.user_id, &first_user_msg)
        .await;
    if !cross_session.is_empty() {
        loop_steps::inject_after_zone_b(&mut messages, &cross_session);
    }
}
```

### 2.8 会话结束提取流程

在 session 结束时（`AgentExecutor::stop()` 或 session close API）：

```rust
// 1. 运行 SessionEndMemoryHook（现有组件，接线）
let hook = SessionEndMemoryHook::with_defaults();
let extracted = hook.on_session_end(&messages, store, &user_id).await;

// 2. 【新增】运行 EventExtractor
let events = EventExtractor::extract_events(provider, &messages, model).await?;
for event in events {
    let entry = MemoryEntry::new_episodic(&user_id, &event, &session_id);
    store.store(entry).await?;
}

// 3. 【新增】生成会话摘要
let summary = SessionSummarizer::summarize(provider, &messages, model).await?;
SessionSummaryStore::save(store, &session_id, &summary).await?;
```

### 2.9 memory_timeline 工具设计

```rust
/// Agent 可用的时间线查询工具
///
/// 参数:
///   - date: 查询指定日期 (格式: YYYY-MM-DD)
///   - range: 查询范围 (today, yesterday, last_week, last_month, 或 YYYY-MM-DD..YYYY-MM-DD)
///   - query: 模糊语义搜索 (如 "注册")，结果按时间排序
///   - session_id: 查询指定会话的所有记忆
///   - type: 过滤记忆类型 (semantic, episodic, procedural)
///   - limit: 返回数量上限 (默认 20)
///
/// 返回: 按时间排序的记忆列表，每条包含时间、类型、内容、关联会话
```

### 2.10 memory_edit 工具设计

```rust
/// Agent 可用的 Working Memory 编辑工具（Letta 模式）
///
/// 参数:
///   - action: "update" | "append" | "clear"
///   - block: "user_profile" | "task_context" | "custom:{name}"
///   - content: 新内容 (update 时完整替换, append 时追加)
///
/// 示例:
///   memory_edit(action="update", block="user_profile", content="用户偏好暗色模式，使用 Rust")
///   memory_edit(action="append", block="task_context", content="已完成 MoltBook 注册")
```

### 2.11 Zone B 周期性刷新

在 agent loop 每 N 轮（默认 5 轮）重新编译 Zone B：

```rust
// 在 harness.rs loop 内部
if round % ZONE_B_REFRESH_INTERVAL == 0 && round > 0 {
    if let Some(ref memory) = config.memory {
        let new_xml = memory.compile(&config.user_id, &config.sandbox_id).await?;
        loop_steps::refresh_zone_b(&mut messages, &new_xml);
        debug!("Zone B refreshed at round {}", round);
    }
}
```

### 2.12 ObservationMasker 接入

在 `harness.rs` 的降级检查之前：

```rust
// 在 compute_degradation_level 之前
let masker = ObservationMasker::default(); // keep_recent_turns=3
let masked = masker.mask(&messages);
// 用 masked 版本计算 token 和发送请求
// 原始 messages 保留用于记忆提取
```

### 2.13 System Prompt 记忆指令

在 `SystemPromptBuilder` 的 core instructions 中追加：

```
## Memory Management

You have access to a persistent memory system that survives across sessions.

### Automatic behaviors:
- Important facts, preferences, and decisions are automatically extracted at session end.
- Events (tool operations with clear outcomes) are automatically recorded.

### Your responsibilities:
- When you learn important NEW information about the user (name, preferences, goals),
  use `memory_store` to save it immediately. Don't wait for session end.
- When you complete a significant action (registration, deployment, configuration),
  the system will auto-record it, but you can add context via `memory_store`.
- Use `memory_timeline` to answer questions about past events and history.
- Use `memory_edit` to update your working context as tasks evolve.
- Use `memory_search` to recall relevant past knowledge before making decisions.
```

---

## 3. 影响范围分析

### 3.1 需要修改的文件

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `octo-types/src/memory.rs` | 扩展 | +MemoryType, +EventData, +SortField; SearchOptions/MemoryFilter/MemoryEntry 新增字段 |
| `octo-engine/src/memory/sqlite_store.rs` | 扩展 | search/list SQL 支持 time_range/session_id/memory_type/sort_by |
| `octo-engine/src/memory/session_hook.rs` | 不改 | 现有代码不变，只需在外部调用 |
| `octo-engine/src/memory/memory_injector.rs` | 不改 | 现有代码不变，只需在外部调用 |
| `octo-engine/src/agent/harness.rs` | 扩展 | +MemoryInjector 接线, +Zone B 刷新, +ObservationMasker |
| `octo-engine/src/agent/executor.rs` | 扩展 | +session end hook 调用 |
| `octo-engine/src/db/mod.rs` | 扩展 | Migration v12 |
| `octo-engine/src/memory/mod.rs` | 扩展 | +EventExtractor, +SessionSummarizer, +SessionSummaryStore |
| `octo-engine/src/tools/` | 新增 | memory_timeline.rs, memory_edit.rs |
| `octo-engine/src/context/system_prompt.rs` | 扩展 | +记忆管理指令 |

### 3.2 不需要修改的文件

- `octo-engine/src/memory/working.rs` — L0 接口不变
- `octo-engine/src/memory/graph*.rs` — KG 本 Phase 不改动
- `octo-engine/src/memory/vector_index.rs` — 向量索引本 Phase 不改动
- `octo-engine/src/memory/hybrid_query.rs` — 混合查询引擎本 Phase 不改动（留给 Phase AH）
- `octo-engine/src/context/pruner.rs` — Summarize 策略实现在 harness 层处理
- `web/` — 前端本 Phase 不改动

### 3.3 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| LLM 调用增加（事件提取 + 会话摘要） | 延迟、成本 | 仅在 session end 时调用一次；使用 haiku/小模型 |
| MemoryEntry schema 变更 | 旧数据兼容 | 新增字段均有默认值；ALTER TABLE ADD COLUMN |
| Zone B 刷新频率过高 | 性能 | 默认 5 轮一次；可配置 |
| 事件提取误报 | 噪声记忆 | confidence 阈值过滤；用户可通过 memory_forget 删除 |

---

## 4. Deferred Items（Phase AH+）

| ID | 内容 | 前置条件 | 优先级 |
|----|------|---------|--------|
| AG-D1 | 程序记忆提取（工作流模式学习） | AG 完成 + Skill 执行数据积累 | P2 |
| AG-D2 | 情景→语义巩固（高频事件自动升级） | AG 完成 + 足够的 episodic 数据 | P3 |
| AG-D3 | 智能遗忘（时间衰减 + 访问频率自动清理） | AG 完成 | P3 |
| AG-D4 | 记忆冲突解决（新旧事实矛盾更新） | AG 完成 | P3 |
| AG-D5 | HybridQueryEngine 接入 memory tools | AG 完成 | P2 |
| AG-D6 | KG 语义搜索（实体向量化） | AG 完成 + Embedding 链路稳定 | P3 |
| AG-D7 | Summarize 压缩策略（LLM 摘要替代截断） | AG 完成 | P2 |
| AG-D8 | Memory Explorer 前端页面增强 | AG 完成 | P3 |

---

## 5. 参考资料

- [Mem0 论文 (arXiv 2504.19413)](https://arxiv.org/abs/2504.19413)
- [Letta/MemGPT 记忆管理文档](https://docs.letta.com/advanced/memory-management/)
- [Memory in the Age of AI Agents (arXiv 2512.13564)](https://arxiv.org/abs/2512.13564)
- [ChatGPT Memory 逆向工程](https://llmrefs.com/blog/reverse-engineering-chatgpt-memory)
- [A-Mem: Agentic Memory (NeurIPS 2025)](https://openreview.net/forum?id=FiM0M8gcct)
- [PRAXIS: Real-Time Procedural Learning](https://arxiv.org/html/2511.22074)
- [MACLA: Hierarchical Procedural Memory](https://arxiv.org/html/2512.18950v1)
- [Benchmarking AI Agent Memory (Letta)](https://www.letta.com/blog/benchmarking-ai-agent-memory)
- [Agent Memory Paper List (GitHub)](https://github.com/Shichun-Liu/Agent-Memory-Paper-List)
- 现有设计文档: `docs/design/CONTEXT_ENGINEERING_DESIGN.md`, `docs/design/MEMORY_PLAN.md`
