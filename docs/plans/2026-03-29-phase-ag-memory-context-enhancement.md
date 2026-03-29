# Phase AG — 记忆和上下文机制增强

**日期**: 2026-03-29
**设计文档**: `docs/design/MEMORY_CONTEXT_ENHANCEMENT_DESIGN.md`
**基线**: 2476 tests, commit df54865, DB migration v11
**目标**: 让 agent 具备跨会话记忆、事件记忆、时间线查询、主动记忆管理能力

---

## 任务清单

### G1: 接线修复 + 基础设施（3 tasks）

> 目标: 把已实现但未接线的组件激活

#### Task 1: 类型系统扩展 (octo-types)

**文件**: `crates/octo-types/src/memory.rs`

1. 新增 `MemoryType` 枚举: `Semantic`, `Episodic`, `Procedural`
2. 新增 `EventData` 结构体: `event_type`, `target`, `outcome`, `artifacts` (JSON), `tool_chain` (Vec)
3. 新增 `SortField` 枚举: `Relevance`, `CreatedAt`, `UpdatedAt`, `Importance`
4. `MemoryEntry` 新增字段: `memory_type: MemoryType` (default Semantic), `session_id: Option<String>`, `event_data: Option<EventData>`
5. `SearchOptions` 新增字段: `time_range: Option<(i64, i64)>`, `session_id: Option<String>`, `memory_types: Option<Vec<MemoryType>>`, `sort_by: SortField`
6. `MemoryFilter` 新增字段: `time_range`, `session_id`, `memory_types`
7. 所有新增字段有默认值，保持向后兼容

**验证**: `cargo check -p octo-types` + 现有 tests 不 break

#### Task 2: DB Migration v12 + SqliteMemoryStore 扩展

**文件**:
- `crates/octo-engine/src/db/mod.rs` — Migration v12
- `crates/octo-engine/src/memory/sqlite_store.rs` — SQL 查询扩展

1. Migration v12:
   - `ALTER TABLE memories ADD COLUMN memory_type TEXT DEFAULT 'semantic'`
   - `ALTER TABLE memories ADD COLUMN session_id TEXT`
   - `ALTER TABLE memories ADD COLUMN event_data TEXT` (JSON)
   - `CREATE INDEX idx_memories_created_at ON memories(created_at)`
   - `CREATE INDEX idx_memories_session_id ON memories(session_id)`
   - `CREATE INDEX idx_memories_memory_type ON memories(memory_type)`
   - `CREATE TABLE session_summaries (...)` — session_id PK, summary, event_count, key_topics, memory_count, created_at, updated_at
2. `SqliteMemoryStore::store()` — 写入新字段 (memory_type, session_id, event_data)
3. `SqliteMemoryStore::search()` — WHERE 子句增加 time_range / session_id / memory_type 过滤；支持 SortField
4. `SqliteMemoryStore::list()` — 同上
5. 新增 `SessionSummaryStore` 结构体: `save()`, `recent(n)`, `get_by_session()`
6. 从 row 解析时兼容旧数据（新字段允许 NULL）

**验证**: 现有 memory tests 不 break + 新增查询 tests

#### Task 3: 接线 SessionEndMemoryHook + MemoryInjector

**文件**:
- `crates/octo-engine/src/agent/executor.rs` — session end hook
- `crates/octo-engine/src/agent/harness.rs` — MemoryInjector 注入

1. `AgentExecutor` 在 session 结束时（cancel/complete/stop）调用 `SessionEndMemoryHook::on_session_end()`
   - 传入当前 session 的 messages, memory_store, user_id
   - 用 `tokio::spawn` 异步执行，不阻塞返回
   - 日志记录提取数量
2. `harness.rs` loop 开始前，Zone B 注入后，追加 MemoryInjector 调用:
   - 从第一条 user message 提取 query
   - 调用 `MemoryInjector::build_memory_context(store, user_id, query)`
   - 非空时注入到 messages（Zone B+ 位置）
3. `AgentLoopConfig` 确认 `memory_store` 字段传递正确

**验证**: 手动测试 — session 结束后 L2 有数据 + 新 session 能注入

---

### G2: 情景记忆系统（4 tasks）

> 目标: 事件记忆 + 会话摘要 + 时间线查询

#### Task 4: EventExtractor — 事件提取器

**文件**: `crates/octo-engine/src/memory/event_extractor.rs` (新增)

1. `EventExtractor::extract_events(provider, messages, model)` → `Vec<EventData>`
2. 实现逻辑:
   - 筛选包含 ToolUse + ToolResult 的消息
   - 构建提取 prompt（让 LLM 从 tool chain 中提取结构化事件）
   - 解析 LLM JSON 响应为 Vec<EventData>
   - 过滤纯查询类操作（只保留有明确结果的变更类操作）
3. Prompt 模板:
   ```
   Extract structured events from the following tool call results.
   An event is an action with a clear outcome (not a read/query).
   Return JSON array: [{event_type, target, outcome, artifacts, tool_chain}]
   ```
4. 在 `memory/mod.rs` 中导出

**验证**: 单元测试 — mock provider 返回固定 JSON，验证解析

#### Task 5: SessionSummarizer — 会话摘要生成

**文件**: `crates/octo-engine/src/memory/session_summarizer.rs` (新增)

1. `SessionSummarizer::summarize(provider, messages, model)` → `SessionSummary`
2. `SessionSummary` 结构: `text: String`, `key_topics: Vec<String>`, `event_count: usize`
3. Prompt:
   ```
   Summarize this conversation in 2-3 sentences. Include:
   - What was done and the outcomes
   - Key decisions made
   - Any important artifacts (accounts, files, configs created)
   Return JSON: {text, key_topics: [...], event_count: N}
   ```
4. 在 `memory/mod.rs` 中导出

**验证**: 单元测试 — mock provider 返回固定 JSON

#### Task 6: Session End 完整流程接线

**文件**: `crates/octo-engine/src/agent/executor.rs`

1. 在 session end 流程中串联三步:
   - Step 1: `SessionEndMemoryHook::on_session_end()` — 规则提取（已有）
   - Step 2: `EventExtractor::extract_events()` — 事件提取（新增）
     - 每个事件创建 `MemoryEntry` with `memory_type=Episodic`, `session_id`, `event_data`
     - 存入 L2
   - Step 3: `SessionSummarizer::summarize()` — 摘要生成（新增）
     - 存入 `session_summaries` 表
2. 整个流程用 `tokio::spawn` 异步，不阻塞 session close 返回
3. 错误处理: 每步独立 try，某步失败不影响其他步骤

**验证**: 集成测试 — mock provider, 验证 L2 + session_summaries 有数据

#### Task 7: memory_timeline 工具

**文件**: `crates/octo-engine/src/tools/memory_timeline.rs` (新增)

1. 工具参数:
   - `date`: Option<String> — 查询指定日期 (YYYY-MM-DD)
   - `range`: Option<String> — 范围 (today/yesterday/last_week/last_month/YYYY-MM-DD..YYYY-MM-DD)
   - `query`: Option<String> — 语义搜索，结果按时间排序
   - `session_id`: Option<String> — 按会话查询
   - `type`: Option<String> — semantic/episodic/procedural
   - `limit`: Option<usize> — 默认 20
2. 实现:
   - 解析 date/range → time_range (i64, i64)
   - 构建 SearchOptions with time_range, session_id, memory_types, sort_by=CreatedAt
   - 调用 SqliteMemoryStore::search() 或 list()
   - 格式化输出: 每条包含时间、类型、内容、session_id
3. 注册到 `register_memory_tools()`

**验证**: 单元测试 — 验证 date/range 解析 + 查询结果格式

---

### G3: Agent 主动记忆管理（3 tasks）

> 目标: Agent 可编辑记忆 + 跨会话摘要注入 + 记忆管理指令

#### Task 8: memory_edit 工具

**文件**: `crates/octo-engine/src/tools/memory_edit.rs` (新增)

1. 工具参数:
   - `action`: "update" | "append" | "clear"
   - `block`: "user_profile" | "task_context" | "custom:{name}"
   - `content`: String — 新内容
2. 实现:
   - `update`: 调用 `WorkingMemory::update_block()` 完整替换 value
   - `append`: 获取当前 block.value + "\n" + content，然后 update
   - `clear`: 调用 `WorkingMemory::update_block()` with empty value
3. 注册到 `register_memory_tools()`
4. 安全: 不允许编辑 is_readonly blocks

**验证**: 单元测试 — update/append/clear 三种 action

#### Task 9: Session Summaries 注入到 Zone B

**文件**: `crates/octo-engine/src/agent/harness.rs`

1. 在 Zone B 注入后、loop 开始前:
   - 调用 `SessionSummaryStore::recent(5)` 获取最近 5 个会话摘要
   - 格式化为:
     ```
     ## Recent Sessions
     - [2026-03-28] Registered on MoltBook website, created octo-agent account
     - [2026-03-27] Fixed sub-agent streaming events, updated TUI
     ```
   - 注入到 messages (与 MemoryInjector 输出合并)
2. 字符预算: 最多 2000 chars（约 500 tokens），超出则截断旧摘要

**验证**: 集成测试 — 验证新 session 的 system context 包含摘要

#### Task 10: System Prompt 记忆管理指令 + Zone B 周期刷新

**文件**:
- `crates/octo-engine/src/context/system_prompt.rs` — 指令追加
- `crates/octo-engine/src/agent/harness.rs` — Zone B 刷新

1. `SystemPromptBuilder::build_static()` 的 core instructions 追加记忆管理段落:
   - 告诉 agent 有哪些记忆工具
   - 何时应主动存储（学到新信息、完成重要操作）
   - 何时应使用 timeline 工具
2. Zone B 周期刷新:
   - 常量 `ZONE_B_REFRESH_INTERVAL = 5`（可配置）
   - 在 loop 内 `if round % interval == 0 && round > 0` 时重新 compile Zone B
   - `loop_steps::refresh_zone_b()` — 找到原 Zone B 注入位置并替换

**验证**: 单元测试 — 验证 system prompt 包含记忆指令; 验证 refresh 逻辑

---

### G4: 上下文工程增强（1 task）

> 目标: ObservationMasker 接入，减少 token 浪费

#### Task 11: ObservationMasker 接入 Agent Loop

**文件**: `crates/octo-engine/src/agent/harness.rs`

1. 在 `compute_degradation_level()` 之前:
   - 当 `budget.usage_ratio() > 0.5` 时激活 ObservationMasker
   - `masker.mask(&messages)` 生成 masked 版本
   - 用 masked 版本发送 LLM 请求
   - 原始 messages 保留用于后续记忆提取
2. ObservationMasker 配置: `keep_recent_turns=3`, `min_mask_length=200`
3. 不改动 ObservationMasker 自身实现

**验证**: 单元测试 — 验证 mask 触发条件 + messages 不被破坏

---

## 执行顺序

```
G1 (Task 1-3): 接线修复 + 基础设施
  Task 1: 类型系统扩展
  Task 2: DB Migration + Store 扩展 (依赖 Task 1)
  Task 3: Hook 接线 (依赖 Task 2)
      ↓
G2 (Task 4-7): 情景记忆
  Task 4: EventExtractor (可与 Task 5 并行)
  Task 5: SessionSummarizer (可与 Task 4 并行)
  Task 6: Session End 完整流程 (依赖 Task 4, 5)
  Task 7: memory_timeline 工具 (依赖 Task 2)
      ↓
G3 (Task 8-10): Agent 主动管理
  Task 8: memory_edit 工具 (独立)
  Task 9: Session Summaries 注入 (依赖 Task 5, 6)
  Task 10: System Prompt + Zone B 刷新 (独立)
      ↓
G4 (Task 11): 上下文工程增强
  Task 11: ObservationMasker 接入 (独立)
```

## Deferred Items

| ID | 内容 | 前置条件 |
|----|------|---------|
| AG-D1 | 程序记忆提取（工作流模式学习） | AG 完成 |
| AG-D2 | 情景→语义巩固 | AG 完成 + 足够 episodic 数据 |
| AG-D3 | 智能遗忘 | AG 完成 |
| AG-D4 | 记忆冲突解决 | AG 完成 |
| AG-D5 | HybridQueryEngine 接入 memory tools | AG 完成 |
| AG-D6 | KG 语义搜索 | AG 完成 + Embedding 稳定 |
| AG-D7 | Summarize 压缩策略 | AG 完成 |
| AG-D8 | Memory Explorer 前端增强 | AG 完成 |

## 测试策略

- **每个 Task 完成后运行**: `cargo test -p octo-types -- --test-threads=1` 或 `cargo test -p octo-engine -- --test-threads=1`
- **G1 完成后全量验证**: `cargo test --workspace -- --test-threads=1`
- **全部完成后**: 全量测试 + `cargo check --workspace`
- **目标**: tests ≥ 2476 (基线) + 新增 tests
