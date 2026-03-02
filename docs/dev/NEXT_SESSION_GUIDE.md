# octo-sandbox 下一会话指南

**最后更新**: 2026-03-02 14:30 GMT+8
**当前分支**: `octo-workbench`
**当前状态**: 🔄 Phase 2.8 - Agent 增强 + Secret Manager 开始

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
| **Phase 2.8 Agent 增强 + Secret Manager** | 🔄 **进行中** | 26 任务，约 1080 LOC |

---

## Phase 2.8 任务清单

### 模块 1: Secret Manager

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 1.1 | CredentialVault: AES-256-GCM 加密存储 | ⬜ |
| Task 1.2 | Argon2id 密钥派生 | ⬜ |
| Task 1.3 | CredentialResolver 优先级链 | ⬜ |
| Task 1.4 | Taint Tracking 信息流安全 | ⬜ |
| Task 1.5 | 配置集成 ${SECRET:xxx} | ⬜ |
| Task 1.6 | 单元测试 | ⬜ |

### 模块 2: Agent Loop 增强

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 2.1 | AgentConfig 扩展 | ⬜ |
| Task 2.2 | 50轮/无限轮支持 | ⬜ |
| Task 2.3 | Typing 信号 | ⬜ |

### 模块 3: Extension 钩子系统

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 3.1 | Extension trait + Event | ⬜ |
| Task 3.2 | ExtensionRegistry | ⬜ |
| Task 3.3 | AgentLoop 钩子集成 | ⬜ |
| Task 3.4 | LoggingExtension 示例 | ⬜ |

### 模块 4: CancellationToken

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 4.1 | CancellationToken | ⬜ |
| Task 4.2 | ChildCancellationToken | ⬜ |
| Task 4.3 | ToolRegistry 集成 | ⬜ |

### 模块 5: 并行执行

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 5.1 | execute_parallel 函数 | ⬜ |
| Task 5.2 | AgentLoop 并行分支 | ⬜ |
| Task 5.3 | 错误处理 | ⬜ |
| Task 5.4 | 集成测试 | ⬜ |

### 模块 6: OAuth2 PKCE (P2)

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 6.1 | PKCE 流程 | ⬜ |
| Task 6.2 | OAuth Extension | ⬜ |
| Task 6.3 | Provider 支持 | ⬜ |

### 模块 7: 构建验证

| 任务 | 内容 | 状态 |
|------|------|------|
| Task 7.1 | cargo check | ⬜ |
| Task 7.2 | cargo test | ⬜ |
| Task 7.3 | 文档更新 | ⬜ |

**实施计划**: `docs/plans/2026-03-02-phase2-8-agent-enhancement.md`

---

## 关键代码路径

| 组件 | 路径 |
|------|------|
| Secret Manager | `crates/octo-engine/src/secret/` |
| Agent Loop | `crates/octo-engine/src/agent/loop_.rs` |
| Extension | `crates/octo-engine/src/agent/extension.rs` |
| Cancellation | `crates/octo-engine/src/agent/cancellation.rs` |
| Parallel | `crates/octo-engine/src/agent/parallel.rs` |

---

## 设计参考

| 来源 | 特性 |
|------|------|
| OpenFang | AES-256-GCM, Argon2id, Keyring, Taint Tracking |
| pi_agent_rust | Extension 钩子, 8并行, AbortSignal |
| openclaw | Typing 信号 |

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
# 开始 Phase 2.8 实施
superpowers:executing-plans
```

---

## 重要记忆引用

| claude-mem ID | 内容 |
|---------------|------|
| #3000 | Phase 2.6 Provider Chain 完成总结 |
| #2999 | octo-workbench v1.0 完成总结 |
| #2886 | Phase 2.4 Engine Hardening 完成总结 |

---
