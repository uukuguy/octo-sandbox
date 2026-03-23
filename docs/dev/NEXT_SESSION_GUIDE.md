# octo-sandbox 下一会话指南

**最后更新**: 2026-03-23 08:00 GMT+8
**当前分支**: `main`
**当前状态**: Phase AB 设计完成，待实施

---

## 项目状态

```
Phase AB: 智能体工具执行环境 (0/10)   → PLANNING_COMPLETE
Phase AA: Octo 部署配置架构 (6/6+D2)  → COMPLETE @ 4fbc30d
Phase Z:  Landmine Scan & Fix (2/2)   → COMPLETE @ 81fa923
Phase Y:  Playbook Skill SubAgent (1/1)→ COMPLETE @ c0f92b4
Phase X:  TUI 运行状态增强 (4/4)       → COMPLETE
Phase W:  OctoRoot 统一目录管理 (10/10) → COMPLETE
Phase V:  Agent Skills 完整实现 (11/12) → COMPLETE @ 19d3f30
Phase U:  TUI Production Hardening     → COMPLETE @ 77c2297
Phase T-A: 评估框架+TUI+基准           → ALL COMPLETE
Wave 1-10: Core Engine + CLI          → COMPLETE @ 675155d
```

### 基线数据

- **Tests**: 2394 passing (workspace)
- **测试命令**: `cargo test --workspace -- --test-threads=1`

---

## Phase AB — 智能体工具执行环境

### 设计文档

- **设计方案**: `docs/design/TOOL_EXECUTION_ENVIRONMENT_DESIGN.md`
- **实施计划**: `docs/plans/2026-03-23-phase-ab-tool-execution-environment.md`
- **前置设计**: `docs/design/SANDBOX_SECURITY_DESIGN.md` (Phase J)

### 核心架构

```
两种部署模式:
  A: Octo 在沙箱中运行 → 所有工具直接执行（已隔离）
  B: Octo 在主机运行   → 工具调用路由到沙箱

四种沙箱后端:
  Native(Subprocess) → 最快，弱隔离（开发模式）
  WASM(Wasmtime)     → 快，强隔离（纯计算/插件）
  Docker(Bollard)    → 较慢，最强本地隔离（脚本执行）
  External(第三方)    → 远程隔离（E2B/Modal/Firecracker）

SandboxProfile 一行切换:
  development → 直接执行，零摩擦
  staging     → Docker 优先，可降级
  production  → 必须沙箱隔离
  custom      → 精细控制
```

### 任务组 (10 tasks, 4 groups)

```
G1: 基础设施 (3 tasks)
  AB-T1: SandboxProfile 枚举 + 配置解析
  AB-T2: OctoRunMode 检测（Docker/K8s/Podman）
  AB-T3: SandboxType/ToolCategory 扩展

G2: 执行链路串联 (3 tasks)
  AB-T4: ExecutionTarget + 路由决策引擎
  AB-T5: BashTool 沙箱集成
  AB-T6: SkillRuntime 沙箱集成

G3: 可观测性 (2 tasks)
  AB-T7: ToolExecutionRecord 沙箱字段扩展
  AB-T8: TUI StatusBar 沙箱状态 + 错误信息增强

G4: External 沙箱 + CLI (2 tasks)
  AB-T9: ExternalSandboxProvider trait
  AB-T10: CLI 沙箱诊断命令 (dry-run/status/list-backends)
```

### 执行顺序

```
G1 (T1→T2→T3) → G2 (T4→T5→T6) → G3 (T7→T8)
G4 (T9, T10) 可与 G2/G3 并行
```

### 关键代码路径

| 文件 | 作用 |
|------|------|
| `crates/octo-engine/src/sandbox/profile.rs` | SandboxProfile (新建) |
| `crates/octo-engine/src/sandbox/run_mode.rs` | OctoRunMode (新建) |
| `crates/octo-engine/src/sandbox/target.rs` | ExecutionTarget + 路由 (新建) |
| `crates/octo-engine/src/sandbox/external.rs` | External provider trait (新建) |
| `crates/octo-engine/src/sandbox/traits.rs` | SandboxType 扩展 |
| `crates/octo-engine/src/sandbox/router.rs` | ToolCategory 扩展 + 路由矩阵 |
| `crates/octo-engine/src/tools/bash.rs` | BashTool 沙箱集成 |
| `crates/octo-engine/src/skill_runtime/*.rs` | SkillRuntime 沙箱集成 |
| `crates/octo-cli/src/commands/sandbox.rs` | CLI 诊断命令 (新建) |

---

## Deferred 未清项（下次 session 启动时必查）

| 来源 | ID | 内容 | 前置条件 | 状态 |
|------|----|----|---------|------|
| Phase AB | AB-D1 | Octo 沙箱 Docker 镜像 (Dockerfile + CI) | 基础串联完成 | ⏳ |
| Phase AB | AB-D2 | E2B provider 完整实现 | External trait 稳定 | ⏳ |
| Phase AB | AB-D3 | WASM 插件加载框架 | WASM 路由激活 | ⏳ |
| Phase AB | AB-D4 | Session Sandbox 持久化 | BashTool 沙箱集成完成 | ⏳ |
| Phase AB | AB-D5 | CredentialResolver → 沙箱 env 注入 | Z-D1 完成 | ⏳ |
| Phase AA | AA-D1 | `octo auth login/status/logout` CLI 命令 | UX 设计 | ⏳ |
| Phase AA | AA-D3 | XDG Base Directory 支持 | 低优先级 | ⏳ |
| Phase AA | AA-D4 | Config 热重载 | 未来增强 | ⏳ |
| Phase Z | Z-D1 | CredentialResolver → provider chain 对接 | Config 加载稳定后 | 🟡 |
| Phase U | U-D1 | Agent Debug Panel 重设计 | 前置已满足 | 前置已满足 |
| Phase S | S-D1 | Agent Skills 规范研究 | 前置已满足 | 前置已满足 |

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

# 开始实施 Phase AB
/dev-phase-manager:resume-plan
```
