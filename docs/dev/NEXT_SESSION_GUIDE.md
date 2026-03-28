# octo-sandbox 下一会话指南

**最后更新**: 2026-03-28 10:30 GMT+8
**当前分支**: `main` (ahead of origin by 4 commits)
**当前状态**: SubAgent Streaming Events 阶段完成，scheduler tool WIP 未提交

---

## 项目状态

```
SubAgent Streaming Events                  -> COMPLETE @ cc05eeb
Builtin Commands Redesign                  -> COMPLETE @ 1916320
Custom Commands + TUI Fixes                -> COMPLETE @ 263eeb2
Post-AF: Builtin Skills + Config + TUI Fix -> COMPLETE @ 072c15b
Phase AF: SSM Wiring + Deferred Batch      -> COMPLETE @ 976e813
Phase AE: Agent Workspace Architecture     -> COMPLETE @ ee4986f
Phase AD-AC-AB-AA-Z-Y-X-W-V-U-T           -> ALL COMPLETE
```

### 基线数据

- **Tests**: 2476 passing
- **测试命令**: `cargo test --workspace -- --test-threads=1`
- **DB migrations**: CURRENT_VERSION=11

---

## 进行中的工作 (未提交)

### Scheduler Tool (schedule_task)

在 TUI 模式下暴露调度器 CRUD 操作给 agent。

**已完成文件**:
- `crates/octo-engine/src/tools/scheduler.rs` — 新建：ScheduleTaskTool 实现
- `crates/octo-engine/src/tools/mod.rs` — 新增 scheduler 模块 + register_scheduler_tools()
- `crates/octo-engine/src/agent/runtime.rs` — 创建 SqliteSchedulerStorage + 注册 scheduler tools

**设计文档**: `.claude/plans/tranquil-wibbling-unicorn.md`

**设计要点**:
- 单一 action-dispatch tool（非多个独立 tool）
- 仅处理 CRUD via SchedulerStorage，不触发执行
- 可选注册：仅在 SchedulerStorage 可用时注册

**下一步**:
1. 完成 ScheduleTaskTool 的 execute() 实现（list/create/update/delete/run_now actions）
2. 编译验证：`cargo check --workspace`
3. 添加单元测试
4. 提交

---

## Deferred 未清项

| 来源 | ID | 内容 | 状态 |
|------|----|----|------|
| Phase AB | AB-D1 | Octo sandbox Docker image | 可实施 |
| Phase AB | AB-D2 | E2B provider 完整实现 | 可实施 |
| Phase AB | AB-D3 | WASM plugin loading | 待定 |
| Phase AB | AB-D4 | Session Sandbox 持久化 | 可实施 |
| Phase AB | AB-D5 | CredentialResolver -> sandbox env 注入 | 待定 |
| Phase AB | AB-D6 | gVisor / Firecracker provider | 可实施 |
| Phase AC | AC-D1 | CI/CD pipeline | 低优先级 |
| Phase AC | AC-D4~D6 | Multi-image, snapshots, compose | 低优先级 |
| Phase AD | AD-D1~D6 | LibreOffice, cloud, cosign, CLI, docling | 低优先级 |
| Phase AA | AA-D1 | octo auth login/status/logout | 待 UX 设计 |
| Phase AA | AA-D3 | XDG Base Directory | 低优先级 |
| Phase AA | AA-D4 | Config 热重载 | 未来增强 |
| Phase Z | Z-D1 | CredentialResolver -> provider chain | 待定 |

---

## 关键代码路径

| 文件 | 作用 |
|------|------|
| `crates/octo-engine/src/tools/scheduler.rs` | scheduler tool (WIP) |
| `crates/octo-engine/src/commands.rs` | 自定义命令加载器 + builtin sync |
| `crates/octo-engine/src/skills/execute_tool.rs` | sub-agent 事件转发 |
| `crates/octo-engine/src/agent/events.rs` | SubAgent* 事件变体 |
| `crates/octo-engine/src/sandbox/` | SandboxProfile, SSM, Docker, 路由 |
| `crates/octo-engine/src/agent/runtime.rs` | AgentRuntime 初始化 |
| `crates/octo-cli/src/tui/` | TUI 核心 |
| `config.default.yaml` | 全量配置参考 |

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
