# Phase AX 剩余 Deferred Items 深入分析

> 2026-04-03 讨论记录，供后续实施参考

## 总览

| ID | 核心缺失 | 最小改动 | 依赖链 | 优先级 |
|----|---------|---------|--------|--------|
| AX-D3 | HookHandler +agent_scope | ~60 行 | 无依赖 | 高 |
| AX-D1 | ApprovalManager per-agent 实例 | ~80 行 | 无依赖 | 高 |
| AX-D2 | McpManager scoped server lifecycle | ~120 行 | server_owners 已有 | 中 |
| AX-D7 | CompletionRequest + Provider thinking | ~100 行(P1) | thinking persistence 后续 | 中 |

---

## AX-D3: per-agent hooks (~60 行)

### 方案

在 `HookRegistry.execute()` 中统一过滤，而非每个 handler 内部检查。

### 改动点

1. **HookHandler trait** 新增默认方法：
   ```rust
   fn agent_scope(&self) -> Option<&str> { None }
   ```

2. **HookRegistry.execute()** 过滤逻辑：
   - handler.agent_scope() 是 Some(scope) 且 context.agent_id 不匹配 → 跳过
   - None = 全局生效

3. **HookEntry** 新增 `agent_scope: Option<String>` 字段（YAML 配置）

4. **DeclarativeHookBridge** 从 HookEntry 读取 agent_scope

### YAML 示例

```yaml
PreToolUse:
  - matcher: "bash"
    agent_scope: "coder"
    actions:
      - type: command
        command: "cargo fmt --check"
```

### 设计决策

- **不做 cleanup** — handler 一直在，scope 不匹配时跳过即可
- **精确匹配** — 不支持通配符，后续按需加 glob
- **多 scope** — 用 `"coder|reviewer"` regex 语法
- **不继承** — 子 agent 有自己的 manifest，scope 从 manifest 决定

---

## AX-D1: per-agent permissionMode (~80 行)

### 方案

per-agent 创建新的 ApprovalManager 实例（轻量，只是策略配置）。

### PermissionMode 枚举

```rust
pub enum PermissionMode {
    Default,            // 继承父 agent
    AcceptEdits,        // auto-approve file_edit/file_write/notebook_edit
    BypassPermissions,  // auto-approve 全部
    Plan,               // 只读（复用 disallowed_tools）
    Auto,               // 同 BypassPermissions
}
```

### 改动点

1. `AgentManifest` 新增 `permission_mode: Option<PermissionMode>`
2. `SpawnSubAgentTool.execute()` 按 mode 创建新 ApprovalManager：
   - BypassPermissions/Auto → `ApprovalPolicy::AlwaysApprove`
   - AcceptEdits → `SmartApprove` + auto_approve `file_edit/file_write/notebook_edit`
   - Plan → 继承（Plan 已通过 disallowed_tools 实现）
   - Default/None → 继承父 agent
3. **后台 agent** 无 permission_mode 且 background=true → 自动 AlwaysApprove（无交互通道）

### 设计决策

- 不改 PermissionEngine 的 6 层规则链，只改 ApprovalManager 层
- Plan mode 复用 disallowed_tools（已有），不额外用 PermissionEngine deny
- AcceptEdits 只覆盖编辑类工具，bash 仍需审批

---

## AX-D2: per-agent MCP servers (~120 行)

### 方案

利用已有的 `server_owners` + `cleanup_session()` 基础设施（AJ-T4）。

### 改动点

1. **AgentManifest** 新增：
   ```rust
   pub mcp_servers: Vec<McpAgentServerConfig>,
   ```
   ```rust
   pub struct McpAgentServerConfig {
       pub name: String,
       pub command: String,
       pub args: Vec<String>,
       pub env: HashMap<String, String>,
   }
   ```

2. **McpManager** 新增 `add_server_for_agent()` 方法（复用 add_server_v2 逻辑）

3. **SpawnSubAgentTool** 新增 `mcp_manager` 字段：
   - execute() 前：连接 manifest 指定的 MCP servers
   - bridge_tools() 注册到子 agent 的 ToolRegistry
   - tokio::spawn 结束后：cleanup_session()

### 关键设计

- 子 agent ToolRegistry = 父 snapshot + scoped MCP tools
- 命名隔离：scoped server 名字加 `{agent_id}/` 前缀避免冲突
- 连接失败：warn + 继续（不阻塞 agent 启动）
- Cleanup 在 tokio::spawn 末尾执行

### YAML 示例

```yaml
name: data-analyst
mcp_servers:
  - name: postgres
    command: mcp-postgres
    args: ["--db=analytics"]
```

---

## AX-D7: Agent 级 effort 控制 (~100 行 Phase 1)

### 方案

分三个 Phase 实现，Phase 1 可独立做。

### Phase 1: 核心映射 (~100 行)

1. **CompletionRequest** +2 字段：
   ```rust
   pub thinking_budget: Option<u32>,
   pub reasoning_effort: Option<String>,
   ```

2. **AnthropicProvider** ApiRequest 加：
   ```rust
   thinking: Option<ThinkingConfig>,  // { type: "enabled", budget_tokens: N }
   ```
   - thinking_budget > 0 时启用，自动 temperature=1，加 beta header

3. **OpenAIProvider** `create_thinking_config` 改为可配置（当前硬编码 "high"）

4. **AgentManifest** 新增 `effort: Option<String>`（"low"/"medium"/"high"）

5. **Harness** 映射：
   - low → reasoning_effort: Some("low"), thinking_budget: None
   - medium → 默认（不传）
   - high → reasoning_effort: Some("high"), thinking_budget: Some(10000)

### Phase 2: thinking persistence (后续)

- ContentBlock 新增 `Thinking { text }` variant
- Message history 中保留 thinking blocks（Anthropic API 要求回传）

### Phase 3: cost tracking (后续)

- TokenUsage 扩展 thinking token 字段
- CostTracker 区分 thinking 计费

### 设计决策

- effort 只控制 thinking/reasoning，model 由 manifest.model 独立控制（正交）
- Anthropic thinking 模式强制 temperature=1
- Phase 1 可独立部署，不需要 Phase 2-3
