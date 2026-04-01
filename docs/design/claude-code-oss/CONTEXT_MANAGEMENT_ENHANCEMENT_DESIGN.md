# Octo-Engine 上下文管理增强设计

> 基于 Claude Code OSS 代码级分析，设计 octo-engine 上下文管理的系统性改进方案。
> 日期：2026-04-01
> 目标：从当前的 truncate-only 策略升级为多层渐进式压缩管道。

---

## 一、背景与动机

### 当前问题

Octo 的上下文管理只有两种策略：**截断**（丢弃旧消息）和**遮蔽**（ObservationMasker 替换旧工具输出为占位符）。这导致：

1. **长对话信息丢失** — 截断直接删除旧消息，agent 丢失关键上下文
2. **prompt_too_long 直接死亡** — LLM 返回 PTL 错误时 loop 直接终止，无法恢复
3. **压缩后失忆** — 截断后没有状态重建，agent 丢失 working memory、active skill 等运行状态
4. **无 LLM 摘要** — `CompactionStrategy::Summarize` 枚举已定义但未实现

### 参照基准

Claude Code OSS 的上下文管理有 3 个已实现策略（microCompact、autoCompact、sessionMemoryCompact）和 3 个 stub 策略（reactive、collapse、snip）。本设计参照已实现的策略，并补充 reactive compact（因为依赖 P0 其他组件，实现成本极低）。

---

## 二、设计范围

### P0（本次实施）

| 编号 | 改进项 | 概述 |
|------|--------|------|
| P0-1 | prompt_too_long 恢复 | harness 捕获 PTL 错误 → 紧急截断 → 重试 |
| P0-2 | LLM 摘要压缩 | CompactionPipeline + 9 段结构化 prompt |
| P0-3 | 压缩后状态重建 | Zone B/B+/B++ 重注入、skill 重注入、hooks 重触发 |
| P0-4 | Reactive compact | PTL 路径调用 CompactionPipeline，失败回退 truncate |
| P0-5 | 微压缩增强 | ObservationMasker 增加时间触发 + 工具白名单 |
| P0-6 | Streaming tool execution | Tool trait 增加 on_progress 回调 |

### P1（后续实施）

| 编号 | 改进项 | 概述 |
|------|--------|------|
| P1-1 | Context collapse | 粒度级折叠，扩展 ObservationMasker |
| P1-2 | Snip compact | 用户主动标记裁剪点 |
| P1-3 | Tool 接口增强 | validate_input, is_read_only, is_destructive |
| P1-4 | Permission 细粒度化 | 规则语法、多来源优先级、Bash 命令分析 |

---

## 三、详细设计

### P0-1: prompt_too_long 恢复

**位置**：`crates/octo-engine/src/agent/harness.rs`

**当前行为**：LLM 返回 PTL 错误 → `RetryPolicy` 判定为不可重试 → loop 终止

**改进后行为**：

```
LLM stream 失败
  → RetryPolicy 判断
  → 如果是 prompt_too_long:
      → 不走 retry，走 compact 路径
      → compact_attempts += 1
      → if compact_attempts <= 3:
          → pruner.apply(messages, OverflowCompaction)
          → emit AgentEvent::ContextCompacted { strategy: "truncate_ptl" }
          → continue（重新进入 loop）
      → else:
          → emit AgentEvent::Error { "Context too large after 3 compact attempts" }
          → 终止
```

**关键代码位置**：`harness.rs` 第 620-670 行（当前的 LLM stream 错误处理后）

**PTL 检测逻辑**：
```rust
fn is_prompt_too_long(err: &anyhow::Error) -> bool {
    let s = err.to_string().to_lowercase();
    s.contains("prompt_too_long")
        || s.contains("prompt is too long")
        || (s.contains("400") && s.contains("too many tokens"))
}
```

**新增字段**（AgentLoopConfig 不需要改，只需 harness 局部变量）：
```rust
let mut compact_attempts: u32 = 0;
const MAX_COMPACT_ATTEMPTS: u32 = 3;
```

---

### P0-2: LLM 摘要压缩（CompactionPipeline）

**新增文件**：`crates/octo-engine/src/context/compaction_pipeline.rs`

#### 数据结构

```rust
/// 压缩管道配置
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// 摘要使用的模型（None 则复用当前模型）
    pub compact_model: Option<String>,
    /// 摘要最大输出 tokens
    pub summary_max_tokens: u32,
    /// 保留最近 N 条消息不压缩
    pub keep_recent_messages: usize,
    /// 摘要调用自身 PTL 时最大重试次数
    pub max_ptl_retries: u32,
    /// 压缩后重注入最近读过的文件数
    pub max_files_to_restore: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            compact_model: None,
            summary_max_tokens: 2000,
            keep_recent_messages: 6,
            max_ptl_retries: 3,
            max_files_to_restore: 5,
        }
    }
}

/// 压缩结果
pub struct CompactionResult {
    /// 压缩边界标记消息
    pub boundary_marker: ChatMessage,
    /// LLM 生成的摘要消息
    pub summary_messages: Vec<ChatMessage>,
    /// 保留的最近消息
    pub kept_messages: Vec<ChatMessage>,
    /// 需要重注入的状态消息（Zone B、skill 等）
    pub reinjections: Vec<ChatMessage>,
    /// 压缩前 token 数
    pub pre_compact_tokens: usize,
    /// 压缩后 token 数
    pub post_compact_tokens: usize,
}
```

#### 压缩流程

```rust
pub struct CompactionPipeline {
    config: CompactionConfig,
}

impl CompactionPipeline {
    pub async fn compact(
        &self,
        messages: &[ChatMessage],
        provider: &dyn Provider,
        model: &str,                    // 当前会话模型
        context: &CompactionContext,    // 包含 memory、skills、hooks 等状态
    ) -> Result<CompactionResult> {
        // 1. 确定压缩边界
        let keep_count = self.config.keep_recent_messages;
        let boundary = messages.len().saturating_sub(keep_count);
        if boundary < 2 {
            return Err(anyhow!("Not enough messages to compact"));
        }
        let to_summarize = &messages[..boundary];
        let to_keep = &messages[boundary..];

        // 2. 预处理要摘要的消息
        let preprocessed = Self::preprocess_for_summary(to_summarize);

        // 3. 构建摘要 prompt
        let prompt = Self::build_compact_prompt(context.custom_instructions.as_deref());

        // 4. 调用 LLM 生成摘要（带 PTL 自重试）
        let compact_model = self.config.compact_model.as_deref().unwrap_or(model);
        let summary = self.generate_summary(
            provider, compact_model, &preprocessed, &prompt
        ).await?;

        // 5. 格式化摘要（剥离 <analysis> 块，提取 <summary> 内容）
        let formatted = Self::format_summary(&summary);

        // 6. 压缩后状态重建
        let reinjections = self.rebuild_state(context).await;

        // 7. 创建 boundary marker
        let boundary_marker = ChatMessage::system(
            "[Context compacted: earlier conversation summarized below]"
        );

        // 8. 构建结果
        Ok(CompactionResult {
            boundary_marker,
            summary_messages: vec![ChatMessage::user(&formatted)],
            kept_messages: to_keep.to_vec(),
            reinjections,
            pre_compact_tokens: estimate_tokens(messages),
            post_compact_tokens: estimate_tokens(&[/* summary + kept + reinjections */]),
        })
    }
}
```

#### 摘要 Prompt（参照 CC 9 段结构，中文化）

```rust
const COMPACT_PROMPT: &str = r#"你的任务是为以下对话创建详细摘要。这个摘要需要完整捕获技术细节、代码模式和架构决策，以确保后续开发工作不丢失上下文。

在生成摘要之前，先用 <analysis> 标签整理你的思路：
1. 按时间顺序分析每条消息，识别用户意图、技术方案、代码变更、错误修复
2. 特别关注用户的反馈和纠正

摘要应包含以下 9 个部分：

1. 主要请求与意图：用户的显式需求和目标
2. 关键技术概念：讨论的技术框架、模式、工具
3. 文件与代码：查看/修改/创建的文件，包含关键代码片段
4. 错误与修复：遇到的错误及修复方法，特别是用户反馈
5. 问题解决：已解决的问题和进行中的排查
6. 用户消息：列出所有非工具结果的用户消息
7. 待处理任务：明确被要求的待办事项
8. 当前工作：压缩前正在进行的具体工作，包含文件名和代码片段
9. 下一步（可选）：与最近工作直接相关的下一步，引用原文防止任务漂移

<example>
<analysis>
[分析过程]
</analysis>

<summary>
1. 主要请求与意图：[详细描述]
2. 关键技术概念：- [概念1] - [概念2]
3. 文件与代码：- [文件名] - [代码片段]
...
</summary>
</example>

请严格按此结构输出。不要调用任何工具，只输出纯文本。"#;
```

#### 消息预处理

```rust
fn preprocess_for_summary(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    messages.iter().map(|m| {
        let mut msg = m.clone();
        for block in &mut msg.content {
            match block {
                // 图片 → 占位符（减少摘要成本）
                ContentBlock::Image { .. } => {
                    *block = ContentBlock::Text { text: "[image]".into() };
                }
                // 超长工具结果 → 截断
                ContentBlock::ToolResult { content, .. } if content.len() > 2000 => {
                    let truncated = &content[..2000];
                    *content = format!("{}... [truncated, {} chars total]", truncated, content.len());
                }
                _ => {}
            }
        }
        msg
    }).collect()
}
```

#### 摘要生成（带 PTL 自重试）

```rust
async fn generate_summary(
    &self,
    provider: &dyn Provider,
    model: &str,
    messages: &[ChatMessage],
    prompt: &str,
) -> Result<String> {
    let mut to_summarize = messages.to_vec();

    for attempt in 0..self.config.max_ptl_retries {
        // 构建摘要请求
        let request = CompletionRequest {
            model: model.to_string(),
            system: Some(prompt.to_string()),
            messages: to_summarize.clone(),
            max_tokens: self.config.summary_max_tokens,
            tools: vec![],  // 摘要不需要工具
            stream: false,
            temperature: None,
        };

        match provider.complete(request).await {
            Ok(response) => {
                return Ok(response.text_content());
            }
            Err(e) if is_prompt_too_long(&e) => {
                // 摘要调用自身也 PTL → 截掉最老的 20% 消息重试
                let drop_count = (to_summarize.len() / 5).max(1);
                tracing::warn!(
                    attempt, drop_count,
                    "Compact summary itself hit PTL, dropping oldest messages"
                );
                to_summarize = to_summarize[drop_count..].to_vec();

                if to_summarize.len() < 2 {
                    return Err(anyhow!("Not enough messages left after PTL retry"));
                }
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(anyhow!("Compact summary failed after {} PTL retries", self.config.max_ptl_retries))
}
```

#### 摘要格式化

```rust
fn format_summary(raw: &str) -> String {
    let mut result = raw.to_string();

    // 剥离 <analysis> 块（drafting scratchpad，只用于提升摘要质量）
    if let Some(start) = result.find("<analysis>") {
        if let Some(end) = result.find("</analysis>") {
            result = format!("{}{}", &result[..start], &result[end + "</analysis>".len()..]);
        }
    }

    // 提取 <summary> 内容
    if let Some(start) = result.find("<summary>") {
        if let Some(end) = result.find("</summary>") {
            let content = &result[start + "<summary>".len()..end];
            result = format!("Summary:\n{}", content.trim());
        }
    }

    // 添加上下文提示
    format!(
        "This session is being continued from a previous conversation. \
         The summary below covers the earlier portion.\n\n{}",
        result.trim()
    )
}
```

---

### P0-3: 压缩后状态重建

**位置**：`CompactionPipeline::rebuild_state()`

```rust
async fn rebuild_state(&self, ctx: &CompactionContext) -> Vec<ChatMessage> {
    let mut reinjections = Vec::new();

    // 1. 重新编译 Zone B working memory
    if let Some(ref memory) = ctx.memory {
        let xml = memory.compile(&ctx.user_id, &ctx.sandbox_id).await.unwrap_or_default();
        if !xml.is_empty() {
            reinjections.push(ChatMessage::system(&format!(
                "<working_memory>\n{}\n</working_memory>", xml
            )));
        }
    }

    // 2. 重新编译 Zone B+ cross-session memory
    if let Some(ref store) = ctx.memory_store {
        let injector = crate::memory::MemoryInjector::with_defaults();
        let cross = injector.build_memory_context(
            store.as_ref(), &ctx.user_id, ""
        ).await;
        if !cross.is_empty() {
            reinjections.push(ChatMessage::system(&cross));
        }
    }

    // 3. 重注入 active skill 上下文
    if let Some(ref skill) = ctx.active_skill {
        if let Some(ref content) = skill.system_prompt {
            reinjections.push(ChatMessage::system(&format!(
                "[Active skill: {}]\n{}", skill.name, content
            )));
        }
    }

    // 4. 重触发 SessionStart hooks
    if let Some(ref hooks) = ctx.hook_registry {
        let hook_ctx = HookContext::new_session_start();
        hooks.execute(HookPoint::SessionStart, &hook_ctx).await;
    }

    reinjections
}

/// 压缩上下文，包含状态重建所需的所有引用
pub struct CompactionContext {
    pub memory: Option<Arc<dyn WorkingMemory>>,
    pub memory_store: Option<Arc<dyn MemoryStore>>,
    pub active_skill: Option<SkillManifest>,
    pub hook_registry: Option<Arc<HookRegistry>>,
    pub user_id: String,
    pub sandbox_id: String,
}
```

---

### P0-4: Reactive Compact

**位置**：`crates/octo-engine/src/agent/harness.rs`（替代 P0-1 的简单 truncate）

当 P0-2 CompactionPipeline 完成后，P0-1 的 PTL 恢复路径升级为：

```rust
// 在 harness.rs 的 LLM 错误处理中
if is_prompt_too_long(&e) && compact_attempts < MAX_COMPACT_ATTEMPTS {
    compact_attempts += 1;

    // 优先尝试 LLM 摘要压缩
    if let Some(ref pipeline) = config.compaction_pipeline {
        let ctx = build_compaction_context(&config);
        match pipeline.compact(&messages, &*provider, &config.model, &ctx).await {
            Ok(result) => {
                // 成功：用压缩结果替换消息
                messages.clear();
                messages.push(result.boundary_marker);
                messages.extend(result.summary_messages);
                messages.extend(result.kept_messages);
                messages.extend(result.reinjections);

                let _ = tx.send(AgentEvent::ContextCompacted {
                    strategy: "llm_summary".into(),
                    pre_tokens: result.pre_compact_tokens,
                    post_tokens: result.post_compact_tokens,
                }).await;

                continue; // 重新进入 loop
            }
            Err(compact_err) => {
                warn!("LLM compact failed: {compact_err}, falling back to truncate");
            }
        }
    }

    // 回退：紧急截断
    pruner.apply(&mut messages, DegradationLevel::OverflowCompaction);
    let _ = tx.send(AgentEvent::ContextCompacted {
        strategy: "truncate_fallback".into(),
        pre_tokens: 0,
        post_tokens: 0,
    }).await;

    continue; // 重新进入 loop
}
```

---

### P0-5: 微压缩增强

**位置**：`crates/octo-engine/src/context/observation_masker.rs`

增加两个能力：

#### 时间触发

```rust
pub struct ObservationMaskConfig {
    pub keep_recent_turns: usize,
    pub min_mask_length: usize,
    /// 新增：距上次 assistant 消息超过此分钟数时主动清除
    pub time_trigger_minutes: Option<u64>,
    /// 新增：只清除这些工具的输出
    pub compactable_tools: Option<HashSet<String>>,
}

impl ObservationMasker {
    /// 检查是否应该基于时间触发微压缩
    pub fn should_time_trigger(
        &self,
        messages: &[ChatMessage],
        now: std::time::Instant,
        last_assistant_time: Option<std::time::Instant>,
    ) -> bool {
        if let (Some(threshold), Some(last)) = (
            self.config.time_trigger_minutes,
            last_assistant_time,
        ) {
            let gap = now.duration_since(last);
            return gap.as_secs() / 60 >= threshold;
        }
        false
    }
}
```

#### 工具白名单

```rust
const DEFAULT_COMPACTABLE_TOOLS: &[&str] = &[
    "bash", "file_read", "file_write", "file_edit",
    "grep", "glob", "find",
    "web_fetch", "web_search",
];

impl ObservationMasker {
    pub fn mask(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        // ... 现有逻辑 ...
        // 新增：检查工具白名单
        if let Some(ref whitelist) = self.config.compactable_tools {
            if !whitelist.contains(&tool_name) {
                continue; // 不在白名单中的工具输出不压缩
            }
        }
    }
}
```

---

### P0-6: Streaming Tool Execution

**位置**：`crates/octo-engine/src/tools/traits.rs` + `crates/octo-engine/src/agent/harness.rs`

#### Tool trait 扩展

```rust
/// 工具执行进度回调
pub type ProgressCallback = Arc<dyn Fn(ToolProgress) + Send + Sync>;

#[derive(Debug, Clone)]
pub enum ToolProgress {
    /// 标准输出（如 bash 命令的 stdout）
    Stdout(String),
    /// 标准错误
    Stderr(String),
    /// 进度百分比
    Percent(f32),
    /// 自定义状态消息
    Status(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    // ... 现有方法 ...

    /// 带进度回调的执行方法（默认实现调用无进度版本）
    async fn execute_with_progress(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
        on_progress: Option<ProgressCallback>,
    ) -> Result<ToolOutput> {
        // 默认忽略 progress callback
        self.execute(params, ctx).await
    }
}
```

#### BashTool 进度流实现

```rust
// 在 BashTool 中覆盖 execute_with_progress
async fn execute_with_progress(
    &self,
    params: Value,
    ctx: &ToolContext,
    on_progress: Option<ProgressCallback>,
) -> Result<ToolOutput> {
    // ... 启动子进程 ...

    // 如果有 progress callback，逐行转发 stdout/stderr
    if let Some(cb) = &on_progress {
        // 在 tokio::spawn 中读取 stdout 并回调
        let cb_clone = cb.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                cb_clone(ToolProgress::Stdout(line));
            }
            Ok::<(), anyhow::Error>(())
        });
    }

    // ... 等待完成 ...
}
```

#### Harness 集成

```rust
// harness.rs 工具执行时，将 progress 转发为 AgentEvent
let progress_tx = tx.clone();
let tool_id = tu.id.clone();
let on_progress: ProgressCallback = Arc::new(move |p| {
    let event = AgentEvent::ToolProgress {
        tool_id: tool_id.clone(),
        progress: p,
    };
    let _ = progress_tx.try_send(event);
});

let result = tool.execute_with_progress(input, &tool_ctx, Some(on_progress)).await;
```

---

## 四、集成点

### harness.rs 修改清单

1. **新增 `compact_attempts` 计数器**（P0-1/P0-4）
2. **LLM 错误处理新增 PTL 分支**（P0-1/P0-4）
3. **AgentLoopConfig 新增 `compaction_pipeline: Option<CompactionPipeline>`**（P0-2）
4. **AgentLoopConfig 新增 `compact_model: Option<String>`**（P0-2）
5. **工具执行增加 progress callback 转发**（P0-6）
6. **ObservationMasker 时间触发检查**（P0-5）

### 新增 AgentEvent 变体

```rust
pub enum AgentEvent {
    // ... 现有变体 ...

    /// 上下文压缩完成
    ContextCompacted {
        strategy: String,        // "llm_summary" | "truncate_fallback" | "truncate_ptl"
        pre_tokens: usize,
        post_tokens: usize,
    },

    /// 工具执行进度
    ToolProgress {
        tool_id: String,
        progress: ToolProgress,
    },
}
```

### 新增文件

| 文件 | 内容 | 预估行数 |
|------|------|---------|
| `context/compaction_pipeline.rs` | CompactionPipeline 核心逻辑 | ~350 行 |
| `context/compact_prompt.rs` | 摘要 prompt 模板 | ~80 行 |

### 修改文件

| 文件 | 修改内容 | 预估改动 |
|------|---------|---------|
| `agent/harness.rs` | PTL 恢复 + reactive compact + progress 转发 | ~100 行 |
| `agent/loop_config.rs` | 新增 compaction_pipeline/compact_model 字段 | ~15 行 |
| `agent/events.rs` | 新增 ContextCompacted/ToolProgress 事件 | ~20 行 |
| `context/observation_masker.rs` | 时间触发 + 工具白名单 | ~60 行 |
| `context/mod.rs` | 导出新模块 | ~5 行 |
| `tools/traits.rs` | execute_with_progress + ToolProgress 类型 | ~30 行 |
| `tools/bash.rs` | 覆盖 execute_with_progress | ~40 行 |

**总计约 ~700 行新代码 + ~300 行修改**

---

## 五、实施分组

### G1: PTL 恢复 + Reactive Compact（P0-1 + P0-4）

依赖：无
改动：harness.rs + events.rs + loop_config.rs
可独立测试：是（在 truncate fallback 模式下可独立工作）

### G2: LLM 摘要压缩 + 状态重建（P0-2 + P0-3）

依赖：无（但 G1 完成后 reactive compact 路径可调用）
改动：compaction_pipeline.rs (新) + compact_prompt.rs (新) + harness.rs 集成
可独立测试：是（CompactionPipeline 可独立调用测试）

### G3: 微压缩增强（P0-5）

依赖：无
改动：observation_masker.rs
可独立测试：是

### G4: Streaming Tool Execution（P0-6）

依赖：无
改动：traits.rs + bash.rs + harness.rs
可独立测试：是

### 推荐实施顺序

```
G1 (PTL恢复) → G2 (LLM摘要) → G3 (微压缩) → G4 (进度流)
     │                │
     └── G2 完成后，G1 的 truncate fallback 自动升级为 reactive compact
```

G3 和 G4 与 G1/G2 无依赖，可并行实施。

---

## 六、测试策略

### 单元测试

| 测试目标 | 测试内容 |
|---------|---------|
| `is_prompt_too_long()` | 各种 PTL 错误格式的检测 |
| `CompactionPipeline::preprocess_for_summary()` | 图片替换、长内容截断 |
| `CompactionPipeline::format_summary()` | analysis 剥离、summary 提取 |
| `ObservationMasker::should_time_trigger()` | 时间阈值判断 |
| 工具白名单 | 白名单内/外工具的差异行为 |

### 集成测试

| 测试目标 | 测试内容 |
|---------|---------|
| PTL 恢复 | mock provider 返回 PTL → 验证 loop 不终止、消息被截断 |
| Reactive compact | mock provider 返回 PTL → 验证 LLM 摘要被调用 → 消息替换 |
| LLM 摘要自身 PTL | mock provider 对摘要也返回 PTL → 验证消息被截掉后重试 |
| 状态重建 | 压缩后验证 Zone B memory 和 active skill 被重注入 |
| Streaming progress | BashTool 执行带 callback → 验证 AgentEvent::ToolProgress |

---

## 七、改进后预期效果

| 指标 | 当前 | 改进后 |
|------|------|--------|
| 长对话存活率 | PTL 直接死亡 | 自动压缩恢复，3 次降级机会 |
| 信息保留 | 截断丢失 | LLM 摘要保留关键上下文 |
| 压缩后连贯性 | 失忆（无状态重建） | Zone B + skill + hooks 重建 |
| 工具执行反馈 | 黑屏等待 | 实时 stdout/stderr 流 |
| Agent Loop 能力覆盖 | 68% (vs CC) | ~95% (vs CC) |
