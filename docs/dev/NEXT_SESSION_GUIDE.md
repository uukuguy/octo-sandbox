# octo-sandbox 下一会话指南

**最后更新**: 2026-03-03 00:10 GMT+8
**当前分支**: `octo-workbench`
**当前状态**: ✅ Phase 2.9 - MCP SSE Transport 完成

---

## 当前阶段进度

| 阶段 | 状态 | 说明 |
|------|------|------|
| Phase 1 核心引擎 | ✅ 完成 | 32 Rust + 16 TS 文件，E2E 验证通过 |
| Phase 2 Batch 1-3 | ✅ 完成 | 上下文工程 + 记忆系统 + Debug UI |
| Phase 2.1 调试面板 | ✅ 完成 | Timeline + JsonViewer + Inspector |
| Phase 2.2 记忆系统 | ✅ 完成 | 5 memory tools + Explorer |
| Phase 2.3 MCP Workbench | ✅ 完成 | 动态 MCP Server 管理 + 前端 |
| Phase 2.4 Engine Hardening | ✅ 完成 | Loop Guard + 4+1阶段 + Retry + EventBus + Tool Security |
| Phase 2.5 用户隔离 | ✅ 完成 | DB migration v4 + Auth middleware + API handlers + WebSocket |
| Phase 2.6 Provider Chain | ✅ 完成 | LlmInstance + ProviderChain + ChainProvider + REST API |
| Phase 2.7 Metrics + Audit | ✅ 完成 | MetricsRegistry + AuditStorage + REST API + EventBus 集成 |
| Phase 2.8 Agent 增强 + Secret Manager | ✅ 完成 | Secret Manager + Agent Loop 增强 (10/10 tasks) |
| Phase 2.9 MCP SSE Transport | ✅ 完成 | SseMcpClient + add_server_v2() + API |
| Phase 2.10 Knowledge Graph | ✅ 完成 | Entity/Relation + Graph + FTS5 + 持久化 |
| Phase 2.11 AgentRegistry | ⏳ 待实施 | 多代理注册表 |

---

## Phase 2.9: MCP SSE Transport

**状态**: 设计完成，开始实施
**计划**: `docs/plans/2026-02-27-mcp-sse-transport.md`

### 任务清单

| Task | 内容 | 状态 |
|------|------|------|
| Task 1 | 添加 transport 字段 + 依赖 | ✅ |
| Task 2 | 实现 SseMcpClient | ✅ |
| Task 3 | McpManager 支持 transport 分发 | ✅ |
| Task 4 | REST API list_servers + create_server | ✅ |
| Task 5 | 全量构建验证 | ✅ |

### 目标

为 octo-engine MCP 客户端增加 Streamable HTTP（SSE）transport 支持，使 octo 能连接远程 MCP 服务器（如通过 URL 暴露的服务），同时保持与现有 Stdio transport 的完全兼容。

### 架构

采用方案 A——在 `McpServerConfigV2` 添加 `transport` 字段（`stdio` / `sse`），`McpManager::add_server_v2()` 根据该字段选择创建 `StdioMcpClient` 或 `SseMcpClient`。两者都实现相同的 `McpClient` trait，上层调用无需感知差异。

---

## 待实施阶段 (CC 驱动)

| Phase | 任务 | 计划文档 | 状态 |
|-------|------|----------|------|
| Phase 2.9 | MCP SSE Transport | `2026-02-27-mcp-sse-transport.md` | ✅ 已完成 |
| Phase 2.11 | AgentRegistry 多代理 | `2026-03-02-phase2-9-agent-registry.md` | ⏳ 待实施 |

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| MCP Traits | `crates/octo-engine/src/mcp/traits.rs` |
| MCP Stdio | `crates/octo-engine/src/mcp/stdio.rs` |
| MCP Manager | `crates/octo-engine/src/mcp/manager.rs` |
| MCP SSE (新增) | `crates/octo-engine/src/mcp/sse.rs` |
| MCP API | `crates/octo-server/src/api/mcp_servers.rs` |

---

## 快速启动命令

```bash
# 构建验证
cargo check --workspace
cd web && npx tsc --noEmit && cd ..

# 运行测试
cargo test -p octo-engine

# 启动开发服务器
make dev
```

---

## 下一步操作

```bash
# Phase 2.9 开始实施
# 使用 executing-plans 或 subagent-driven-development 执行计划

/executing-plans
# 或
/subagent-driven-development
```

---

## 重要记忆引用

| claude-mem ID | 内容 |
|---------------|------|
| #3007 | Phase 2.8 Agent 增强 + Secret Manager 完成总结 |
| #3000 | Phase 2.6 Provider Chain 完成总结 |
| #2999 | octo-workbench v1.0 完成总结 |
| #2886 | Phase 2.4 Engine Hardening 完成总结 |

---
