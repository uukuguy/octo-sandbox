# octo-sandbox 下一会话指南

**最后更新**: 2026-03-30 07:30 GMT+8
**当前分支**: `main`
**当前状态**: Phase AH 完成，无活跃 Phase

---

## 项目状态

### 已完成 Phases
- Phase A-H: Core Engine + Eval 基础
- Phase I-R: 外部 Benchmark + 标准测试 + 评估
- Phase S: Agent Capability Boost
- Phase T: TUI OpenDev 整合 (24 tasks)
- Phase U: TUI Production Hardening (10 tasks)
- Phase V: Agent Skills (11 tasks)
- Phase W: OctoRoot 统一目录管理 (10 tasks)
- Phase X: TUI 运行状态增强 (4 tasks)
- Phase Y: Playbook Skill SubAgent (1 task)
- Phase Z: Landmine Scan & Fix (2 tasks)
- Phase AA: 部署配置架构 (6 tasks)
- Phase AB: 智能体工具执行环境 (10 tasks)
- Phase AC: 沙箱容器 (9 tasks)
- Phase AD: 容器镜像增强 (5 tasks)
- Phase AE: Agent Workspace Architecture (7 tasks)
- Phase AF: SSM Wiring (3 tasks)
- Phase AG: 记忆和上下文机制增强 (11 tasks + 5 deferred)
- **Phase AH: Hook 系统增强 (15 tasks + 3 deferred) @ 4ebc7fa**

### 测试基线
- 2476+ tests (pre-AH baseline)
- 104 new hook tests
- DB version: 12

---

## Phase AH 产出摘要

三层混合 Hook 架构：
- **L1 编程式** (priority=10): SecurityPolicyHandler + AuditLogHandler, 自动注册
- **L2 策略引擎** (priority=100): policies.yaml 零代码规则
- **L3 声明式** (priority=500): hooks.yaml (command + prompt + webhook)

关键文件：
```
crates/octo-engine/src/hooks/
├── context.rs          # HookContext (增强后含 env/history/query + Serialize)
├── builtin/            # SecurityPolicyHandler + AuditLogHandler
├── declarative/        # config + command_executor + prompt_renderer/executor + webhook_executor + bridge + loader
└── policy/             # config + matcher + bridge
```

Runtime 接线: `agent/runtime.rs` 初始化时自动从 `.octo/hooks.yaml` 和 `.octo/policies.yaml` 加载

---

## ⚠️ Deferred 未清项

| 来源 | ID | 内容 | 前置条件 | 优先级 |
|------|----|----|---------|--------|
| Phase AH | D2 | WASM 插件 hook | WASM 基础设施 | P5 |
| Phase AH | D3 | 平台租户策略合并 | octo-platform-server | P4 |
| Phase AH | D4 | TUI hook 状态面板 | — | P4 |
| Phase AH | D5 | Stop/SubagentStop 声明式 | 新 HookPoint variant | P3 |
| Phase AH | D6 | ask → ApprovalGate | approval 系统 | P3 |

Landmine: `DeclarativeHookBridge::with_provider()` 未在 runtime 调用，prompt hooks 优雅跳过

---

## 下一步建议

1. **示例配置**: 创建 `examples/hooks.yaml` 和 `examples/policies.yaml` 供用户参考
2. **集成测试**: 在真实 agent loop 中验证三层 hook 链端到端
3. **D5 Stop events**: 添加 `HookPoint::Stop` / `HookPoint::SubagentStop` variant
4. **with_provider 接线**: 在 runtime 中将 provider clone 传入 DeclarativeHookBridge
5. **新 Phase 方向**: 可考虑 eval 增强、前端 MCP workbench 改进、或平台多租户推进
