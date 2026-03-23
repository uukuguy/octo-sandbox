# 沙箱执行环境操作指南

## 概述

octo-sandbox 通过两个维度控制工具（bash、file_read 等）的执行位置：

1. **OctoRunMode** — Octo 自身运行在哪里（宿主机 or 容器内）
2. **SandboxProfile** — 工具执行的安全隔离级别

两个维度交叉形成路由决策矩阵，决定每个工具调用最终在哪里执行。

---

## 核心概念

### OctoRunMode（运行模式）

| 模式 | 说明 | 工具路由 |
|------|------|----------|
| **Host** | Octo 运行在宿主机（默认） | 由 SandboxProfile 决定 |
| **Sandboxed** | Octo 运行在容器内部 | 全部本地执行（容器已提供隔离） |

**自动检测优先级**（见 `crates/octo-engine/src/sandbox/run_mode.rs`）：
1. `OCTO_SANDBOXED` 环境变量（最高优先级）
2. `/.dockerenv` 文件（Docker 容器）
3. `/run/.containerenv` 文件（Podman 容器）
4. `KUBERNETES_SERVICE_HOST` 环境变量（K8s Pod）
5. 默认：Host

### SandboxProfile（沙箱配置文件）

| Profile | 工具路由 | 环境变量透传 | 审批门控 | 审计 | 超时 | 适用场景 |
|---------|----------|-------------|---------|------|------|----------|
| **development** | 全部本地 | 完全透传 | 无 | 无 | 120s | 日常开发调试 |
| **staging** | 优先容器，无后端时降级本地 | 受限（不透传 API key） | 破坏性操作需审批 | 警告级 | 60s | 集成测试/预发布验证 |
| **production** | 强制容器隔离 | 受限 | 所有操作需审批 | 完整 | 30s | 生产环境 |

**Profile 解析优先级**（见 `crates/octo-engine/src/sandbox/profile.rs`）：
1. `--sandbox-bypass` CLI 标志 → 强制 development
2. `--sandbox-profile <name>` CLI 参数
3. `OCTO_SANDBOX_PROFILE` 环境变量
4. 配置文件中的 `sandbox.profile`
5. 默认：development

---

## 路由决策矩阵

```
                        ┌─────────────────────────────────────────┐
                        │            SandboxProfile               │
                        ├──────────────┬────────────┬─────────────┤
                        │ development  │  staging   │ production  │
┌───────────┬───────────┼──────────────┼────────────┼─────────────┤
│           │ Sandboxed │   Local      │   Local    │   Local     │
│ RunMode   ├───────────┼──────────────┼────────────┼─────────────┤
│           │ Host      │   Local      │  Container │  Container  │
│           │           │              │ (降级Local) │ (强制,无降级) │
└───────────┴───────────┴──────────────┴────────────┴─────────────┘
```

**关键规则**：
- RunMode=Sandboxed 时，**无论** Profile 是什么，都直接本地执行
- RunMode=Host + Development 时，零摩擦本地执行
- RunMode=Host + Staging 时，优先 Docker，Docker 不可用则降级本地（有审计警告）
- RunMode=Host + Production 时，强制 Docker/WASM/External，不允许降级

---

## 常用操作

### 1. 查看当前环境状态

```bash
# 查看沙箱状态（RunMode、Profile、Policy 等）
make sandbox-status

# 预览所有工具类别的路由决策（不实际执行）
make sandbox-dry-run

# 查看已注册的沙箱后端
make sandbox-backends
```

### 2. 宿主机本地开发（默认，零配置）

```bash
# 默认即为 Host + Development，无需额外配置
make cli-run

# 等价于
OCTO_SANDBOX_PROFILE=dev make cli-run
```

所有工具直接在宿主机执行，无 Docker 开销。

### 3. 切换到容器隔离模式

```bash
# 前提：Docker 已运行，且已构建沙箱镜像
make container-build-dev

# 方式 A：Staging 模式（优先容器，Docker 不可用时降级本地）
make sandbox-staging

# 方式 B：Production 模式（强制容器，Docker 不可用则报错）
make sandbox-production

# 方式 C：手动设置环境变量
OCTO_SANDBOX_PROFILE=staging make cli-run
OCTO_SANDBOX_PROFILE=prod make cli-run
```

### 4. 在容器内运行 Octo（模拟部署环境）

```bash
# 构建 dev 镜像
make container-build-dev

# 进入容器（自动检测为 Sandboxed 模式，所有工具本地执行）
make sandbox-shell

# 容器内运行 octo
octo sandbox status    # 应显示 RunMode: sandboxed
```

### 5. 强制覆盖运行模式

```bash
# 强制 Octo 认为自己在容器内（即使在宿主机上）
OCTO_SANDBOXED=1 make cli-run

# 强制 Octo 认为自己在宿主机上（即使在容器内）
OCTO_SANDBOXED=0 make cli-run

# 绕过沙箱（任何 Profile 强制降为 Development）
# 仅用于紧急调试
octo --sandbox-bypass run
```

---

## 工具类别路由

在 Staging/Production 模式下，不同类别的工具路由到不同的沙箱后端：

| 工具类别 | 默认后端 | 说明 |
|----------|---------|------|
| Shell | Docker | bash、命令执行 |
| FileSystem | Docker | 文件读写 |
| Network | Docker | 网络请求 |
| Script | Docker | Python/Node 脚本 |
| Compute | WASM | 纯计算任务 |
| Gpu | Docker | GPU 计算 |
| Untrusted | Docker | 不可信代码 |

后端优先级回退：Docker → WASM → Subprocess（仅 Staging 允许回退到 Subprocess）

---

## 配置文件方式

除了环境变量，也可以在项目配置文件中设定：

```yaml
# .octo/config.yaml
sandbox:
  profile: staging        # development | staging | production
```

---

## Makefile 命令速查

| 命令 | 说明 |
|------|------|
| `make sandbox-status` | 查看当前沙箱状态 |
| `make sandbox-dry-run` | 预览工具路由决策 |
| `make sandbox-backends` | 列出已注册后端 |
| `make sandbox-dev` | Development 模式运行 CLI |
| `make sandbox-staging` | Staging 模式运行 CLI |
| `make sandbox-production` | Production 模式运行 CLI |
| `make sandbox-shell` | 进入容器内交互式 shell |
| `make container-build` | 构建 base 沙箱镜像 |
| `make container-build-dev` | 构建 dev 沙箱镜像 |
| `make container-list` | 列出镜像和运行中容器 |
| `make container-clean` | 清理停止的容器和镜像 |
| `make container-test` | 构建并验证镜像工具可用 |

---

## 常见问题

### Q: 开发时需要切换到容器模式吗？

不需要。默认的 Development 模式完全够用，所有工具直接本地执行，零开销。只有以下场景需要切换：
- 想验证 agent 在容器隔离环境下的行为
- 测试安全策略（审批门控、环境变量过滤等）
- 模拟生产部署环境

### Q: Staging 和 Production 的区别是什么？

- **Staging**：容器优先但容错——Docker 不可用时降级到本地执行（带审计警告）
- **Production**：容器强制——Docker 不可用直接报错，不允许本地执行

### Q: 如何快速确认当前路由状态？

```bash
make sandbox-dry-run
```

会输出每种工具类别（Shell、FileSystem、Network 等）的路由目标和原因。

### Q: 容器内的 Octo 为什么不需要配置 Profile？

因为 `OctoRunMode::detect()` 会自动检测到容器环境（通过 `/.dockerenv` 等标志），此时无论 Profile 设为什么，都直接本地执行——容器本身已经提供了隔离。

---

## 相关源码

| 文件 | 职责 |
|------|------|
| `crates/octo-engine/src/sandbox/run_mode.rs` | OctoRunMode 定义与自动检测 |
| `crates/octo-engine/src/sandbox/profile.rs` | SandboxProfile 定义与解析 |
| `crates/octo-engine/src/sandbox/target.rs` | ExecutionTargetResolver 路由引擎 |
| `crates/octo-cli/src/commands/sandbox.rs` | CLI `octo sandbox` 诊断命令 |
