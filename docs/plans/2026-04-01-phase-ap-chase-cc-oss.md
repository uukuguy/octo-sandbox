# Phase AP — 追赶 CC-OSS（全量 P0-P2）

> 目标：系统性缩小 Octo 与 Claude Code OSS 的精细工程差距，同时保持 Octo 自身优势。
> 日期：2026-04-01
> 依据：`docs/design/claude-code-oss/` 9 份分析设计文档 + CC-OSS 源码深度审计
> 拓扑排序策略：按代码依赖图排最优执行顺序，不严格按 P0/P1/P2 标签

---

## 一、依赖图

```
              ┌─────────────┐
              │ T1 提示词增强  │  零依赖，效果立竿见影
              └──────┬──────┘
                     │
              ┌──────┴──────┐
              │ T2 工具描述升级 │  零依赖，纯文本
              └──────┬──────┘
                     │
     ┌───────────────┼───────────────┐
     │               │               │
┌────┴────┐   ┌──────┴──────┐  ┌─────┴─────┐
│ T3 PTL  │   │ T4 Tool trait│  │ T5 微压缩  │  零/低依赖
│  恢复   │   │   增强       │  │   增强     │
└────┬────┘   └──────┬──────┘  └─────┬─────┘
     │               │               │
┌────┴────┐   ┌──────┴──────┐        │
│ T6 LLM  │   │ T7 Streaming│        │
│ 摘要压缩 │   │  进度流     │        │
└────┬────┘   └─────────────┘        │
     │                               │
┌────┴────────────────────────────────┴────┐
│ T8 PermissionEngine 6 层规则引擎          │
└────┬────────────────────────────────────┘
     │
     ├──── T9  Context Collapse 粒度折叠
     ├──── T10 Snip Compact 用户裁剪
     ├──── T11 Hook 输出能力增强
     │
┌────┴──────────────┐
│ T12 多 Agent 工具   │  依赖 SessionRegistry (已有)
│ (session_*/task_*) │
└────┬──────────────┘
     │
┌────┴──────────────┐
│ T13 TeamManager   │  依赖 T12
│ (team_*)          │
└───────────────────┘
     │
┌────┴──────────────┐
│ T14 自主运行模式   │  依赖 T6 (压缩) + T8 (权限)
│ Phase 1           │
└───────────────────┘
     │
┌────┴──────────────┐
│ T15 成本追踪      │  依赖 Provider trait
│ CostTracker       │
└───────────────────┘

TUI 独立并行线（无引擎依赖）:
  T16 TUI Phase 1 (符号+动画+快捷键)
  T17 TUI Phase 2 (Shimmer+历史搜索+权限UI)
  T18 TUI Phase 3 (Vim+模型选择器+Spinner Tree)
```

---

## 二、任务详细清单

### Wave 1：零依赖高 ROI（立竿见影）

#### T1 — 提示词体系增强
- **原优先级**: P0-7 + P1-5
- **设计文档**: `PROMPT_SYSTEM_ENHANCEMENT_DESIGN.md`
- **依赖**: 无
- **修改文件**: `crates/octo-engine/src/context/system_prompt.rs`
- **内容**:
  - 新增 6 个静态段：System / Code Style (YAGNI) / Actions / Using Tools / Output Efficiency / Output Format
  - 拆分 `CORE_INSTRUCTIONS` 为多段落，保留 Octo 独有段（Memory/ReAct/Search/File）
  - 新增 `with_git_status()` 动态注入 git 状态
  - 新增 `build_using_tools_section()` 条件注入（根据可用工具动态包含指导）
- **预估**: ~100 行新增/修改
- **测试**: 现有 system_prompt 测试更新，新增段落内容验证

#### T2 — 现有工具描述升级
- **原优先级**: P1-9
- **设计文档**: `TOOL_SYSTEM_ENHANCEMENT_DESIGN.md` 第五节
- **依赖**: 无
- **修改文件**: `crates/octo-engine/src/tools/` 下 9 个核心工具文件
- **内容**:
  - bash: 从 1 行 → ~80 行（timeout/working_dir、优先专用工具提醒、危险命令警告、git commit/PR 指引）
  - file_read: → ~20 行（支持格式、行号范围、二进制/PDF）
  - file_edit: → ~25 行（必须先读再编、old_string 唯一性、不创建不必要文件）
  - file_write: → ~15 行（优先编辑现有文件）
  - grep: → ~20 行（正则语法、output_mode）
  - glob: → ~15 行（glob 语法）
  - web_search/web_fetch: → ~15 行/个
  - subagent: → ~60 行（参照 CC AgentTool 结构，何时用/不用/示例/反模式）
- **预估**: ~500 行纯文本改写（无逻辑代码变更）
- **测试**: 编译通过即可（description 返回值变更）

### Wave 2：核心上下文管理（最大差距）

#### T3 — prompt_too_long 恢复
- **原优先级**: P0-1
- **设计文档**: `CONTEXT_MANAGEMENT_ENHANCEMENT_DESIGN.md` P0-1
- **依赖**: 无
- **修改文件**: `harness.rs` (~60 行), `events.rs` (~10 行)
- **内容**:
  - `is_prompt_too_long()` 检测函数（多种错误格式）
  - harness 主循环 LLM 错误处理后新增 PTL 分支
  - `compact_attempts` 计数器（最多 3 次）
  - 第一阶段用 `pruner.apply(OverflowCompaction)` 紧急截断
  - PTL 恢复路径**不触发 Stop hooks**（防 death spiral，CC 经验 `query.ts:1171-1175`）
  - **静默恢复语义**：恢复过程不向用户暴露错误，成功恢复后继续
  - 新增 `AgentEvent::ContextCompacted { strategy, pre_tokens, post_tokens }`
- **预估**: ~70 行
- **测试**: mock provider 返回 PTL → 验证 loop 不终止 + 消息被截断

#### T4 — Tool trait 接口增强
- **原优先级**: P1-4 + P0-6 (Streaming)
- **设计文档**: `PERMISSION_AND_P1_ENHANCEMENT_DESIGN.md` P1-4 + `CONTEXT_MANAGEMENT_ENHANCEMENT_DESIGN.md` P0-6
- **依赖**: 无
- **修改文件**: `tools/traits.rs` (~30 行), `tools/bash.rs` (~40 行), 各工具声明 (~60 行), `harness.rs` (~30 行)
- **内容**:
  - Tool trait 新增: `is_read_only()`, `is_destructive()`, `is_concurrency_safe()`, `validate_input()`
  - Tool trait 新增: `execute_with_progress()` + `ToolProgress` enum (Stdout/Stderr/Percent/Status)
  - `ProgressCallback = Arc<dyn Fn(ToolProgress) + Send + Sync>`
  - BashTool 覆盖 `execute_with_progress` 逐行转发 stdout/stderr
  - BashTool 覆盖 `validate_input` 基础危险命令检测
  - 各工具标记 is_read_only/is_destructive/is_concurrency_safe
  - harness 集成 validate_input 前置检查 + progress 转发为 AgentEvent::ToolProgress
  - 新增 `AgentEvent::ToolProgress { tool_id, progress }`
- **预估**: ~160 行
- **测试**: validate_input 单元测试 + BashTool progress callback 测试

#### T5 — 微压缩增强 (ObservationMasker)
- **原优先级**: P0-5
- **设计文档**: `CONTEXT_MANAGEMENT_ENHANCEMENT_DESIGN.md` P0-5
- **依赖**: 无
- **修改文件**: `context/observation_masker.rs` (~60 行)
- **内容**:
  - `ObservationMaskConfig` 新增: `time_trigger_minutes: Option<u64>`, `compactable_tools: Option<HashSet<String>>`
  - `should_time_trigger()` 方法
  - `mask()` 内增加工具白名单过滤
  - `DEFAULT_COMPACTABLE_TOOLS` 常量（bash, file_read, file_write, file_edit, grep, glob, find, web_fetch, web_search）
- **预估**: ~60 行
- **测试**: 时间触发 + 白名单过滤单元测试

### Wave 3：LLM 摘要压缩 + Reactive Compact

#### T6 — CompactionPipeline + 状态重建
- **原优先级**: P0-2 + P0-3 + P0-4
- **设计文档**: `CONTEXT_MANAGEMENT_ENHANCEMENT_DESIGN.md` P0-2/P0-3/P0-4
- **依赖**: T3 (PTL 恢复路径升级为 reactive compact)
- **新增文件**: `context/compaction_pipeline.rs` (~350 行), `context/compact_prompt.rs` (~80 行)
- **修改文件**: `harness.rs` (~40 行), `loop_config.rs` (~15 行), `context/mod.rs` (~5 行)
- **内容**:
  - `CompactionConfig` 结构（compact_model, summary_max_tokens, keep_recent_messages, max_ptl_retries, max_files_to_restore）
  - `CompactionPipeline::compact()` 主流程：确定边界 → 预处理（图片→占位、长结果截断）→ 构建 9 段摘要 prompt → LLM 调用（带 PTL 自重试）→ 格式化（剥离 analysis 块）→ 状态重建
  - `CompactionContext` 包含 memory, memory_store, active_skill, hook_registry, user_id, sandbox_id
  - `rebuild_state()`: Zone B/B+/B++ 重注入、active skill 重注入、SessionStart hooks 重触发
  - T3 的 PTL 路径升级：优先调 CompactionPipeline，失败回退 truncate
  - `AgentLoopConfig` 新增 `compaction_pipeline: Option<CompactionPipeline>`, `compact_model: Option<String>`
  - `CompactionResult` 结构（boundary_marker, summary_messages, kept_messages, reinjections, token 统计）
- **预估**: ~490 行
- **测试**:
  - CompactionPipeline 单元测试（preprocess、format_summary、PTL 自重试）
  - 集成测试（mock provider PTL → LLM 摘要 → 消息替换 → 状态重建验证）

### Wave 4：权限引擎 + Hook 增强

#### T8 — PermissionEngine 6 层规则引擎
- **原优先级**: P1-1
- **设计文档**: `PERMISSION_AND_P1_ENHANCEMENT_DESIGN.md` P1-1
- **依赖**: 无（但 T4 的 validate_input 与其协作）
- **新增文件**: `security/permission_engine.rs` (~250 行), `security/permission_rule.rs` (~150 行), `security/permission_types.rs` (~60 行)
- **修改文件**: `security/mod.rs` (~5 行), `harness.rs` (~40 行), `loop_config.rs` (~10 行)
- **内容**:
  - `PermissionRule::parse()`: "bash(git *)" → tool_name + pattern
  - `PermissionRule::matches()`: 提取 per-tool 匹配目标 (bash→command, file_edit→file_path, ...) + glob 匹配
  - `RuleSource` 6 层 (Platform/Tenant/Project/User/Session/ToolDefault)
  - `PermissionRuleSet` (source + allow/deny/ask 规则列表)
  - `PermissionEngine::evaluate()`: deny 向下穿透 → allow/ask 首匹配 → UseToolDefault 回退
  - `PermissionDecision` (Allow/Deny/Ask/UseToolDefault) 结构化决策带来源追踪
  - `PermissionEngine::from_files()` 加载 YAML 规则文件
  - harness 集成：PermissionEngine → Deny/Allow/Ask → ApprovalManager (保留现有下游)
  - 规则跨源 merge 不去重（CC 行为，`permissions.ts`）
- **预估**: ~515 行
- **测试**: 规则解析、匹配、多层合并、deny 穿透、YAML 加载

#### T9 — Context Collapse 粒度折叠
- **原优先级**: P1-2
- **设计文档**: `PERMISSION_AND_P1_ENHANCEMENT_DESIGN.md` P1-2
- **依赖**: T6 (作为 AutoCompaction 前的轻量替代)
- **新增文件**: `context/collapse.rs` (~200 行)
- **修改文件**: `context/mod.rs`, `harness.rs` (~20 行)
- **内容**:
  - `ContextCollapser::collapse()`: 对可折叠消息打重要性分（0-100），从最低分开始折叠
  - `score_message()`: User=100, System=90, Assistant 按工具结果大小/代码/错误分级
  - `collapse_message()`: 替换为一行摘要 `[Collapsed: tool1(), tool2() → N chars output]`
  - harness 集成：DegradationLevel::AutoCompaction 前先尝试 collapse
- **预估**: ~220 行
- **测试**: 评分逻辑 + 折叠效果 + token 释放验证

#### T10 — Snip Compact 用户裁剪
- **原优先级**: P1-3
- **设计文档**: `PERMISSION_AND_P1_ENHANCEMENT_DESIGN.md` P1-3
- **依赖**: T6 (摘要能力)
- **新增到**: `context/compaction_pipeline.rs` (~55 行)
- **内容**:
  - `SNIP_MARKER = "[SNIP]"`
  - `CompactionPipeline::snip_compact()`: 从标记处截断，有 pipeline 时先摘要再截断
  - harness 集成：在 microCompact 之前检查 snip 标记（与 CC 行为一致，snip 独立于 auto/micro）
- **预估**: ~55 行
- **测试**: 标记检测 + 截断 + 摘要模式

#### T11 — Hook 输出能力增强
- **原优先级**: P2-2 + P2-3 + P2-4
- **设计文档**: `HOOK_SYSTEM_ENHANCEMENT_DESIGN.md`
- **依赖**: T8 (PermissionRule 用于 `if` 条件过滤)
- **修改文件**: `hooks/handler.rs` (~30 行), `hooks/mod.rs` (~15 行), `hooks/declarative/config.rs` (~40 行), `harness.rs` (~55 行)
- **内容**:
  - HookAction 新增 3 个变体: `ModifyInput(Value)`, `InjectContext(String)`, `PermissionOverride(PermissionHookDecision)`
  - `PermissionHookDecision` enum (Allow/Deny/Ask)
  - HookPoint 新增 `UserPromptSubmit`
  - harness 集成: PreToolUse 处理 ModifyInput/InjectContext/PermissionOverride
  - harness 集成: round 0 触发 UserPromptSubmit，支持 InjectContext
  - `pending_context_injections: Vec<String>` 在 CompletionRequest 构建前注入为 `<system-reminder>`
  - 声明式 hook 新增 `if_condition: Option<String>` 字段，复用 `PermissionRule::matches()`
  - HookHandler trait 新增 `fn is_async(&self) -> bool { false }`
  - Registry execute() 对 is_async handlers spawn fire-and-forget
- **预估**: ~170 行
- **测试**: ModifyInput/InjectContext 单元测试 + UserPromptSubmit 集成测试

### Wave 5：多 Agent 上层抽象 + 工具

#### T12 — 多 Agent LLM 工具 (session_*/task_*)
- **原优先级**: P1-6 + P1-7
- **设计文档**: `MULTI_AGENT_ORCHESTRATION_DESIGN.md` + `TOOL_SYSTEM_ENHANCEMENT_DESIGN.md`
- **依赖**: SessionRegistry (已有, Phase AJ)
- **新增文件**: `tools/session_create.rs` (~100 行), `tools/session_message.rs` (~60 行), `tools/session_status.rs` (~50 行), `tools/session_stop.rs` (~40 行), `agent/task_tracker.rs` (~150 行), `tools/task_create.rs` (~60 行), `tools/task_update.rs` (~50 行), `tools/task_list.rs` (~40 行)
- **修改文件**: `tools/mod.rs`, `agent/runtime.rs` (~20 行)
- **内容**:
  - `session_create`: 创建子会话（prompt, agent_type, model, run_in_background）+ 详细描述手册（~60 行）
  - `session_message`: 向子会话发消息 (to, message) + 描述
  - `session_status`: 查询子会话状态 + 描述
  - `session_stop`: 停止子会话 + 描述
  - `TaskTracker`: DashMap<String, TrackedTask> + atomic ID 生成
  - `TrackedTask`: id, subject, description, status (Pending/InProgress/Completed/Blocked), owner, team
  - `task_create/update/list` 工具 + 描述手册
  - AgentRuntime 新增 `task_tracker: Arc<TaskTracker>` 字段
  - 系统提示词条件注入相关指导
- **预估**: ~610 行
- **测试**: 各工具单元测试 + TaskTracker CRUD 测试

#### T13 — TeamManager
- **原优先级**: P1-8
- **设计文档**: `MULTI_AGENT_ORCHESTRATION_DESIGN.md`
- **依赖**: T12 (session_create 用于 add_member)
- **新增文件**: `agent/team.rs` (~200 行), `tools/team_create.rs` (~40 行), `tools/team_add_member.rs` (~50 行), `tools/team_dissolve.rs` (~40 行)
- **修改文件**: `agent/runtime.rs` (~10 行), `tools/mod.rs`
- **内容**:
  - `TeamManager`: DashMap<String, Team> + create/add_member/dissolve/find/list
  - `Team`: name, description, leader_session_id, members HashMap
  - `TeamMember`: name, session_id, agent_type, role (Leader/Worker)
  - 3 个 LLM 工具 + 描述手册（何时用团队/何时不用/使用模式）
  - AgentRuntime 新增 `team_manager: Arc<TeamManager>`
- **预估**: ~340 行
- **测试**: TeamManager CRUD + 工具集成

### Wave 6：自主模式 + 成本追踪

#### T14 — 自主运行模式 Phase 1
- **原优先级**: P2-7
- **设计文档**: `AUTONOMOUS_MODE_DESIGN.md`
- **依赖**: T6 (压缩后继续循环), T8 (权限控制)
- **新增文件**: `agent/autonomous.rs` (~130 行), `tools/sleep_tool.rs` (~50 行)
- **修改文件**: `harness.rs` (~80 行), `loop_config.rs` (~15 行), `events.rs` (~20 行), `system_prompt.rs` (~30 行)
- **内容**:
  - `AutonomousConfig`: enabled, idle_sleep_secs, active_sleep_secs, max_rounds, max_duration, max_tokens_per_round, max_cost_usd, trigger (Manual only in Phase 1), user_presence_aware
  - `AutonomousState`: session_id, config, status, rounds_completed, total_tokens, total_cost_usd, started_at, last_tick_at, user_online
  - `AutonomousStatus`: Running/Sleeping(u64)/Paused/BudgetExhausted/RoundsExhausted/Completed/Failed
  - harness 主循环结束后: 检查 autonomous → 预算检查 → Sleep(tokio::select! 等待 tick/用户消息/暂停信号) → 注入 `<tick>` 消息 → continue
  - `SleepTool`: seconds + reason 参数，实际等待由 harness 控制
  - 自主模式系统提示词追加（AUTONOMOUS_PROMPT ~30 行）
  - AgentEvent 新增: AutonomousSleeping/Tick/Paused/Exhausted
- **预估**: ~325 行
- **测试**: 自主循环单元测试 + 预算耗尽测试 + 用户干预测试

#### T15 — CostTracker 成本追踪
- **原优先级**: P2-1
- **设计文档**: 待本 Phase 补充（参照 CC `modelCost.ts`）
- **依赖**: Provider trait (已有)
- **新增文件**: `metering/cost_tracker.rs` (~150 行)
- **修改文件**: `metering/mod.rs`, `harness.rs` (~20 行), `events.rs` (~10 行)
- **内容**:
  - `ModelCostTable`: HashMap<String, ModelCost> 按模型名查价格
  - `ModelCost`: input_per_million, output_per_million, cache_read_per_million, cache_write_per_million
  - 内置 Anthropic/OpenAI 主流模型价格（含 fast mode 10x 价格）
  - `CostTracker`: 累计 per-model input/output/cache_read/cache_write tokens + USD 成本
  - `CostTracker::record()`: 每次 LLM 调用后记录
  - `CostTracker::summary()`: 返回 CostSummary (per_model 分拆 + total_usd)
  - `AgentEvent::CostUpdate { model, input_tokens, output_tokens, cache_read, cache_write, usd_cost }`
  - harness 集成：LLM 调用后调用 cost_tracker.record()
- **预估**: ~180 行
- **测试**: 价格计算 + 多模型累计 + unknown model fallback

### Wave 7：TUI 体验层（并行线）

#### T16 — TUI Phase 1 (纯视觉)
- **原优先级**: TUI-Ph1
- **设计文档**: `TUI_EXPERIENCE_ENHANCEMENT_DESIGN.md` Phase 1
- **依赖**: 无（与引擎改进完全并行）
- **新增文件**: `crates/octo-cli/src/tui/figures.rs` (~60 行)
- **修改文件**: TUI 渲染相关文件 (~230 行)
- **内容**:
  - E-1: Unicode 符号集（`figures.rs`，30+ 符号定义）
  - E-2: Stalled animation（超时变色 10s→黄 30s→红）
  - E-3: Spinner verbs（40 个随机动词）
  - E-5: ⎿ elbow bracket 消息指示符（可选风格）
  - E-6: Effort indicator（○◐●◉ 状态栏）
  - E-15: Thinking shimmer（正弦波颜色变化）
  - E-16: Byline middot 分隔（`Enter to submit · Esc to cancel`）
  - E-18: Reduced motion 配置
  - E-19: 上下文感知快捷键提示
  - E-20: 时间格式增强（亚秒精度）
- **预估**: ~290 行
- **测试**: 编译 + 手动 TUI 验证

#### T17 — TUI Phase 2 (功能增强)
- **原优先级**: TUI-Ph2
- **设计文档**: `TUI_EXPERIENCE_ENHANCEMENT_DESIGN.md` Phase 2
- **依赖**: T16, T8 (权限模式循环)
- **修改文件**: TUI 相关文件 (~380 行)
- **内容**:
  - E-4: Shimmer/Glimmer 效果（字符级颜色波浪 + interpolate_color）
  - E-7: Ctrl+R 历史搜索（增量过滤 prompt 历史）
  - E-9: Shift+Tab 权限模式循环（配合 PermissionEngine）
  - E-11: Meta+O 快速模式切换
  - E-12: Ctrl+X Ctrl+E 外部编辑器
  - E-13: 选区复制增强
  - E-14: 权限请求 UI 增强（风险颜色 + diff 预览 + YNAD 快捷键）
- **预估**: ~380 行
- **测试**: 编译 + 手动 TUI 验证

#### T18 — TUI Phase 3 (大功能)
- **原优先级**: TUI-Ph3
- **设计文档**: `TUI_EXPERIENCE_ENHANCEMENT_DESIGN.md` Phase 3
- **依赖**: T17, T12 (多 Session Spinner Tree)
- **修改文件**: TUI 相关文件 (~380 行)
- **内容**:
  - E-8: Vim 模式（Normal/Insert/Visual 基础切换）
  - E-10: Meta+P 模型选择器浮层
  - E-17: 多 Session Spinner Tree（树形并行进度）
- **预估**: ~380 行
- **测试**: 编译 + 手动 TUI 验证

---

## 三、Wave 执行顺序与并行策略

```
时间 →
─────────────────────────────────────────────────────────────────
Wave 1  │ T1(提示词) + T2(工具描述)                    │ 零依赖
────────┤                                              ├────────
Wave 2  │ T3(PTL) + T4(Tool trait) + T5(微压缩)        │ 可并行
────────┤                                              ├────────
Wave 3  │ T6(LLM摘要+状态重建) ← T3                    │
────────┤                                              ├────────
Wave 4  │ T8(权限) + T9(Collapse←T6) + T10(Snip←T6) + T11(Hook←T8) │
────────┤                                              ├────────
Wave 5  │ T12(Agent工具) + T13(Team←T12)               │
────────┤                                              ├────────
Wave 6  │ T14(自主模式←T6,T8) + T15(成本追踪)          │
─────────────────────────────────────────────────────────────────
TUI     │ T16 ──→ T17(←T8) ──→ T18(←T12)              │ 并行线
─────────────────────────────────────────────────────────────────
```

**每个 Wave 内的任务可用 worktree 并行实施。Wave 之间的依赖用 `←` 标注。**

---

## 四、工作量总结

| Wave | 任务 | 新增/修改代码 | 累计 |
|------|------|-------------|------|
| W1 | T1 + T2 | ~600 行 | 600 |
| W2 | T3 + T4 + T5 | ~290 行 | 890 |
| W3 | T6 | ~490 行 | 1380 |
| W4 | T8 + T9 + T10 + T11 | ~960 行 | 2340 |
| W5 | T12 + T13 | ~950 行 | 3290 |
| W6 | T14 + T15 | ~505 行 | 3795 |
| TUI | T16 + T17 + T18 | ~1050 行 | 4845 |
| **总计** | **18 任务** | **~4845 行** | |

> 注：较原始设计的 7480 行减少了约 35%。原因：
> - 多个设计文档有重叠内容（工具描述在多处计数）
> - 实际代码结构比预估更紧凑（Rust 表达力高于预估）
> - 部分 P2 项（Session Memory 持续提取、会话抄本、会话 fork/rewind、MCP OAuth）推迟到后续 Phase

---

## 五、CC-OSS 源码审计补充发现（集成到相关任务）

以下发现已集成到上述任务设计中，此处汇总记录：

| # | 发现 | 集成到 | 处理方式 |
|---|------|--------|---------|
| 1 | PTL 恢复静默 + 3 阶段 | T3 | 加静默恢复语义 |
| 2 | Stop hooks 在 PTL 时跳过 | T3 | 加 death spiral 防护 |
| 3 | 工具结果外部持久化 | — | Deferred（需 session 文件架构变更） |
| 4 | Snip 在 auto/micro 之前独立运行 | T10 | Snip 放到 microCompact 之前 |
| 5 | BashTool 369 行描述 | T2 | bash 描述升级为 ~80 行 |
| 6 | AgentTool 列表动态注入消息 | T12 | session_create 列表通过消息注入 |
| 7 | max_output_tokens 自动升级 8K→64K | — | Deferred（ContinuationTracker 增强） |
| 8 | Fallback 剥离 thinking 签名 | — | Deferred（ProviderChain 增强） |
| 9 | Cost 含 cache read/write 分拆 | T15 | ModelCost 加 4 种价格 |
| 10 | Hooks 25 种事件 | T11 | 只加 UserPromptSubmit（最关键） |
| 11 | Permission 规则不去重 | T8 | 采用相同语义 |
| 12 | Teleport 自动生成 branch | — | Deferred（按需） |

---

## Deferred（暂缓项）

> 本阶段已知但暂未实现的功能点。每次开始新 Task 前先检查此列表。

| ID | 内容 | 前置条件 | 状态 |
|----|------|---------|------|
| AP-D1 | 工具结果外部持久化（recordContentReplacement） | Session 文件架构设计 | ⏳ |
| AP-D2 | max_output_tokens 自动升级（8K→64K→recovery） | ContinuationTracker 重构 | ⏳ |
| AP-D3 | Fallback model 剥离 thinking 签名 | ProviderChain failover 重构 | ⏳ |
| AP-D4 | Teleport 自动生成 branch 名 | 按需，优先级低 | ⏳ |
| AP-D5 | Session Memory 持续提取（每轮而非 session-end） | T6 CompactionPipeline 完成 | ⏳ |
| AP-D6 | 会话抄本（sessionTranscript） | session 文件架构 | ⏳ |
| AP-D7 | 会话 fork/rewind | session 状态管理重构 | ⏳ |
| AP-D8 | MCP OAuth 支持 | MCP SDK 升级 | ⏳ |
| AP-D9 | 自主模式 Phase 2（Webhook/Cron 触发 + 暂停恢复 API） | T14 完成 | ⏳ |
| AP-D10 | 自主模式 Phase 3（用户感知 + 审计日志） | AP-D9 完成 | ⏳ |
| AP-D11 | Bash AST 命令分析增强（CC bashClassifier 风格） | T8 PermissionEngine 完成 | ⏳ |
| AP-D12 | plan_mode 工具（enter_plan_mode/exit_plan_mode） | T8 PermissionEngine 完成 | ⏳ |
| AP-D13 | tool_search 工具（大量 MCP 工具时的搜索） | 按需 | ⏳ |
| AP-D14 | ask_user 工具（结构化提问） | ApprovalGate 扩展 | ⏳ |
| AP-D15 | toolUseSummary 工具执行摘要（上下文降级注入） | T6 CompactionPipeline 完成 | ⏳ |

---

## 六、改进后预期效果

| 指标 | 当前 | Phase AP 后 |
|------|------|-----------|
| Agent Loop 能力覆盖 (vs CC) | 68% | ~95% |
| 长对话存活率 | PTL 直接死亡 | 自动 3 级恢复 (collapse → LLM 摘要 → truncate) |
| 工具描述质量 | 1 行/工具 | 15~80 行/工具使用手册 |
| 提示词行为控制 | 64 行通用 | 170+ 行精细 7 段 |
| 权限粒度 | 3 级 AutonomyLevel | 6 层规则引擎 + 通配符 + 决策追踪 |
| 多 Agent 工具 | 1 个 subagent | 10 个 (4 session + 3 task + 3 team) |
| Hook 输出能力 | 通知型 | 拦截型 (ModifyInput/InjectContext/Permission) |
| 自主运行 | 无 | Tick 循环 + Sleep + 预算控制 |
| 成本追踪 | 基础 token | per-model + cache + USD |
| TUI 精致度 | 生产就绪 | CC 同级精致度 (20 项改进) |
