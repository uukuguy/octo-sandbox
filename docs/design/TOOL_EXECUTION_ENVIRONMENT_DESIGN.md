# 智能体工具执行环境设计方案

**版本**: v1.0
**创建日期**: 2026-03-23
**阶段**: Phase AB — 设计阶段
**前置**: `SANDBOX_SECURITY_DESIGN.md` v2.0 (Phase J)
**状态**: 设计完成，待实施

---

## 一、设计背景与目标

### 1.1 问题陈述

当前 Octo 的沙箱基础设施（SandboxRouter、SandboxPolicy、Docker/WASM/Subprocess 适配器）已在 Phase J 完成，但存在一个核心矛盾：

**执行层（BashTool、SkillRuntime）没有接入沙箱基础设施**。

```
已建好:
  SecurityPolicy       →  命令风险评估 + 审批门控
  SandboxRouter        →  按 ToolCategory 路由到 Docker/WASM/Subprocess
  SandboxPolicy        →  Strict/Preferred/Development 三级策略

未串联:
  BashTool            →  直接 subprocess 执行，绕过 SandboxRouter
  ShellRuntime        →  直接 bash -c 执行，无沙箱
  PythonRuntime       →  宿主机 venv 执行，无隔离
  NodeJsRuntime       →  宿主机 node 执行，无隔离
```

### 1.2 两种部署模式

Octo 存在两种截然不同的部署场景，执行环境设计必须同时覆盖：

```
模式 A: Octo 在沙箱中运行              模式 B: Octo 在主机运行
┌─────────────────────┐              ┌─────────────────────┐
│     沙箱容器          │              │      主机            │
│  ┌───────────────┐  │              │  ┌───────────────┐  │
│  │  Octo Agent   │  │              │  │  Octo Agent   │  │
│  │  + 工具执行    │  │              │  └───────┬───────┘  │
│  │  + 文件读写    │  │              │          │ 工具调用   │
│  └───────────────┘  │              │          ▼          │
│  已隔离，天然安全     │              │  ┌───────────────┐  │
└─────────────────────┘              │  │   沙箱容器     │  │
                                     │  │  执行工具调用   │  │
                                     │  └───────────────┘  │
                                     └─────────────────────┘
```

**模式 A（沙箱内运行）**：类似 Devin、OpenHands — Agent 整体在 Docker/VM 中运行，所有工具调用天然隔离，不需要额外沙箱。

**模式 B（主机运行）**：类似 Claude Code、Cursor — Agent 在用户机器上运行，工具调用需要路由到沙箱执行。

### 1.3 设计目标

1. **统一执行环境抽象** — 无论 Octo 在哪里运行，工具调用通过统一接口执行
2. **多沙箱后端支持** — Native/WASM/Docker/External（第三方），按场景选择
3. **开发调试零摩擦** — 开发模式下直接执行，无沙箱启停开销
4. **生产安全强隔离** — 生产模式下强制沙箱隔离，不可绕过
5. **向后兼容** — 不改变 Tool trait 和 SkillRuntime trait 的接口

---

## 二、沙箱类型谱系

### 2.1 四种沙箱后端

```
┌─────────────────────────────────────────────────────────────┐
│                    SandboxBackend                           │
│                                                             │
│  ┌─────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Native  │  │   WASM   │  │  Docker  │  │  External  │  │
│  │ (进程)  │  │(Wasmtime)│  │(Bollard) │  │  (第三方)   │  │
│  └────┬────┘  └────┬─────┘  └────┬─────┘  └─────┬──────┘  │
│       │            │             │               │          │
│  直接执行     Rust 原生     容器隔离       API/SDK 调用    │
│  env_clear   内存安全       进程+网络      远程执行环境      │
│  最快        次快           较慢           取决于网络       │
│  最弱隔离    强隔离(计算)   最强本地隔离    最强隔离         │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 各后端特性对比

| 属性 | Native (Subprocess) | WASM (Wasmtime) | Docker (Bollard) | External (第三方) |
|------|-------------------|-----------------|-----------------|-----------------|
| **启动时间** | ~0ms | ~10ms | ~500ms | ~1-5s |
| **隔离级别** | 弱（进程级） | 强（内存安全） | 最强（内核级） | 最强（物理隔离） |
| **文件系统** | 共享宿主机 | 无（WASI 预映射） | 独立（volume mount） | 独立（API 传输） |
| **网络** | 共享宿主机 | 无 | 可控（Docker network） | 可控（云 VPC） |
| **子进程** | 可以 | 不可以 | 可以 | 可以 |
| **多语言** | 依赖宿主机 | 需预编译 WASM | 依赖镜像 | 依赖平台 |
| **依赖** | 无 | wasmtime crate | Docker daemon | 网络 + API key |
| **适用场景** | 开发调试、只读工具 | 纯计算、数据处理、插件 | 脚本执行、完整环境 | 不信任代码、GPU、企业合规 |
| **现有实现** | `SubprocessAdapter` | `WasmAdapter` | `DockerAdapter` | 待实现 |

### 2.3 WASM 沙箱的发力点

Octo 作为 Rust 项目，WASM 是天然优势。Wasmtime 已集成（feature-gated）。

**WASM 能执行的任务**:

```
✅ 纯计算:
   - JSON 解析/转换/查询 (jq 等)
   - 正则匹配/文本处理
   - 数学计算、数据格式转换
   - 哈希/编码/解码

✅ WASI CLI 程序 (已实现 execute_wasi_cli):
   - stdin/stdout/stderr 支持
   - 命令行参数
   - 有限文件系统访问 (预映射目录)

❌ 不适合:
   - 网络请求 (WASI preview1 无网络)
   - 子进程调用 (git, pip, npm)
   - 完整 OS 环境需求
   - Python/Node 脚本直接执行
```

**WASM 插件生态设想**:

```
.octo/plugins/
├── jq.wasm          ← JSON 查询工具
├── csvkit.wasm      ← CSV 处理
├── semgrep.wasm     ← 代码静态分析
├── markdown.wasm    ← Markdown 渲染
└── calculator.wasm  ← 表达式计算

执行方式:
  WasmAdapter.execute_wasi_cli(
    id, &wasm_bytes,
    &["--query", ".items[].name"],
    Some(json_data)  // stdin
  )
```

### 2.4 第三方沙箱服务

| 服务 | 类型 | 特点 | API 模式 |
|------|------|------|---------|
| **E2B** | 云沙箱 | 按需 VM，多语言，持久文件系统 | REST API + WebSocket |
| **Modal** | 云函数 | Python 为主，GPU 支持，按秒计费 | Python SDK |
| **Daytona** | 开发环境 | 完整 IDE 环境，devcontainer 支持 | REST API |
| **Firecracker** | microVM | 亚秒启动，内核级隔离 | REST API (本地) |
| **gVisor (runsc)** | 用户态内核 | Docker 兼容，无需完整 VM | Docker runtime |

---

## 三、核心架构设计

### 3.1 OctoRunMode — 运行模式检测

```rust
/// Octo 的运行模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctoRunMode {
    /// 在沙箱容器中运行（整个 Octo 已隔离）
    Sandboxed,
    /// 在主机运行（工具调用需要沙箱）
    Host,
}

impl OctoRunMode {
    /// 自动检测运行环境
    pub fn detect() -> Self {
        // 显式声明优先
        if std::env::var("OCTO_SANDBOXED").is_ok() {
            return OctoRunMode::Sandboxed;
        }
        // 容器环境检测
        if Path::new("/.dockerenv").exists()            // Docker
            || Path::new("/run/.containerenv").exists()  // Podman
            || std::env::var("KUBERNETES_SERVICE_HOST").is_ok() // K8s
        {
            return OctoRunMode::Sandboxed;
        }
        OctoRunMode::Host
    }
}
```

**行为差异**:

| 行为 | Sandboxed 模式 | Host 模式 |
|------|---------------|----------|
| 工具执行 | 全部 Local（已隔离） | 按路由决策 |
| 文件操作 | 直接操作（容器内） | 直接操作（宿主机） |
| 环境变量 | 容器注入 | 白名单透传 |
| 安全策略 | 容器边界保障 | SecurityPolicy + SandboxRouter |

### 3.2 ExecutionTarget — 执行目标

```rust
/// 工具执行目标
#[derive(Debug, Clone)]
pub enum ExecutionTarget {
    /// 本地直接执行（Octo 已在沙箱中，或 builtin 安全工具）
    Local,
    /// 在指定沙箱中执行
    Sandbox(SandboxRef),
}

/// 沙箱引用
#[derive(Debug, Clone)]
pub enum SandboxRef {
    /// 持久沙箱 — session 生命周期，工具调用复用
    Session { id: SandboxId },
    /// 临时沙箱 — 单次调用后销毁
    Ephemeral { config: SandboxConfig },
}
```

### 3.3 执行路由决策

```
OctoRunMode 检测
  │
  ├── Sandboxed (模式 A)
  │     → 所有工具 → ExecutionTarget::Local
  │     → 不需要额外沙箱（已隔离）
  │
  └── Host (模式 B)
        → 按 (工具类型, SandboxProfile, 可用后端) 决策:
            │
            ├── builtin 安全工具 (grep/glob/file_read/file_write)
            │     → ExecutionTarget::Local（Rust 实现，无风险）
            │
            ├── bash / shell 命令
            │     → Development: Local
            │     → Staging:     Docker → fallback Subprocess
            │     → Production:  Docker / External（必须隔离）
            │
            ├── skill 脚本 (Python/Node/Shell)
            │     → Development: Local（宿主机解释器）
            │     → Staging:     Docker → fallback Subprocess
            │     → Production:  Docker / External（必须隔离）
            │
            ├── WASM 插件
            │     → 所有 Profile: WASM（天然隔离）
            │
            ├── MCP 工具
            │     → ExecutionTarget::Local（MCP server 自管隔离）
            │
            └── 不信任代码（第三方 skill）
                  → Production:  External（最强隔离）
                  → Staging:     Docker
                  → Development: Docker → fallback Subprocess
```

### 3.4 SandboxType 扩展

```rust
/// 沙箱类型（扩展自 Phase J）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SandboxType {
    /// 原生子进程 (最快，最弱隔离)
    Subprocess,
    /// WASM/WASI (快，强隔离，无网络)
    Wasm,
    /// Docker 容器 (较慢，最强本地隔离)
    Docker,
    /// 第三方远程沙箱 (E2B, Modal, Firecracker 等)
    External(String),  // provider name
}
```

### 3.5 ToolCategory 扩展

```rust
/// 工具类别（扩展自 Phase J）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// Shell 命令 (bash, sh)
    Shell,
    /// 纯计算任务 (适合 WASM)
    Compute,
    /// 文件系统操作 (read, write, edit, glob, grep)
    FileSystem,
    /// 网络请求 (curl, web_fetch, web_search)
    Network,
    /// 脚本执行 (Python/Node/Shell skill)
    Script,
    /// GPU 计算 (只能 External)
    Gpu,
    /// 不信任代码（第三方 skill、用户上传）
    Untrusted,
}
```

---

## 四、SandboxProfile — 预设环境配置

### 4.1 Profile 定义

不让开发者手动配置每个沙箱参数，提供预设 profile，一行切换：

```yaml
# config.yaml
sandbox:
  # 一行切换，其他全自动
  profile: development   # development | staging | production | custom

  # profile: custom 时生效
  custom:
    policy: preferred
    default_backend: subprocess
    wasm_enabled: true
    docker_enabled: false
    external_provider: null
    env_passthrough: all        # all | whitelist | minimal
    approval_gate: high_only    # off | high_only | medium_and_high
    timeout_secs: 60
    audit_level: warn           # off | warn | full
```

### 4.2 Profile 行为矩阵

```
┌─────────────┬───────────────┬────────────────┬──────────────┬──────────────┐
│             │ development   │ staging        │ production   │ custom       │
├─────────────┼───────────────┼────────────────┼──────────────┼──────────────┤
│ SandboxPolicy│ Development  │ Preferred      │ Strict       │ 用户定义     │
│ 默认后端    │ Native        │ Docker→Native  │ Docker/WASM  │ 用户定义     │
│ bash 执行   │ 直接执行      │ Docker 优先    │ 必须沙箱     │ 用户定义     │
│ skill 脚本  │ 宿主机解释器  │ Docker 优先    │ 必须沙箱     │ 用户定义     │
│ WASM 插件   │ WASM          │ WASM           │ WASM         │ 用户定义     │
│ 文件操作    │ 直接操作      │ 直接操作       │ 受限挂载     │ 用户定义     │
│ 网络        │ 无限制        │ 无限制         │ 白名单       │ 用户定义     │
│ env 传递    │ 全部透传      │ 白名单         │ 最小集       │ 用户定义     │
│ 审批门控    │ 关闭          │ High risk only │ Medium+High  │ 用户定义     │
│ 超时        │ 120s          │ 60s            │ 30s          │ 用户定义     │
│ 审计日志    │ 关闭          │ 警告级         │ 全量         │ 用户定义     │
│ 错误详情    │ 完整输出      │ 完整输出       │ 安全过滤     │ 用户定义     │
└─────────────┴───────────────┴────────────────┴──────────────┴──────────────┘
```

### 4.3 配置优先级

```
--sandbox-bypass > --sandbox-profile > OCTO_SANDBOX_PROFILE > config.yaml > 默认值(development)
```

- `--sandbox-bypass`：紧急调试，跳过沙箱层但保留 SecurityPolicy.forbidden_paths
- `--sandbox-profile`：临时切换 profile
- `OCTO_SANDBOX_PROFILE`：CI/CD 环境变量覆盖
- `config.yaml` 中 `sandbox.profile`：项目级配置
- 默认值：`development`（对开发者友好）

---

## 五、模式 A 详细设计 — Octo 在沙箱中运行

### 5.1 沙箱容器结构

```
Docker 容器
├── /workspace/          ← 项目代码挂载（可读写）
├── /home/octo/          ← Octo 工作目录
│   ├── .octo/           ← 配置 + 凭据
│   └── scratch/         ← 临时文件
├── /usr/local/bin/octo  ← Octo 二进制
└── 预装工具链
    ├── python3 + pip + venv
    ├── node + npm
    ├── git, curl, jq, ripgrep, fd
    └── 语言特定工具
```

### 5.2 Octo 沙箱镜像

```dockerfile
FROM ubuntu:24.04

# 系统工具
RUN apt-get update && apt-get install -y \
    python3 python3-pip python3-venv \
    nodejs npm \
    git curl jq ripgrep fd-find \
    && rm -rf /var/lib/apt/lists/*

# Octo 二进制
COPY target/release/octo-cli /usr/local/bin/octo

# 非 root 用户
RUN useradd -m -s /bin/bash octo
USER octo
WORKDIR /home/octo

ENTRYPOINT ["octo"]
```

### 5.3 启动方式

```bash
# 在沙箱中启动 Octo agent
docker run -it \
  --memory=4g --cpus=2 --pids-limit=256 \
  -v $(pwd):/workspace \
  -e ANTHROPIC_API_KEY \
  -e OCTO_SANDBOXED=1 \
  octo-sandbox:latest \
  agent run --workspace /workspace
```

### 5.4 凭据注入

API keys 通过环境变量注入容器，不落盘：

```bash
# Docker secrets (Swarm mode)
docker secret create anthropic_key ./key.txt
docker service create --secret anthropic_key octo-sandbox

# 或简单 env var
docker run -e ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY octo-sandbox
```

### 5.5 资源限制

```bash
docker run \
  --memory=4g              # 内存上限
  --cpus=2                 # CPU 核数限制
  --pids-limit=256         # 进程数限制
  --network=octo-net       # 自定义网络（可选隔离）
  --read-only              # 根文件系统只读
  --tmpfs /tmp:rw,noexec   # /tmp 可写但不可执行
```

---

## 六、模式 B 详细设计 — Octo 在主机运行

### 6.1 Session Sandbox 架构

```
主机 Octo Agent
  │
  ├── AgentExecutor 启动时
  │     → 创建 Session Sandbox（Docker 容器/WASM 实例）
  │     → 挂载项目目录到容器 /workspace
  │     → 容器保持运行（exec 模式）
  │
  ├── 工具调用时
  │     → BashTool("pip install requests")
  │     → 路由决策: Shell + production → Docker
  │     → docker exec <container> bash -c "pip install requests"
  │     → 结果回传给 Agent
  │
  └── Session 结束时
        → 容器销毁 + scratch 清理
```

### 6.2 BashTool 执行路径改造

```
当前:
  BashTool.execute()
    → Command::new("bash").arg("-c").arg(command)   ← 直接在主机执行

目标:
  BashTool.execute()
    → ExecutionTarget 判断
      → Local:    Command::new("bash")...           ← 不变
      → Sandbox:  SandboxRouter.execute(Shell, cmd)  ← 路由到沙箱
```

**关键设计**: 不改 `Tool` trait 接口，在 BashTool 内部根据 `OctoRunMode` 和 `SandboxProfile` 切换执行方式。

### 6.3 SkillRuntime 沙箱对接

```
当前:
  PythonRuntime.execute()
    → 宿主机创建 venv → python script.py

目标:
  PythonRuntime.execute()
    → OctoRunMode::Host + Production:
        → docker exec <id> python3 script.py       ← 容器已有 Python
    → OctoRunMode::Host + Development:
        → Command::new("python3")...               ← 直接执行（现有行为）
    → OctoRunMode::Sandboxed:
        → Command::new("python3")...               ← 直接执行（已在沙箱内）
```

### 6.4 文件系统交互模型

**模式 A（沙箱内）**:
```
Agent 视角: /workspace/src/main.rs  → 直接读写
宿主机:     $(pwd)/src/main.rs      → 通过 volume mount 同步
```

**模式 B（主机 → 沙箱）** — 两种策略:

| 策略 | 做法 | 优点 | 缺点 |
|------|------|------|------|
| **共享挂载** | 项目目录挂载到沙箱 | 文件自动同步 | 沙箱可写主机文件 |
| **显式传输** | stdin/stdout 传递 | 完全隔离 | 大文件操作困难 |

**推荐方案**: 共享挂载 + 读写分区

```
沙箱容器挂载:
  /workspace    → $(pwd)        (读写，项目目录)
  /scratch      → tmpfs         (读写，临时空间，session 结束销毁)
  /home, /etc   → 容器内部      (隔离，不暴露主机)
```

### 6.5 环境变量传递策略

按 profile 控制传递范围:

```rust
/// Development: 全部透传
const DEV_PASSTHROUGH: &[&str] = &["*"];  // 所有 env vars

/// Staging: 白名单
const STAGING_PASSTHROUGH: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "LANG", "TERM", "USER", "SHELL",
    "CARGO_HOME", "RUSTUP_HOME",
    "VIRTUAL_ENV", "PYTHONPATH",
    "NODE_PATH",
    "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENAI_BASE_URL",
    "TAVILY_API_KEY", "JINA_API_KEY",
    "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY",
];

/// Production: 最小集（API keys 通过 CredentialResolver 注入）
const PROD_PASSTHROUGH: &[&str] = &[
    "PATH", "LANG", "TERM",
];
```

---

## 七、第三方沙箱集成设计

### 7.1 ExternalSandboxProvider trait

```rust
/// 第三方沙箱适配器 trait
#[async_trait]
pub trait ExternalSandboxProvider: Send + Sync {
    /// 提供者名称 (e.g., "e2b", "modal", "firecracker")
    fn name(&self) -> &str;

    /// 创建远程沙箱实例
    async fn create(&self, config: &ExternalSandboxConfig)
        -> Result<ExternalSandboxId, SandboxError>;

    /// 在远程沙箱中执行命令
    async fn execute(
        &self,
        id: &ExternalSandboxId,
        request: &ExecRequest,
    ) -> Result<ExecResult, SandboxError>;

    /// 上传文件到沙箱
    async fn upload(
        &self,
        id: &ExternalSandboxId,
        remote_path: &str,
        content: &[u8],
    ) -> Result<(), SandboxError>;

    /// 下载文件从沙箱
    async fn download(
        &self,
        id: &ExternalSandboxId,
        remote_path: &str,
    ) -> Result<Vec<u8>, SandboxError>;

    /// 销毁远程沙箱
    async fn destroy(&self, id: &ExternalSandboxId) -> Result<(), SandboxError>;

    /// 检查服务可用性
    async fn health_check(&self) -> Result<bool, SandboxError>;
}

/// 外部沙箱配置
pub struct ExternalSandboxConfig {
    /// 模板/镜像名
    pub template: String,
    /// 超时 (秒)
    pub timeout_secs: u64,
    /// 环境变量
    pub env: HashMap<String, String>,
    /// 资源限制
    pub memory_limit_mb: Option<u64>,
    pub cpu_count: Option<u32>,
}

/// 执行请求
pub struct ExecRequest {
    pub command: String,
    pub working_dir: Option<String>,
    pub stdin: Option<String>,
    pub timeout_secs: Option<u64>,
}
```

### 7.2 与本地沙箱的关键差异

第三方沙箱需要显式文件传输:

```
本地 Docker:    volume mount → 文件自动同步
第三方沙箱:     upload/download → 显式传输

Agent 执行 Python 脚本流程:
  1. upload(script.py) → 远程沙箱
  2. execute("python script.py") → 远程执行
  3. download(output.json) ← 结果回传
```

### 7.3 配置

```yaml
# config.yaml
sandbox:
  external:
    provider: e2b           # e2b | modal | firecracker | custom
    api_key_env: E2B_API_KEY  # 从环境变量读取 API key
    template: "base"         # 沙箱模板
    timeout_secs: 300
    region: "us-east-1"
```

---

## 八、统一路由矩阵

### 8.1 完整路由决策表

```
┌────────────────────────────────────────────────────────────────────┐
│                     SandboxRouter 路由决策                          │
│                                                                    │
│  输入: (ToolCategory, OctoRunMode, SandboxProfile, 可用后端列表)    │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                    路由优先级矩阵                             │  │
│  │                                                              │  │
│  │  工具类型         Strict        Preferred     Development    │  │
│  │  ─────────────  ───────────   ───────────   ─────────────   │  │
│  │  纯计算(Compute) WASM          WASM          WASM/Native    │  │
│  │  Shell命令       Docker/Ext    Docker→Sub    Native         │  │
│  │  脚本(Script)    Docker/Ext    Docker→Sub    Native         │  │
│  │  文件操作(FS)    Native        Native        Native         │  │
│  │  网络请求        Docker/Ext    Docker→Sub    Native         │  │
│  │  GPU计算         External      External      External       │  │
│  │  不信任代码      External      Docker/Ext    Docker         │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  降级链: External → Docker → WASM → Subprocess → 拒绝(Strict)     │
│         External → Docker → Subprocess(Preferred, 带审计警告)       │
│         任意可用后端(Development)                                    │
└────────────────────────────────────────────────────────────────────┘
```

### 8.2 Sandboxed 模式的简化

当 `OctoRunMode::Sandboxed` 时，路由矩阵退化为:

```
所有 ToolCategory → ExecutionTarget::Local
```

因为 Octo 本身已在沙箱中，再套一层沙箱是多余的。

---

## 九、开发调试便利性设计

### 9.1 Development Profile 特殊待遇

开发模式要做到**零摩擦**:

1. **无沙箱启停开销** → 直接 subprocess，无 Docker 容器启动等待
2. **完整环境变量透传** → API keys、PATH、venv 全部可用
3. **工作目录就是项目目录** → 无文件同步问题
4. **无审批弹窗** → AutonomyLevel::Full
5. **长超时** → 120s（调试需要时间）
6. **错误信息完整** → stderr 不截断，堆栈全量输出
7. **热重载** → 修改 skill 脚本后立即生效，无需重建沙箱

### 9.2 `--sandbox-bypass` 标志

紧急调试时一键跳过沙箱层:

```bash
# 正常运行（按 profile 走）
octo agent run

# 调试模式：跳过沙箱隔离，直接执行
octo agent run --sandbox-bypass

# 等价于临时设置
OCTO_SANDBOX_PROFILE=development octo agent run
```

**安全保障**: bypass 模式下仍保留 `SecurityPolicy.forbidden_paths` 检查（防止误删 `/etc` 等），只跳过沙箱隔离层。

### 9.3 Dry-run 模式

查看工具调用会被路由到哪个沙箱，但不实际执行:

```bash
octo sandbox dry-run --command "pip install requests"

# 输出:
# Command:     pip install requests
# Risk Level:  Medium
# Profile:     staging
# Routed to:   Docker (octo-sandbox/python:1.0)
# Reason:      Shell + package install → Docker sandbox
# Approval:    Required (Medium risk under Supervised autonomy)
```

### 9.4 Skill 脚本开发体验

Skill 开发者的核心诉求是**快速迭代**:

```
修改 Python 脚本 → 立即测试 → 看到结果 → 继续修改

不能:
  修改脚本 → 重建 Docker 镜像 → 重启容器 → 等 30 秒 → 看结果
```

**解决方案**: Development 模式下 skill 脚本直接用宿主机解释器执行

```
development:
  Python skill → 宿主机 python3 (PythonRuntime + venv, 现有行为)
  Shell skill  → 宿主机 bash (ShellRuntime, 现有行为)
  Node skill   → 宿主机 node (NodeJsRuntime, 现有行为)

staging/production:
  Python skill → Docker 容器内 python3
  Shell skill  → Docker 容器内 bash
  Node skill   → Docker 容器内 node
```

### 9.5 错误信息增强

沙箱执行失败时，提供开发者友好的诊断信息:

```
❌ 不友好:
  "Sandbox execution failed: exit code 1"

✅ 友好:
  Tool execution failed in Docker sandbox (octo-sandbox/python:1.0)

  Command: python3 /workspace/script.py
  Exit code: 1

  STDERR:
    ModuleNotFoundError: No module named 'pandas'

  Hint: The Docker image may not have 'pandas' installed.
  Try one of:
    1. Add 'pandas' to your skill's requirements.txt
    2. Use --sandbox-profile development to run with your local Python
    3. Build a custom image with 'pandas' pre-installed
```

### 9.6 TUI 沙箱状态显示

StatusBar 集成当前沙箱 profile 显示:

```
┌─ StatusBar ─────────────────────────────────────────────┐
│ OCTO | sandbox:dev | tools:12 | tokens:1.2k | 00:23    │
└─────────────────────────────────────────────────────────┘
         ^^^^^^^^
         显示当前 profile，颜色编码:
           dev  = 绿色 (无沙箱，最快)
           stg  = 黄色 (沙箱可降级)
           prod = 红色 (严格隔离)
```

### 9.7 ToolExecutionRecord 扩展

记录工具调用的沙箱上下文，方便调试:

```rust
/// 扩展现有 ToolExecutionRecord
struct ToolExecutionRecord {
    // 现有字段...
    tool_name: String,
    params: Value,
    result: ToolOutput,
    duration_ms: u64,

    // 新增沙箱相关字段
    sandbox_profile: String,           // "development"
    execution_target: ExecutionTarget, // Local / Sandbox(Docker)
    actual_backend: SandboxType,       // Subprocess
    routing_reason: String,            // "dev profile → Native"
}
```

---

## 十、与现有架构的兼容性

### 10.1 不变的接口

```
✅ Tool trait      — execute(params, ctx) → ToolOutput    不改
✅ SkillRuntime    — execute(script, args, ctx) → Value   不改
✅ SandboxRouter   — execute(category, code, lang)        只扩展
✅ RuntimeAdapter  — create/execute/destroy                只扩展
```

### 10.2 改动的实现

```
🔧 BashTool 内部        — 增加 ExecutionTarget 判断分支
🔧 SkillRuntime 实现    — 包装一层执行目标判断
🔧 SandboxRouter       — 新增 External 后端 + profile 感知
🔧 SandboxType 枚举    — 新增 External(String) 变体
🔧 ToolCategory 枚举   — 新增 Script/Gpu/Untrusted 变体
🔧 SandboxConfig       — 新增 profile 字段
🔧 config.yaml 结构    — 新增 sandbox 配置段
```

### 10.3 两套 RuntimeAdapter trait 的处理

当前存在两套:
- `octo-sandbox/src/traits.rs`: 简单版 `(cmd, working_dir) → ExecResult`
- `octo-engine/src/sandbox/traits.rs`: 完整版 `create/execute/destroy`

**策略**: 保留两套，不合并。`octo-sandbox` crate 定位为轻量级 adapter，`octo-engine` 的版本面向完整沙箱生命周期管理。用 `From` trait 实现两种 `ExecResult` 之间的转换。

---

## 十一、安全考量

### 11.1 与 SecurityPolicy 的协同

```
执行链路:
  工具调用
    → SecurityPolicy.check_command()     // 命令级安全检查
    → SecurityPolicy.assess_command_risk() // 风险评估
    → AutonomyLevel.requires_approval()  // 审批门控
    → SandboxRouter.execute()            // 沙箱路由 + 隔离执行
    → SandboxAuditEvent.record()         // 审计记录
```

### 11.2 sandbox-bypass 的安全边界

即使使用 `--sandbox-bypass`，以下安全检查仍然生效:

```
✅ 始终生效:
  SecurityPolicy.forbidden_paths    — 禁止操作 /etc, ~/.ssh 等
  SecurityPolicy.check_command      — 命令白名单/黑名单（如果配置）
  SecurityPolicy.max_actions_per_hour — 速率限制

❌ bypass 跳过:
  SandboxRouter 路由                — 直接 subprocess 执行
  Docker/WASM/External 隔离        — 不创建沙箱
  容器资源限制                      — 无 CPU/内存/网络限制
```

### 11.3 CredentialResolver 集成

生产模式下，API keys 不通过环境变量透传，而是通过 `CredentialResolver` just-in-time 注入:

```
Development: env vars 直接透传（方便）
Production:  CredentialResolver 从 credentials.yaml 读取
             → 注入到沙箱容器的环境变量中
             → 不出现在命令行参数或日志中
```

---

## 十二、实施路径

见 `docs/plans/2026-03-23-phase-ab-tool-execution-environment.md`。

---

## 附录 A: 配置示例

### A.1 开发环境配置

```yaml
# .octo/config.yaml
sandbox:
  profile: development
```

### A.2 生产环境配置

```yaml
# ~/.octo/config.yaml
sandbox:
  profile: production
  docker:
    default_image: "octo-sandbox/general:1.0"
    memory_limit: "4g"
    cpu_limit: 2
    network: "octo-internal"
  external:
    provider: e2b
    api_key_env: E2B_API_KEY
    template: "python3"
    timeout_secs: 300
```

### A.3 自定义配置

```yaml
# .octo/config.yaml
sandbox:
  profile: custom
  custom:
    policy: preferred
    default_backend: docker
    wasm_enabled: true
    docker_enabled: true
    external_provider: null
    env_passthrough: whitelist
    approval_gate: high_only
    timeout_secs: 60
    audit_level: warn
```

## 附录 B: CLI 命令参考

| 命令 | 描述 |
|------|------|
| `octo agent run --sandbox-profile <PROFILE>` | 指定沙箱 profile 运行 |
| `octo agent run --sandbox-bypass` | 跳过沙箱隔离（保留安全策略） |
| `octo sandbox dry-run --command <CMD>` | 预览命令的沙箱路由结果 |
| `octo sandbox status` | 查看当前沙箱状态 |
| `octo sandbox list-backends` | 列出可用的沙箱后端 |
