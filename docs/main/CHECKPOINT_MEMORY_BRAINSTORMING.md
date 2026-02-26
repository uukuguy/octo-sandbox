# octo-sandbox 记忆模块架构设计 Brainstorming（第八段）

**日期**: 2026-02-26
**阶段**: 记忆模块深度分析 + 架构设计
**状态**: ✅ 综合分析完成

---

## 一、参考项目深度分析总结

对 `./github.com/memory/` 下 6 个记忆专项项目 + `openclaw` + 类 OpenClaw 项目（nanoclaw/zeroclaw/happyclaw）+ 智能体框架（pi_agent_rust/pi-mono/craft-agents-oss）+ 上下文工程最佳实践（Agent-Skills-for-Context-Engineering）进行了代码级深度分析。

### 项目对照矩阵

| 项目 | 语言 | 存储后端 | 记忆层次 | 检索方式 | 核心亮点 |
|------|------|---------|---------|---------|---------|
| **mem0** | Python | 28+ 向量库 + Neo4j/Memgraph 知识图谱 | 语义/情景/程序性 | 向量 + 图谱双路并发 | LLM 驱动事实提取，ADD/UPDATE/DELETE 决策 |
| **Letta (MemGPT)** | Python | pgvector/Turbopuffer/Pinecone | 三层：Core(上下文)/Archival(向量)/Recall(消息) | 自编辑 memory tools | Agent 自主修改核心记忆块，Memory.compile() |
| **OpenViking** | Rust+Python+C++ | 自研 C++ VectorDB (SQLite) | L0/L1/L2 三级上下文精度 | 分层意图检索 + 目录递归 | viking:// URI 协议，6 类记忆分类 |
| **agent-file** | 多语言 | JSON 文件 | 标签化记忆块 | 工具调用 | .af 跨平台序列化格式 |
| **memos** | Go | SQLite/MySQL/PostgreSQL | 单层 Memo | 全文搜索 | Protobuf API，插件系统 |
| **openclaw** | TypeScript | SQLite + sqlite-vec + FTS5 | 文件级/会话级 | 混合检索 0.7 向量 + 0.3 FTS | 压缩前记忆刷写，Hash 变更检测 |
| **zeroclaw** | Rust | SQLite + FTS5 + embedding cache | 4 类别（core/daily/conversation/custom） | 混合搜索 | Memory trait，MAX_HISTORY=50 |
| **happyclaw** | TypeScript | SQLite | 三层（用户全局/会话/日期） | FTS + 关键词 | 多用户隔离，PreCompact hook，RBAC |
| **pi_agent_rust** | Rust | JSONL 文件 | 会话级树形分支 | 线性扫描 | 自动压缩（200K 上下文），零拷贝 Cow<> |
| **craft-agents-oss** | TypeScript | JSONL 文件 | 工作区级 | 快速头部加载（8KB） | 大结果自动摘要（60KB 阈值） |

---

## 二、关键设计模式提取

### 模式 1：LLM 驱动的事实提取（mem0 模式）

**核心机制**：
```
用户对话 → LLM 提取事实（结构化 JSON）→ 与现有记忆比对 → ADD/UPDATE/DELETE/NONE 决策
```

**关键实现**（mem0 `memory/main.py`）：
- 使用专用提示词提取事实：`USER_MEMORY_EXTRACTION_PROMPT`（用户记忆）/ `AGENT_MEMORY_EXTRACTION_PROMPT`（智能体记忆）
- 双路并发写入：向量存储 + 知识图谱同时执行
- 事实去重：对比现有记忆，由 LLM 判断是新增、更新还是删除

**octo-sandbox 适用性**：⭐⭐⭐⭐ 高度适用
- 沙箱调试场景下，用户的工具偏好、配置习惯、常用命令都是宝贵的跨会话记忆
- Rust 实现可直接调用 LLM Provider Trait 完成事实提取
- 知识图谱对 MVP 而言过重，Phase 1 仅用向量存储

### 模式 2：三层自编辑记忆（Letta/MemGPT 模式）

**核心架构**：
```
┌─────────────────────────────────────────┐
│  Core Memory（上下文内，~8K chars）       │  ← Agent 可直接读写
│  - human: 用户信息块                     │
│  - persona: Agent 人设块                 │
│  - 自定义块...                           │
├─────────────────────────────────────────┤
│  Archival Memory（向量索引，无限）        │  ← archival_memory_insert/search
├─────────────────────────────────────────┤
│  Recall Memory（消息历史，向量索引）      │  ← conversation_search
└─────────────────────────────────────────┘
```

**关键机制**：
- `Memory.compile()` 将核心记忆块渲染进系统提示词，3 种策略可选
- Agent 通过工具自主修改记忆：`core_memory_append()`, `core_memory_replace()`, `rethink_memory()`
- `memory()` 工具甚至支持 git 版本控制记忆变更
- 摘要器：`STATIC_MESSAGE_BUFFER` 保留最近 N 条，`PARTIAL_EVICT` 按比例淘汰（~30%）

**octo-sandbox 适用性**：⭐⭐⭐⭐⭐ 核心参考
- 三层分离非常适合沙箱场景：
  - Core = 当前沙箱配置 + 用户偏好（始终在上下文中）
  - Archival = 工具使用历史、调试日志、MCP Server 文档（按需检索）
  - Recall = 会话消息历史（支持搜索回顾）
- Agent 自编辑核心记忆的理念可以直接采用

### 模式 3：分层上下文精度（OpenViking L0/L1/L2 模式）

**核心理念**：同一知识以不同精度级别存储，按需加载
```
L0（抽象层）：~100 tokens    → "该用户偏好 Docker 运行模式，常用 Python 工具"
L1（概要层）：~2,000 tokens  → 用户配置详情、常用命令列表、MCP Server 清单
L2（完整层）：完整内容        → 完整对话记录、工具执行日志、配置文件全文
```

**关键实现**：
- `HierarchicalRetriever`：先意图分析 → 确定目标目录 → 目录递归搜索 → 分数传播
- `viking://` URI 统一寻址：`viking://user/{user_id}/memory/preferences`
- 6 类记忆分类：
  - 用户侧：profile（身份）、preferences（偏好）、entities（实体）、events（事件）
  - Agent 侧：cases（案例）、patterns（模式）

**octo-sandbox 适用性**：⭐⭐⭐⭐ 高度适用
- 渐进式披露（Progressive Disclosure）直接优化 token 使用效率
- 从 L0 摘要开始，仅在需要时深入到 L2 详情
- 6 类分类体系可以简化为 4 类适应沙箱场景

### 模式 4：压缩前记忆刷写（openclaw 模式）

**核心机制**：上下文窗口即将压缩/摘要前，先将重要记忆持久化
```
上下文接近限制 → 触发 PreCompact Hook → 提取本轮对话关键事实 → 写入持久存储 → 执行压缩/摘要
```

**关键实现**（openclaw + happyclaw）：
- `agent-runner-memory.ts`：压缩前执行记忆刷写
- happyclaw `PreCompact` hook：自动存档会话记忆到日期分区
- 混合检索：向量相似度 0.7 权重 + FTS 全文搜索 0.3 权重
- Hash 变更检测：仅同步变更的记忆文件
- 分块策略：400 tokens/chunk，80 tokens overlap

**octo-sandbox 适用性**：⭐⭐⭐⭐⭐ 必须采用
- 直接解决长会话记忆丢失问题
- Context Manager（已在第二段设计中）的核心扩展点
- Rust 实现：在 compaction 流程中插入 memory_flush 步骤

### 模式 5：上下文工程最佳实践（Agent-Skills-for-Context-Engineering）

**五大上下文退化模式**：
1. **Lost-in-the-Middle**：中间位置信息被忽略（注意力 U 型曲线）
2. **Context Poisoning**：错误信息被当作真实信息处理
3. **Context Distraction**：不相关信息降低整体质量
4. **Context Confusion**：矛盾信息导致不一致输出
5. **Context Clash**：不同来源的指令冲突

**三大压缩策略**（含基准数据）：
| 策略 | 压缩率 | 质量（1-5） | 适用场景 |
|------|--------|------------|---------|
| 锚定迭代 (Anchored Iterative) | 98.6% | 3.70 ⭐最佳 | 需要高质量的重要记忆 |
| 不透明 (Opaque) | 99.3% | 3.35 | 归档存储，最大压缩 |
| 再生式 (Regenerative) | 98.7% | 3.44 | 结构化知识重建 |

**关键优化原则**：
- **tokens-per-task 而非 tokens-per-request**：优化整体任务效率
- **渐进式披露**：仅加载当前步骤所需的信息（650 tokens vs 5000+）
- **模块化注入**：Digital Brain 示例 6 个模块，每次任务仅注入相关模块
- **结构化注入**：用 JSONL、XML 标签等结构化格式注入记忆
- **Probe-based 测试**：用探针问题验证上下文质量

**octo-sandbox 适用性**：⭐⭐⭐⭐⭐ 必须遵循
- 记忆注入必须遵循反退化原则
- 关键记忆放在上下文首尾（避免 Lost-in-the-Middle）
- 采用结构化格式注入（XML/JSONL），避免自由文本混杂
- 实现 token budget 管理器

---

## 三、octo-sandbox 记忆模块架构设计

### 3.1 总体架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Memory Manager（记忆管理器）                │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Memory Store  │  │ Memory       │  │ Context      │      │
│  │ (持久化层)    │  │ Retriever    │  │ Injector     │      │
│  │              │  │ (检索层)      │  │ (注入层)      │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                  │              │
│  ┌──────┴─────────────────┴──────────────────┴───────┐      │
│  │              Memory Index (SQLite WAL)              │      │
│  │  ┌─────────┐  ┌──────────┐  ┌──────────────┐      │      │
│  │  │ FTS5    │  │ Vec Store │  │ Metadata     │      │      │
│  │  │ 全文索引 │  │ 向量索引   │  │ 结构化索引    │      │      │
│  │  └─────────┘  └──────────┘  └──────────────┘      │      │
│  └────────────────────────────────────────────────────┘      │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Fact         │  │ Memory       │  │ Token Budget │      │
│  │ Extractor    │  │ Compressor   │  │ Manager      │      │
│  │ (事实提取器)  │  │ (记忆压缩器)  │  │ (预算管理器)  │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 四层记忆架构

采用 **Letta 三层 + OpenViking 精度分级** 的混合设计：

```
┌─────────────────────────────────────────────────────────┐
│  Layer 0: Working Memory（工作记忆 — 上下文窗口内）       │
│                                                         │
│  [System Prompt Blocks]                                 │
│  ├── sandbox_context: 当前沙箱配置、运行时状态            │
│  ├── user_profile: 用户偏好、技能水平、常用工具            │
│  ├── agent_persona: Agent 人设和行为规则                  │
│  └── task_context: 当前任务上下文、关键决策               │
│                                                         │
│  容量：~4K tokens（可配置），始终在 system prompt 中       │
│  更新：Agent 自编辑（memory_update 工具）                 │
├─────────────────────────────────────────────────────────┤
│  Layer 1: Session Memory（会话记忆 — 短期）              │
│                                                         │
│  本次会话的完整消息历史 + 工具执行记录                    │
│  - 最近 N 条消息保留原文                                 │
│  - 较早消息自动摘要压缩                                  │
│  - 工具执行结果保留结构化摘要                             │
│                                                         │
│  容量：受上下文窗口限制，自动管理                         │
│  存储：内存 + SQLite（会话结束后持久化）                  │
├─────────────────────────────────────────────────────────┤
│  Layer 2: Persistent Memory（持久记忆 — 长期）           │
│                                                         │
│  跨会话持久化的结构化知识：                               │
│  - 用户偏好和习惯（从对话中自动提取）                     │
│  - 工具使用模式和最佳实践                                │
│  - 沙箱配置历史和调试经验                                │
│  - MCP Server 使用笔记                                   │
│  - 错误解决方案和 workaround                             │
│                                                         │
│  存储：SQLite WAL（向量索引 + FTS5 全文索引）             │
│  检索：混合检索（向量 0.7 + FTS 0.3 权重）               │
├─────────────────────────────────────────────────────────┤
│  Layer 3: Archive Memory（归档记忆 — 冷存储）            │
│                                                         │
│  - 已结束会话的压缩摘要                                  │
│  - 历史工具执行日志（结构化）                             │
│  - 日期分区的时序记忆                                    │
│                                                         │
│  存储：SQLite archive 表 / JSONL 导出                    │
│  检索：仅在显式搜索时加载                                │
└─────────────────────────────────────────────────────────┘
```

### 3.3 核心组件设计

#### A. Memory Store（持久化层）

**数据库 Schema**（SQLite WAL，与已确认的 Session Store 统一）：

```sql
-- 核心记忆表（Layer 2 持久记忆）
CREATE TABLE memories (
    id          TEXT PRIMARY KEY,        -- ULID（时间有序）
    user_id     TEXT NOT NULL,           -- 所属用户
    sandbox_id  TEXT,                    -- 关联沙箱（可为空 = 全局记忆）
    category    TEXT NOT NULL,           -- profile/preferences/tools/debug/patterns
    content     TEXT NOT NULL,           -- 记忆内容（纯文本）
    metadata    TEXT,                    -- JSON 元数据（来源、置信度等）
    embedding   BLOB,                   -- 向量嵌入（f32 数组）
    created_at  INTEGER NOT NULL,        -- Unix timestamp
    updated_at  INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,        -- 最后访问时间（用于衰减）
    access_count INTEGER DEFAULT 0,      -- 访问次数
    importance  REAL DEFAULT 0.5,        -- 重要性评分 0.0-1.0
    ttl         INTEGER,                -- 可选过期时间
    source_type TEXT NOT NULL,           -- extracted/manual/system
    source_ref  TEXT                     -- 来源引用（session_id/tool_execution_id）
);

-- 向量索引（使用 sqlite-vec 或内置 BLOB + 余弦相似度）
-- Phase 1: 内置余弦相似度（纯 Rust，无外部依赖）
-- Phase 2: 可选 sqlite-vec 扩展

-- FTS5 全文索引
CREATE VIRTUAL TABLE memories_fts USING fts5(
    content,
    category,
    content=memories,
    content_rowid=rowid,
    tokenize='porter unicode61'
);

-- 会话记忆表（Layer 1 的持久化部分）
CREATE TABLE session_memories (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    role        TEXT NOT NULL,           -- user/assistant/system/tool
    content     TEXT NOT NULL,
    summary     TEXT,                    -- 压缩后的摘要
    tool_calls  TEXT,                    -- JSON: 关联的工具调用
    token_count INTEGER,
    created_at  INTEGER NOT NULL,
    is_pinned   INTEGER DEFAULT 0        -- 固定不被压缩
);

-- 归档表（Layer 3）
CREATE TABLE memory_archive (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL,
    archive_date TEXT NOT NULL,          -- YYYY-MM-DD 日期分区
    session_id  TEXT,
    summary     TEXT NOT NULL,           -- 压缩摘要
    key_facts   TEXT,                    -- JSON: 关键事实列表
    metadata    TEXT,
    created_at  INTEGER NOT NULL
);

-- 记忆块定义表（Working Memory 的结构化配置）
CREATE TABLE memory_blocks (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL,
    sandbox_id  TEXT,
    label       TEXT NOT NULL,           -- sandbox_context/user_profile/agent_persona/task_context
    value       TEXT NOT NULL DEFAULT '',
    char_limit  INTEGER NOT NULL DEFAULT 2000,
    is_readonly INTEGER DEFAULT 0,
    updated_at  INTEGER NOT NULL
);
```

**记忆分类体系**（简化 OpenViking 6 类为 5 类）：

| 类别 | 说明 | 示例 |
|------|------|------|
| `profile` | 用户身份和背景 | "用户是 Rust 开发者，3 年经验" |
| `preferences` | 使用偏好和习惯 | "偏好 Docker 运行模式，使用 vim 编辑" |
| `tools` | 工具使用知识 | "MCP Server X 需要先配置 API Key" |
| `debug` | 调试经验和解决方案 | "WASM 运行时内存限制需设为 256MB" |
| `patterns` | 工作模式和最佳实践 | "该用户习惯先测试再提交" |

#### B. Memory Retriever（检索层）

**混合检索策略**（参考 openclaw 0.7/0.3 + OpenViking 分层）：

```
查询请求
  │
  ├── 1. 意图分析（轻量 LLM 调用或规则匹配）
  │     → 确定查询类别 + 目标记忆层
  │
  ├── 2. 向量检索（语义相似度）
  │     → embedding(query) → cosine_similarity → top-K
  │     → 权重: 0.7
  │
  ├── 3. FTS5 检索（关键词匹配）
  │     → BM25 评分
  │     → 权重: 0.3
  │
  ├── 4. 分数融合 + 时间衰减
  │     → score = 0.7 * vec_score + 0.3 * fts_score
  │     → score *= time_decay(accessed_at)     // 近期记忆权重更高
  │     → score *= importance                   // 重要性加权
  │
  └── 5. 结果排序 + Token 预算裁剪
        → 按分数排序
        → 按 token budget 截断
        → 返回结构化结果
```

**渐进式精度加载**（L0/L1/L2 参考 OpenViking）：

```rust
enum MemoryPrecision {
    Abstract,   // ~100 tokens — 用于系统提示词中的概要
    Summary,    // ~500 tokens — 用于检索结果的摘要展示
    Full,       // 完整内容 — 用于用户显式请求查看详情
}
```

#### C. Fact Extractor（事实提取器）

**参考 mem0 的 LLM 驱动提取，但 Rust 原生实现**：

```
对话消息（用户/助手）
  │
  ├── 1. 触发条件判断
  │     - 每 N 轮对话自动触发（默认 5 轮）
  │     - 用户显式提到偏好/配置时立即触发
  │     - 会话结束/压缩前强制触发
  │
  ├── 2. 事实提取（调用 LLM Provider）
  │     - 输入：最近对话 + 提取提示词
  │     - 输出：结构化事实列表
  │     [
  │       {"fact": "用户偏好 Docker 模式", "category": "preferences", "importance": 0.8},
  │       {"fact": "WASM 工具需要 256MB 内存", "category": "debug", "importance": 0.9}
  │     ]
  │
  ├── 3. 去重与合并（对比现有记忆）
  │     - 向量相似度 > 0.85 → 视为重复
  │     - LLM 判断：ADD（新增）/ UPDATE（更新）/ DELETE（过时）/ NONE（忽略）
  │
  └── 4. 写入持久存储
        - 生成 embedding
        - 写入 memories 表
        - 更新 FTS5 索引
```

**提取提示词设计**（中英双语，参考 mem0 提示词工程）：

```
你是一个记忆提取器。分析以下对话，提取值得长期记住的事实。

规则：
1. 只提取明确的事实，不推测
2. 偏好、配置、经验最重要
3. 一次性的临时信息忽略
4. 每条事实独立且自包含
5. 标注类别和重要性

输出 JSON 数组：
[{"fact": "...", "category": "profile|preferences|tools|debug|patterns", "importance": 0.0-1.0}]
```

#### D. Context Injector（上下文注入器）

**遵循上下文工程最佳实践的注入策略**：

```
构建 System Prompt
  │
  ├── 1. 基础层（始终存在）
  │     [SYSTEM INSTRUCTIONS]
  │     核心系统指令
  │
  ├── 2. Working Memory 注入（Layer 0）
  │     [MEMORY: sandbox_context]
  │     当前沙箱: Docker 模式, Python 3.12, MCP Server: filesystem
  │     [/MEMORY]
  │     [MEMORY: user_profile]
  │     用户偏好: Rust 开发者, 偏好 vim, 习惯先测试再提交
  │     [/MEMORY]
  │
  ├── 3. 相关记忆注入（Layer 2 检索结果）
  │     [RECALLED MEMORIES]
  │     - 该 MCP Server 需要先设置 ANTHROPIC_API_KEY 环境变量 [tools, 0.92]
  │     - Docker 运行时需要挂载 /workspace 目录 [debug, 0.87]
  │     [/RECALLED MEMORIES]
  │
  └── 4. 会话历史（Layer 1）
        [CONVERSATION HISTORY]
        最近消息...
        [/CONVERSATION HISTORY]
```

**反退化设计原则**：
1. **关键记忆前置**：Working Memory 块紧跟系统指令，避免 Lost-in-the-Middle
2. **结构化标签**：使用 `[MEMORY]` / `[RECALLED MEMORIES]` 等标签，帮助模型区分来源
3. **去重过滤**：注入前检查与当前对话是否重复，避免 Context Distraction
4. **一致性检查**：检测矛盾记忆，优先使用最新的，避免 Context Confusion
5. **Token 预算硬限制**：记忆注入不超过总上下文的 15%

#### E. Token Budget Manager（Token 预算管理器）

**参考 Letta 的 ContextWindowCalculator，Rust 原生实现**：

```rust
struct TokenBudget {
    total_limit: usize,          // 总上下文窗口（如 200K）
    system_prompt_budget: usize, // 系统提示词预算（~10K）
    memory_budget: usize,        // 记忆注入预算（总量的 15%）
    tool_defs_budget: usize,     // 工具定义预算
    history_budget: usize,       // 会话历史预算（剩余空间）
    reserve: usize,              // 预留（输出空间 + 安全边际，~16K）
}

impl TokenBudget {
    fn allocate(&self) -> BudgetAllocation {
        // 1. 固定分配：system_prompt + tool_defs + reserve
        // 2. 弹性分配：memory + history 共享剩余空间
        // 3. 记忆优先：先分配 memory_budget（上限 15%），剩余给 history
    }
}
```

#### F. Memory Compressor（记忆压缩器）

**三阶段压缩流程**（参考 Letta + openclaw + pi_agent_rust）：

```
上下文接近限制（>80% 使用率）
  │
  ├── Stage 1: 记忆刷写（Memory Flush）
  │     - 提取本轮对话中的新事实 → 写入 Layer 2
  │     - 保存当前 Working Memory 块状态
  │     - 这步必须在压缩前完成！
  │
  ├── Stage 2: 消息压缩（Message Compaction）
  │     - 保留最近 N 条消息不压缩（STATIC_BUFFER = 10）
  │     - 较早消息进行锚定迭代压缩（最佳质量 3.70/5）
  │     - 压缩摘要注入到消息历史开头
  │     - 工具调用结果压缩为结构化摘要
  │
  └── Stage 3: 归档（Archive）
        - 压缩后的完整会话摘要 → Layer 3 归档
        - 更新日期分区索引
```

### 3.4 Memory Trait 设计（Rust）

```rust
/// 核心记忆 Trait — 统一接口
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// 存储新记忆
    async fn store(&self, entry: MemoryEntry) -> Result<MemoryId>;

    /// 混合检索（向量 + FTS）
    async fn search(&self, query: &str, opts: SearchOptions) -> Result<Vec<MemoryResult>>;

    /// 按 ID 获取
    async fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;

    /// 更新记忆内容
    async fn update(&self, id: &MemoryId, content: &str) -> Result<()>;

    /// 删除记忆
    async fn delete(&self, id: &MemoryId) -> Result<()>;

    /// 按类别列出
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>>;

    /// 批量写入（事实提取后）
    async fn batch_store(&self, entries: Vec<MemoryEntry>) -> Result<Vec<MemoryId>>;
}

/// 检索选项
pub struct SearchOptions {
    pub user_id: UserId,
    pub sandbox_id: Option<SandboxId>,
    pub categories: Vec<MemoryCategory>,    // 过滤类别
    pub precision: MemoryPrecision,          // Abstract/Summary/Full
    pub limit: usize,                        // 最大返回数
    pub token_budget: usize,                 // Token 预算限制
    pub min_score: f32,                      // 最低相关性阈值
    pub time_decay: bool,                    // 是否启用时间衰减
}

/// 记忆条目
pub struct MemoryEntry {
    pub id: MemoryId,
    pub user_id: UserId,
    pub sandbox_id: Option<SandboxId>,
    pub category: MemoryCategory,
    pub content: String,
    pub summary: Option<String>,             // L0/L1 摘要
    pub metadata: serde_json::Value,
    pub embedding: Option<Vec<f32>>,
    pub importance: f32,
    pub source: MemorySource,
    pub timestamps: MemoryTimestamps,
}

/// Working Memory 块管理
#[async_trait]
pub trait WorkingMemory: Send + Sync {
    /// 获取所有活跃记忆块
    async fn get_blocks(&self, user_id: &UserId, sandbox_id: &SandboxId) -> Result<Vec<MemoryBlock>>;

    /// 更新记忆块（Agent 自编辑）
    async fn update_block(&self, block_id: &str, value: &str) -> Result<()>;

    /// 追加到记忆块
    async fn append_block(&self, block_id: &str, content: &str) -> Result<()>;

    /// 编译为 system prompt 片段
    async fn compile(&self, user_id: &UserId, sandbox_id: &SandboxId) -> Result<String>;
}
```

### 3.5 Agent Memory Tools（智能体记忆工具）

参考 Letta 的自编辑理念，为 octo-sandbox Agent 提供 5 个记忆工具：

| 工具名 | 功能 | 参考来源 |
|--------|------|---------|
| `memory_store` | 显式存储一条记忆到持久层 | mem0 add / Letta archival_memory_insert |
| `memory_search` | 搜索持久记忆（混合检索） | openclaw memory_search / Letta archival_memory_search |
| `memory_update` | 更新 Working Memory 块 | Letta core_memory_replace |
| `memory_recall` | 搜索会话历史 | Letta conversation_search |
| `memory_forget` | 删除/过期指定记忆 | zeroclaw forget |

**关键设计决策**：
- Agent 可以通过 `memory_update` 自主修改自己的 Working Memory（如更新对用户的理解）
- 事实提取是后台自动进行的，不作为 Agent 工具暴露
- `memory_search` 默认使用混合检索，对 Agent 透明

### 3.6 Embedding 策略

**Phase 1（MVP，零外部依赖）**：
- 使用 LLM Provider 的 embedding API（Anthropic → Voyage AI，OpenAI → text-embedding-3-small）
- 向量维度：1024（Voyage）或 1536（OpenAI）
- 缓存策略：SQLite 存储 embedding，避免重复计算
- 分块：400 tokens/chunk，80 tokens overlap（参考 openclaw）

**Phase 2（可选优化）**：
- 本地 embedding 模型（ONNX Runtime，如 all-MiniLM-L6-v2）
- 混合 embedding：dense + sparse（参考 OpenViking）
- sqlite-vec 扩展加速向量检索

**余弦相似度 Rust 实现**（Phase 1 无需外部库）：
```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    dot / (norm_a * norm_b)
}
```

### 3.7 多用户记忆隔离

**与第四段 RBAC + Per-User 隔离设计统一**：

```
data/users/{user_id}/
├── memory/
│   ├── blocks/           # Working Memory 块（JSON）
│   ├── memories.db       # 该用户的 SQLite 记忆库（或共享库中按 user_id 隔离）
│   └── archive/          # 归档 JSONL
├── sessions/
└── sandboxes/

data/shared/
├── memory/
│   └── system_facts.db   # 系统级共享知识（工具文档、通用最佳实践）
└── ...
```

**隔离规则**：
- 每个用户的记忆完全隔离（user_id 过滤）
- 沙箱级记忆可选隔离（sandbox_id 过滤）
- 系统级知识（如 MCP 工具文档）所有用户共享
- Admin 可查看所有用户记忆（审计需求）
- Viewer 不可访问记忆系统

---

## 四、与现有架构的集成点

### 4.1 与 Agent Engine（第二段）的集成

```
Agent Loop 每轮迭代
  │
  ├── 1. Context Manager 构建提示词
  │     └── 调用 Memory Manager 的 Context Injector
  │         ├── compile Working Memory blocks
  │         ├── 检索相关持久记忆（基于当前对话意图）
  │         └── 注入结构化记忆到 system prompt
  │
  ├── 2. Agent 执行（可能调用 memory tools）
  │     ├── memory_store: 显式保存记忆
  │     ├── memory_search: 搜索历史经验
  │     ├── memory_update: 更新工作记忆块
  │     └── memory_recall: 搜索会话历史
  │
  ├── 3. 后台事实提取（每 N 轮 / 触发条件）
  │     └── Fact Extractor 异步处理
  │
  └── 4. 上下文压缩触发（接近限制时）
        └── Memory Compressor 执行三阶段压缩
            ├── Stage 1: Memory Flush（最重要！）
            ├── Stage 2: Message Compaction
            └── Stage 3: Archive
```

### 4.2 与 Session Store（第一段）的集成

- Session Store 和 Memory Store 共享同一 SQLite WAL 数据库
- Session 表存会话元数据和消息历史
- Memory 表存持久化记忆
- 两者通过 `session_id` / `user_id` 关联
- Session 结束时触发记忆归档

### 4.3 与调试面板（第五段）的集成

新增 **Memory 调试页面**（Tab 扩展）：

| 功能 | 说明 |
|------|------|
| Memory Explorer | 浏览/搜索持久记忆，按类别/时间/重要性过滤 |
| Working Memory Viewer | 实时查看当前 Working Memory 块状态 |
| Fact Extraction Log | 事实提取历史，显示 ADD/UPDATE/DELETE 决策过程 |
| Token Budget Dashboard | 实时上下文 token 分配可视化 |
| Memory Search Tester | 手动输入查询，查看混合检索结果和分数 |

### 4.4 与 Web UI（第六段）的集成

- 新增 Tab：Chat | **Memory** | Tools | MCP | Skills | Compare | Debug
- Jotai atoms：`memoryBlocksAtom`, `memorySearchAtom`, `tokenBudgetAtom`
- WebSocket 事件扩展：`memory_block_update`, `memory_extracted`, `token_budget_update`

### 4.5 与 MVP 路线图的集成

| Phase | 记忆模块内容 |
|-------|-------------|
| **Phase 1** | 内存 Working Memory 块 + 基础 Context Injector + Token Budget Manager + 内存 Session Memory |
| **Phase 2** | SQLite 持久记忆 + FTS5 全文搜索 + 基础 Fact Extractor + Memory Flush + 3 个 memory tools |
| **Phase 3** | 向量检索 + 混合检索 + 完整 5 个 memory tools + 多用户隔离 + Memory 调试页面 |
| **Phase 4** | 记忆压缩优化 + 本地 embedding + 记忆导入导出 + 归档策略 + 性能调优 |

---

## 五、关键技术决策汇总

| 决策项 | 选择 | 理由 |
|--------|------|------|
| 记忆层次 | 四层（Working/Session/Persistent/Archive） | 综合 Letta 三层 + 归档层，覆盖全场景 |
| 持久存储 | SQLite WAL（统一数据库） | 与已确认的 Session Store 一致，零外部依赖 |
| 全文检索 | FTS5（内置） | SQLite 原生，无需额外服务 |
| 向量检索 | Phase 1 内置余弦 / Phase 2 sqlite-vec | 渐进式，Phase 1 零依赖 |
| 混合检索权重 | 向量 0.7 + FTS 0.3 | 参考 openclaw 实践验证的比例 |
| 事实提取 | LLM 驱动（异步后台） | 参考 mem0，利用现有 Provider Trait |
| Working Memory 编辑 | Agent 自编辑工具 | 参考 Letta，Agent 主动维护认知状态 |
| 压缩前刷写 | 必须实现 | 参考 openclaw/happyclaw，防止记忆丢失 |
| Embedding | Phase 1 远程 API / Phase 2 本地 ONNX | 渐进式，避免 Phase 1 复杂度 |
| 上下文注入格式 | 结构化 XML 标签 | 参考上下文工程最佳实践，反退化 |
| Token 预算 | 记忆上限 15% 总上下文 | 平衡记忆丰富度和对话空间 |
| 记忆分类 | 5 类（profile/preferences/tools/debug/patterns） | 简化 OpenViking 6 类，适应沙箱场景 |
| 多用户隔离 | user_id + 可选 sandbox_id | 与第四段 RBAC 设计统一 |
| 知识图谱 | 不纳入 MVP | mem0 的图谱对沙箱场景 ROI 不高，Phase 4+ 考虑 |

---

## 六、与上下文工程的深度融合

### 6.1 记忆如何提升上下文质量

```
无记忆系统                          有记忆系统
─────────────                     ─────────────
[System Prompt]                   [System Prompt]
[对话历史 全部]                    [Working Memory Blocks]  ← 精准用户画像
                                  [Recalled Memories]       ← 相关经验
                                  [对话历史 压缩+最近]       ← 智能管理

问题：                             优势：
- 重复询问用户偏好                  - 自动知道用户偏好
- 长会话信息丢失                    - 压缩前刷写关键信息
- 上下文全是噪声                    - 精准检索相关记忆
- Token 浪费严重                    - 650 tokens vs 5000+ (渐进披露)
```

### 6.2 反退化检查清单

- [ ] 关键记忆（Working Memory）放在 system prompt 开头 → 避免 Lost-in-the-Middle
- [ ] 检索结果用 `[RECALLED MEMORIES]` 标签包裹 → 模型知道这是记忆不是指令
- [ ] 注入前去重过滤 → 避免 Context Distraction
- [ ] 矛盾记忆检测，保留最新 → 避免 Context Confusion
- [ ] 记忆和系统指令分离 → 避免 Context Clash
- [ ] Token 预算硬限制 → 防止记忆溢出挤占对话空间
- [ ] 定期记忆清理（重要性衰减）→ 保持记忆库质量

---

## 七、关键参考文件路径

```
mem0 核心:               github.com/memory/mem0/mem0/memory/main.py (~2400行)
mem0 提示词:             github.com/memory/mem0/mem0/configs/prompts.py (~900行)
mem0 图谱记忆:           github.com/memory/mem0/mem0/memory/graph_memory.py
Letta 记忆 schema:       github.com/memory/letta/letta/schemas/memory.py (67-514行)
Letta 记忆工具:          github.com/memory/letta/letta/functions/function_sets/base.py
Letta 摘要器:            github.com/memory/letta/letta/services/summarizer/summarizer.py
Letta 上下文计算:        github.com/memory/letta/letta/services/context_window_calculator/
OpenViking 分层检索:     github.com/memory/OpenViking/openviking/retrieve/hierarchical_retriever.py
OpenViking 记忆提取:     github.com/memory/OpenViking/openviking/session/memory_extractor.py
OpenViking 会话:         github.com/memory/OpenViking/openviking/session/session.py
agent-file 格式:         github.com/memory/agent-file/
openclaw 记忆管理:       github.com/openclaw/src/memory/manager.ts
openclaw 混合搜索:       github.com/openclaw/src/agents/tools/memory-tool.ts
openclaw 刷写:           github.com/openclaw/src/auto-reply/reply/agent-runner-memory.ts
zeroclaw Memory trait:   github.com/zeroclaw/src/memory/traits.rs
zeroclaw SQLite 实现:    github.com/zeroclaw/src/memory/sqlite.rs
happyclaw 三层记忆:      github.com/happyclaw/src/routes/memory.ts
pi_agent_rust 压缩:      github.com/pi_agent_rust/src/compaction.rs
上下文工程:              github.com/memory/Agent-Skills-for-Context-Engineering/skills/
```

---

## 八、下一步

记忆模块 Brainstorming 完成（第八段）。与前 7 段合并后，架构设计全面覆盖：

1. ✅ 系统分层与核心组件
2. ✅ Agent Engine 内部架构
3. ✅ 沙箱管理器与容器隔离
4. ✅ 外部渠道和多用户体系
5. ✅ 工具调试面板
6. ✅ Web UI 架构
7. ✅ MVP 分阶段路线图
8. ✅ **记忆模块架构**（本文档）

下一步：
1. 将全部 8 段 brainstorming 整合为正式设计文档 `docs/design/ARCHITECTURE_DESIGN.md`
2. 更新 Phase 1-4 路线图，纳入记忆模块里程碑
3. 创建 Phase 1 详细实施计划
4. 初始化项目脚手架
