# Memory 系统代码级竞品分析报告

> 分析日期：2026-03-12
> 分析方法：逐文件源码阅读，基于实际代码实现评分，非文档声明
> 分析范围：octo-sandbox + 5 个竞品框架

---

## 一、竞品概览

| 项目 | 语言 | Memory 核心路径 | 架构风格 |
|------|------|----------------|----------|
| **octo-sandbox** | Rust | `crates/octo-engine/src/memory/` (12 文件) | 五组件分层（L0/L1/L2+KG+FTS） |
| **moltis** | Rust | `crates/memory/src/` (20+ 子模块) | 文件+chunk+嵌入缓存，RRF 融合 |
| **openfang** | Rust | `crates/openfang-memory/src/` (6 文件) | 三存储分离（Structured/Semantic/Knowledge） |
| **localgpt** | Rust | `crates/core/src/memory/` (5 文件) | 工作区文件 + SQLite + 多嵌入后端 |
| **goose** | Rust | `crates/goose-mcp/src/memory/` (1 文件) | 纯文本文件分类存储 |
| **zeroclaw** | Rust | `src/tools/memory_store.rs` (1 文件) | 工具级 KV 存储 |

---

## 二、8 维度逐项对比

### 维度 1：记忆层级设计

**评分标准**：层级数量、职责分离清晰度、层间数据流转机制

| 项目 | 层级 | 实现细节 | 评分 |
|------|------|----------|------|
| **octo-sandbox** | L0(Working) + L1(Session) + L2(Persistent) + KnowledgeGraph + FTS | `MemorySystem` 统一结构体持有 5 个组件；L0 基于 HashMap 的 block 机制支持 turn 过期(`max_age_turns`)；L1 按 session 隔离；L2 用 SQLite 持久化；KG 独立内存图+SQLite 后端 | **8.5/10** |
| **moltis** | Files + Chunks + EmbeddingCache | `MemoryStore` trait 抽象 files/chunks/embedding_cache 三表；chunks 是文件分片；无显式 working/session 分层，通过 file scope 实现类似效果 | **6.5/10** |
| **openfang** | Structured + Semantic + Knowledge | 三个独立 Store 各司其职：StructuredStore(KV+版本)、SemanticStore(向量)、KnowledgeStore(图)；职责分离最清晰，但缺少 session 级和 working 级分层 | **7.5/10** |
| **localgpt** | Workspace 文件 + SQLite 索引 | MEMORY.md/HEARTBEAT.md/SOUL.md 等 markdown 文件 + SQLite FTS5 + 可选 sqlite-vec；daily log 自动按日期分文件；实质两层（文件+索引） | **6.0/10** |
| **goose** | 单层文本文件 | 按 category 分 .txt 文件存储，global vs local 两个目录；无搜索、无索引、无层级 | **2.0/10** |
| **zeroclaw** | 单层 SQLite | 通过 `Memory` trait 操作 SQLite；有 category 分类但无层级概念 | **3.0/10** |

**octo 真实优势**：唯一实现了 L0 turn-based 过期的 working memory，`max_age_turns` 自动淘汰旧 block，XML 格式编译输出——其他框架均无此精细上下文管理。

**octo 真实短板**：L0->L1->L2 自动提升依赖 `SessionEndMemoryHook`，仅 session 结束时批量触发，缺少实时连续提取。

---

### 维度 2：向量搜索实现质量

**评分标准**：算法选择、索引结构、性能可扩展性、生产就绪度

| 项目 | 实现 | 关键代码 | 评分 |
|------|------|----------|------|
| **octo-sandbox** | 默认暴力扫描 + feature-gated HNSW (`hnsw_rs`) | `vector_index.rs`: `VectorBackend` enum 统一接口；HNSW 配置 `m:16, ef_construction:200, max_elements:100_000`；`spawn_blocking` 异步适配。**默认路径是 `sqlite_store.rs` 的 `vector_search()` 全表加载所有 embedding 做内存余弦相似度** | **6.0/10** |
| **moltis** | 本地 GGUF embedding + SQLite 存储 | `embeddings_local.rs`: llama-cpp-2 FFI，EmbeddingGemma-300M 自动下载；向量搜索走 SQLite 查询后内存计算 | **5.5/10** |
| **openfang** | 暴力余弦 + 10x 过采样 | `semantic.rs`: 加载全部 embedding 计算余弦相似度，`limit * 10` 过采样后截断；LIKE 降级 fallback | **4.5/10** |
| **localgpt** | FTS5 + 可选 sqlite-vec 扩展 | `index.rs`: 检测 sqlite-vec 扩展可用时用 `vec_distance_cosine()` SQL 函数；不可用则降级内存扫描；hybrid 搜索用 `1/(1+rank)` 评分 | **7.0/10** |
| **goose** | 无向量搜索 | 纯文本文件 | **0/10** |
| **zeroclaw** | 无向量搜索 | SQLite KV，无 embedding | **0/10** |

**localgpt 实际领先**：sqlite-vec 扩展将向量搜索下推到 SQL 引擎层，避免全表加载，是唯一在 SQL 层面做近似最近邻的实现。

**octo 真实短板**：
1. **默认路径是全表暴力扫描**。`sqlite_store.rs` 的 `vector_search` 函数执行 `SELECT id, embedding FROM memories WHERE user_id = ?1`，将所有 embedding 加载到内存后逐条计算余弦相似度。HNSW 虽已实现但在 feature flag 后面（`#[cfg(feature = "hnsw")]`），默认构建不包含。
2. **HNSW 索引与 SQLite 存储分离**。HNSW 索引在内存中，需启动时从 SQLite 重建，未见持久化 HNSW 索引到磁盘的代码。
3. `hnsw_rs` crate 相比 `hnswlib`/Qdrant 的实现成熟度较低。

---

### 维度 3：知识图谱实现深度

**评分标准**：图结构设计、查询能力、类型系统、推理能力

| 项目 | 实现 | 关键特性 | 评分 |
|------|------|----------|------|
| **octo-sandbox** | 内存 HashMap 图 + SQLite 持久化 + FTS5 索引 | `graph.rs`: entities/relations/outgoing/incoming/by_type 五个 HashMap；BFS 遍历、最短路径、name/type 搜索。`graph_store.rs`: SQLite CRUD + 外键约束。`fts.rs`: porter+unicode61 分词的 content-less FTS5 | **7.0/10** |
| **openfang** | **类型化知识图谱** | `knowledge.rs`: `EntityType` 枚举(Person/Organization/Custom)、`RelationType` 枚举(WorksAt/RelatedTo/等)、`GraphPattern` 结构化查询（source/relation/target 模式匹配）、关系上有 confidence 分数 | **8.0/10** |
| **moltis** | 无独立 KG 模块 | 通过 chunks + FTS 实现类似效果，但没有实体-关系图结构 | **2.0/10** |
| **localgpt** | 无 KG | 依赖 markdown 文件的隐式关联 | **1.0/10** |
| **goose** | 无 | — | **0/10** |
| **zeroclaw** | 无 | — | **0/10** |

**openfang 在此维度真正领先**：
- `EntityType` 和 `RelationType` 枚举提供类型安全的图谱操作
- `GraphPattern` 查询允许 `source: Option<EntityType>, relation: Option<RelationType>, target: Option<EntityType>` 的模式匹配——一个真正的图查询 DSL
- 关系上的 `confidence` 字段支持知识衰减和不确定性推理

**octo 真实短板**：
1. 实体和关系都是 `String` 类型——无类型枚举，编译期无法捕获拼写错误
2. 无 confidence/权重字段，不能表达知识的不确定性
3. BFS/最短路径是基础图算法，缺少模式匹配查询能力
4. 内存图与 SQLite 图的同步完全靠调用方保证，无事务性一致保障

---

### 维度 4：全文搜索与融合策略

**评分标准**：分词器质量、查询灵活性、与向量搜索的融合算法

| 项目 | 实现 | 融合方式 | 评分 |
|------|------|----------|------|
| **octo-sandbox** | SQLite FTS5 (porter+unicode61) 双表 | Memory 表和 KG 实体表各有独立 FTS5 索引；`sqlite_store.rs` 的 hybrid 搜索用固定权重线性融合：`0.3*fts + 0.7*vector` | **7.0/10** |
| **moltis** | SQLite FTS5 + **RRF 融合** | `search.rs`: `merge_results_rrf()` 实现 Reciprocal Rank Fusion `score = sum(weight / (rrf_k + rank + 1))`；也支持 linear merge；可配置 `MergeStrategy` 枚举切换 | **9.0/10** |
| **openfang** | LIKE 降级 | `semantic.rs`: 无 embedding 时降级为 `WHERE content LIKE '%query%'`，不是真正的 FTS | **3.0/10** |
| **localgpt** | SQLite FTS5 + hybrid rank | `index.rs`: FTS5 + sqlite-vec 双路搜索，hybrid 用 `1/(1+rank)` 评分后加权合并 | **7.5/10** |
| **goose** | 无 | — | **0/10** |
| **zeroclaw** | 无 | — | **0/10** |

**moltis 在 FTS+向量融合维度明显领先**：
- RRF (Reciprocal Rank Fusion) 是 IR 领域公认的多路召回融合最佳实践（Cormack et al., 2009），相比线性加权更鲁棒
- 线性融合的问题：FTS 和向量搜索的 score 分布不同，直接加权容易被某一路的极端分数主导
- RRF 只依赖排名而非原始分数，天然归一化，不受分数尺度影响

**octo 真实短板**：
1. 固定 `0.3/0.7` 权重硬编码，不可配置
2. 使用 linear 融合而非 RRF，对分数分布敏感
3. 没有 `MergeStrategy` 抽象，无法切换融合算法

---

### 维度 5：自动记忆提取/注入

**评分标准**：提取方法（规则 vs LLM）、覆盖面、注入时机、上下文感知

| 项目 | 提取 | 注入 | 评分 |
|------|------|------|------|
| **octo-sandbox** | `RuleBasedExtractor`: 正则匹配文件路径、`$ ` 命令、cargo/npm/make 命令、偏好模式("always use"/"prefer"/"we decided")；5 个类别，confidence 0.6-0.8 | `MemoryInjector`: session 启动时从 L2 检索相关记忆，格式化为 "## Cross-Session Memory" 注入 system prompt；max_memories=10, min_relevance=0.3 | **7.0/10** |
| **moltis** | 无显式自动提取 | 无显式自动注入 | **2.0/10** |
| **openfang** | 无显式自动提取 | 无显式自动注入 | **2.0/10** |
| **localgpt** | 文件系统自动采集 | **Session 启动自动加载** MEMORY.md + 最近 daily logs + HEARTBEAT.md + SOUL.md + IDENTITY.md；workspace file-watcher 自动索引新文件 | **7.5/10** |
| **goose** | 手动保存到 category | 无自动注入 | **1.0/10** |
| **zeroclaw** | 无 | 无 | **0/10** |

octo-sandbox 和 localgpt 各有优势：
- **octo 的规则提取器**能从对话中自动识别技术决策和偏好（主动提取）
- **localgpt 的文件 watcher + 模板系统**更擅长被动采集环境信息

**octo 真实短板**：
1. **没有 LLM 提取器**——`auto_extractor.rs` 完全基于正则规则，无法提取隐含语义知识（如 "上次那个方案不好，我们换成了 X"）
2. 提取仅在 `SessionEndMemoryHook` 触发，不是实时的
3. 5 个 category 覆盖面有限，缺少 PersonalInfo、TeamKnowledge 等类别
4. confidence 是静态值（0.6-0.8），不随上下文或重复出现动态调整

---

### 维度 6：时间衰减机制

**评分标准**：衰减模型、衰减触发方式、与搜索的集成、可配置性

| 项目 | 衰减模型 | 实现位置 | 评分 |
|------|----------|----------|------|
| **octo-sandbox** | 指数衰减 `e^(-0.05 * days)` | `sqlite_store.rs`: `time_decay(accessed_at, now)` 在搜索时计算，乘入最终得分；同时更新 `access_count` 和 `accessed_at` | **6.5/10** |
| **openfang** | **独立衰减引擎** | `consolidation.rs`: `ConsolidationEngine` 定期运行，对 7 天未访问的记忆执行 `decay_factor = 1.0 - decay_rate`（线性），最低 0.1；Phase 2 计划合并相似记忆 | **8.0/10** |
| **localgpt** | 可配置指数衰减 | `search.rs`: `temporal_decay_lambda` 可配置，`decay = exp(-lambda * age_days)`；`mod.rs`: 配置级别的 lambda 参数 | **7.5/10** |
| **moltis** | 无显式衰减 | 未发现时间衰减代码 | **1.0/10** |
| **goose** | 无 | — | **0/10** |
| **zeroclaw** | 无 | — | **0/10** |

**openfang 的 ConsolidationEngine 是此维度冠军**：
- 独立后台引擎，与搜索解耦，不依赖"被搜索到"才衰减
- 支持批量处理，定期扫描所有过期记忆
- 架构上为 Phase 2 的记忆合并预留了扩展点

**octo 真实短板**：
1. **衰减 lambda=0.05 硬编码**，不可配置（对比 localgpt 的 `temporal_decay_lambda` 配置项）
2. **衰减只在搜索时生效**——如果一条记忆从不被搜索，其 accessed_at 永远不更新，但 time_decay 分数会在下次搜索时突然剧降（cold start 问题）
3. 没有独立的 consolidation 后台任务来主动清理或合并过时记忆
4. `access_count` 没有参与衰减计算——高频访问的记忆不应衰减得快

---

### 维度 7：LLM Reranking

**评分标准**：是否实现、reranking 策略、降级处理、多样性保障

| 项目 | 实现 | 策略 | 评分 |
|------|------|------|------|
| **moltis** | **完整 LLM Reranking** | `reranking.rs`: `LlmReranker` 用 prompt 工程让 LLM 打相关性分；`70% LLM + 30% original` 分数混合；`NoOpReranker` 降级；LLM 失败时 graceful degradation 返回原序 | **9.0/10** |
| **localgpt** | **MMR 多样性 Reranking** | `search.rs`: `MmrReranker` 用 Jaccard 相似度惩罚重复结果；不是 LLM reranking，但解决了结果多样性问题 | **6.0/10** |
| **octo-sandbox** | **无** | 搜索结果直接按 score 排序返回，无任何 reranking 步骤 | **1.0/10** |
| **openfang** | 10x 过采样 | 过采样后截断，不是真正的 reranking | **2.0/10** |
| **goose** | 无 | — | **0/10** |
| **zeroclaw** | 无 | — | **0/10** |

**这是 octo-sandbox 最大的差距维度。**

moltis 的 LLM Reranking 实现了完整的 retrieve-then-rerank 管线：

```
初始召回（FTS+向量）-> LLM 打分（prompt 工程）-> 分数混合（70/30）-> 最终排序
```

关键实现（`reranking.rs`）：
- Prompt 让 LLM 对每个 chunk 打 0-1 相关性分
- 混合公式：`final = 0.7 * llm_score + 0.3 * original_score`
- 失败降级：LLM 调用失败时返回原始排序，不阻断搜索

localgpt 的 MMR 虽非 LLM reranking，但解决了 embedding 空间中高度相似结果的去重问题。

**octo 完全没有 reranking 层**——搜索结果质量完全取决于 FTS 和向量搜索的原始 score，无法利用 LLM 语义理解修正排序错误。

---

### 维度 8：跨会话记忆持久化

**评分标准**：持久化机制、Session 间数据流转、启动恢复、数据一致性

| 项目 | 持久化 | 跨 Session 机制 | 评分 |
|------|--------|----------------|------|
| **octo-sandbox** | SQLite (L2) + SQLite (KG) | `SessionEndMemoryHook` 提取->存 L2；`MemoryInjector` 在新 session 启动时检索 L2 相关记忆注入 system prompt；`session_store.rs` 持久化 session 元数据 | **8.0/10** |
| **localgpt** | SQLite + Markdown 文件 | MEMORY.md 手动维护 + daily log 自动写入 + session transcript 持久化(JSONL)；新 session 自动加载 MEMORY.md + recent logs | **8.0/10** |
| **openfang** | SQLite 三表 | 每个 Store 独立持久化；structured store 支持版本化 KV；agent manifest 用 msgpack 序列化 | **7.0/10** |
| **moltis** | SQLite (files+chunks+embedding_cache) | Embedding cache 支持批量 eviction；但无显式跨 session 记忆注入机制 | **5.0/10** |
| **goose** | 文件系统 | .txt 文件持久化，global 目录跨项目共享 | **3.0/10** |
| **zeroclaw** | SQLite | 基础持久化，无跨 session 策略 | **2.5/10** |

octo-sandbox 和 localgpt 并列第一：
- **octo 优势**：自动提取+自动注入形成闭环，用户无需手动维护
- **localgpt 优势**：MEMORY.md 是用户可读可编辑的，提供人类可审计的持久记忆

**octo 的真实短板**：
1. 注入时的 `max_memories=10` 硬编码上限可能不够
2. `min_relevance=0.3` 阈值较低，可能注入噪声记忆
3. 没有 localgpt 那样的人类可读 MEMORY.md 文件，调试和审计困难

---

## 三、综合评分矩阵

| 维度 | octo-sandbox | moltis | openfang | localgpt | goose | zeroclaw |
|------|:---:|:---:|:---:|:---:|:---:|:---:|
| 1. 记忆层级设计 | **8.5** | 6.5 | 7.5 | 6.0 | 2.0 | 3.0 |
| 2. 向量搜索质量 | 6.0 | 5.5 | 4.5 | **7.0** | 0 | 0 |
| 3. 知识图谱深度 | 7.0 | 2.0 | **8.0** | 1.0 | 0 | 0 |
| 4. 全文搜索融合 | 7.0 | **9.0** | 3.0 | 7.5 | 0 | 0 |
| 5. 自动提取/注入 | 7.0 | 2.0 | 2.0 | **7.5** | 1.0 | 0 |
| 6. 时间衰减 | 6.5 | 1.0 | **8.0** | 7.5 | 0 | 0 |
| 7. LLM Reranking | 1.0 | **9.0** | 2.0 | 6.0 | 0 | 0 |
| 8. 跨会话持久化 | **8.0** | 5.0 | 7.0 | **8.0** | 3.0 | 2.5 |
| **总分 (80 满分)** | **51.0** | **40.0** | **42.0** | **50.5** | **6.0** | **5.5** |
| **平均分** | **6.38** | **5.00** | **5.25** | **6.31** | **0.75** | **0.69** |

---

## 四、octo-sandbox 真实差距分析（按优先级排序）

### P0 — 严重缺失（竞品已有成熟实现）

#### 1. LLM Reranking（差距: 8 分）

**当前状态**：完全没有 reranking 层。搜索结果直接 `ORDER BY score DESC` 返回。

**竞品参考**（moltis `reranking.rs`）：
- `LlmReranker` + `NoOpReranker` 的 trait 抽象
- 70/30 分数混合，LLM 失败 graceful degradation
- 与搜索管线完全解耦

**影响**：搜索质量上限被初始召回质量锁死，无法利用 LLM 深度语义理解修正排序错误。在 query 含歧义或多义词时，缺乏 reranking 的搜索召回精度会显著低于 moltis。

**建议实现路径**：
```
memory/reranker.rs:
  - trait Reranker { async fn rerank(query, results) -> Vec<ScoredResult> }
  - LlmReranker: 调用当前 provider 打分
  - MmrReranker: Jaccard/cosine 多样性惩罚
  - NoOpReranker: 直通
  - CompositeReranker: LLM -> MMR 串联
```

#### 2. FTS+向量融合策略（差距: 2 分，但技术债务高）

**当前状态**：`sqlite_store.rs` 硬编码 `0.3*fts + 0.7*vector` 线性融合。

**竞品参考**（moltis `search.rs`）：
- `MergeStrategy` 枚举：`Rrf { k: f32 }` | `Linear { fts_weight, vec_weight }`
- `merge_results_rrf()`: `score = sum(weight / (rrf_k + rank + 1))`

**建议**：实现 `MergeStrategy` 抽象 + RRF 作为默认策略。RRF 不依赖原始分数尺度，对 FTS/向量分布差异更鲁棒。

### P1 — 明显落后（竞品有更好实现）

#### 3. 向量搜索默认路径（差距: 1 分，但性能瓶颈）

**当前状态**：默认全表加载暴力扫描。HNSW 在 feature flag 后面。

**问题量化**：memories 表超过 10K 条目时，每次搜索都要从 SQLite 加载全部 embedding（每条 1536 维 * 4 字节 = 6KB）到内存做逐条计算。10K 条 = 60MB 数据加载 + 10K 次余弦计算。

**竞品对比**：
- localgpt 的 sqlite-vec 扩展将计算下推到 SQL 引擎
- octo 自己的 HNSW 已实现但默认不启用

**建议**：将 HNSW 从 feature flag 升级为默认启用，或集成 sqlite-vec 作为 SQL 层面的近似搜索。

#### 4. 知识图谱类型系统（差距: 1 分，但影响可靠性）

**当前状态**：Entity 和 Relation 都是 `String` 类型的 name/relation_type。

**竞品参考**（openfang `knowledge.rs`）：
- `EntityType` 枚举提供编译期类型安全
- `RelationType` 枚举防止拼写错误
- `GraphPattern` 结构化查询 DSL
- 关系 `confidence` 字段

**建议**：为 entity_type 和 relation_type 引入枚举（支持 `Custom(String)` 扩展），添加 confidence 字段。

#### 5. 时间衰减硬编码（差距: 1-1.5 分）

**当前状态**：`lambda = 0.05` 硬编码，仅搜索时衰减，无独立 consolidation。

**竞品对比**：
- localgpt：`temporal_decay_lambda` 可配置
- openfang：独立 `ConsolidationEngine` 后台衰减 + 最低分数 floor

**建议**：lambda 移入配置；添加 consolidation 后台任务；让 access_count 参与衰减公式。

### P2 — 有提升空间

#### 6. 自动提取能力（差距: 0.5 分）

**当前状态**：纯规则提取（正则），5 个类别，静态 confidence。

**改进方向**：
- 添加可选的 LLM 提取器
- 支持实时提取（不只是 session 结束时）
- 增加类别：PersonalInfo, TeamKnowledge, ToolUsagePattern
- 让 confidence 随重复出现次数增长

#### 7. 本地 Embedding 支持

**当前状态**：仅 OpenAI + Voyage AI（均需网络）。

**竞品对比**：
- localgpt：FastEmbed (ONNX, 5+ 模型选择) + OpenAI + GGUF (llama-cpp-2)
- moltis：GGUF (llama-cpp-2) + EmbeddingGemma-300M 自动下载

**建议**：集成 FastEmbed 或 candle 作为离线 embedding 后端，对隐私敏感场景尤为重要。

#### 8. 文档分块（Chunking）

**当前状态**：无分块能力。整篇文档作为单条记忆。

**竞品参考**（moltis `chunker.rs`）：
- Markdown 感知分块（按标题层级）
- tree-sitter AST 感知分块（按函数/类边界）
- 行保留 + overlap 策略

**建议**：添加 chunking 模块，至少支持 markdown heading 分块和固定大小 + overlap 分块。

---

## 五、竞品真实优势总结

| 竞品 | 核心优势 | 值得借鉴的源码文件 |
|------|----------|-------------------|
| **moltis** | RRF 融合 + LLM Reranking + AST Chunking + 全面 metrics/tracing | `search.rs`, `reranking.rs`, `chunker.rs` |
| **openfang** | 类型化知识图谱 + 独立记忆衰减引擎 + 三存储清晰分离 | `knowledge.rs`, `consolidation.rs` |
| **localgpt** | 3 嵌入后端 + sqlite-vec 原生向量搜索 + MMR 多样性 + 可配置衰减 | `embeddings.rs`, `index.rs`, `search.rs` |

---

## 六、增强路线图建议

### Phase 1（高 ROI，建议立即实施）
1. **Reranker trait 抽象 + MMR 实现**——不依赖额外 LLM 调用，纯算法，约 200 行
2. **RRF 融合策略**——替换硬编码线性融合，约 100 行
3. **时间衰减可配置化**——lambda 移入 config，添加 access_count 权重，约 50 行

### Phase 2（中等工作量）
4. **HNSW 默认启用**（或集成 sqlite-vec），约 100 行配置改动
5. **知识图谱类型枚举 + confidence**，约 300 行
6. **Consolidation 后台任务**，约 200 行

### Phase 3（大工作量，但长期价值高）
7. **LLM Reranker**——需要 provider 抽象适配，约 300 行
8. **本地 Embedding 后端**——集成 FastEmbed/candle，约 500 行
9. **文档 Chunking 模块**——markdown + 固定大小，约 400 行
10. **实时记忆提取**——从 session-end 触发改为流式触发，约 300 行

---

## 七、方法论说明

本报告所有评分基于以下原则：

1. **只评代码实现，不评文档声明**：例如 openfang 的 consolidation Phase 2（记忆合并）在代码注释中提到但未实现，不计分
2. **feature-gated 功能降低评分**：octo 的 HNSW 虽已实现但默认不启用，按 50% 计入
3. **硬编码 vs 可配置**：硬编码参数（如 lambda=0.05、权重 0.3/0.7）扣分
4. **graceful degradation 加分**：moltis 的 LLM reranker 失败降级、openfang 的 LIKE fallback 等获得额外分数
5. **生产就绪度权重**：有测试、有错误处理、有日志的实现得分更高
