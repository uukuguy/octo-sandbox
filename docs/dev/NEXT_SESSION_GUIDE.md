# octo-sandbox 下一会话指南

**最后更新**: 2026-03-29 00:30 GMT+8
**当前分支**: `main` (ahead of origin by 10 commits)
**当前状态**: Phase AG 计划编写完成，待实施

---

## 项目状态

```
Phase AG — 记忆和上下文机制增强      -> PLANNED (11 tasks, 4 groups)
CI Fix + Z-D1 + AA-D1 + AB-D1(partial)  -> COMPLETE @ 9f7c163
Scheduler Tool (schedule_task)           -> COMPLETE @ a922159
SubAgent Streaming Events                -> COMPLETE @ cc05eeb
Builtin Commands Redesign                -> COMPLETE @ 1916320
Custom Commands + TUI Fixes              -> COMPLETE @ 263eeb2
Post-AF: Builtin Skills + Config + TUI Fix -> COMPLETE @ 072c15b
Phase AF-AE-AD-AC-AB-AA-Z-Y-X-W-V-U-T  -> ALL COMPLETE
```

### 基线数据

- **Tests**: 2476 passing
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **DB migrations**: CURRENT_VERSION=11 (Phase AG 将升级到 v12)

---

## Phase AG 概览

**设计文档**: `docs/design/MEMORY_CONTEXT_ENHANCEMENT_DESIGN.md`
**实施计划**: `docs/plans/2026-03-29-phase-ag-memory-context-enhancement.md`
**Checkpoint**: `docs/plans/.checkpoint.json`

### 核心目标

让 agent 具备跨会话记忆、事件记忆、时间线查询、主动记忆管理能力。

### 任务分组

| Group | Tasks | 目标 |
|-------|-------|------|
| G1 | Task 1-3 | 接线修复 + 基础设施（类型扩展、DB migration、Hook 接线） |
| G2 | Task 4-7 | 情景记忆系统（事件提取、会话摘要、时间线查询） |
| G3 | Task 8-10 | Agent 主动记忆管理（memory_edit、摘要注入、指令+刷新） |
| G4 | Task 11 | 上下文工程增强（ObservationMasker 接入） |

### 执行顺序

```
G1 (Task 1→2→3) → G2 (Task 4‖5→6, 7) → G3 (Task 8, 9, 10) → G4 (Task 11)
```

### 五大断裂点修复

1. ✅ 会话结束不提取 → Task 3 + Task 6 接线 SessionEndMemoryHook + EventExtractor
2. ✅ 新会话不注入 → Task 3 + Task 9 接线 MemoryInjector + Session Summaries
3. ⏳ 搜索只有 FTS → Deferred AG-D5 (HybridQueryEngine)
4. ✅ Zone B 只注入一次 → Task 10 周期刷新
5. ⏳ 压缩只能截断 → Deferred AG-D7 (Summarize 策略)

---

## 下一步操作

```bash
# 1. 确认基线
cargo check --workspace
cargo test --workspace -- --test-threads=1  # 应为 2476

# 2. 开始 G1-Task 1: 类型系统扩展
# 编辑 crates/octo-types/src/memory.rs
# 新增 MemoryType, EventData, SortField 等

# 3. 逐 Task 推进
# 每完成一个 Task: cargo test + /checkpoint-progress
```

---

## 关键代码路径

| 文件 | 作用 |
|------|------|
| `crates/octo-types/src/memory.rs` | 记忆类型定义 (本 Phase 重点修改) |
| `crates/octo-engine/src/memory/` | 记忆子系统 (本 Phase 重点区域) |
| `crates/octo-engine/src/agent/harness.rs` | Agent Loop (接线目标) |
| `crates/octo-engine/src/agent/executor.rs` | Session 生命周期 (hook 接线目标) |
| `crates/octo-engine/src/context/system_prompt.rs` | Zone A 记忆指令 |
| `crates/octo-engine/src/context/observation_masker.rs` | ObservationMasker (待接入) |
| `crates/octo-engine/src/tools/` | 记忆工具 (新增 timeline, edit) |
| `crates/octo-engine/src/db/mod.rs` | DB migration v12 |

---

## Deferred 未清项

### Phase AG Deferred
| ID | 内容 | 优先级 |
|----|------|--------|
| AG-D1 | 程序记忆提取（工作流模式学习） | P2 |
| AG-D2 | 情景→语义巩固 | P3 |
| AG-D3 | 智能遗忘 | P3 |
| AG-D4 | 记忆冲突解决 | P3 |
| AG-D5 | HybridQueryEngine 接入 memory tools | P2 |
| AG-D6 | KG 语义搜索 | P3 |
| AG-D7 | Summarize 压缩策略 | P2 |
| AG-D8 | Memory Explorer 前端增强 | P3 |

### 历史 Deferred（不影响 Phase AG）
| 来源 | ID | 内容 |
|------|----|------|
| Phase AB | AB-D2~D6 | E2B, WASM, sandbox 持久化, gVisor |
| Phase AC | AC-D4~D6 | Multi-image, snapshots, compose |
| Phase AD | AD-D1~D6 | LibreOffice, cloud, cosign, CLI, docling |

---

## 快速启动

```bash
# 编译检查
cargo check --workspace

# 全量测试
cargo test --workspace -- --test-threads=1

# TUI 模式
make cli-tui

# CLI 交互模式
make cli-run

# 启动 server + web
make dev
```
