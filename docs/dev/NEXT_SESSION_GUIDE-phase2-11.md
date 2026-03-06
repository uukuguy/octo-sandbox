# octo-sandbox 下一会话指南

**最后更新**: 2026-03-03 GMT+8
**当前分支**: `octo-workbench`
**当前状态**: 🔄 Phase 2.11 - AgentRegistry + 上下文工程重构（设计完成，待实施）

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
| Phase 2.11 AgentRegistry | 🔄 设计完成 | 见下方详细说明 |

---

## Phase 2.11: AgentRegistry + 上下文工程重构

**状态**: 设计完成（经过完整 brainstorming），待实施
**计划**: `docs/plans/2026-03-02-phase2-9-agent-registry.md`（1223 行，7 Tasks）

### 核心设计决策（重要，勿遗忘）

1. **Agent 身份**：三段式（role/goal/backstory），对齐 CrewAI 最佳实践
   - 优先级：`system_prompt` > `role/goal/backstory` > `SOUL.md` > `CORE_INSTRUCTIONS`

2. **AgentManifest**：创建时提供，不可变；含 name/tags/role/goal/backstory/system_prompt/model/tool_filter/config

3. **AgentRunner**（新增）：持有启动依赖，负责 AgentLoop 生命周期，AppState 持有 AgentRunner 而非 AgentRegistry

4. **per-agent ToolRegistry**：按 `tool_filter` 白名单从全局 ToolRegistry 裁剪，Skills 也在其中（SkillTool 已注册进 ToolRegistry）

5. **Zone A/B 分离**（上下文工程重构）：
   - Zone A（System Prompt，静态）：Agent 身份 + Bootstrap 文件 + 工具规范
   - Zone B（首条 Human Message，每轮刷新）：datetime + UserProfile + TaskContext + AutoExtracted
   - 废弃：AgentPersona block（身份移到 manifest），SandboxContext block（移到 Zone A）

6. **SQLite 持久化**：AgentEntry 持久化，重启加载（参考 McpStorage 模式）

7. **Budget 统一**：ContextInjector 与 TokenBudget 对齐，system_prompt budget = 16,000

### 任务清单

| Task | 内容 | 状态 |
|------|------|------|
| Task 1 | AgentRegistry 核心（DashMap 三索引 + AgentManifest） | ⏳ |
| Task 2 | SQLite 持久化（AgentStore） | ⏳ |
| Task 3 | AgentRunner（per-agent ToolRegistry + 生命周期） | ⏳ |
| Task 4 | 上下文工程重构（Zone A/B 分离，MemoryBlockKind 清理） | ⏳ |
| Task 5 | Budget 统一 | ⏳ |
| Task 6 | AppState 集成 + REST API（8 个端点） | ⏳ |
| Task 7 | 构建验证 | ⏳ |

---

## ⚠️ Deferred 未清项（启动时必查）

| 计划文档 | ID | 内容 | 前置条件 |
|---------|----|----|---------|
| phase2-9-agent-registry.md | D1 | SkillRegistry 热重载后同步 per-agent ToolRegistry | Task 3 完成后 |
| phase2-9-agent-registry.md | D2 | SOUL.md/AGENTS.md 项目文件加载接入 AgentLoop | Task 4 Zone A 重构完成后 |
| phase2-9-agent-registry.md | D3 | AgentLoop 实际运行与 session 层集成（AgentRunner 目前 spawn 空任务） | WebSocket/session 层与 AgentRunner 集成设计后 |

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| Agent Loop | `crates/octo-engine/src/agent/loop_.rs` |
| Agent Config | `crates/octo-engine/src/agent/config.rs` |
| CancellationToken | `crates/octo-engine/src/agent/cancellation.rs` |
| SystemPromptBuilder | `crates/octo-engine/src/context/builder.rs` |
| ContextInjector | `crates/octo-engine/src/memory/injector.rs` |
| WorkingMemory | `crates/octo-engine/src/memory/working.rs` |
| MemoryBlockKind | `crates/octo-types/src/memory.rs` |
| ToolRegistry | `crates/octo-engine/src/tools/mod.rs` |
| SkillRegistry | `crates/octo-engine/src/skills/registry.rs` |
| SkillTool | `crates/octo-engine/src/skills/tool.rs` |
| AppState | `crates/octo-server/src/state.rs` |
| McpStorage（参考） | `crates/octo-engine/src/mcp/storage.rs` |

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
# Phase 2.11 开始实施（计划已完整，直接执行）
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
