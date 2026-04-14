# ADR-V2-015 — L2 Memory Engine Semantic 检索选型

**Status:** Accepted
**Date:** 2026-04-14
**Phase:** Phase 2 — Memory and Evidence
**Author:** Claude + 用户 brainstorming 决策
**Related:** ADR-V2-002 (Event Stream Backend), Phase 6 多租户 ADR (待定)

---

## Context / 背景

Phase 0.5 MVP 的 L2 Memory Engine 检索**只有 keyword**（SQLite FTS5 BM25）。
Phase 2 目标是升级为 **semantic 检索**（向量相似度），让语义相近的 memory 也能召回。

**核心约束**（用户 brainstorming 明确）：

1. **embedding 模型必须可配置**——不是 EAASP 的架构选择，而是可替换组件
   - dev 环境用本地 ollama（bge-m3:fp16）
   - 生产可切 TEI / OpenAI / 客户私有模型
2. **架构必须吸收"未来会换模型"**——维度不一致、模型升级、多租户各用各的模型都是现实
3. **pgvector 是重要架构更新，必须在合适时机完成**——不急于 Phase 2 上，但要预埋迁移路径

## Decision / 决策

### 四层抽象

**Layer 1: EmbeddingProvider 接口**

```python
class EmbeddingProvider(Protocol):
    async def embed(self, text: str) -> list[float]: ...
    async def embed_batch(self, texts: list[str]) -> list[list[float]]: ...
    @property
    def dimension(self) -> int: ...
    @property
    def model_id(self) -> str: ...  # e.g. "bge-m3:fp16@ollama"
```

**Layer 2: 实现层**

| 实现 | 使用场景 | Phase 2 状态 |
|------|---------|------------|
| `OllamaEmbedding` | dev 环境（当前） | ✅ 实现 |
| `TEIEmbedding` | 生产推荐 | ⏳ 接口预留，Phase 3+ 实现 |
| `OpenAIEmbedding` | 云上 fallback | ⏳ 接口预留 |
| `MockEmbedding` | 单元测试 | ✅ 实现 |

**Layer 3: 向量索引元数据**

SQLite schema 增强：

```sql
ALTER TABLE memories ADD COLUMN embedding_model_id TEXT;
ALTER TABLE memories ADD COLUMN embedding_dim INTEGER;
ALTER TABLE memories ADD COLUMN embedding_vec BLOB;  -- packed f32 array

CREATE INDEX idx_embedding_model ON memories(embedding_model_id);
```

**Layer 4: VectorIndex 抽象 + HNSW in-process**

```python
class VectorIndex(Protocol):
    async def add(self, id: str, vec: list[float]) -> None: ...
    async def search(self, vec: list[float], top_k: int) -> list[Hit]: ...
    async def delete(self, id: str) -> None: ...
    async def save(self) -> None: ...

class HNSWVectorIndex(VectorIndex):
    """Phase 2 实现：hnswlib in-process"""
    # 按 model_id 分索引目录
    # .octo/l2-memory/hnsw-bge-m3-fp16/index.bin
    # .octo/l2-memory/hnsw-qwen3-4b/index.bin
```

### 三条铁律

**铁律 1：维度跟着模型走，不跟着架构走**

每条 memory 记录 `embedding_model_id` + `embedding_dim`，永远不会错配。

**铁律 2：查询时必须 model_id 匹配**

```python
async def semantic_search(query: str, top_k: int) -> list[Memory]:
    q_vec = await current_embedder.embed(query)
    model_id = current_embedder.model_id

    # 只在同模型的索引里搜，维度不一致立即报错
    index = hnsw_indexes[model_id]
    hits = index.search(q_vec, top_k)
    return hits
```

**铁律 3：换模型 = 渐进式双写迁移（不停服）**

```
Step 1: 配置新 embedder，启用双写模式
Step 2: 新写入的 memory 同时写两个索引
Step 3: 后台任务 reindex 老 memory 到新索引
Step 4: 全部迁移完 → 切换 current_embedder → 删除旧索引
```

### HNSW in-process 选型理由

**Phase 2 选 HNSW in-process，不选 pgvector**：

| 选项 | 评估 |
|------|------|
| **HNSW (选)** | ✅ 零外部依赖、✅ 与 SQLite WAL 一致、✅ 工作量小、⚠️ 单进程 |
| pgvector | 需引入 PostgreSQL 新依赖，与现有 SQLite 架构冲突，检索速度在小规模下不占优 |
| Qdrant/Weaviate | docker-compose 膨胀，运维复杂 |
| sqlite-vec | 新兴生态，风险高 |

### pgvector 迁移的 3 个硬触发条件

**Phase 2 不做 pgvector，但定义清晰迁移条件**：

| 条件 | 阈值 | 触发时动作 |
|------|------|----------|
| **T1: 多租户 Phase 启动** | Phase 6 | 迁移 pgvector，HNSW 索引难按租户隔离 |
| **T2: memory 规模** | 单租户 > 100 万条 | HNSW 加载慢、内存占用大 |
| **T3: 多 worker 写入** | `--workers 4+` + 高频写 | HNSW 单写者限制 |

**不触发 = 不迁移**。避免"什么时候该换"这种主观判断。

### HNSW 单进程对多用户的真实影响

**读多写少场景**（EAASP 真实画像）：多 worker read 完全 OK，每个 worker 各加载一份索引。
**写密集场景**（batch 导入历史 memory）：单 worker 跑导入任务，完成后通知其他 worker 重载索引。

EAASP memory 写入频率低（每 session 几条 anchor），搜索频率高。**HNSW 够用到 Phase 5**。

## Consequences / 后果

### Positive

- ✅ **embedding 模型可热切换**——dev/prod 各用各的，客户私有部署灵活
- ✅ **零外部依赖**——Phase 2 不引入 PostgreSQL，维持 SQLite WAL 单体架构
- ✅ **迁移路径预埋**——VectorIndex 接口 + 元数据列 + model_id 分索引目录让 Phase 6 切 pgvector 不伤业务代码
- ✅ **中文场景质量**——bge-m3 在 MTEB 多语言榜领先 OpenAI

### Negative

- ⚠️ **ollama 并发瓶颈**——单 ollama 进程 embedding 吞吐有限，生产需切 TEI
- ⚠️ **多 worker 写入限制**——需要通过"单 worker 跑 batch 导入 + 完成后通知其他 worker 重载"模式处理

### Risks

- 🚨 **维度错配**——如果 embedding_model_id 元数据列记录错误或被清空，跨模型查询会导致 silent 返回错误结果。**缓解**：add / search 路径都强制校验 model_id，不匹配直接 raise
- 🚨 **HNSW 索引 corruption**——多 worker 并发写入会 corrupt 索引文件。**缓解**：Phase 2 约定单 writer；通过 advisory lock 防止并发写
- 🚨 **迁移时机误判**——如果 T1/T2/T3 触发条件不清晰，可能拖延到出问题才迁。**缓解**：ADR 明确写死阈值，监控指标达阈值自动告警

## Affected Modules / 影响范围

| Module | Impact |
|--------|--------|
| `tools/eaasp-l2-memory-engine/src/eaasp_l2_memory_engine/embedding/` | 新增：Provider 接口 + Ollama/Mock 实现 |
| `tools/eaasp-l2-memory-engine/src/eaasp_l2_memory_engine/vector_index.py` | 新增：VectorIndex 接口 + HNSW 实现 |
| `tools/eaasp-l2-memory-engine/src/eaasp_l2_memory_engine/search.py` | 修改：hybrid retrieval（keyword + semantic + time-decay） |
| `tools/eaasp-l2-memory-engine/migrations/` | 新增：embedding_model_id / embedding_dim / embedding_vec 列 |
| `tools/eaasp-l2-memory-engine/pyproject.toml` | 新增依赖：`hnswlib`、`httpx` (for ollama) |
| `tools/eaasp-l2-memory-engine/config/` | 新增：embedding provider 配置 schema |

## Alternatives Considered / 候选方案

### Option A: pgvector（选用 Phase 6+）

需要 PostgreSQL，打破现有 SQLite WAL 单体架构。Phase 2 上 pgvector 会引入大量运维复杂度，而小规模下检索速度不占优势。**推迟到 Phase 6**（多租户启动时）。

### Option B: 独立向量库（Qdrant / Weaviate）

外部服务，docker-compose 膨胀，运维复杂。客户私有部署增加新组件依赖。**不选**。

### Option C: sqlite-vec

SQLite 扩展，生态弱、新兴风险高。虽然理论上理想（继承 SQLite WAL 架构），但 Phase 2 Production Readiness 不足。**不选**。

### Option D: HNSW in-process（选用）

零外部依赖，与 Phase 1 SQLite WAL 策略一致，工作量小，渐进式改进。单进程限制通过"读多写少 + 单 writer"模式规避。**Phase 2 选用**。

## References / 参考

- EAASP v2.0 MVP Scope §5 N10/N14
- EVOLUTION_PATH §3.1 Phase 2
- Brainstorming 设计文档：`docs/plans/2026-04-14-phase2-s0-brainstorming-design.md`
- hnswlib 项目：https://github.com/nmslib/hnswlib
- bge-m3 模型：https://huggingface.co/BAAI/bge-m3
- Text Embeddings Inference (TEI)：https://github.com/huggingface/text-embeddings-inference
