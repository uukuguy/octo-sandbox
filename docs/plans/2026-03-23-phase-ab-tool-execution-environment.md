# Phase AB — 智能体工具执行环境

**日期**: 2026-03-23
**状态**: 设计完成，待实施
**设计文档**: `docs/design/TOOL_EXECUTION_ENVIRONMENT_DESIGN.md`
**基线**: 2394 tests, commit 4d45e9a (main)

---

## 目标

将已建好的沙箱基础设施（SandboxRouter、SandboxPolicy、Docker/WASM/Subprocess 适配器）与实际执行层（BashTool、SkillRuntime）串联，实现统一的工具执行环境。支持两种部署模式（沙箱内运行 / 主机运行），四种沙箱后端（Native/WASM/Docker/External），并确保开发调试零摩擦。

---

## 任务分组

### G1: 基础设施 — Profile + RunMode + 配置 (3 tasks)

**目标**: 建立沙箱环境的配置和检测基础。

#### AB-T1: SandboxProfile 枚举 + 配置解析

**文件**: `crates/octo-engine/src/sandbox/profile.rs` (新建), `crates/octo-server/src/config.rs`

- 定义 `SandboxProfile` 枚举: `Development`, `Staging`, `Production`, `Custom`
- 每个 profile 包含完整的行为参数（policy, default_backend, env_passthrough, approval_gate, timeout, audit_level）
- 在 `config.yaml` 中新增 `sandbox` 配置段
- `SandboxProfile::from_config()` 解析配置
- 配置优先级: `--sandbox-bypass` > `--sandbox-profile` > `OCTO_SANDBOX_PROFILE` > config.yaml > 默认值(development)
- 测试: 4 个 profile 的默认值验证, 配置解析, 优先级覆盖

#### AB-T2: OctoRunMode 检测

**文件**: `crates/octo-engine/src/sandbox/run_mode.rs` (新建)

- 定义 `OctoRunMode` 枚举: `Sandboxed`, `Host`
- `OctoRunMode::detect()` 自动检测:
  - `OCTO_SANDBOXED` 环境变量（显式声明）
  - `/.dockerenv` (Docker)
  - `/run/.containerenv` (Podman)
  - `KUBERNETES_SERVICE_HOST` (K8s)
- 测试: 各检测路径的单元测试（通过临时设置 env var）

#### AB-T3: SandboxType / ToolCategory 扩展

**文件**: `crates/octo-engine/src/sandbox/traits.rs`, `crates/octo-engine/src/sandbox/router.rs`

- `SandboxType` 新增 `External(String)` 变体
- `ToolCategory` 新增 `Script`, `Gpu`, `Untrusted` 变体
- 更新 `SandboxRouter` 的默认路由映射
- 更新 `SandboxPolicy::allows()` 对 External 的处理
- 测试: 新变体的路由映射, Display/Debug/Serialize 验证

---

### G2: 执行链路串联 — BashTool + SkillRuntime (3 tasks)

**目标**: 将沙箱路由实际接入工具执行链路。

#### AB-T4: ExecutionTarget + 路由决策引擎

**文件**: `crates/octo-engine/src/sandbox/target.rs` (新建)

- 定义 `ExecutionTarget` 枚举: `Local`, `Sandbox(SandboxRef)`
- 定义 `SandboxRef` 枚举: `Session { id }`, `Ephemeral { config }`
- `ExecutionTargetResolver` 结构体:
  - 输入: `(ToolCategory, OctoRunMode, SandboxProfile, 可用后端列表)`
  - 输出: `ExecutionTarget`
  - 实现完整的路由决策矩阵（见设计文档第八节）
- 测试: 各种 (category, mode, profile) 组合的路由验证

#### AB-T5: BashTool 沙箱集成

**文件**: `crates/octo-engine/src/tools/bash.rs`

- BashTool 内部增加 `ExecutionTargetResolver` 引用
- `execute()` 方法根据 `ExecutionTarget` 分支:
  - `Local`: 保持现有行为（直接 subprocess）
  - `Sandbox(Session)`: 通过 `SandboxRouter.execute()` 路由到沙箱
- 移除现有的 `#[cfg(feature = "sandbox-wasm")]` 条件编译（统一到 ExecutionTarget 路由）
- 保留 `PASSTHROUGH_ENV_VARS` 在 Development 模式下的行为
- 测试: Development/Staging/Production 下的行为验证, Sandboxed 模式验证

#### AB-T6: SkillRuntime 沙箱集成

**文件**: `crates/octo-engine/src/skill_runtime/shell.rs`, `python.rs`, `nodejs.rs`

- 各 SkillRuntime 实现增加 `ExecutionTarget` 判断:
  - Development / Sandboxed: 保持现有行为（直接执行）
  - Staging / Production: 委托给 `SandboxRouter` 在 Docker 容器中执行
- PythonRuntime: Production 下不创建宿主机 venv，使用容器内 Python
- ShellRuntime: Production 下通过 SandboxRouter 执行
- 测试: 各 runtime 在不同 profile 下的行为验证

---

### G3: 可观测性 — 记录 + 显示 (2 tasks)

**目标**: 让开发者看到沙箱路由决策，方便调试。

#### AB-T7: ToolExecutionRecord 沙箱字段扩展

**文件**: `crates/octo-engine/src/tools/recorder.rs`, `crates/octo-types/src/execution.rs`

- `ToolExecutionRecord` 新增字段:
  - `sandbox_profile: Option<String>`
  - `execution_target: Option<String>` (序列化后的 ExecutionTarget)
  - `actual_backend: Option<String>` (实际使用的 SandboxType)
  - `routing_reason: Option<String>` (路由决策原因)
- 在 BashTool/SkillRuntime 执行后填充这些字段
- 测试: 记录字段的正确填充验证

#### AB-T8: TUI StatusBar 沙箱状态 + 错误信息增强

**文件**: `crates/octo-cli/src/tui/widgets/status_bar.rs`, `crates/octo-engine/src/sandbox/error.rs` (新建)

- StatusBar 新增 sandbox profile 显示（颜色编码: dev=绿, stg=黄, prod=红）
- 沙箱执行失败时的诊断信息增强:
  - 显示失败的沙箱类型和镜像
  - 显示完整 stderr
  - 提供修复建议（切换 profile、安装依赖、构建镜像等）
- 测试: StatusBar 渲染验证, 错误信息格式验证

---

### G4: External 沙箱 + CLI (2 tasks)

**目标**: 第三方沙箱 trait 定义和 CLI 诊断命令。

#### AB-T9: ExternalSandboxProvider trait

**文件**: `crates/octo-engine/src/sandbox/external.rs` (新建)

- 定义 `ExternalSandboxProvider` trait:
  - `name()`, `create()`, `execute()`, `upload()`, `download()`, `destroy()`, `health_check()`
- 定义 `ExternalSandboxConfig`, `ExternalSandboxId`, `ExecRequest` 类型
- `SandboxRouter` 支持注册 External provider
- Stub 实现（E2B provider 结构，不实现实际 API 调用）
- 测试: trait 定义编译验证, stub 基础行为

#### AB-T10: CLI 沙箱诊断命令

**文件**: `crates/octo-cli/src/commands/sandbox.rs` (新建)

- `octo sandbox dry-run --command <CMD>`: 预览命令的沙箱路由结果
- `octo sandbox status`: 显示当前 profile, 可用后端, OctoRunMode
- `octo sandbox list-backends`: 列出已注册的沙箱后端及其可用性
- `octo agent run --sandbox-profile <PROFILE>`: CLI 参数支持
- `octo agent run --sandbox-bypass`: bypass 参数支持
- 测试: 各命令的输出格式验证

---

## Deferred（暂缓项）

| ID | 内容 | 前置条件 |
|----|------|---------|
| AB-D1 | Octo 沙箱 Docker 镜像构建 (Dockerfile + CI) | 基础串联完成 |
| AB-D2 | E2B provider 完整实现（API 调用） | ExternalSandboxProvider trait 稳定 |
| AB-D3 | WASM 插件加载框架（.octo/plugins/*.wasm） | WASM 路由激活 |
| AB-D4 | Session Sandbox 持久化（per-session Docker 容器复用） | BashTool 沙箱集成完成 |
| AB-D5 | CredentialResolver → 沙箱 env 注入 | Z-D1 完成 |
| AB-D6 | gVisor / Firecracker provider 实现 | External trait 稳定 |

---

## 风险与注意事项

1. **两套 RuntimeAdapter trait**: `octo-sandbox` 和 `octo-engine` 各有一套，本阶段不合并，用 `From` 转换
2. **Docker daemon 依赖**: Staging/Production 需要 Docker，CI 环境可能没有 → 测试用 Development profile
3. **WASM 限制**: WASM 无网络、无子进程，只适合纯计算 → 路由矩阵已考虑
4. **环境变量安全**: Production 下 API keys 不应透传 → 依赖 CredentialResolver (AB-D5)
5. **向后兼容**: 所有改动在 BashTool/SkillRuntime 内部，不改变 Tool/SkillRuntime trait 接口

---

## 验证标准

- [ ] `cargo check --workspace` 通过
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过，无回归
- [ ] Development profile 下行为与当前完全一致（零功能回归）
- [ ] `octo sandbox status` 能正确显示 profile 和可用后端
- [ ] `octo sandbox dry-run` 能正确预览路由决策
- [ ] TUI StatusBar 正确显示 sandbox profile

---

## 依赖关系图

```
AB-T1 (Profile)  ──┐
AB-T2 (RunMode)  ──┤
AB-T3 (Type扩展) ──┴── AB-T4 (Target决策) ──┬── AB-T5 (BashTool)
                                             ├── AB-T6 (SkillRuntime)
                                             ├── AB-T7 (Record扩展)
                                             └── AB-T8 (TUI + 错误)
                   AB-T9 (External trait) ── 独立
                   AB-T10 (CLI命令)      ── 依赖 AB-T1, AB-T2, AB-T4
```

**建议执行顺序**:
- G1 (T1→T2→T3) → G2 (T4→T5→T6) → G3 (T7→T8)
- G4 (T9, T10) 可与 G2/G3 并行
