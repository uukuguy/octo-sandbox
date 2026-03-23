# Phase AE — Agent Workspace Architecture

**创建时间**: 2026-03-23
**状态**: 待执行
**基线**: 2467 tests @ `73295f5`
**前置**: Phase AD (Container Image Enhancement) COMPLETE

---

## 目标

将 agent 工作目录从隐式 `$PWD` 改为可通过 `--project` 参数显式指定，并清理容器镜像中遗留的 `/workspace/*` 路径，使容器内外路径一致。

### 核心原则

1. **Agent 在项目目录里工作**，不再有独立的 `workspace_dir()`
2. **容器绑定 `$PWD:$PWD`**，容器内外路径一致，BashTool 不需要路径翻译
3. **`--project` 参数**让 CLI 可以指向任意项目目录，不依赖 `cd`

---

## 影响范围

| 文件 | 改动类型 | 说明 |
|------|----------|------|
| `crates/octo-cli/src/lib.rs` | 新增 | `--project` 全局参数 |
| `crates/octo-cli/src/main.rs` | 修改 | 用 `--project` 构建 OctoRoot |
| `crates/octo-engine/src/root.rs` | 修改 | 新增 `with_project_dir()`；删除 `workspace_dir()` |
| `crates/octo-engine/src/sandbox/session_sandbox.rs` | 修改 | `working_dir` 改为动态 |
| `crates/octo-engine/src/security/policy.rs` | 修改 | `workspace_dir` 字段改名 `working_dir` |
| `crates/octo-cli/src/commands/root.rs` | 修改 | 删除 workspace 行 |
| `container/Dockerfile` | 修改 | 删除 `/workspace/*`，WORKDIR 改 `/home/sandbox` |
| `container/Dockerfile.dev` | 修改 | 同上 |
| `examples/demo-project/` | 新建 | 最小测试项目 |
| `Makefile` | 修改 | 添加 `TEST_PROJECT` 变量 |

---

## 任务清单

### G1: `--project` CLI 参数 + OctoRoot 扩展

**AE-T1**: 添加 `--project` 全局 CLI 参数

- 文件: `crates/octo-cli/src/lib.rs`
- 在 `Cli` struct 添加 `--project <PATH>` optional 参数
- 语义: 指定目标项目目录（默认: 当前工作目录）

**AE-T2**: OctoRoot 新增 `with_project_dir(path)` 构造器

- 文件: `crates/octo-engine/src/root.rs`
- 新增 `pub fn with_project_dir(project_dir: impl AsRef<Path>) -> Result<Self>`
- 内部调用 `with_working_dir(project_dir)`，即把 `working_dir` 指向指定路径
- `main.rs` 中: 如果 `--project` 有值则调用 `with_project_dir()`，否则 `discover()`

**验收**: `octo --project /tmp/test root show` 显示 working_dir 为 `/tmp/test`

### G2: 清理 `workspace_dir()` + SecurityPolicy 改名

**AE-T3**: 删除 `OctoRoot::workspace_dir()` 方法

- 文件: `crates/octo-engine/src/root.rs`
- 删除 `workspace_dir()` 方法
- 从 `ensure_dirs()` 中移除 `workspace_dir` 的目录创建
- 更新 `octo root show` 输出（`crates/octo-cli/src/commands/root.rs`）

**AE-T4**: SecurityPolicy `workspace_dir` → `working_dir` 改名

- 文件: `crates/octo-engine/src/security/policy.rs`
- 字段 `workspace_dir: PathBuf` → `working_dir: PathBuf`
- 方法 `.with_workspace()` → `.with_working_dir()`
- 更新所有调用点（`runtime.rs` 等）

**验收**: `cargo check --workspace` 通过；`cargo test --workspace -- --test-threads=1` 全绿

### G3: Dockerfile 清理

**AE-T5**: 清理容器 `/workspace/*` 路径

- 文件: `container/Dockerfile`
  - 删除 `mkdir -p /workspace/project /workspace/session`
  - 删除 `chown -R sandbox:sandbox /workspace/session`
  - `WORKDIR /home/sandbox`
- 文件: `container/Dockerfile.dev`
  - `WORKDIR /home/sandbox`
- 文件: `crates/octo-engine/src/sandbox/session_sandbox.rs`
  - `SessionSandboxConfig::default()` 中 `working_dir` 改为动态（从调用方传入），默认 fallback 为 `/home/sandbox`

**验收**: `make container-build` 成功；容器启动后 `pwd` 输出 `/home/sandbox`

### G4: Demo 项目 + Makefile

**AE-T6**: 创建 `examples/demo-project/`

- 文件结构:
  ```
  examples/demo-project/
  ├── .octo/
  │   └── config.yaml    # 最小配置
  ├── src/
  │   └── main.py        # 简单 Python 文件
  └── README.md           # 一行说明
  ```
- 用途: CLI 测试的固定目标项目

**AE-T7**: Makefile 添加 `TEST_PROJECT` 变量

- 文件: `Makefile`
- 新增 `TEST_PROJECT ?= $(PWD)/examples/demo-project`
- 修改 `cli-*` targets 使用 `--project $(TEST_PROJECT)`（可选，不破坏默认行为）

**验收**: `make cli-config TEST_PROJECT=examples/demo-project` 能正确加载 demo 项目配置

---

## Deferred Items

| ID | 描述 | 原因 |
|----|------|------|
| AE-D1 | Container bind mount `$PWD:$PWD` 实际实现 | `SessionSandboxManager` 未 wired（AC-T7 = `None`） |
| AE-D2 | `octo init` 在 `--project` 目标目录创建 `.octo/` | 需 init 命令重构 | ✅ 已补 — `--project` 参数已在 AE-T1 实现，init 自动尊重 |
| AE-D3 | 容器内 OctoRoot 自动检测 global root | 需容器环境变量注入 | ✅ 已补 — Dockerfile 添加 `ENV OCTO_GLOBAL_ROOT` + `OCTO_SANDBOXED=1` |

---

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 删除 `workspace_dir()` 破坏现有测试 | Explore 确认只有 `root show` 和 `ensure_dirs` 使用，影响可控 |
| `--project` 路径不存在 | CLI 层 canonicalize + 存在性校验 |
| Dockerfile WORKDIR 改动影响 CI | CI workflow 尚未使用容器运行测试 |
| SecurityPolicy 改名影响编译 | 全局 rename，`cargo check` 立即验证 |

---

## 执行顺序

```
G1 (AE-T1, AE-T2) ─── CLI + OctoRoot
        │
        ├── G2 (AE-T3, AE-T4) ─── 清理 workspace_dir + rename
        │
        └── G4 (AE-T6, AE-T7) ─── demo 项目 + Makefile

G3 (AE-T5) ─── Dockerfile 清理（可与 G1 并行）
```

G1 是核心依赖，G2 和 G4 依赖 G1，G3 可独立并行。
