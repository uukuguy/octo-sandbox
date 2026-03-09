# octo-sandbox 下一会话指南

**最后更新**: 2026-03-09 GMT+8
**当前分支**: `main`
**当前状态**: Pre-Harness Refactor 计划已重新组织（P0/P1/P2/P3），待执行

---

## 当前活跃阶段: pre-harness-refactor

**计划文档**: `docs/plans/2026-03-09-pre-harness-refactor.md`
**Checkpoint**: `docs/plans/.checkpoint.json`
**进度**: 0/42 tasks (0%)

### 任务概览

| 阶段 | 名称 | 任务数 | 状态 | 估计 LOC |
|------|------|--------|------|----------|
| P0 | 核心重构 — 基础架构 | 10 | 待开始 | ~1200 |
| P1 | 安全与可靠性 | 12 | 待开始 | ~1000 |
| P2 | Provider 基础设施与 Skill 高级功能 | 10 | 待开始 | ~800 |
| P3 | 高级功能 | 10 | 待开始 | ~600 |

### P0 任务明细（优先执行）

| ID | 名称 | 关键文件 |
|----|------|----------|
| P0.1 | AgentLoopConfig 依赖注入 | `agent/loop_.rs`, `agent/config.rs` |
| P0.2 | Step Functions 提取 | `agent/loop_.rs` → `agent/steps.rs` |
| P0.3 | AgentEvent 事件流 | `agent/events.rs`, `event/bus.rs` |
| P0.4 | SkillDefinition 增强字段 | `octo-types/src/skill.rs` |
| P0.5 | SkillTool action-based dispatch | `skills/tool.rs` |
| P0.6 | TrustManager 三级信任 | `skills/trust.rs` (新) |
| P0.7 | ToolCallInterceptor | `tools/interceptor.rs` (新) |
| P0.8 | TurnGate 并发控制 | `agent/turn_gate.rs` (新) |
| P0.9 | 错误响应不持久化 | `agent/loop_.rs` |
| P0.10 | ProviderErrorKind 语义路由 | `providers/retry.rs` |

### 推荐执行顺序

1. **P0.1** → P0.2 → P0.3（AgentLoop 重构链，有依赖）
2. **P0.4** → P0.5 → P0.6 → P0.7（Skills 修复链，有依赖）
3. **P0.8**（独立，可并行）
4. **P0.9** → P0.10（Provider 修复链，有依赖）

### 关键文件路径

| 文件 | 用途 |
|------|------|
| `crates/octo-engine/src/agent/loop_.rs` | AgentLoop 909行 run() — P0.1/P0.2/P0.9 |
| `crates/octo-engine/src/skills/tool.rs` | SkillTool — P0.5 修复目标 |
| `crates/octo-types/src/skill.rs` | SkillDefinition — P0.4 扩展目标 |
| `crates/octo-engine/src/tools/mod.rs` | ToolRegistry — P0.7 集成点 |
| `crates/octo-engine/src/tools/traits.rs` | Tool trait — P1.1 增强目标 |
| `crates/octo-engine/src/hooks/handler.rs` | HookHandler — P1.4 扩展目标 |
| `crates/octo-engine/src/providers/retry.rs` | LlmErrorKind — P0.9/P0.10 扩展目标 |
| `crates/octo-engine/src/skills/runtime_bridge.rs` | SkillRuntimeBridge — P0.5/P1.7 集成点 |

---

## 设计文档索引

| 文档 | 来源 | 内容 |
|------|------|------|
| `docs/design/AGENT_HARNESS_BEST_IMPLEMENTATION_DESIGN.md` | Opus | Harness 最佳实现设计（§3.2-§3.7, §4 优先级矩阵） |
| `docs/design/AGENT_SKILLS_BEST_IMPLEMENTATION_DESIGN.md` | Opus | Skills 最佳实现设计（§5.3-§5.10, §6 优先级矩阵） |
| `docs/design/AGENT_HARNESS_INDUSTRY_RESEARCH_2025_2026.md` | Opus | 行业研究报告 |
| `docs/found-by-sonnet4.6/AGENT_HARNESS_DESIGN.md` | Sonnet 4.6 | TurnGate/nanobot/moltis/HookFailureMode 等独到发现 |
| `docs/found-by-sonnet4.6/AGENT_SKILLS_DESIGN.md` | Sonnet 4.6 | SkillSelector 4阶段/ToolConstraintEnforcer/SlashRouter 等 |

---

## 挂起阶段

| 阶段 | 进度 | 说明 |
|------|------|------|
| octo-platform Phase 1 | 100% | P1+P2 已完成 |
| Phase 2.11: AgentRegistry | 0% | 切换到 octo-platform 时挂起 |

---

## 下一步操作

```bash
# 开始执行计划
# 方式 1: Subagent-Driven（推荐）
superpowers:subagent-driven-development

# 方式 2: Executing-Plans
superpowers:executing-plans
```

---

## 快速命令

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```
