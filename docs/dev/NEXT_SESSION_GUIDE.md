# octo-sandbox 下一会话指南

**最后更新**: 2026-03-11 09:30 GMT+8
**当前分支**: `main`
**当前状态**: Deferred 项完成方案 (Wave 1+2) — 设计完成，待执行

---

## 当前活跃阶段：Deferred 项完成 (Wave 1+2)

### 计划文档

- **实施方案**: `docs/plans/2026-03-11-deferred-completion.md` (597 行, 10 tasks, 2 waves)
- **Checkpoint**: `docs/plans/.checkpoint.json` (status: DESIGN, 0/10 tasks)
- **Phase Stack**: `docs/dev/.phase_stack.json`

### 方案概要

从 17 个累积 Deferred 项中选取 10 个高价值核心项（方案 B），分 2 个 Wave 实施。

#### Wave 1 (P0): 安全加固 + 可观测性 (~1.5 天)

| Task | 内容 | 关键文件 | 工作量 |
|------|------|---------|--------|
| T1 | Canary Token 接入 AgentLoop | harness.rs, loop_config.rs, runtime.rs | 0.5 天 |
| T2 | Symlink 防护 file_read/write/edit | file_read.rs, file_write.rs, file_edit.rs | 0.5 天 |
| T3 | Observability publish() 补齐 | harness.rs (3 个新事件) | 0.5 天 |
| T4 | EventStore 初始化 + REST API | runtime.rs, api/events.rs (新) | 1 天 |
| T5 | Memory TTL 清理定时任务 | sqlite_store.rs, store_traits.rs | 0.5 天 |

#### Wave 2 (P1): 实时交互 + 平台补全 + 智能路由 (~5-7 天)

| Task | 内容 | 关键文件 | 工作量 |
|------|------|---------|--------|
| T6 | Platform WS AgentRuntime 集成 | platform-server/ws.rs, agent_pool.rs | 2-3 天 |
| T7 | ApprovalGate wiring (已实现未接通!) | main.rs, state.rs, executor.rs | 0.5 天 |
| T8 | Dashboard 实时事件推送 | ws/types.ts, events.ts, Debug.tsx | 1-2 天 |
| T9 | Agent 协作 Dashboard 面板 | api/collaboration.rs (新), Collaboration.tsx (新) | 2-3 天 |
| T10 | SmartRouting 复杂度分类器 | providers/smart_router.rs (新), pipeline.rs | 3-4 天 |

### 关键发现（必读）

1. **T7 ApprovalGate 已完整实现** — `tools/approval.rs` 中 struct + methods 全部存在，`AppState` 和 `AgentLoopConfig` 都有字段，WS handler 已处理 `ApprovalResponse`。**唯一缺失**: `main.rs` 从未创建共享实例，两端均 `None`。修复只需 wiring。

2. **T1 SafetyPipeline 已在 harness 3 个检查点调用** — 仅需注入 canary 到 system prompt 即可。

3. **T4 EventStore + Reconstructor 已完全实现** — 仅需 init + REST endpoints。

4. **T6 平台 WS 需扩展 13 个 ServerMessage 变体** — 参考 `octo-server/src/ws.rs` 的完整模式。

5. **T10 SmartRouting V1 仅需 model override** — 单 Provider + 修改 request.model 字段，无需跨 Provider 路由。

### 执行依赖

```
T2 (Symlink) ──(独立)
T5 (TTL) ────(独立)
T10 (SmartRouting) ──(独立)
T3 (Observability) → T8 (Dashboard 实时) → T9 (协作面板)
T1 (Canary) → T7 (ApprovalGate) → T6 (Platform WS)
```

### 推荐执行顺序

Wave 1 并行: `[T2+T5] || [T3→T1] || [T4]`
Wave 2 并行: `[T7→T6] || [T8→T9] || [T10]`

### 基线

- **Tests**: 1343 passing @ commit `49bd94f`
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **检查命令**: `cargo check --workspace`

### 启动命令

```bash
# 1. 恢复计划
/resume-plan

# 2. 或直接开始执行
npx @claude-flow/cli@latest swarm init --topology hierarchical --max-agents 8 --strategy specialized
# 然后启动 subagent-driven-development
```

---

## 上一阶段（已完成）

| 阶段 | Tasks | 状态 |
|------|-------|------|
| Octo-CLI 设计与实现 | 34/34 | COMPLETE |
| Deferred D3/D6/D7 | 24/24 | COMPLETE |
| Deferred D2/D4/D5 | 20/20 | COMPLETE |
