# R4: HexAgent Computer 协议评估报告

> **评估日期**: 2026-04-12
> **源码路径**: `3th-party/eaasp-runtimes/hexagent/`
> **仓库来源**: `github.com/an7tang/hexagent` (MIT)
> **语言/框架**: Python 3.11+, LangChain/LangGraph, Tokio-style async
> **状态**: Pre-Experimental (0.0.x)

---

## 1. 项目概况

HexAgent 是一个开源 **agent harness**（非 framework），核心理念是 **"give any LLM a computer"**。其独特价值在于将 agent runtime 和 agent 的执行环境（computer）**物理分离**，通过 `Computer` Protocol 解耦。

**关键定位语**（来自 README）：

> Unlike every other agent framework, HexAgent **separates the agent runtime from the computer it operates on**. Your agent gets a sandboxed machine; your runtime keeps its API keys, config, and source code private.

**项目规模**：
- `libs/hexagent/` — 核心框架，~3500 行 `computer/` 模块
- `libs/hexagent_demo/` — Chat + Cowork 示例应用
- 总计约 90+ Python 源文件

**自我定位对比**：

| 维度 | Framework (LangChain) | Runtime (LangGraph) | Harness (HexAgent) |
|------|----------------------|---------------------|---------------------|
| 提供什么 | 积木 | 调度引擎 | 完整操作系统 |
| 用户做什么 | 从零组装 | 写编排逻辑 | 告诉 agent 做什么 |

---

## 2. Computer 协议分析

### 2.1 协议定义

Computer 协议定义在 `libs/hexagent/hexagent/computer/base.py`，使用 Python `typing.Protocol`（runtime_checkable）：

```python
# base.py:72-118
@runtime_checkable
class Computer(Protocol):
    """Protocol for Computer implementations.
    A Computer runs CLI commands. Each command is transient - no state persists.
    """

    @property
    def is_running(self) -> bool: ...

    async def start(self) -> None: ...

    async def run(self, command: str, *, timeout: float | None = None) -> CLIResult: ...

    async def upload(self, src: str, dst: str) -> None: ...

    async def download(self, src: str, dst: str) -> None: ...

    async def stop(self) -> None: ...
```

**协议表面积**：仅 **6 个方法**，极度精简：

| 方法 | 语义 | 方向 |
|------|------|------|
| `is_running` | 健康检查 (property) | Computer → Harness |
| `start()` | 启动/恢复，幂等 | Harness → Computer |
| `run(command, timeout)` | 执行 shell 命令 | Harness → Computer |
| `upload(src, dst)` | 文件传入（host→computer） | Harness → Computer |
| `download(src, dst)` | 文件传出（computer→host） | Computer → Harness |
| `stop()` | 停止/暂停，幂等 | Harness → Computer |

**通信载体**：`CLIResult` 数据类（stdout/stderr/exit_code/metadata）。**不是** protobuf、gRPC 或 JSON-RPC——是纯 Python async 方法调用。协议是**进程内接口**，远程化通过实现类内部完成。

### 2.2 协议设计哲学

1. **以终端为通用接口**：所有工具通过 shell 命令执行，不定义工具专有 RPC。这和 EAASP 的 `RuntimeAdapter::execute(cmd, working_dir)` 理念一致。

2. **无状态命令模型**：每条命令是独立子进程，不共享状态。文档明确写 "Each command is transient - no state persists."

3. **文件传输是一等公民**：`upload/download` 被提升为协议方法（不是 run 的变体），因为跨信任域的文件搬运**需要不同的实现路径**（limactl copy、E2B files.write 等）。

4. **生命周期管理**：`start/stop` 配合 `AsyncComputerMixin` 提供 `async with` 上下文管理。

### 2.3 辅助抽象

**Mount**（`base.py:28-62`）：声明式挂载描述，将主机目录映射到 Computer 内的 guest 路径：

```python
@dataclass(frozen=True)
class Mount:
    source: str      # 主机绝对路径
    target: str      # Guest 路径（相对 → /mnt/ 下，绝对 → 精确路径）
    writable: bool = False
```

Mount 机制实现了**数据平面和控制平面分离**：harness 决定哪些目录可见、是否可写，Computer 实现负责具体的挂载技术（Lima mount、WSL bind mount、E2B files.write）。

---

## 3. 架构详解

### 3.1 三种 Computer 实现

| 实现 | 文件 | 隔离级别 | 底层技术 | 代码行数 |
|------|------|---------|---------|---------|
| `LocalNativeComputer` | `local/native.py` | 无（同进程 shell） | `asyncio.create_subprocess_shell` | 142 |
| `LocalVM` (macOS) | `local/vm.py` + `_lima.py` | Linux VM 内 Linux user 隔离 | Lima (macOS) / WSL (Windows) | 567 + 421 |
| `LocalVM` (Windows) | `local/vm_win.py` + `_wsl.py` | WSL2 distro 内 Linux user 隔离 | WSL2 | 657 + 896 |
| `RemoteE2BComputer` | `remote/e2b.py` | 完全隔离（云端容器） | E2B Sandbox API | 494 |

#### LocalNativeComputer — T3 级别（开发用）

最简实现。每条命令 `asyncio.create_subprocess_shell`，upload/download 是 `shutil.copy2`。**无任何隔离**，等同于 EAASP 的 Subprocess sandbox。

关键实现细节：
- `start_new_session=True` — 避免 SIGINT 泄漏到子进程
- `NO_COLOR=1` — 关闭 ANSI escape for agent 友好
- 超时后 SIGTERM → 5s → SIGKILL（两阶段杀进程组）

#### LocalVM — T0 的核心实现

这是 HexAgent 最有价值的创新。以 macOS Lima 为例：

**分层设计**：

```
LocalVM (session 管理, mount 解析)
  └── LimaVM (limactl 命令调用, lima.yaml 操作)
        └── limactl (macOS Lima 守护进程)
              └── Linux VM (QEMU/VZ framework)
                    └── Session users (useradd 隔离)
```

**会话隔离模型**（`vm.py:400-446`）：

```python
async def computer(self, *, mounts=None, resume=None) -> _VMSessionComputer:
    # 1. 生成唯一 petname（3词，10字母上限）
    name = await self._generate_unique_name()
    # 2. 在 VM 内创建 Linux user
    await self._create_user(name)
    # 3. 如有 mount，绑定到 /sessions/{name}/mnt/
    if mounts:
        await self.mount(mounts, session=name)
    # 4. 返回绑定到该 user 的 Computer handle
    return _VMSessionComputer(vm=self._vm, session_name=name)
```

每个会话是一个 **Linux 用户**，拥有独立的：
- 主目录 `/sessions/{name}/`
- 标准子目录：`tmp/`, `mnt/`, `mnt/outputs/`, `mnt/uploads/`
- 文件系统权限隔离（非 root）
- 挂载点隔离（session-scoped mounts vs system-scoped mounts）

**命令执行路径**（`_lima.py:278-351`）：

```
Harness: computer.run("ls -la")
  → _VMSessionComputer.run()
    → LimaVM.shell(command, user=session_name)
      → limactl shell --workdir / {instance} bash -c "sudo -u {user} -H bash -l -c 'cd && {command}'"
```

每条命令通过 `limactl shell` 进入 VM，以 `sudo -u` 切换到会话用户执行。

**挂载管理**（`vm.py:255-320, _lima.py:96-161`）：
- lima.yaml 是 **单一事实来源**（不做内存缓存）
- 支持 `defer=True`（批量修改，一次重启）
- 冲突检测：同一 guest path 不同配置 → `VMMountConflictError`
- 幂等性：已存在的 mount 静默跳过

#### RemoteE2BComputer — 云端隔离

特色功能：
- **Auto-pause/resume**：sandbox 接近超时时自动暂停（不销毁），保存完整状态（文件、包、环境变量），可跨进程重连
- **Sandbox ID persistence**：可序列化 sandbox_id，后续会话重连
- **Timer management**：检测剩余时间 < 命令超时 + 60s buffer 时自动延期或 pause-resume 重置 E2B 1小时硬限制

### 3.2 Harness 层架构

```
Agent (langchain/agent.py)
  ├── AgentContext (model, tools, skills, mcps, environment, agents)
  ├── AgentMiddleware (compaction, permissions, reminders, image adaptation)
  ├── Tools (12+ built-in)
  │   ├── CLI tools: Bash, Read, Write, Edit, Glob, Grep
  │   ├── Web tools: WebSearch, WebFetch
  │   ├── Task tools: Agent (subagent), TaskOutput, TaskStop
  │   ├── SkillTool, TodoWriteTool
  │   └── MCP tools (动态发现)
  ├── PermissionGate (SafetyRule chain)
  ├── SkillResolver (filesystem discovery)
  ├── EnvironmentResolver (via Computer)
  └── Prompts (35+ Markdown fragments, variable substitution)
```

**关键通路**：所有 CLI 工具（Bash/Read/Write/Edit/Glob/Grep）持有 `Computer` 引用，通过 `computer.run()` 执行。

```python
# tools/cli/bash.py:69
result: CLIResult = await self._computer.run(params.command, timeout=timeout)
```

**EnvironmentResolver**（`harness/environment.py`）通过 Computer 做环境探测——单条批量 shell 命令收集 pwd、git status、uname、shell、日期。这个设计意味着**所有环境感知都来自 Computer 侧**，而不是 harness 侧。

### 3.3 权限和安全模型

**PermissionGate**（`harness/permission.py`）：

```python
class SafetyRule(ABC):
    def check(self, tool_name: str, tool_args: dict) -> PermissionDecision | None:
        # ALLOWED / DENIED / NEEDS_APPROVAL
```

- 基于 tool_name + tool_args 的 **PreToolUse** 拦截
- 支持 human-in-the-loop 审批流
- 但**无 PostToolUse hook**——是单向 gate，不是双向 hook

这与 EAASP 的 `PreToolUse/PostToolUse/Stop` 三点 hook 相比有差距。HexAgent 的 `SafetyRule` 只覆盖了 EAASP `PreToolUse` 的 `Allow/Deny` 部分，没有 `Modify` 语义（修改工具参数后放行），也没有 PostToolUse。

---

## 4. 与 EAASP SANDBOX_EXECUTION_DESIGN 对比

### 4.1 接口对比

| 维度 | HexAgent `Computer` | EAASP `RuntimeAdapter` |
|------|---------------------|----------------------|
| **定义方式** | Python `Protocol` | Rust `#[async_trait]` |
| **核心方法** | `run(command, timeout)` → `CLIResult` | `execute(cmd, working_dir)` → `Result<ExecResult>` |
| **文件传输** | `upload/download` 一等方法 | 无（通过 mount/bind 解决） |
| **生命周期** | `start/stop/is_running` | 无（由 SandboxManager 外部管理） |
| **Working dir** | 调用侧不指定（会话 home 隐含） | 参数传入 |
| **超时** | 协议级 `timeout` 参数 | 实现级 |
| **挂载** | `Mount` dataclass + `LocalVM.mount()` | Docker volume 绑定 |
| **结果类型** | `CLIResult(stdout, stderr, exit_code, metadata)` | `ExecResult` (类似) |

### 4.2 隔离能力对比

| 维度 | HexAgent | EAASP (SANDBOX_SECURITY_DESIGN) |
|------|----------|-------------------------------|
| **无隔离** | `LocalNativeComputer` | Subprocess (Development policy) |
| **进程级** | - | Subprocess with env_clear |
| **VM/用户级** | `LocalVM` (Lima/WSL user isolation) | - |
| **容器级** | - | Docker (Strict policy) |
| **WASM级** | - | WASM/WASI Runtime |
| **云端沙箱** | `RemoteE2BComputer` | - (External sandbox 接口) |
| **网络隔离** | Lima 可配 | Docker network=none |
| **资源限制** | Lima 可配 (CPU/Memory) | Docker cgroup limits |
| **审计日志** | 无 | SHA-256 hash-chain |
| **策略分级** | 无（隐含在选择哪种 Computer） | `SandboxPolicy::Strict/Preferred/Development` |

### 4.3 核心差异

1. **HexAgent 是协议层设计**，EAASP 是**策略层设计**：
   - HexAgent 说"这是接口，你选哪种 Computer"
   - EAASP 说"这是策略，生产必须用 Docker/WASM，开发才允许 Subprocess"

2. **HexAgent 有文件传输原语**，EAASP 没有：
   - `upload/download` 解决了跨信任域的文件搬运问题
   - EAASP 依赖 Docker bind mount 或 WASM 虚拟文件系统

3. **HexAgent 无审计日志**，EAASP 有 SHA-256 hash-chain

4. **HexAgent 的 VM 隔离方案（Lima/WSL + user 隔离）在 EAASP 中无对应物**——这是一个有价值的中间地带：比 Subprocess 安全，比 Docker 轻量

---

## 5. T0 L1 Runtime 可行性

### 5.1 T0 特征验证

回到 `L1_RUNTIME_STRATEGY.md` 中 T0 的定义：

| T0 判别特征 | HexAgent 证据 |
|---|---|
| harness 和 tools 在不同进程/容器/机器 | **完全满足**：`LocalVM` — tools 在 Lima VM 里，harness 在主机；`RemoteE2BComputer` — tools 在 E2B 云端 |
| 协议层是解耦关键 | **完全满足**：`Computer` Protocol 6 方法即全部耦合面 |
| tools 容器可替换不影响 harness | **完全满足**：换 `LocalNativeComputer` → `LocalVM` → `RemoteE2BComputer` 只改一行 `computer=` 参数 |

**结论**：HexAgent 是 **T0 的最佳开源实证**，这一判断在源码层面完全成立。

### 5.2 包装为 EAASP L1 Runtime 的可行性

如果要将 HexAgent 包装为 EAASP L1 Runtime，需要：

**已有的（可直接映射）**：

| EAASP 需求 | HexAgent 对应 |
|---|---|
| `RuntimeAdapter::execute()` | `Computer.run()` |
| MCP Client | `hexagent.mcp._connector.McpConnector` — 支持 stdio/SSE/HTTP 三种 transport |
| Skills | `hexagent.harness.skills.SkillResolver` — filesystem-based SKILL.md discovery |
| PreToolUse Hook | `hexagent.harness.permission.PermissionGate` + `SafetyRule` chain |
| Session 管理 | `LocalVM.computer()` 返回会话级 Computer handle |
| 环境探测 | `EnvironmentResolver` via Computer |

**缺失的（需 adapter 补齐）**：

| EAASP 需求 | 缺失程度 | Adapter 工作 |
|---|---|---|
| **PostToolUse Hook** | 完全缺失 | 在 `AgentMiddleware` 工具调用后添加 hook 点 |
| **Stop Hook** | 完全缺失 | 在 agent 结束时添加 hook 点 |
| **Hook Modify 语义** | 缺失 | `SafetyRule.check()` 当前返回 Allow/Deny/NeedsApproval，需扩展为 Allow/Deny/Modify |
| **gRPC 服务层** | 完全缺失 | 需包装 16 方法 gRPC service（类似 claude-code-runtime） |
| **SessionPayload 5-block** | 不对齐 | 需映射 `managed_settings`/`managed_hooks`/`mcp_servers`/`skill_refs`/`policy_context` |
| **Skill v2 扩展字段** | 部分缺失 | `runtime_affinity`/`access_scope`/`scoped_hooks`/`dependencies` 需适配 |
| **Telemetry** | 无 | 需对接 EAASP L3 遥测上报 |
| **审计日志** | 无 | 需对接 EAASP 审计通路 |

**Adapter 厚度估计**：

- 如果作为 **T0 adapter**（保留 Computer 协议作为核心价值，重点展示 harness-tools 分离）：**5-8 天**
- 如果作为 **T2 adapter**（忽略 Computer 协议，把 HexAgent 当普通 agent 框架接）：**3-5 天**（但失去 T0 独特价值）

### 5.3 推荐路径

**推荐作为 T0 代表**，因为：

1. EAASP 当前 T0 为空——`L1_RUNTIME_STRATEGY.md` 明确写"T0 未交付，Phase 0 明确不做"
2. HexAgent 的 Computer 协议是**唯一开源的、production-quality 的** T0 实现
3. HexAgent 已经有 3 种 Computer 实现覆盖开发/测试/生产全场景
4. 与 EAASP 现有 `grid-engine/src/sandbox/` 的 `external` adapter 理念一致

---

## 6. 对 EAASP 的关键贡献

### 6.1 Computer 协议对跨信任域部署的价值

HexAgent 的 Computer 协议直接解决了 EAASP 在 `L1_RUNTIME_STRATEGY.md` T0 定义中描述的核心场景：

> 支持"agent 在云端，tools 在客户侧内网"这类跨信任域部署

具体价值点：

1. **文件传输原语**（upload/download）：EAASP 现有 `RuntimeAdapter` 没有文件传输方法，跨信任域部署时无法搬运数据。这是 Computer 协议的明确改进。

2. **生命周期管理**（start/stop）：远程 sandbox 需要显式生命周期控制（创建、暂停、恢复、销毁），EAASP `RuntimeAdapter` 没有这些语义。

3. **挂载声明式 API**（Mount）：声明 source/target/writable 三元组，实现类决定具体挂载技术——这比 EAASP Docker 适配器中直接写 bind mount 参数更清晰。

### 6.2 可移植的设计理念

| 理念 | HexAgent 实现 | EAASP 可吸收方式 |
|------|-------------|----------------|
| **Protocol-based 多态** | `Computer` Protocol + 3 实现 | 扩展 `RuntimeAdapter` trait 加入 upload/download/start/stop |
| **会话级用户隔离** | Linux useradd + session dirs | 可用于 Docker 内的多租户隔离 |
| **Auto-pause/resume** | E2B sandbox 状态保持 | 有助于 EAASP 的 session 持久化设计 |
| **环境探测通过 Computer** | `EnvironmentResolver` via `computer.run()` | 当前 EAASP 假设环境信息在 harness 侧，跨信任域需改 |
| **Mount 冲突检测** | `VMMountConflictError` | EAASP Docker bind mount 缺乏冲突检测 |

### 6.3 可移植的代码

以下 HexAgent 代码对 EAASP 有直接参考价值：

- `computer/base.py` (137行) — Protocol 定义，可作为 EAASP `RuntimeAdapter` v2 的设计参考
- `computer/remote/e2b.py` (494行) — E2B 集成的完整实现，包括 auto-pause/resume/reconnect，可移植为 EAASP 的 RemoteSandbox adapter
- `harness/environment.py` (142行) — 通过 shell 命令批量探测环境信息的模式，可用于 EAASP 跨信任域场景的环境感知

---

## 7. 风险和限制

### 7.1 技术风险

1. **Python 限制**：HexAgent 是纯 Python 项目，与 EAASP 的 Rust 核心（grid-engine）不同语言栈。如果做 T0 adapter，需要像 claude-code-runtime 一样作为独立 Python 进程通过 gRPC 通信。

2. **LangChain 耦合**：虽然 `computer/` 和 `harness/` 模块是框架无关的，但 `langchain/agent.py` 中的 `create_agent()` 深度依赖 LangChain/LangGraph。作为 EAASP L1 Runtime 时需要重写 agent loop。

3. **无 PostToolUse/Stop hook**：HexAgent 的安全模型是单向 gate（PreToolUse only），不满足 EAASP 完整 hook 需求。需要扩展。

4. **状态**: Pre-Experimental (0.0.x)，API 可能随时变化，明确声明"Backward compatibility is not a concern."

### 7.2 架构限制

1. **Computer 协议是进程内 Python Protocol**：不是跨进程 RPC 协议。虽然 `RemoteE2BComputer` 在实现内部做了远程调用（E2B HTTP API），但协议本身是同进程的。真正的跨信任域部署需要将 Computer 协议 RPC 化（gRPC/HTTP）。

2. **无容器级隔离**：HexAgent 没有 Docker Computer 实现。从 LocalNativeComputer 直接跳到 LocalVM（Lima/WSL），缺少 Docker 中间层。EAASP 的 Docker sandbox 填补了这个空缺。

3. **单租户设计**：LocalVM 的会话隔离是 Linux user 级别，但仍然共享 VM 资源（CPU/Memory/Network）。没有硬性资源配额，不适合多租户生产。

### 7.3 评估局限

- 本次评估基于源码静态分析，未运行测试或实际部署
- 未深入分析 `libs/hexagent_demo/` 示例应用
- 未评估 WSL 后端（`_wsl.py` 896行）的完整度

---

## 8. 结论

### 核心判断

**HexAgent 确认为 T0 tier 的最佳开源实证**，源码完全验证了 `L1_RUNTIME_STRATEGY.md` 中的预判。

### Computer 协议评价

| 维度 | 评分 | 说明 |
|------|------|------|
| 设计简洁性 | ★★★★★ | 6 方法即全部接口，零冗余 |
| 隔离能力 | ★★★★☆ | 3 级梯度（Native/VM/Cloud），缺 Docker |
| 跨信任域适配 | ★★★★☆ | upload/download 是亮点，但协议本身不跨进程 |
| 安全模型 | ★★☆☆☆ | 仅 PreToolUse gate，无审计、无策略分级 |
| 生产就绪度 | ★★☆☆☆ | 0.0.x，显式声明不保证兼容性 |
| EAASP 对齐度 | ★★★☆☆ | MCP+Skills 可用，Hooks 缺 Post+Stop |

### 行动建议

1. **Phase 1 纳入 T0 代表**：在 L1 Runtime Pool 中正式登记 HexAgent 为 T0 tier 唯一代表
2. **移植 Computer Protocol 理念**：扩展 EAASP `RuntimeAdapter` trait 加入 `upload/download/start/stop` 方法，为跨信任域部署做准备
3. **不急于做完整 adapter**：Phase 1 先做 PoC 级集成（Computer + gRPC wrapper），验证 T0 模式在 EAASP 中的可行性
4. **关注 E2B 集成**：`RemoteE2BComputer` 的 auto-pause/resume 模式对 EAASP 的 session 持久化有直接参考价值

### Deferred Items

| ID | 内容 | 触发条件 |
|---|---|---|
| R4-D1 | HexAgent adapter PoC（Computer + gRPC 16-method wrapper） | Phase 1 启动 |
| R4-D2 | RuntimeAdapter v2 设计（加入 upload/download/lifecycle） | Phase 1 T0 工作线 |
| R4-D3 | Docker Computer 实现（填补 HexAgent 的 Docker gap） | 如果选择 HexAgent 作为 T0 基座 |
| R4-D4 | Computer Protocol RPC 化（gRPC/HTTP 跨进程版） | 跨信任域部署需求确认 |
| R4-D5 | PostToolUse/Stop hook 扩展 HexAgent PermissionGate | adapter 实施时 |
