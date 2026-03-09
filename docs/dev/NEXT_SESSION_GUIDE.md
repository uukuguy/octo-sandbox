# octo-sandbox 下一会话指南

**最后更新**: 2026-03-09 17:10 GMT+8
**当前分支**: `main`
**当前状态**: Harness 实现阶段 — 28 任务计划已创建，待执行 P0-1

---

## 当前活跃阶段: Harness 实现

**计划文档**: `docs/plans/2026-03-09-harness-implementation.md`
**Checkpoint**: `docs/plans/.checkpoint.json`
**进度**: 0/28 tasks (0%)

### 任务概览

| 阶段 | 名称 | 任务数 | 状态 | 核心目标 |
|------|------|--------|------|---------|
| P0 | 核心接口切换 | 8 | 待开始 | AgentEvent 统一、run_agent_loop()、BoxStream 返回 |
| P1 | 模块集成 | 8 | 待开始 | Continuation/Masker/Interceptor/DeferredAction/TurnGate |
| P2 | 消费者适配 | 6 | 待开始 | Executor/WS/Scheduler/Runtime 切换 |
| P3 | 端到端验证 | 6 | 待开始 | 回归测试、集成测试、清理 |

### P0 任务明细（优先执行，严格顺序）

| ID | 名称 | 关键文件 |
|----|------|----------|
| P0-1 | 统一 AgentEvent 到 events.rs | `agent/events.rs`, `agent/loop_.rs`, `agent/mod.rs` |
| P0-2 | 扩展 AgentLoopConfig 为完整依赖注入 | `agent/loop_config.rs` |
| P0-3 | 实现 run_agent_loop() 骨架 | `agent/harness.rs` (新) |
| P0-4 | 迁移 Zone A/B 构建逻辑 | `agent/harness.rs`, `agent/loop_steps.rs` |
| P0-5 | 迁移 Provider 调用 + Stream 处理 | `agent/harness.rs` |
| P0-6 | 迁移 Tool 执行逻辑 | `agent/harness.rs` |
| P0-7 | 迁移 Context 管理 + AIDefence | `agent/harness.rs` |
| P0-8 | 完成主循环 + Hook 生命周期 | `agent/harness.rs`, tests |

### 关键文件路径

| 文件 | 用途 |
|------|------|
| `crates/octo-engine/src/agent/loop_.rs` | AgentLoop 949行 — 重构来源 |
| `crates/octo-engine/src/agent/harness.rs` | **新建** — 纯函数式 run_agent_loop() |
| `crates/octo-engine/src/agent/executor.rs` | AgentExecutor — P2 适配 |
| `crates/octo-engine/src/agent/loop_config.rs` | AgentLoopConfig — P0-2 扩展 |
| `crates/octo-engine/src/agent/events.rs` | AgentEvent — P0-1 统一 |
| `crates/octo-server/src/ws.rs` | WS handler — P2-2 适配 |

### 执行策略

```
P0: 严格顺序 P0-1 → P0-2 → P0-3 → P0-4 → P0-5 → P0-6 → P0-7 → P0-8
P1: 可并行（P1-1~P1-5 独立, P1-6 独立, P1-7~P1-8 独立）
P2: P2-5(兼容层) → P2-1(Executor) → P2-2(WS)，P2-3/P2-4 独立
P3: P3-1 → P3-2~P3-3(并行) → P3-4 → P3-5~P3-6(并行)
```

---

## 设计文档索引

| 文档 | 核心章节 |
|------|---------|
| `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` | §3.2 Agent Loop 重构, §3.3 Tool 系统, §3.4 Provider 链, §3.5 上下文管理 |
| `docs/design/AGENT_CLI_DESIGN.md` | CLI 设计方案 |

---

## 挂起阶段

| 阶段 | 进度 | 说明 |
|------|------|------|
| pre-harness-refactor | 100% | 42/42 + 5 Deferred, 857 tests |
| octo-platform Phase 1 | 100% | P1+P2 已完成 |
| Phase 2.11: AgentRegistry | 0% | 切换到 octo-platform 时挂起 |

---

## 下一步操作

```bash
# 开始执行计划
# 方式 1: Subagent-Driven（推荐）
/superpowers:subagent-driven-development

# 方式 2: Executing-Plans
/superpowers:executing-plans
```

---

## 快速命令

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```
