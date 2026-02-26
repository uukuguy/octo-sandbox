# octo-sandbox 上下文工程架构设计

**日期**: 2026-02-26
**阶段**: Phase 2 设计
**状态**: brainstorming 验证通过，待实施
**相关文档**: `docs/design/ARCHITECTURE_DESIGN.md` (主架构文档)

---

## 1. 设计背景与目标

### 1.1 核心洞察

上下文工程不仅仅是处理溢出问题——它**直接决定智能体是否智能**。通过深度分析 6 个参考项目（OpenClaw、ZeroClaw、NanoClaw、HappyClaw、pi_agent_rust、Craft Agents）的上下文工程实现，提炼出以下共识模式：

| 维度 | 跨项目共识 | octo-sandbox 采纳 |
|------|-----------|------------------|
| Token 估算 | 3-4 chars/token | 4 chars/token + 真实 usage 双轨 |
| 混合检索 | 70% 向量 + 30% FTS | ✅ 采纳 |
| 渐进式降级 | soft→hard→compact 多级 | ✅ 三级 + 压缩边界保护 |
| 压缩边界 | 不在工具调用链中间截断 | ✅ 采纳（pi_agent_rust） |
| 大结果处理 | 摘要 + 文件引用 | 三层防御策略 |
| 提示缓存 | 静态系统提示 + 动态每消息上下文 | ✅ 采纳（Craft Agents） |
| 压缩前保护 | 记忆冲刷 / PreCompact hook | ✅ Memory Flush |

### 1.2 设计原则

1. **上下文即智能** — 精心组装的上下文比增加轮次更能提升 agent 能力
2. **渐进式降级** — 从不裁剪到逐步裁剪，优先保护最新和最相关的信息
3. **预算驱动** — Token 预算管理器是核心调度器，驱动所有降级决策
4. **关注点分离** — 预算计算、降级决策、实际执行三者分离，可独立测试

---

## 2. 三区上下文分配模型

### 2.1 架构总览

```
┌─────────────────────────────────────────────────────────┐
│                    Context Window                        │
│                                                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │  区域 A — 系统提示区（静态，可缓存）              │    │
│  │  ~8K-15K tokens                                  │    │
│  │  核心指令 + Bootstrap 文件 + 输出格式指导         │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │  区域 B — 上下文注入区（每消息动态重建）          │    │
│  │  ~3K-8K tokens                                   │    │
│  │  Working Memory + 记忆检索 + 会话状态 + 日期时间  │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │  区域 C — 对话历史区（滚动窗口）                  │    │
│  │  剩余所有空间                                     │    │
│  │  用户/助手消息 + 工具调用/结果                     │    │
│  │  渐进式裁剪：soft-trim → hard-clear → 压缩摘要    │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │  预留区 — output_reserve + safety_margin          │    │
│  │  ~10K tokens                                      │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Token 预算公式

```
C_history = W_total - R_output - A_system - B_dynamic - R_safety
```

- `W_total`: 模型上下文窗口（如 200K）
- `R_output`: 输出预留（默认 8192 tokens）
- `A_system`: 系统提示实际占用
- `B_dynamic`: 动态上下文实际占用
- `R_safety`: 安全余量（默认 2048 tokens）

### 2.3 设计优势

- **区域 A** 利用 Anthropic prompt caching 降低成本（会话内不变）
- **区域 B** 总是反映最新状态（每轮重新计算，不累积在历史中）
- **区域 C** 获得最大空间，且有明确的降级路径

---

## 3. 系统提示构建策略（区域 A）

### 3.1 组装顺序（优先级从高到低）

```
1. 核心身份与规则    — 角色定义、安全约束、行为准则
                      固定文本，编译时嵌入
                      ~2K tokens

2. 工具声明          — 已注册工具的 JSON Schema
                      通过 Anthropic API 的 `tools` 参数传递
                      不占用系统提示空间

3. Bootstrap 文件    — AGENTS.md / CLAUDE.md 等项目上下文文件
                      截断策略：单文件 ≤20K chars
                      超出时 70% 头部 + 20% 尾部 + 10% 省略标记
                      最多 10 个文件，总量上限 50K chars

4. 技能声明          — 已加载 Skill 的 name/description
                      Phase 3 启用

5. 输出格式指导      — Markdown 渲染、代码块格式等
                      ~500 tokens
```

### 3.2 关键设计决策

- **不含时间戳**：日期时间放入区域 B，确保系统提示可被 prompt caching 复用
- **工具声明分离**：工具定义通过 API `tools` 参数传递，不占用系统提示空间
- **Bootstrap 截断是防御性的**：用户可能放入任意大小的 AGENTS.md，必须有硬上限保护
- **UTF-8 安全截断**：在字符边界截断，不破坏多字节字符（借鉴 ZeroClaw）

### 3.3 Rust 接口设计

```rust
pub struct SystemPromptBuilder {
    core_instructions: &'static str,        // 编译时嵌入
    bootstrap_files: Vec<BootstrapFile>,     // 运行时加载
    output_guidelines: &'static str,
}

pub struct BootstrapFile {
    path: PathBuf,
    content: String,      // 已截断
    truncated: bool,
}

const BOOTSTRAP_MAX_CHARS: usize = 20_000;   // 单文件上限
const BOOTSTRAP_TOTAL_MAX_CHARS: usize = 50_000; // 总量上限
const BOOTSTRAP_MAX_FILES: usize = 10;

impl SystemPromptBuilder {
    /// 发现并加载 Bootstrap 文件
    pub fn discover_bootstrap_files(workspace_dir: &Path) -> Vec<BootstrapFile>;

    /// UTF-8 安全截断，70% 头 + 20% 尾
    fn truncate_bootstrap(content: &str, max_chars: usize) -> (String, bool);

    /// 组装完整系统提示
    pub fn build(&self) -> String;
}
```

### 3.4 与现有代码的关系

现有 `agent/context.rs` 的 `ContextBuilder` 已有基础框架，需要增加：
- Bootstrap 文件发现逻辑（扫描工作目录的 AGENTS.md, CLAUDE.md 等）
- 截断函数（UTF-8 安全，头尾保留策略）
- 分离动态内容到区域 B

---

## 4. 对话历史管理与渐进式降级（区域 C）

### 4.1 三级渐进式降级策略

```
Level 0 — 正常模式（使用率 < 60%）
  ► 完整保留所有消息，无裁剪

Level 1 — 软裁剪（使用率 60%-80%）
  ► 对 ≥2 轮前的工具结果做头尾截断
  ► 保留前 1500 chars + 后 500 chars
  ► 中间替换为 "[... 已省略 N chars ...]"
  ► 不触碰用户消息和助手文本

Level 2 — 硬清除（使用率 80%-90%）
  ► 对更早的工具结果替换为占位符
  ► "[工具 {name} 已执行，结果已省略]"
  ► 保留工具调用的 name + input 摘要（≤200 chars）
  ► 仍然保留所有用户/助手文本消息

Level 3 — 压缩摘要（使用率 > 90%）
  ► 触发完整压缩流程：
  ► 1. 记忆冲刷（Memory Flush）
  ► 2. 结构化摘要生成
  ► 3. 替换旧历史
```

### 4.2 压缩边界保护

压缩点不能在工具调用链中间切割（借鉴 pi_agent_rust）。

工具调用链定义：
```
Assistant: [ToolUse: bash "ls -la"]
Tool:      [ToolResult: "file1.txt file2.txt ..."]
Assistant: [ToolUse: file_read "file1.txt"]
Tool:      [ToolResult: "contents..."]
Assistant: "基于以上文件分析..."    ← 安全压缩边界
```

算法：从最后一条助手文本消息向前扫描，找到完整的 user→assistant(tool_use)→tool_result→...→assistant(text) 边界。

### 4.3 Token 使用率计算

```rust
fn usage_ratio(&self, total_window: usize) -> f64 {
    let used = self.estimate_tokens(); // 双轨估算
    let available = total_window - self.reserved_output - self.safety_margin;
    used as f64 / available as f64
}
```

采用 4 chars/token 估算，但优先使用 Anthropic API 返回的真实 `usage.input_tokens`。

### 4.4 结构化压缩摘要模板

借鉴 pi_agent_rust 的格式：
```
## Goal
[用户的原始目标]

## Progress
### Done
- [已完成的步骤]
### In Progress
- [正在进行的步骤]

## Key Decisions
- [关键技术决策及原因]

## Next Steps
- [下一步计划]

## Critical Context
- [不能丢失的关键信息]
```

---

## 5. 工具结果处理策略

### 5.1 三层防御

```
第一层 — 工具侧硬限制（工具执行时）
  ├─ FileReadTool: 1MB 限制 + 行号 + offset/limit（Phase 1 已有）
  ├─ BashTool: stdout+stderr ≤100K chars
  ├─ GrepTool: 最多 100 个匹配结果
  ├─ GlobTool: 最多 200 个文件路径
  ├─ WebSearchTool: 最多 10 条搜索结果摘要
  └─ FileWriteTool/FileEditTool: 无输出限制（结果通常很小）

第二层 — 结果注入时软裁剪（进入上下文前）
  ├─ TOOL_RESULT_SOFT_LIMIT = 30,000 chars（≈7.5K tokens）
  ├─ 超出时：67% 头部 + 27% 尾部 + 省略标记
  └─ 最新一轮的工具结果永远不裁剪

第三层 — 历史裁剪时降级（Level 1/2 触发时）
  ├─ ≥2 轮前的工具结果按降级策略压缩
  └─ 工具调用的 name + input 摘要始终保留
```

### 5.2 软裁剪实现

```rust
const TOOL_RESULT_SOFT_LIMIT: usize = 30_000; // chars

fn maybe_trim_tool_result(result: &str) -> String {
    if result.len() <= TOOL_RESULT_SOFT_LIMIT {
        return result.to_string();
    }
    let head_size = 20_000;  // 67%
    let tail_size = 8_000;   // 27%
    let head = &result[..head_size]; // UTF-8 安全截断
    let tail = &result[result.len() - tail_size..];
    format!(
        "{head}\n\n[... 已省略 {} chars ...]\n\n{tail}",
        result.len() - head_size - tail_size
    )
}
```

### 5.3 关键原则

- **最新一轮的工具结果永远不裁剪**：agent 正在基于它推理
- **只对 ≥2 轮前的历史工具结果做降级**
- **工具调用的 name 和 input 摘要始终保留**：让 agent 知道"我之前做过什么"

---

## 6. 记忆系统与上下文的集成

### 6.1 三层记忆架构

```
Layer 0 — Working Memory（始终在上下文中，区域 B）
  ├─ Phase 1 已有：InMemoryWorkingMemory（默认 4 blocks）
  ├─ Phase 2 增强：
  │   ├─ priority 排序注入
  │   ├─ max_age_turns 自动过期
  │   └─ Agent 可通过工具主动 add/update/remove
  ├─ 注入格式：<working_memory><block kind="...">...</block></working_memory>
  └─ 预算：≤3K tokens（硬限制，超出按优先级低的先丢弃）

Layer 1 — Session Memory（语义检索注入，区域 B）
  ├─ Phase 2 新增
  ├─ 每轮以用户消息为 query 做混合检索
  │   ├─ 70% 向量 + 30% FTS
  │   └─ top-5 结果，按相关性排序
  ├─ 注入格式：<memory_recall><entry score="0.82">...</entry></memory_recall>
  └─ 预算：≤2K tokens

Layer 2 — Persistent Memory（跨会话，Phase 3）
  ├─ 设计预留，Phase 2 不实现
  └─ 5 类：profile / preferences / tools / debug / patterns
```

### 6.2 压缩前记忆冲刷（Memory Flush）

当 Level 3 压缩触发时：

```
1. 扫描即将被丢弃的历史消息
2. 提取关键事实：
   ├─ 用户偏好和修正
   ├─ 关键技术决策及原因
   ├─ 错误原因和解决方案
   └─ 重要的文件路径和代码模式
3. 写入 Working Memory 的 auto_extracted kind blocks
4. 然后才执行压缩
```

记忆冲刷补充保存压缩摘要可能遗漏的细节。pi_agent_rust 的结构化摘要模板用于压缩摘要本身，而记忆冲刷保存更细粒度的事实。

---

## 7. Token Budget Manager 重构

### 7.1 核心结构

```rust
pub struct ContextBudgetManager {
    /// 模型上下文窗口总量（tokens）
    context_window: u32,
    /// 输出预留（tokens），默认 8192
    output_reserve: u32,
    /// 安全余量（tokens），默认 2048
    safety_margin: u32,
    /// 上次 API 返回的真实 input_tokens
    last_actual_usage: Option<u64>,
}

pub enum DegradationLevel {
    None,       // < 60%
    SoftTrim,   // 60% - 80%
    HardClear,  // 80% - 90%
    Compact,    // > 90%
}
```

### 7.2 预算分配算法

```
可用空间 = context_window - output_reserve - safety_margin

固定开销 = estimate(system_prompt)     // 区域 A
         + estimate(dynamic_context)   // 区域 B

历史空间 = 可用空间 - 固定开销         // 区域 C
```

### 7.3 Token 估算双轨制

1. **有真实数据时**：使用上次 API 返回的 `usage.input_tokens` + 此后新增消息的 chars/4 估算
2. **无真实数据时**：全部使用 chars/4 估算

每次 API 响应更新 `last_actual_usage`，第二轮开始就有基准值。

### 7.4 降级触发逻辑

```rust
impl ContextBudgetManager {
    pub fn compute_degradation_level(
        &self,
        messages: &[ChatMessage],
    ) -> DegradationLevel {
        let ratio = self.usage_ratio(messages);
        match ratio {
            r if r < 0.60 => DegradationLevel::None,
            r if r < 0.80 => DegradationLevel::SoftTrim,
            r if r < 0.90 => DegradationLevel::HardClear,
            _ => DegradationLevel::Compact,
        }
    }
}
```

### 7.5 关注点分离

- `ContextBudgetManager`: 只负责预算计算和降级决策
- `ContextPruner`（新模块）: 执行实际的裁剪/清除/压缩操作
- 两者可独立测试

### 7.6 与现有代码的关系

现有 `memory/budget.rs` 的 `TokenBudgetManager` 重构为 `ContextBudgetManager`，保持向后兼容的 `estimate_tokens()` 方法。

---

## 8. 参考项目分析摘要

### 8.1 各项目关键特征

| 项目 | 系统提示 | Bootstrap | 历史管理 | Token 估算 | 工具结果 | 记忆 |
|------|---------|-----------|---------|-----------|---------|------|
| **OpenClaw** | 20+ 有序段，PromptMode | 20K chars，70%头+20%尾 | soft→hard→compact 三级 | 4 chars/token | 1500+1500 头尾 | 70%向量+30%FTS，6条 |
| **ZeroClaw** | 7 段固定顺序 | 20K chars，UTF-8 感知 | FIFO 50 条上限 | 4 chars/token | 无显式截断 | 70%向量+30%FTS，5条 |
| **NanoClaw** | SDK preset + append | 无显式限制 | SDK 自动压缩 | 无显式 | 哨兵标记流 | 无 |
| **HappyClaw** | preset + 多层注入 | 无显式限制 | SDK + 记忆冲刷 | 无显式 | 无显式 | 日期文件 + MCP |
| **pi_agent_rust** | Context + Cow | 无 bootstrap | 智能压缩边界 | 3 chars/token | 无显式 | 无 |
| **Craft Agents** | 静态缓存 + 动态每消息 | 30文件×10KB | SDK resume + 恢复 | 4 chars/token | 15K→Haiku摘要 | 文件引用 |

### 8.2 关键发现

1. **Token 估算一致性**：所有项目用 3-4 chars/token，pi_agent_rust 最保守(3)
2. **混合检索共识**：OpenClaw 和 ZeroClaw 都用 70%向量+30%FTS
3. **渐进式降级**：最成熟的项目（OpenClaw）有三级降级
4. **压缩边界**：pi_agent_rust 不在工具调用链中间截断
5. **大结果处理**：Craft Agents 的 Haiku 摘要+文件引用最优雅
6. **两层提示架构**：Craft Agents 的静态/动态分离是成本优化最佳实践

---

## 9. 实施路径

### 9.1 Phase 2 实施优先级

```
第一批（Agent Loop + 上下文工程核心）：
  1. ContextBudgetManager 重构（双轨估算 + 降级决策）
  2. ContextPruner 新模块（三级降级执行）
  3. SystemPromptBuilder 增强（Bootstrap 文件发现 + 截断）
  4. 工具结果三层防御
  5. 压缩边界保护

第二批（记忆集成）：
  6. Working Memory 增强（priority + max_age_turns + Agent 工具）
  7. Session Memory + 混合检索（SQLite FTS5 + 向量）
  8. Memory Flush 机制
  9. 结构化压缩摘要

第三批（精细化）：
  10. 动态上下文注入区（区域 B 每消息重建）
  11. Token 预算监控（debug panel 集成）
```

### 9.2 新增/修改文件预估

| 文件 | 操作 | 说明 |
|------|------|------|
| `crates/octo-engine/src/context/budget.rs` | 重构 | ContextBudgetManager（从 memory/budget.rs 迁移） |
| `crates/octo-engine/src/context/pruner.rs` | 新增 | ContextPruner 三级降级执行 |
| `crates/octo-engine/src/context/builder.rs` | 重构 | SystemPromptBuilder + 区域 B 构建（从 agent/context.rs 迁移） |
| `crates/octo-engine/src/context/mod.rs` | 新增 | context 模块声明 |
| `crates/octo-engine/src/agent/loop_.rs` | 修改 | 集成 ContextBudgetManager + ContextPruner |
| `crates/octo-engine/src/memory/working.rs` | 修改 | 增加 priority + max_age_turns |
| `crates/octo-engine/src/memory/flush.rs` | 新增 | Memory Flush 机制 |
| `crates/octo-engine/src/tools/*.rs` | 修改 | 各工具增加输出硬限制 |

---

## 10. 附录：上下文分配示例

### 200K 窗口，正常模式（Level 0）

```
区域 A（系统提示）:     ~5,000 tokens  (2.5%)
区域 B（动态上下文）:    ~4,000 tokens  (2.0%)
  ├─ Working Memory:    ~2,000 tokens
  ├─ Memory Recall:     ~1,500 tokens
  └─ Session State:     ~500 tokens
区域 C（对话历史）:     ~178,000 tokens (89.0%)
输出预留:               ~8,192 tokens  (4.1%)
安全余量:               ~2,048 tokens  (1.0%)
未使用:                 ~2,760 tokens  (1.4%)
──────────────────────────────────────────────
总计:                   200,000 tokens (100%)
```

### 200K 窗口，Level 1 软裁剪后

```
区域 C 使用率降至 ~70% 后稳定
旧工具结果被头尾截断，释放约 20-30% 空间
```

### 200K 窗口，Level 3 压缩后

```
区域 C 替换为：
  ├─ 压缩摘要:    ~3,000 tokens
  └─ 最近 20K tokens 的完整历史
Working Memory 新增 auto_extracted blocks（Memory Flush 产物）
```
