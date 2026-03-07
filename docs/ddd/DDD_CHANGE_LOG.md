# DDD 变更追踪日志

> 由 RuFlo post-task hook 自动生成。
> 记录每次架构变更对限界上下文的影响，提醒更新 DDD 领域模型。

---

### 2026-03-06 14:20 — 限界上下文变更

**受影响的限界上下文**：Agent 执行上下文、安全策略上下文

**变更文件**：
- `crates/octo-engine/src/agent/mod.rs`
- `crates/octo-engine/src/security/policy.rs`

**变更类别**：Agent 架构变更、安全策略变更

> 请检查 `DDD_DOMAIN_ANALYSIS.md` 中对应限界上下文的类型定义和聚合根是否需要更新。

---

### 2026-03-06 14:20 — 限界上下文变更

**受影响的限界上下文**：MCP 集成上下文

**变更文件**：
- `crates/octo-engine/src/mcp/client.rs`

**变更类别**：MCP 集成变更

> 请检查 `DDD_DOMAIN_ANALYSIS.md` 中对应限界上下文的类型定义和聚合根是否需要更新。

---

### 2026-03-06 14:27 — 限界上下文变更

**受影响的限界上下文**：Agent 执行上下文

**变更文件**：
- `crates/octo-engine/src/agent/mod.rs`

**变更类别**：Agent 架构变更

> 请检查 `DDD_DOMAIN_ANALYSIS.md` 中对应限界上下文的类型定义和聚合根是否需要更新。

---

### 2026-03-06 14:27 — 限界上下文变更

**受影响的限界上下文**：通用结构、工具执行上下文

**变更文件**：
- `crates/octo-engine/src/tools/registry.rs`

**变更类别**：结构性变更

> 请检查 `DDD_DOMAIN_ANALYSIS.md` 中对应限界上下文的类型定义和聚合根是否需要更新。

---

### 2026-03-07 01:20 — 限界上下文变更（代码审查修复批次）

**受影响的限界上下文**：认证上下文、MCP 集成上下文、平台用户上下文

**变更文件**：
- `crates/octo-engine/src/auth/config.rs` — 安全加固：HMAC Secret 强制检查
- `crates/octo-engine/src/agent/runtime_mcp.rs` — 并发修复：call_mcp_tool 锁外 I/O
- `crates/octo-server/src/api/mcp_servers.rs` — 数据一致性修复：args/env 序列化、状态检测
- `crates/octo-platform-server/src/db/users.rs` — 可靠性修复：mutex 毒化防护
- `crates/octo-platform-server/src/tenant/manager.rs` — 可靠性修复：mutex 毒化防护

**变更类别**：安全策略变更、MCP 集成变更、平台用户管理变更

**变更摘要**：

| 上下文 | 变更 | 影响 |
|--------|------|------|
| 认证上下文 | `warn_if_insecure()` 在 api_key/full 模式下使用默认 HMAC Secret 时 panic | 破坏性：未配置 `OCTO_HMAC_SECRET` 的部署无法启动 |
| MCP 集成上下文 | `call_mcp_tool()` 改为 clone-under-lock 模式，消除并发序列化 | 非破坏性：外部 API 不变 |
| MCP 存储上下文 | `args` 改为 JSON 数组序列化，`env` 统一为 JSON 对象，状态查找按名称匹配 | 已有数据库记录中的旧格式 args 需要重新创建 |
| 平台用户上下文 | `UserDatabase` 和 `TenantManager` 的 mutex 操作改为毒化安全模式 | 非破坏性：行为等价，panic 防护提升 |

> **`DDD_DOMAIN_ANALYSIS.md` 待更新项**：
> - 认证上下文：`AuthConfig::warn_if_insecure()` 行为约束已变更为 fail-fast
> - MCP 集成上下文：`call_mcp_tool()` 并发语义更新（锁粒度收窄至 Arc clone）
> - 平台用户上下文：`UserDatabase` 聚合根的并发处理策略更新

---

### 2026-03-07 14:30 — 限界上下文变更（ADR 文档补充）

**受影响的限界上下文**：全部 6 个核心限界上下文

**新增 ADR 文档**：
- `docs/adr/ADR_AGENT_ARCHITECTURE.md` — Agent 模块化架构、AgentRouter、ManifestLoader
- `docs/adr/ADR_MCP_INTEGRATION.md` — MCP Manager、Client 多协议、Tool Bridge
- `docs/adr/ADR_MEMORY_SYSTEM.md` — 四层记忆架构、HNSW 向量、混合查询
- `docs/adr/ADR_HOOKS_SYSTEM.md` — 11 个钩子点、HookHandler、HookContext
- `docs/adr/ADR_EVENT_SOURCING.md` — EventBus、EventStore、Projection、StateReconstructor
- `docs/adr/ARCH_SCAN_2026.md` — 架构全景扫描报告

**DDD 领域模型更新**：

| 限界上下文 | 聚合根 | 新增类型 |
|-----------|--------|---------|
| Agent 执行上下文 | AgentRuntime | AgentRouter, ManifestLoader |
| MCP 集成上下文 | McpManager | McpClient (Stdio/SSE), McpToolBridge |
| 记忆上下文 | MemorySystem | VectorBackend, HybridQueryEngine, ContextInjector |
| 钩子上下文 | HookRegistry | HookHandler, HookPoint |
| 事件溯源上下文 | EventBus | EventStore, ProjectionEngine, StateReconstructor |
| 安全策略上下文 | SecurityPolicy | (已有) |

> 本次更新补充了完整的 ADR 文档体系，覆盖全部核心架构模块。

---
