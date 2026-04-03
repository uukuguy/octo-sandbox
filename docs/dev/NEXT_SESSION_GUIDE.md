# octo-sandbox 下一会话指南

**最后更新**: 2026-04-03 21:40 GMT+8
**当前分支**: `main`
**当前状态**: Phase AY 完成，无活跃 Phase

---

## 项目状态

### 已完成 Phases
- Phase A-H: Core Engine + Eval 基础
- Phase I-R: 外部 Benchmark + 标准测试 + 评估
- Phase S: Agent Capability Boost
- Phase T: TUI OpenDev 整合 (24 tasks)
- Phase U: TUI Production Hardening (10 tasks)
- Phase V: Agent Skills (11 tasks)
- Phase W-Z: OctoRoot + TUI + Playbook + Landmine
- Phase AA-AF: 部署配置 + 沙箱容器 + Workspace + SSM
- Phase AG-AI: 记忆增强 + Hook 系统 + WASM 插件
- Phase AJ-AK: 多会话复用 + Server 安全加固
- Phase AL-AM: Web 前端 + 可观测性
- Phase AO: octo-server 功能完善
- Phase AP-AQ: 追赶 CC-OSS + 自主能力
- Phase AR: CC-OSS 缺口补齐
- Phase AT: 提示词体系增强 + 编译优化
- Phase AU: Autonomous Mode Completion (5 tasks + 7 deferred)
- Phase AV: CC-OSS Gap Closure + Security Parity (6 tasks + 4 deferred)
- Phase AW: CC-OSS 工具体系对齐 (9 tasks)
- Phase AX: Builtin Agents CC-OSS alignment (7 tasks + 3 deferred)
- **Phase AY: SubAgent Runtime 完整生命周期 (7 tasks + 6 deferred + wiring audit) @ e80679f**

### 最新提交
```
941e590 docs: final checkpoint — Phase AY complete with wiring audit
e80679f fix(agent): wire safety_pipeline + recorder + loop_guard into sub-agents
782bd01 fix(agent): wire working_dir into AgentTool parent_config (AY-D1)
6811353 feat(agent): resolve AY-D1~D6 — SubAgentRuntime lifecycle deferred items
c552a87 feat(agent): Phase AY — SubAgentRuntime lifecycle + AgentTool rename
```

### 关键架构变化（Phase AY）

- **SubAgentRuntime**: 子 agent 从 "配置不同的 LLM 调用" 升级为 "有完整生命周期的运行时实体"
- **Agent/Skill 统一**: AgentTool 和 ExecuteSkillTool Playbook 共享 SubAgentRuntime 执行路径
- **工具重命名**: `spawn_subagent` → `agent`, `query_subagent` → `query_agent`
- **安全继承**: 子 agent 继承 safety_pipeline, canary_token, recorder, loop_guard
- **新 AgentManifest 字段**: `permission_mode`, `hook_scope`（预留扩展点）
- **agents/ YAML 已删除**: builtin agents 只在代码中定义

### 测试状态
- 2593+ tests (Phase AX baseline) + 15 (Phase AY) = ~2608 tests
- DB: CURRENT_VERSION=13

---

## 下一步建议

1. **新 Phase 规划** — 可考虑方向：
   - 前端 Agent 交互增强（TUI 中 agent 工具名对齐更新后的体验优化）
   - Multi-agent 协作增强（TeamManager/TaskTracker 实际接线）
   - 评估体系扩展（新增 evaluation suites）
   - 性能优化（编译时间、运行时 token 使用）

2. **可选清理工作**：
   - `manifest_loader.rs` 现在不再被 runtime.rs 调用，可以移除或保留为库
   - 部分 `..Default::default()` 路径可进一步收紧

---

## 关键代码路径

| 模块 | 路径 | 说明 |
|------|------|------|
| SubAgentRuntime | `crates/octo-engine/src/agent/subagent_runtime.rs` | 子 agent 生命周期核心 |
| AgentTool | `crates/octo-engine/src/tools/subagent.rs` | agent/query_agent 工具 |
| ExecuteSkillTool | `crates/octo-engine/src/skills/execute_tool.rs` | Skill Playbook 执行 |
| AgentExecutor | `crates/octo-engine/src/agent/executor.rs` | Agent 主循环 + 工具注册 |
| HookRegistry | `crates/octo-engine/src/hooks/registry.rs` | Hook 注册 + scoped |
| AgentManifest | `crates/octo-engine/src/agent/entry.rs` | Agent 定义 |
