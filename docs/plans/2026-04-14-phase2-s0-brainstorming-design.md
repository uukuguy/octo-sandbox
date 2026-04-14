# Phase 2 S0 Brainstorming 设计文档

**日期**：2026-04-14
**讨论范围**：S0.T1 (L2 Memory semantic 检索选型) + S0.T2 (Agent loop 通用性原则)
**产出**：ADR-V2-015 Accepted + ADR-V2-016 Proposed（待 E2E 验收）
**参与**：用户 + Claude（/brainstorming 会话）

---

## 一、S0.T1 — L2 Memory semantic 检索

### 核心洞察

Embedding 模型是**可配置组件**，不是 EAASP 的架构选择。必须让架构吸收"未来会换模型"这个现实。

### 四层抽象

```
┌────────────────────────────────────────────┐
│ 1. EmbeddingProvider 接口层（Python Protocol）│
│    async def embed(text: str) -> list[float]  │
│    @property dimension: int                   │
│    @property model_id: str                    │
└────────────────────────────────────────────┘
                    ↓
┌────────────────────────────────────────────┐
│ 2. 实现层（可配置切换）                      │
│    - OllamaEmbedding    (Phase 2 dev 使用)   │
│    - TEIEmbedding       (生产推荐)           │
│    - OpenAIEmbedding    (fallback)           │
│    - MockEmbedding      (测试)               │
└────────────────────────────────────────────┘
                    ↓
┌────────────────────────────────────────────┐
│ 3. 向量索引元数据                           │
│    memories.embedding_model_id (TEXT)       │
│    memories.embedding_dim (INTEGER)          │
│    memories.embedding_vec (BLOB)             │
└────────────────────────────────────────────┘
                    ↓
┌────────────────────────────────────────────┐
│ 4. VectorIndex 抽象（预埋 pgvector 迁移）    │
│    - HNSWVectorIndex (Phase 2 实现)         │
│    - PgVectorIndex   (Phase 6 实现)         │
│    按 model_id 分索引目录：                  │
│    .octo/l2-memory/hnsw-bge-m3-fp16/         │
│    .octo/l2-memory/hnsw-qwen3-4b/            │
└────────────────────────────────────────────┘
```

### 三条铁律

**铁律 1：维度跟着模型走**
每条 memory 自带 `embedding_model_id` 标签，永远不会错配。

**铁律 2：查询时必须 model_id 匹配**
绝不跨模型搜索（维度不一致直接报错，不做 silent fallback）。

**铁律 3：换模型 = 渐进式双写迁移**
```
Step 1: 配置新 embedder（双写模式）
Step 2: 新写入的 memory 同时写两个索引
Step 3: 后台任务 reindex 老 memory 到新索引
Step 4: 全部迁移完 → 切换 current_embedder → 删除旧索引
```

### HNSW 单进程对多用户的影响

| 场景 | 行为 | 后果 |
|------|------|------|
| **多 worker 读** | 各 worker 各自加载索引 | ✅ 内存×N 倍，查询 OK |
| **多 worker 写** | 各 worker 追加 disk | ❌ **文件 corruption 风险** |

**EAASP 画像是读多写少**（agent 搜索频率高，memory 创建频率低）→ HNSW 够用到 Phase 5。

### pgvector 迁移的 3 个硬触发条件

| 条件 | 阈值 | 理由 |
|------|------|------|
| **T1: 多租户 Phase 启动** | Phase 6 | HNSW 索引难以按租户隔离 |
| **T2: memory 规模** | 单租户 > 100 万条 | HNSW 加载慢、内存占用大 |
| **T3: 多 worker 写入需求** | `--workers 4+` + 高频写 | HNSW 单写者限制 |

### YAGNI 清单（Phase 2 不做）

❌ 多索引并发查询
❌ A/B 测试框架
❌ 自动模型发现
❌ 维度自适应压缩
❌ pgvector 实际代码（只保留接口抽象）

### Phase 2 实现范围（S2 任务重塑）

| 任务 | 更新后范围 |
|------|-----------|
| **S2.T1** | 装 hnswlib + EmbeddingProvider 接口 + OllamaEmbedding 实现 + model_id/dim 元数据列 |
| **S2.T2** | Hybrid retrieval（keyword + semantic + time-decay），严格 model_id 匹配 |
| **S2.T3** | memory_read / memory_archive 完善 |
| **S2.T4** | 状态机测试 + 换模型场景测试（dim 不匹配报错） |
| **S2.T5**（新增）| 文档化换模型迁移 runbook |

---

## 二、S0.T2 — Agent loop 通用性原则

### 核心原则

**agentic loop 是通用机制，不能为特定任务硬编码动作。**

不加：
- ❌ `completion_marker`（skill-specific）
- ❌ `min_tool_calls`（硬编码步骤数）
- ❌ 注入 "Continue" user message（破坏通用性）
- ❌ system prompt 级任务约束（属于 L2 资产层）

### 通用 agentic loop 的两条规则

1. **LLM 有 tool_use** → 执行工具 → tool_result 进上下文 → 继续
2. **LLM 无 tool_use** → 循环结束

### grid-engine 现有防死锁机制（已核实有效）

| 机制 | 位置 | 触发条件 | 动作 |
|------|------|---------|------|
| **LoopGuard** | `loop_guard.rs` + `harness.rs:1447` | 同 tool 同 args 重复调用 | Block / CircuitBreak |
| **StuckDetector / SelfRepair** | `self_repair.rs` + `harness.rs:2074` | 连续失败 + `no_progress_timeout` | Repair 或 Unrecoverable 终止 |
| **max_rounds 硬上限** | `for round in 0..max_rounds`（L420） | 循环轮次达上限 | 熔断 + `NormalizedStopReason::MaxIterations` |

**D87 与防死锁机制无关**——三个 guard 都存在且有效，D87 是**正常退出条件错误**。

### D87 根因（从 EVOLUTION_PATH L298 还原）

**当前代码**（`harness.rs:1169`）：
```rust
if stop_reason != StopReason::ToolUse || tool_uses.is_empty() {
    // finalize + return
}
```

**问题**：`stop_reason` 是 LLM 给的"叙事性"信号（"我这轮说完了"），不是 loop 控制信号。
loop 控制应该**只看 tool_uses 事实**——有就执行，没有就结束。

**修复方向**：
```rust
if tool_uses.is_empty() {
    // finalize + return
}
```

只看 `tool_uses.is_empty()`，忽略 `stop_reason`。这符合所有厂商（Anthropic / OpenAI / Mistral）的标准 agentic loop 语义。

### 不做什么

- ❌ 不改 skill frontmatter
- ❌ 不改 system prompt 模板
- ❌ 不加任何 completion policy
- ❌ 不注入 "Continue" user message

### 验收标准

**只有 E2E 通过才算修好**：
1. grid-runtime 跑 threshold-calibration skill
2. 真实调用 4+ 个 tool
3. 走完 skill 定义的多步工作流
4. Regression test `d87_multi_step_workflow_regression.rs` 去 `#[ignore]` → PASS
5. grid-engine 全量测试无回归

**代码分析不算数**，以实际 E2E 行为为准。

---

## 三、ADR 产出

| ADR | 标题 | Status | 依据 |
|-----|------|--------|------|
| **ADR-V2-015** | L2 Memory Engine semantic 检索选型 | **Accepted** | 四层抽象 + 三铁律 + pgvector 迁移条件已对齐 |
| **ADR-V2-016** | Agent loop 通用性原则 | **Proposed** | 设计方向已对齐，**E2E 验收通过后升 Accepted** |

---

## 四、下一步

1. 起草 ADR-V2-015 和 ADR-V2-016 文档
2. 启动 S1.T1（D87 修复）
3. E2E 验收（grid-runtime + threshold-calibration）
4. ADR-V2-016 升 Accepted
5. S2（Memory 增强）+ S1 其余任务并行推进

## 五、未解决议题（Phase 2 执行中追踪）

- Phase 6 pgvector 迁移时的实际负载评估
- TEI 生产部署的性能基准（bge-m3:fp16 vs TEI 吞吐量对比）
- D87 修复后 LLM 是否会陷入新的失败模式（需要 E2E 样本积累）
