# Phase AW: CC-OSS 工具体系对齐

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 补齐 Octo 缺失的 CC-OSS 工具，完成工具层面的完整对齐

**Architecture:** Wave 1 (T1-T4) 为简单工具可并行；Wave 2 (T5-T7) 为中等复杂度工具；Wave 3 (T8-T9) 为需要外部集成的工具。所有改动在 `crates/octo-engine/` 内完成。

**Tech Stack:** Rust, Tokio, serde_json, rmcp (MCP SDK)

**前置依赖:** Phase AV 已完成（a58e341）

**排除项（CC-OSS 中也是 disabled/stub）:**
- REPLTool — CC-OSS 中 `isEnabled: () => false`
- WebBrowserTool — CC-OSS 中返回空 stub
- PowerShellTool — Windows 专用，macOS/Linux 不需要
- TungstenTool / SyntheticOutputTool / OverflowTestTool — 内部测试工具
- RemoteTriggerTool — 远程 API 触发，非核心
- SuggestBackgroundPR / ReviewArtifact / MonitorTool — 辅助工具，优先级低
- WorkflowTool / DiscoverSkills / SnipTool — Octo 已有等价机制（SkillRuntime, auto_snip）
- SendUserFileTool / TerminalCaptureTool — CC-OSS 中也是空 stub

---

## Wave 1: 简单工具快速补齐（T1-T4 互不依赖，可并行）

---

### Task 1: SendUserMessage 工具（CC-OSS BriefTool 对齐）

**目标**: 实现单向消息发送工具，允许 agent 主动向用户推送消息而不要求回复

**分析**: CC-OSS 的 BriefTool 是 agent→user 单向推送，支持 markdown 格式和附件。Octo 当前的 `AskUserTool` 只能双向交互（必须等回复）。

**Files:**
- Create: `crates/octo-engine/src/tools/send_message.rs` (~80 行)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module + 注册)
- Modify: `crates/octo-engine/src/tools/interaction.rs` (添加 `Message` variant)
- Test: `crates/octo-engine/tests/tool_send_message.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_send_message.rs
use octo_engine::tools::interaction::{InteractionGate, InteractionRequest};

#[tokio::test]
async fn send_message_fires_and_forgets() {
    // SendMessageTool 应该发送消息后立即返回，不等待用户回复
    let (gate, mut rx) = InteractionGate::new_channel();
    // ... 调用 tool，验证 rx 收到 Message variant，tool 直接返回成功
}

#[tokio::test]
async fn send_message_supports_markdown() {
    // 消息内容应保留 markdown 格式
}

#[tokio::test]
async fn send_message_supports_status_field() {
    // status: "normal" | "proactive"
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/send_message.rs
pub struct SendMessageTool {
    gate: Arc<InteractionGate>,
}

// InteractionRequest 新增 Message variant:
// Message { content: String, status: MessageStatus }
// MessageStatus: Normal | Proactive

// Tool trait:
// name() -> "send_message"
// parameters: message (string, required), status (enum, optional, default "normal")
// execute: 通过 gate 发送 Message，不等待回复，立即返回 { sent: true, timestamp }
```

**Step 3: 注册**
- `mod.rs` 添加 `pub mod send_message;`
- 在 `runtime.rs` 中与 `AskUserTool` 一起注册

**验证**: `cargo test -p octo-engine tool_send_message -- --test-threads=1`

---

### Task 2: TaskGet + TaskStop + TaskOutput 工具补全

**目标**: 补齐 Octo 任务系统缺失的 3 个工具，对齐 CC-OSS TaskV2

**分析**: Octo 已有 TaskCreate/Update/List，缺少按 ID 获取单个任务、停止任务、获取任务输出。

**Files:**
- Modify: `crates/octo-engine/src/tools/task.rs` (~120 行新增)
- Test: `crates/octo-engine/tests/tool_task_extended.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_task_extended.rs

#[tokio::test]
async fn task_get_returns_single_task_by_id() {
    // 创建任务，然后按 ID 获取，验证返回完整信息
}

#[tokio::test]
async fn task_get_returns_null_for_nonexistent() {
    // 不存在的 ID 返回 null 而非错误
}

#[tokio::test]
async fn task_stop_cancels_running_task() {
    // 停止 in_progress 状态的任务，状态变为 cancelled
}

#[tokio::test]
async fn task_stop_fails_on_completed_task() {
    // 不能停止已完成的任务
}

#[tokio::test]
async fn task_output_returns_result() {
    // 获取已完成任务的输出
}
```

**Step 2: 实现**

```rust
// 在 task.rs 中新增三个工具:

// TaskGetTool: name() -> "task_get"
// parameters: task_id (string, required)
// execute: 从 ToolContext.task_store 按 ID 查询，返回完整任务信息（含 description, status, owner, team）
// is_read_only() -> true

// TaskStopTool: name() -> "task_stop"
// parameters: task_id (string, required)
// execute: 检查任务状态为 in_progress/pending，改为 cancelled，返回确认

// TaskOutputTool: name() -> "task_output"
// parameters: task_id (string, required)
// execute: 从 task_store 获取任务结果/输出字段，返回 { task_id, status, output }
// is_read_only() -> true
```

**Step 3: 注册**
- 在 `runtime.rs` 中与现有 TaskCreate/Update/List 一起注册

**验证**: `cargo test -p octo-engine tool_task_extended -- --test-threads=1`

---

### Task 3: ConfigTool — 运行时配置读写

**目标**: 实现运行时配置读写工具，对齐 CC-OSS ConfigTool

**分析**: Octo 有完整的配置系统（OctoRoot, config.yaml, 分层覆盖），但缺少工具形式的暴露。Agent 无法在运行时读取或修改配置。

**Files:**
- Create: `crates/octo-engine/src/tools/config_tool.rs` (~100 行)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_config.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_config.rs

#[tokio::test]
async fn config_get_reads_setting() {
    // 读取已知配置项 (如 "provider", "model")，返回当前值
}

#[tokio::test]
async fn config_get_unknown_returns_null() {
    // 未知 key 返回 null
}

#[tokio::test]
async fn config_set_updates_value() {
    // 设置配置项，验证生效
}

#[tokio::test]
async fn config_set_rejects_readonly_settings() {
    // 某些配置项 (如 "db_path") 是只读的，拒绝修改
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/config_tool.rs
pub struct ConfigTool {
    // 需要访问 AgentRuntimeConfig 或类似共享配置
}

// name() -> "config"
// parameters:
//   setting (string, required): 配置键 (如 "provider", "model", "autonomy_level")
//   value (string|null, optional): 新值。省略则为 GET 操作
//
// GET: 返回 { setting, value, source } (source: "default"/"config"/"env"/"cli")
// SET: 验证 → 更新 → 返回 { setting, old_value, new_value }
//
// risk_level: GET → ReadOnly, SET → HighRisk (需要确认)
// 只读配置列表: ["db_path", "host", "port"] — 运行时不可修改
```

**验证**: `cargo test -p octo-engine tool_config -- --test-threads=1`

---

### Task 4: TodoWrite 工具 — 轻量级待办列表

**目标**: 实现轻量级待办列表工具，对齐 CC-OSS TodoWriteTool

**分析**: CC-OSS 的 TodoWrite 是比 Task 更轻量的待办管理，agent 用它追踪当前工作项。Octo 有 Task 系统但缺这种轻量替代。TodoWrite 是全量替换语义：每次调用传入完整列表。

**Files:**
- Create: `crates/octo-engine/src/tools/todo.rs` (~90 行)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_todo.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_todo.rs

#[tokio::test]
async fn todo_write_stores_list() {
    // 写入 3 个 todo，验证存储成功
}

#[tokio::test]
async fn todo_write_replaces_entire_list() {
    // 第二次调用完全替换第一次的列表
}

#[tokio::test]
async fn todo_all_completed_clears_list() {
    // 所有 todo 标记 completed 后，列表自动清空
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/todo.rs
pub struct TodoWriteTool {
    // 内部 Arc<Mutex<Vec<TodoItem>>> 或存储在 ToolContext
}

// TodoItem { id: String, content: String, status: TodoStatus }
// TodoStatus: Pending | Completed

// name() -> "todo_write"
// parameters: todos (array of { content, status, id? }, required)
// execute:
//   1. 保存 old_todos
//   2. 如果所有 status == completed → 清空
//   3. 否则存储新列表
//   4. 返回 { old_todos, new_todos }
```

**验证**: `cargo test -p octo-engine tool_todo -- --test-threads=1`

---

## Wave 2: 中等复杂度工具（T5-T7 有轻度依赖）

---

### Task 5: MCP Resource 工具 — ListResources + ReadResource

**目标**: 暴露 McpManager 已有的 resource 能力为 agent 可用工具

**分析**: `McpManager` 已实现 `list_resources()` 和 `read_resource()`，只需创建工具包装层。

**Files:**
- Create: `crates/octo-engine/src/tools/mcp_resource.rs` (~120 行)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_mcp_resource.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_mcp_resource.rs

#[tokio::test]
async fn list_resources_returns_all_servers() {
    // 无 server 参数时返回所有连接服务器的资源
}

#[tokio::test]
async fn list_resources_filters_by_server() {
    // 指定 server 参数时只返回该服务器的资源
}

#[tokio::test]
async fn read_resource_returns_content() {
    // 读取指定 URI 的资源内容
}

#[tokio::test]
async fn read_resource_handles_binary() {
    // 二进制资源应保存到临时文件，返回文件路径
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/mcp_resource.rs

// McpListResourcesTool:
// name() -> "mcp_list_resources"
// parameters: server (string, optional)
// execute: 调用 McpManager::list_resources()，聚合返回
// is_read_only() -> true, is_concurrency_safe() -> true

// McpReadResourceTool:
// name() -> "mcp_read_resource"
// parameters: server (string, required), uri (string, required)
// execute: 调用 McpManager::read_resource()
//   - text 内容直接返回
//   - binary (base64) 内容写入临时文件，返回文件路径
// is_read_only() -> true
```

**验证**: `cargo test -p octo-engine tool_mcp_resource -- --test-threads=1`

---

### Task 6: Worktree 工具 — EnterWorktree + ExitWorktree

**目标**: 实现 Git worktree 隔离开发工具，对齐 CC-OSS

**分析**: CC-OSS 的 worktree 工具创建独立 git worktree，切换 CWD，在隔离环境中开发后可保留或删除。Octo 的 SecurityPolicy 已有 `working_dir` 概念可复用。

**Files:**
- Create: `crates/octo-engine/src/tools/worktree.rs` (~180 行)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_worktree.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_worktree.rs

#[tokio::test]
async fn enter_worktree_creates_and_switches() {
    // 在 git repo 中创建 worktree，验证新目录存在
}

#[tokio::test]
async fn enter_worktree_rejects_nested() {
    // 已在 worktree 中时拒绝再次 enter
}

#[tokio::test]
async fn exit_worktree_keep_preserves_branch() {
    // action=keep 保留 worktree 和分支
}

#[tokio::test]
async fn exit_worktree_remove_with_uncommitted_requires_discard() {
    // 有未提交更改时，action=remove 必须 discard_changes=true
}

#[tokio::test]
async fn exit_worktree_remove_cleans_up() {
    // action=remove + discard_changes=true 删除 worktree
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/worktree.rs

// EnterWorktreeTool:
// name() -> "enter_worktree"
// parameters: name (string, optional, auto-generate if omitted)
// execute:
//   1. 验证不在 worktree 中（检查 state）
//   2. git worktree add ../octo-worktree-{name} -b worktree/{name}
//   3. 更新 ToolContext.working_dir 为新路径
//   4. 保存原始 CWD 到 state
//   5. 返回 { worktree_path, worktree_branch }
// risk_level: HighRisk

// ExitWorktreeTool:
// name() -> "exit_worktree"
// parameters:
//   action (enum "keep"|"remove", required)
//   discard_changes (bool, optional)
// execute:
//   1. 验证在 worktree 中
//   2. 如果 action=remove：检查 git status/rev-list
//   3. 如果有未提交内容且 !discard_changes → 拒绝
//   4. 执行 keep/remove 操作
//   5. 恢复 ToolContext.working_dir 为原始 CWD
// risk_level: HighRisk
```

**验证**: `cargo test -p octo-engine tool_worktree -- --test-threads=1`

---

### Task 7: LSP 工具 — 代码智能

**目标**: 实现 LSP 集成工具，支持 go-to-definition、find-references、hover 等代码智能操作

**分析**: CC-OSS 支持 9 种 LSP 操作。Octo 需要一个轻量 LSP 客户端。初始实现支持核心 4 种操作，其余 5 种作为 Deferred。

**Files:**
- Create: `crates/octo-engine/src/tools/lsp.rs` (~200 行)
- Create: `crates/octo-engine/src/lsp/` 目录 (LSP 客户端基础设施)
  - `mod.rs` — LspManager trait
  - `client.rs` — LspClient (基于 stdio transport)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_lsp.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_lsp.rs

#[tokio::test]
async fn lsp_go_to_definition_returns_location() {
    // 给定文件+位置，返回定义位置
}

#[tokio::test]
async fn lsp_find_references_returns_locations() {
    // 给定符号位置，返回所有引用
}

#[tokio::test]
async fn lsp_hover_returns_type_info() {
    // 给定位置，返回类型和文档信息
}

#[tokio::test]
async fn lsp_document_symbols_returns_outline() {
    // 返回文件的符号列表（函数、类、变量等）
}

#[tokio::test]
async fn lsp_rejects_large_files() {
    // >10MB 文件被拒绝
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/lsp.rs
pub struct LspTool {
    manager: Arc<dyn LspManager>,
}

// name() -> "lsp"
// parameters:
//   operation (enum: "go_to_definition"|"find_references"|"hover"|"document_symbols")
//   file_path (string, required)
//   line (number, 1-based, required for definition/references/hover)
//   character (number, 1-based, required for definition/references/hover)
//
// execute: 转换为 LSP 协议调用，格式化结果
// is_read_only() -> true, is_concurrency_safe() -> true
//
// LspManager trait:
//   async fn initialize(root: &Path) -> Result<()>
//   async fn go_to_definition(file, pos) -> Result<Vec<Location>>
//   async fn find_references(file, pos) -> Result<Vec<Location>>
//   async fn hover(file, pos) -> Result<Option<HoverInfo>>
//   async fn document_symbols(file) -> Result<Vec<DocumentSymbol>>
```

**Deferred:**
- AW-D1: LSP 额外操作 (goToImplementation, callHierarchy, workspaceSymbol) — 等核心操作稳定后
- AW-D2: LSP 服务器自动发现与启动 — 初始版本需手动配置
- AW-D3: LSP 文件同步 (didOpen/didChange) — 初始版本每次重新打开

**验证**: `cargo test -p octo-engine tool_lsp -- --test-threads=1`

---

## Wave 3: MCP 认证集成（T8-T9 需要 MCP 子系统支持）

---

### Task 8: McpAuth 工具 — MCP OAuth 认证流程

**目标**: 实现 MCP 服务器 OAuth 认证工具，对齐 CC-OSS McpAuthTool

**分析**: CC-OSS 的 McpAuthTool 是动态生成的伪工具——当 MCP 服务器返回 401 时自动创建，完成 OAuth 后自动替换为真实工具。Octo 的 McpManager 需要扩展认证流程。

**Files:**
- Create: `crates/octo-engine/src/tools/mcp_auth.rs` (~120 行)
- Modify: `crates/octo-engine/src/mcp/client.rs` (添加 OAuth 流程支持)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_mcp_auth.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_mcp_auth.rs

#[tokio::test]
async fn mcp_auth_returns_auth_url_for_http_server() {
    // HTTP MCP 服务器返回 OAuth URL
}

#[tokio::test]
async fn mcp_auth_returns_unsupported_for_stdio() {
    // stdio 传输不支持 OAuth
}

#[tokio::test]
async fn mcp_auth_replaces_self_after_success() {
    // OAuth 完成后，auth 工具自动移除，真实工具注入
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/mcp_auth.rs
pub struct McpAuthTool {
    server_name: String,
    transport_type: TransportType,
    // OAuth 状态
}

// name() -> "mcp_auth_{server_name}" (动态名称)
// parameters: 无 (调用即触发认证)
// execute:
//   1. 检查传输类型 (HTTP → OAuth, stdio → unsupported)
//   2. 启动 OAuth 流程，获取 auth_url
//   3. 返回 { status: "auth_url", auth_url, message }
//   4. 后台：OAuth 完成后 reconnect + 替换工具
```

**验证**: `cargo test -p octo-engine tool_mcp_auth -- --test-threads=1`

---

### Task 9: MCP Prompt 工具 — ListPrompts + GetPrompt

**目标**: 暴露 McpManager 的 prompt 能力为 agent 可用工具

**分析**: MCP 协议的 Prompts 功能允许服务器提供可复用的 prompt 模板。McpManager 已有基础方法。

**Files:**
- Create: `crates/octo-engine/src/tools/mcp_prompt.rs` (~100 行)
- Modify: `crates/octo-engine/src/tools/mod.rs` (添加 module)
- Test: `crates/octo-engine/tests/tool_mcp_prompt.rs`

**Step 1: 写失败测试**

```rust
// crates/octo-engine/tests/tool_mcp_prompt.rs

#[tokio::test]
async fn list_prompts_returns_available() {
    // 列出所有 MCP 服务器提供的 prompts
}

#[tokio::test]
async fn get_prompt_returns_messages() {
    // 获取指定 prompt，传入参数，返回渲染后的消息
}
```

**Step 2: 实现**

```rust
// crates/octo-engine/src/tools/mcp_prompt.rs

// McpListPromptsTool:
// name() -> "mcp_list_prompts"
// parameters: server (string, optional)
// is_read_only() -> true

// McpGetPromptTool:
// name() -> "mcp_get_prompt"
// parameters: server (string, required), name (string, required), arguments (object, optional)
// execute: 调用 McpManager::get_prompt()，返回渲染后的消息列表
// is_read_only() -> true
```

**验证**: `cargo test -p octo-engine tool_mcp_prompt -- --test-threads=1`

---

## Deferred Items

| ID | 描述 | 条件 |
|----|------|------|
| AW-D1 | LSP 额外操作 (goToImplementation, callHierarchy, workspaceSymbol) | T7 核心操作稳定后 |
| AW-D2 | LSP 服务器自动发现与启动（按文件类型） | T7 手动配置验证后 |
| AW-D3 | LSP 文件同步 (didOpen/didChange/didClose) | T7 基础功能验证后 |
| AW-D4 | MCP OAuth 后台完成回调 + 工具自动替换 | T8 基础流程验证后 |
| AW-D5 | ConfigTool SET 操作的配置持久化 | T3 GET 验证后 |

---

## 实施顺序与时间估计

| Wave | Tasks | 依赖 | 预估新增代码 | 预估测试数 |
|------|-------|------|-------------|-----------|
| Wave 1 | T1-T4 (并行) | 无 | ~370 行 | ~16 |
| Wave 2 | T5-T7 (轻度依赖) | Wave 1 不阻塞 | ~500 行 | ~14 |
| Wave 3 | T8-T9 (MCP 依赖) | T5 的 MCP resource 验证 | ~220 行 | ~6 |
| **合计** | **9 Tasks** | | **~1090 行** | **~36** |

---

## 验证清单

- [ ] `cargo check --workspace` — 编译通过
- [ ] 每个 Task 的单元测试通过
- [ ] `cargo test --workspace -- --test-threads=1` — 全量测试无回归
- [ ] 新工具在 `default_tools()` 或 `runtime.rs` 中正确注册

---

## CC-OSS vs Octo 工具对齐状态（Phase AW 完成后）

完成 Phase AW 后，Octo 将覆盖 CC-OSS 所有有效工具：

| 类别 | CC-OSS 工具 | Octo 对应 | 状态 |
|------|------------|----------|------|
| 文件 | Read/Write/Edit/Glob/Grep | file_read/write/edit/glob/grep | ✅ AV 前 |
| 执行 | Bash | bash (+BashGuard) | ✅ AV |
| 笔记本 | NotebookEdit | notebook_edit | ✅ AV 前 |
| Web | WebSearch/WebFetch | web_search/web_fetch | ✅ AV 前 |
| 用户 | AskUserQuestion | ask_user | ✅ AV 前 |
| 用户 | SendUserMessage | **send_message** | 🔵 T1 |
| 子代理 | Agent | spawn_subagent/query_subagent | ✅ AV 前 |
| 规划 | EnterPlanMode/ExitPlanMode | enter_plan_mode/exit_plan_mode | ✅ AV 前 |
| 任务 | TaskCreate/Update/List | task_create/update/list | ✅ AV 前 |
| 任务 | TaskGet/Stop/Output | **task_get/stop/output** | 🔵 T2 |
| 配置 | Config | **config** | 🔵 T3 |
| 待办 | TodoWrite | **todo_write** | 🔵 T4 |
| MCP | MCPTool | McpToolBridge | ✅ AV 前 |
| MCP | ListMcpResources/ReadMcpResource | **mcp_list_resources/read_resource** | 🔵 T5 |
| MCP | McpAuth | **mcp_auth** | 🔵 T8 |
| MCP | (Prompts) | **mcp_list_prompts/get_prompt** | 🔵 T9 |
| Git | EnterWorktree/ExitWorktree | **enter_worktree/exit_worktree** | 🔵 T6 |
| 代码 | LSP | **lsp** | 🔵 T7 |
| 搜索 | ToolSearch | tool_search | ✅ AV 前 |
| 技能 | Skill | SkillRuntime | ✅ AV 前 |
| 定时 | CronCreate/Delete/List | schedule_task | ✅ AV 前 |
| 团队 | TeamCreate/Delete | team_create/dissolve/add_member | ✅ AV 前 |
| 消息 | SendMessage | session_message | ✅ AV 前 |
| 睡眠 | Sleep | sleep | ✅ AV 前 |
