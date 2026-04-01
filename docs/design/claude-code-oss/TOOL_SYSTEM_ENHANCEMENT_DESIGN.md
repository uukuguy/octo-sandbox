# Octo-Engine 工具体系增强设计

> 基于 Claude Code OSS 50+ 工具与 Octo 30 工具的代码级对比分析。
> 日期：2026-04-01
> 核心发现：CC 的工具系统不只是代码，而是**工具代码 + 工具描述手册 + 系统提示词指导**三层紧密耦合设计。

---

## 一、关键架构发现：工具-提示词三层耦合

CC 的每个重要工具都有 3 层配合，缺一不可：

| 层面 | 内容 | 代码位置 | Octo 现状 |
|------|------|---------|----------|
| **系统提示词指导** | 何时用/不用此工具、与其他工具的选择关系 | `prompts.ts` Using Tools 段 + Session Guidance 段 | **缺失** |
| **工具描述手册** | 详细使用手册（10~288 行/工具） | 各工具 `prompt.ts` | **极简**（1 行描述） |
| **条件注入** | 工具不可用时指导消失、列表变化走消息不破缓存 | `getSessionSpecificGuidanceSection()` | **缺失** |

### CC 示例：AgentTool 的三层配合

**层面 1 - 系统提示词** (`prompts.ts`):
> "Use the Agent tool with specialized agents when the task matches the agent's description. Subagents are valuable for parallelizing independent queries or protecting context. Don't duplicate work that subagents are doing."
> "For broader codebase exploration, use Agent with subagent_type=Explore."

**层面 2 - 工具描述** (`AgentTool/prompt.ts`, 288 行!):
- 何时 fork vs 何时用 subagent 的决策矩阵
- "如何写 prompt"最佳实践（"像对刚进门的同事说明"）
- 5 个详细示例（research、audit、review...）
- 反模式警告（"不要偷看 fork 输出"、"不要伪造结果"）

**层面 3 - 条件注入** (`getSessionSpecificGuidanceSection()`):
- 只有当 AgentTool 可用时才注入 agent 使用指导
- agent 类型列表通过消息 attachment 注入（不在工具描述中），避免列表变化破坏 prompt cache

### Octo 现状

Octo 的工具描述是一句话：
```rust
fn description(&self) -> &str {
    "Execute a shell command in the sandbox"
    "Read a file from the filesystem"
}
```

系统提示词中没有任何工具使用指导。LLM 只能靠 tool schema 的 description 字段猜测如何使用。

---

## 二、工具数量对比

| 类别 | CC-OSS | Octo | 差距 |
|------|--------|------|------|
| 文件操作 | 3 (Read/Write/Edit) | 3 (read/write/edit) | 持平 |
| 搜索 | 2 (Glob/Grep) | 3 (glob/grep/find) | **Octo 领先** |
| Shell | 3 (Bash/PowerShell/REPL) | 1 (bash) | 低优先差距 |
| Web | 2 (Fetch/Search) | 2 (fetch/search) | 持平 |
| Memory | 0 (auto-memory 非工具) | **9** (store/search/recall/edit/update/forget/timeline/compress/knowledge_graph) | **Octo 大幅领先** |
| MCP | 3 (MCPTool/ListRes/ReadRes) | 1 (mcp_manage) | CC 多但 Octo 通过 bridge 覆盖 |
| **Task/Agent** | **10** (6 Task + Agent + SendMsg + 2 Team) | **1** (subagent) | **最大差距** |
| Planning | 3 (EnterPlan/ExitPlan/Verify) | 0 | CC 领先 |
| Worktree | 2 (Enter/Exit) | 0 | CC 领先 |
| Notebook | 1 (NotebookEdit) | 0 | 按需 |
| LSP | 1 (9 操作) | 0 | 按需 |
| Misc | 9 (Skill/Config/Sleep/Cron/...) | 1 (scheduler) | CC 多但非核心 |

---

## 三、不需要新增的工具

| CC 工具 | 为什么不需要 |
|---------|------------|
| PowerShellTool | Octo bash 工具可执行 PowerShell |
| REPLTool | 可通过 bash 实现 |
| MCPTool/ListMcpResources/ReadMcpResource | Octo 通过 McpToolBridge 自动暴露 MCP 工具 |
| BriefTool | CC proactive mode 专用，Octo 用 AgentEvent 推送 |
| SyntheticOutputTool/ReviewArtifactTool | 测试专用 |
| TungstenTool/RemoteTriggerTool/McpAuthTool | CC 内部/特定功能 |

---

## 四、建议新增的工具

### P1 优先级：多 Agent 协调（最大差距）

Octo 的 multi-session 架构比 CC 先进（内存 broadcast channel vs 文件邮箱），但**缺少暴露给 LLM 的工具接口**。不是照搬 CC 的 Team/SendMessage，而是为 Octo 现有架构设计对应工具：

#### session_create（对应 CC AgentTool）

```rust
// 工具定义
name: "session_create"
description: // 见下方详细描述

// 参数
{
    "prompt": "string (required) — 子会话的任务指令",
    "agent_type": "string (optional) — 使用的 agent 类型",
    "model": "string (optional) — 模型覆盖",
    "run_in_background": "boolean (optional) — 后台执行"
}
```

**工具描述手册**（不是一句话！）：
```text
创建新的 agent 子会话来处理独立任务。

## 何时使用
- 需要并行处理多个独立子任务时
- 研究性任务需要大量搜索/读取，会填满主上下文时
- 需要不同工具权限或模型的专门任务时

## 何时不使用
- 简单的单步操作（直接用工具即可）
- 需要主会话上下文的任务（子会话没有对话历史）
- 2-3 个文件内的搜索（直接用 grep/glob）

## 写 prompt 的要点
子会话从零上下文开始。像对一个刚进门的聪明同事说明：
- 说明你要实现什么以及为什么
- 描述你已经知道的和已经排除的
- 给出足够的背景让子会话能做判断，而非机械执行
- 如果需要简短回复，明确说明

不要把综合判断推给子会话。"根据你的发现修复 bug"是坏 prompt——你应该在 prompt 中写明具体要改什么文件的什么内容。

## 示例
创建研究任务：
  session_create(prompt: "调查 crates/octo-engine/src/context/ 目录下所有与 token 计数相关的代码。列出每个函数的位置、参数和返回值。200 字以内。")

创建实现任务：
  session_create(prompt: "在 crates/octo-engine/src/context/compaction_pipeline.rs 中实现 format_summary 函数。需求：剥离 <analysis> 标签，提取 <summary> 内容，添加会话延续提示。参照 compact_prompt.rs 中的 COMPACT_PROMPT 结构。", run_in_background: true)
```

#### session_message（对应 CC SendMessageTool）

```rust
name: "session_message"
description: // 见下方

// 参数
{
    "to": "string (required) — 目标会话 ID 或名称",
    "message": "string (required) — 发送的消息"
}
```

**工具描述手册**：
```text
向指定的子会话发送消息。

你的纯文本输出对其他会话不可见——要与子会话通信，必须使用此工具。

## 使用方式
- 用会话名称或 ID 作为 to 字段
- 消息内容是纯文本

## 注意
- 子会话收到消息后会在下一轮处理
- 不要在子会话返回结果前猜测其输出
```

#### session_status / session_stop

简单工具，描述较短即可。

**系统提示词配合**（`system_prompt.rs` Using Tools 段新增）：
```text
- 对于复杂多步任务，使用 session_create 创建子会话并行处理。简单任务直接执行，不需要创建会话。
- 子会话结果会自动通知你。不要轮询检查子会话状态——等待通知即可。
- 如果需要与已创建的子会话通信，使用 session_message。
```

**预估代码量**：4 个工具代码 ~300 行 + 工具描述 ~200 行 + 系统提示词 ~20 行 = **~520 行**

### P1 优先级：任务管理

#### task_create / task_update / task_list

```rust
name: "task_create"
// 参数：subject, description
// 描述：结构化任务跟踪

name: "task_update"
// 参数：task_id, status (pending/in_progress/completed)
// 描述：更新任务状态

name: "task_list"
// 无参数
// 描述：列出所有任务
```

**工具描述手册**（参照 CC TaskCreateTool）：
```text
创建结构化任务来跟踪当前工作进度。

## 何时使用
- 复杂多步任务（3 步以上）
- 用户提供了多个需求
- 进入 plan mode 后
- 收到新指令后立即创建任务

## 使用要点
- 创建后立刻标记 in_progress
- 完成后标记 completed 并检查是否有后续任务
- 任务描述要足够详细，方便回顾
```

**系统提示词配合**：
```text
- 使用 task_create 管理复杂工作的进度。创建任务后立即标记为 in_progress，完成后标记为 completed。
```

**预估代码量**：3 个工具代码 ~200 行 + 描述 ~80 行 + 提示词 ~10 行 = **~290 行**

### P2 优先级：计划模式

#### enter_plan_mode / exit_plan_mode

与 P1 PermissionEngine 配合：

```rust
name: "enter_plan_mode"
// 无参数
// 切换到只读模式

name: "exit_plan_mode"
// 参数：plan (可选，计划文本)
// 退出只读模式，可选提交计划供审批
```

**系统提示词配合**：
```text
- 对于复杂任务，先使用 enter_plan_mode 进入只读探索模式，制定计划后用 exit_plan_mode 提交计划并开始执行。
```

**预估代码量**：~100 行 + 描述 ~40 行 + 提示词 ~10 行 = **~150 行**

### P2 优先级：工具搜索

#### tool_search

```rust
name: "tool_search"
// 参数：query (关键词)
// 搜索已注册工具（含 MCP 工具）
```

当工具列表很长时（大量 MCP 工具），可以先展示工具名称，用户或 LLM 搜索后再加载完整 schema。

**预估代码量**：~100 行

### P2 优先级：用户交互

#### ask_user

```rust
name: "ask_user"
// 参数：question, options (可选)
// 向用户提问并等待回答
```

配合 harness 的 ApprovalGate 机制投递问题到前端。

**预估代码量**：~80 行

---

## 五、工具描述增强（现有工具）

现有 30 个工具的描述应从一句话升级为使用手册。优先处理最常用的工具：

| 工具 | 当前描述 | 建议增强 |
|------|---------|---------|
| **bash** | "Execute a shell command" | 加 timeout/working_dir 说明，加"优先用专用工具"提醒，加危险命令警告 |
| **file_read** | "Read a file" | 加支持格式（二进制/PDF/图片），加行号范围参数说明 |
| **file_edit** | "Edit a file" | 加"必须先读再编辑"，加 old_string 唯一性要求 |
| **file_write** | "Write a file" | 加"优先编辑现有文件"，加"不要创建不必要的文件" |
| **grep** | "Search file contents" | 加正则语法说明，加 output_mode 参数说明 |
| **glob** | "Find files by pattern" | 加 glob 语法说明 |
| **web_search** | "Search the web" | 加查询优化建议 |
| **web_fetch** | "Fetch a URL" | 加内容截断说明 |
| **subagent** | "Execute a subagent" | **大幅增强**（参照 CC AgentTool 288 行描述） |

**预估工作量**：~500 行描述文本改写（无代码逻辑改动，只改 `description()` 返回值）

---

## 六、系统提示词中的工具使用指导

应在 `PROMPT_SYSTEM_ENHANCEMENT_DESIGN.md` 设计的 "Using Tools" 段中，按条件注入工具指导：

```rust
fn build_using_tools_section(available_tools: &[String]) -> String {
    let mut section = String::from("## Using Your Tools\n\n");

    // 基础指导（始终包含）
    section.push_str("Do NOT use bash when a dedicated tool is available:\n");
    section.push_str("- file_read instead of cat/head/tail\n");
    section.push_str("- file_edit instead of sed/awk\n");
    // ...

    // 条件注入（只有工具可用时才包含）
    if available_tools.contains(&"session_create".to_string()) {
        section.push_str("\n- For complex multi-step tasks, use session_create...\n");
    }
    if available_tools.contains(&"task_create".to_string()) {
        section.push_str("\n- Use task_create to manage work progress...\n");
    }
    if available_tools.contains(&"enter_plan_mode".to_string()) {
        section.push_str("\n- For complex tasks, use enter_plan_mode first...\n");
    }

    section
}
```

**预估代码量**：~50 行

---

## 七、实施分组

| 编号 | 内容 | 优先级 | 代码量 |
|------|------|--------|--------|
| T-G1 | session_create/message/status/stop (4 工具 + 描述 + 提示词) | P1 | ~520 行 |
| T-G2 | task_create/update/list (3 工具 + 描述 + 提示词) | P1 | ~290 行 |
| T-G3 | 现有工具描述增强（9 个核心工具） | P1 | ~500 行 |
| T-G4 | enter_plan_mode/exit_plan_mode | P2 | ~150 行 |
| T-G5 | tool_search + ask_user | P2 | ~180 行 |
| T-G6 | 条件注入的 Using Tools 段 | P1 | ~50 行 |
| **合计** | | | **~1690 行** |

其中 T-G3 (~500 行) 是纯文本改写，不涉及代码逻辑。

### 推荐顺序

```
T-G3 (现有工具描述增强) ─┐
T-G6 (Using Tools 提示词) ┤── 优先做，改动最小但效果立竿见影
                          │
T-G1 (Agent 协调 4 工具) ─┤── P1 核心
T-G2 (Task 管理 3 工具) ──┘
                          │
T-G4 (Plan Mode) ─────────┤── P2
T-G5 (Search + Ask) ──────┘
```

---

## 八、与其他设计的关联

| 关联设计 | 影响 |
|---------|------|
| P0 上下文管理 | CompactionPipeline 压缩后需要重注入工具描述 |
| P1 PermissionEngine | enter_plan_mode/exit_plan_mode 与权限模式切换配合 |
| P0/P1 提示词增强 | Using Tools 段 + 条件注入机制 |
| P2 Hook 增强 | UserPromptSubmit hook 可触发工具发现/推荐 |
| 现有 multi-session | session_* 工具是 SessionRegistry API 的 LLM 包装 |

---

## 九、核心结论

> **CC 工具系统的真正优势不在于工具数量（50 vs 30），而在于工具-提示词三层耦合设计**。每个重要工具都有 10~288 行的使用手册（而非一句话描述）+ 系统提示词中的条件注入指导 + 反模式警告和使用示例。
>
> Octo 需要的不是简单补齐工具数量，而是：
> 1. **增强现有 30 个工具的描述**（从一句话升级为使用手册）—— 效果立竿见影
> 2. **新增 7 个 agent 协调和任务管理工具**（填补最大功能差距）
> 3. **在系统提示词中加入条件工具指导**（引导 LLM 正确选择工具）
