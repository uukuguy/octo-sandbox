# octo-sandbox 下一会话指南

**最后更新**: 2026-02-27 19:30 GMT+8
**当前分支**: `octo-workbench`
**当前状态**: Phase 2.4 完成，准备下一阶段

---

## 当前完成状态

| 阶段 | 状态 | 说明 |
|------|------|------|
| Phase 1 核心引擎 | ✅ 完成 | 32 Rust + 16 TS 文件，E2E 验证通过 |
| Phase 2 Batch 1-3 | ✅ 完成 | 上下文工程 + 记忆系统 + Debug UI |
| Phase 2.1 调试面板 | ✅ 完成 | Timeline + JsonViewer + Inspector |
| Phase 2.2 记忆系统 | ✅ 完成 | 5 memory tools + Explorer |
| Phase 2.3 MCP Workbench | ✅ 完成 | 动态 MCP Server 管理 + 前端 |
| **Phase 2.4 Engine Hardening** | ✅ **完成** | Loop Guard + 4+1阶段 + Retry + EventBus + Tool Security |
| Phase 3 octo-platform | ⏳ 待开始 | Docker + 多用户 + 生产环境 |

---

## 下一步优先级

### 优先级 1: MCP SSE 传输支持（Phase 2.4 遗留）
- **背景**: 当前 MCP Client 仅支持 Stdio transport，需增加 SSE transport
- **参考**: `docs/design/MCP_WORKBENCH_DESIGN.md §Phase 2.4 SSE Transport`
- **工作量估算**: ~3-4 个任务，~200 LOC

### 优先级 2: 运行时集成验证
- 启动服务器验证 Loop Guard 触发行为
- 验证 EventBus 事件在前端 Debug 面板可见
- 验证 ExecSecurityMode 安全拦截生效

### 优先级 3: Phase 3 octo-platform 规划
- Docker 容器化（多服务 compose）
- 多用户支持（认证 + 资源隔离）
- AgentRegistry（从 OpenFang 移植，P0 优先级）
- 生产环境配置（监控 + 日志 + 限流）

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| Agent Loop | `crates/octo-engine/src/agent/loop_.rs` |
| Loop Guard | `crates/octo-engine/src/agent/loop_guard.rs` |
| EventBus | `crates/octo-engine/src/event/bus.rs` |
| Context Budget | `crates/octo-engine/src/context/budget.rs` |
| Context Pruner | `crates/octo-engine/src/context/pruner.rs` |
| LLM Retry | `crates/octo-engine/src/providers/retry.rs` |
| BashTool Security | `crates/octo-engine/src/tools/bash.rs` |
| MCP Manager | `crates/octo-engine/src/mcp/manager.rs` |
| REST API Routes | `crates/octo-server/src/api/` |
| Frontend App | `web/src/App.tsx` |

---

## 快速启动命令

```bash
# 构建验证
cargo check --workspace
cd web && npx tsc --noEmit && cd ..

# 运行测试（providers/retry.rs 含 13 个单元测试）
cargo test -p octo-engine
```

---

## 重要记忆引用

| claude-mem ID | 内容 |
|---------------|------|
| #2886 | Phase 2.4 Engine Hardening 完成总结 |
| #2885 | ARCHITECTURE_DESIGN.md v1.1 更新说明 |
| #2880 | OpenFang 架构整合路线图 |
| #2851 | Phase 2 Batch 3 完整实施记录 |

---

## OpenFang 整合状态

| 模块 | 优先级 | 状态 |
|------|--------|------|
| Loop Guard ✅ | P0 | **已整合** |
| LLM 错误分类 + Retry ✅ | P0 | **已整合** |
| Context 4+1 阶段 ✅ | P0 | **已整合** |
| EventBus ✅ | P1 | **已整合** |
| 工具执行安全 ✅ | P1 | **已整合** |
| MCP SSE Transport | P1 | ⏳ 待实施 |
| AgentRegistry | P0 | ⏳ Phase 3 |
| Memory 增强 | P1 | ⏳ Phase 3 |
| Scheduler | P2 | ⏳ Phase 3 |
