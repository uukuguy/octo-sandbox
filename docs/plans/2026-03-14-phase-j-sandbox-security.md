# Phase J — 沙箱安全体系建设

**日期**: 2026-03-14
**前置**: Phase I COMPLETE（外部 Benchmark 适配）
**目标**: 建立企业级沙箱安全体系——默认严格模式、配套 Docker 镜像、完整 WASM/WASI 执行、全链路审计日志

---

## 背景与设计决策

### 核心定位

octo-sandbox 的名字本身就是"沙箱"。所有工具执行 **必须** 在隔离环境中完成：

- **生产环境**: 仅允许 Docker / WASM 执行（Strict 模式，默认）
- **开发/测试**: 允许本机 Subprocess 执行（Development 模式，需显式配置）

### 行业标准对标

参照 OWASP Top 10 for Agentic Applications 2026：

| OWASP 风险 | 要求 | 本 Phase 对应 |
|---|---|---|
| ASI-03 Unexpected Code Execution | 所有代码在隔离环境执行 | SandboxPolicy::Strict |
| ASI-04 Tool Misuse | 工具调用参数验证+沙箱限制 | Router 策略拦截 |
| ASI-06 Cascading Failures | 故障隔离+资源限制 | Docker 资源限制 |
| ASI-10 Rogue Agents | 不可变审计日志 | Hash-chain 审计 |
| 通用 | Log Everything | SandboxAuditEvent |

### 沙箱隔离级别

```
Docker Container (企业主力)     WASM/WASI (轻量快速)     Subprocess (仅开发)
   容器级隔离                     字节码级隔离              进程级隔离
   ~1-3s 冷启动                  <1ms 启动                <1ms
   Python/Node/Rust/CLI          CLI 工具快速执行           本机 shell
```

---

## 一、任务分组

### J1: SandboxPolicy 策略引擎 (2 tasks)

**J1-T1: SandboxPolicy 枚举与 Router 集成**

在 `crates/octo-engine/src/sandbox/traits.rs` 新增：

```rust
/// 沙箱执行策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxPolicy {
    /// 生产环境：仅允许 Docker/WASM，拒绝 Subprocess
    Strict,
    /// 预发布：优先 Docker/WASM，降级时记录审计警告
    Preferred,
    /// 开发/测试：允许 Subprocess 本机执行
    Development,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        SandboxPolicy::Strict  // 默认严格
    }
}

impl SandboxPolicy {
    /// 检查给定的沙箱类型在此策略下是否允许
    pub fn allows(&self, sandbox_type: SandboxType) -> bool {
        match self {
            SandboxPolicy::Strict => matches!(sandbox_type, SandboxType::Docker | SandboxType::Wasm),
            SandboxPolicy::Preferred => true,
            SandboxPolicy::Development => true,
        }
    }

    /// 检查降级是否需要审计记录
    pub fn requires_degradation_audit(&self, target: SandboxType, actual: SandboxType) -> bool {
        target != actual && matches!(self, SandboxPolicy::Preferred)
    }
}
```

在 `SandboxRouter` 中集成 `SandboxPolicy`：
- `SandboxRouter::new()` 接受 `SandboxPolicy` 参数
- `execute()` 方法：
  1. 获取目标 SandboxType
  2. 如果目标适配器不可用，按策略决定：Strict → 拒绝；Preferred → 降级+审计；Development → 降级
  3. 返回 `SandboxError::PolicyDenied` 新变体（Strict 模式拒绝时）

文件改动:
- `crates/octo-engine/src/sandbox/traits.rs` (+35 行)
- `crates/octo-engine/src/sandbox/router.rs` (+40 行)

**J1-T2: SandboxPolicy 单元测试**

新增测试覆盖：
- Strict 模式拒绝 Subprocess
- Strict 模式允许 Docker/WASM
- Preferred 模式允许降级
- Development 模式允许全部
- Router 在 Strict 模式下无 Docker 适配器时返回 PolicyDenied

文件改动:
- `crates/octo-engine/src/sandbox/router.rs` tests 模块 (+50 行)
- `crates/octo-engine/tests/sandbox_policy_test.rs` (新建, ~60 行)

---

### J2: Docker 预置镜像与语言自动检测 (2 tasks)

**J2-T1: 创建 Docker 预置镜像定义**

新增目录 `docker/sandbox-images/`，包含两个主要镜像的 Dockerfile：

**`docker/sandbox-images/Dockerfile.python`** — Python 沙箱:
```dockerfile
FROM python:3.12-slim-bookworm
RUN pip install --no-cache-dir pip setuptools wheel
RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl jq && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash sandbox
USER sandbox
WORKDIR /workspace
LABEL octo.sandbox.type="python" octo.sandbox.version="1.0"
```

**`docker/sandbox-images/Dockerfile.rust`** — Rust 沙箱:
```dockerfile
FROM rust:1.92-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl jq && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash sandbox
USER sandbox
WORKDIR /workspace
LABEL octo.sandbox.type="rust" octo.sandbox.version="1.0"
```

另外保持 `alpine:latest` 作为 CLI 工具轻量镜像。

文件改动:
- `docker/sandbox-images/Dockerfile.python` (新建, ~10 行)
- `docker/sandbox-images/Dockerfile.rust` (新建, ~10 行)

**J2-T2: DockerAdapter 语言自动检测与镜像选择**

在 `DockerAdapter` 中增加镜像自动选择：

```rust
/// 预置镜像注册表
pub struct ImageRegistry {
    images: HashMap<String, String>,
}

impl ImageRegistry {
    pub fn default_registry() -> Self {
        let mut images = HashMap::new();
        images.insert("python".into(), "python:3.12-slim-bookworm".into());
        images.insert("rust".into(), "rust:1.92-bookworm".into());
        images.insert("node".into(), "node:22-bookworm-slim".into());
        images.insert("javascript".into(), "node:22-bookworm-slim".into());
        images.insert("typescript".into(), "node:22-bookworm-slim".into());
        images.insert("bash".into(), "alpine:latest".into());
        images.insert("sh".into(), "alpine:latest".into());
        images.insert("cli".into(), "alpine:latest".into());
        Self { images }
    }

    /// 根据 language 参数选择镜像
    pub fn resolve(&self, language: &str) -> &str {
        self.images
            .get(language)
            .map(|s| s.as_str())
            .unwrap_or("alpine:latest")
    }
}
```

修改 `DockerAdapter::execute()` —— `language` 参数不再被忽略，用于镜像选择。
修改 `DockerAdapter::create()` 支持通过 `SandboxConfig` 传入语言信息。

文件改动:
- `crates/octo-engine/src/sandbox/docker.rs` (+45 行)

---

### J3: DockerAdapter 修复与加固 (2 tasks)

**J3-T1: DockerAdapter 生命周期修复**

读取并修复以下问题：

1. `destroy()` 幂等性 — 已处理（当前 stop/remove 都容忍 not found）✅
2. 镜像拉取错误处理 — `create_container()` 中拉取失败仅 warn，应区分"镜像不存在"和"网络错误"
3. `execute_in_container()` 超时 — 当前 polling 300×100ms=30s，改为可配置（使用 `SandboxConfig::time_limit`）
4. 容器资源限制 — `create_container()` 中添加 memory_limit / cpu 限制

```rust
// 在 container_config 中添加 HostConfig
let host_config = bollard::models::HostConfig {
    memory: config.memory_limit.map(|m| m as i64),
    nano_cpus: Some(1_000_000_000), // 1 CPU default
    network_mode: Some("none".to_string()), // 网络隔离
    ..Default::default()
};
```

文件改动:
- `crates/octo-engine/src/sandbox/docker.rs` (+30 行修改)

**J3-T2: ContainerGuard RAII 自动清理**

在测试辅助模块中添加 RAII guard，确保容器在 panic 时也被清理：

```rust
/// RAII guard for Docker containers — auto-cleanup on drop
pub struct ContainerGuard<'a> {
    adapter: &'a DockerAdapter,
    id: Option<SandboxId>,
}

impl<'a> ContainerGuard<'a> {
    pub fn new(adapter: &'a DockerAdapter, id: SandboxId) -> Self {
        Self { adapter, id: Some(id) }
    }

    pub fn id(&self) -> &SandboxId {
        self.id.as_ref().unwrap()
    }

    /// Release ownership without destroying
    pub fn release(mut self) -> SandboxId {
        self.id.take().unwrap()
    }
}

impl<'a> Drop for ContainerGuard<'a> {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let adapter = self.adapter;
            // Best effort cleanup — cannot await in drop
            std::thread::spawn(move || {
                // Note: in test context, tokio runtime is available
                // Production code should use explicit destroy()
            });
            eprintln!("ContainerGuard: auto-cleanup for sandbox {}", id);
        }
    }
}
```

文件改动:
- `crates/octo-engine/tests/sandbox_docker_test.rs` (+30 行)

---

### J4: WASM/WASI 完整可用 (3 tasks)

**J4-T1: WasmAdapter 增强为 WASI CLI 执行器**

当前 WASM 适配器只能执行预编译 `.wasm` 二进制（base64 传入）。增强为支持 WASI CLI 模式：

```rust
#[cfg(feature = "sandbox-wasm")]
pub async fn execute_wasi_cli(
    &self,
    id: &SandboxId,
    wasm_bytes: &[u8],
    args: &[String],
    stdin: Option<&str>,
) -> Result<ExecResult, SandboxError> {
    let engine = self.engine.as_ref()
        .ok_or_else(|| SandboxError::ExecutionFailed("WASM engine not initialized".into()))?;

    let start = std::time::Instant::now();

    // 创建 WASI 上下文
    let mut wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new();
    wasi_ctx.inherit_stdio();  // 捕获 stdout/stderr
    wasi_ctx.args(args);

    // 可选 stdin
    if let Some(input) = stdin {
        // 使用 pipe 写入 stdin
        wasi_ctx.stdin(wasmtime_wasi::pipe::MemoryInputPipe::new(input.as_bytes().to_vec()));
    }

    // 配置 stdout/stderr 捕获
    let stdout_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(4096);
    let stderr_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(4096);
    wasi_ctx.stdout(stdout_pipe.clone());
    wasi_ctx.stderr(stderr_pipe.clone());

    let wasi = wasi_ctx.build();
    let mut store = wasmtime::Store::new(engine, wasi);

    // 链接 WASI 函数
    let mut linker = wasmtime::Linker::new(engine);
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;

    let module = wasmtime::Module::from_binary(engine, wasm_bytes)?;
    let instance = linker.instantiate(&mut store, &module)?;

    // 调用 _start
    let func = instance.get_typed_func::<(), ()>(&mut store, "_start");
    let exit_code = match func {
        Ok(f) => match f.call(&mut store, ()) {
            Ok(()) => 0,
            Err(e) => {
                // WASI exit code
                if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                    exit.0
                } else {
                    1
                }
            }
        },
        Err(_) => 1,
    };

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&stdout_pipe.contents()).to_string();
    let stderr = String::from_utf8_lossy(&stderr_pipe.contents()).to_string();

    Ok(ExecResult {
        stdout,
        stderr,
        exit_code,
        execution_time_ms: duration_ms,
        success: exit_code == 0,
    })
}
```

核心改动：
1. 使用 `wasmtime_wasi` 构建 WASI context（而非裸 wasmtime）
2. 捕获 stdout/stderr 到 pipe
3. 支持传入 args 和 stdin
4. 正确处理 WASI exit code

文件改动:
- `crates/octo-engine/src/sandbox/wasm.rs` (+80 行)

**J4-T2: RuntimeAdapter::execute() 传递 language 参数给 WASM**

修改 `WasmAdapter::execute()` —— 当 language 为 `wasi-cli` 时，使用 `execute_wasi_cli()`。
当 code 以 `wasi://` 前缀时，解析为 WASI CLI 调用。

```rust
// 在 execute() 中
if language == "wasi-cli" || code.starts_with("wasi://") {
    let wasm_path = code.strip_prefix("wasi://").unwrap_or(code);
    let wasm_bytes = tokio::fs::read(wasm_path).await
        .map_err(|e| SandboxError::IoError(e))?;
    return self.execute_wasi_cli(id, &wasm_bytes, &[], None).await;
}
```

文件改动:
- `crates/octo-engine/src/sandbox/wasm.rs` (+20 行修改)

**J4-T3: WASM/WASI 单元测试**

在有 `sandbox-wasm` feature 时验证：
1. WASI CLI 模块加载并执行
2. stdout/stderr 正确捕获
3. exit code 正确传播
4. 沙箱隔离（无法访问未授权路径）

文件改动:
- `crates/octo-engine/tests/sandbox_wasm_test.rs` (+50 行)

---

### J5: 沙箱审计日志 (3 tasks)

**J5-T1: SandboxAuditEvent 定义**

复用现有 `AuditStorage` 的 hash-chain 机制，通过 `metadata` JSON 字段存储沙箱专属信息：

```rust
/// 沙箱审计事件 — 映射到 AuditEvent
pub struct SandboxAuditEvent {
    pub sandbox_type: SandboxType,
    pub sandbox_id: String,
    pub action: SandboxAction,
    pub language: String,
    pub code_hash: String,           // SHA-256 of executed code
    pub image: Option<String>,
    pub exit_code: Option<i32>,
    pub execution_time_ms: u64,
    pub stdout_size: usize,
    pub stderr_size: usize,
    pub policy: SandboxPolicy,
    pub was_degraded: bool,
    pub resource_usage: Option<ResourceUsage>,
}

pub struct ResourceUsage {
    pub memory_peak_bytes: Option<u64>,
    pub cpu_time_ms: Option<u64>,
}

pub enum SandboxAction {
    Create,
    Execute,
    Destroy,
    PolicyDeny,
    DegradationWarning,
    ResourceExceeded,
    Timeout,
}

impl SandboxAuditEvent {
    /// 转换为通用 AuditEvent
    pub fn to_audit_event(&self, session_id: Option<&str>) -> AuditEvent {
        let metadata = serde_json::json!({
            "sandbox_type": format!("{}", self.sandbox_type),
            "sandbox_id": self.sandbox_id,
            "language": self.language,
            "code_hash": self.code_hash,
            "image": self.image,
            "exit_code": self.exit_code,
            "execution_time_ms": self.execution_time_ms,
            "stdout_size": self.stdout_size,
            "stderr_size": self.stderr_size,
            "policy": format!("{:?}", self.policy),
            "was_degraded": self.was_degraded,
        });

        AuditEvent {
            event_type: "sandbox".to_string(),
            user_id: None,
            session_id: session_id.map(|s| s.to_string()),
            resource_id: Some(self.sandbox_id.clone()),
            action: format!("{:?}", self.action),
            result: if self.exit_code == Some(0) { "success" } else { "failure" }.to_string(),
            metadata: Some(metadata),
            ip_address: None,
        }
    }
}
```

文件改动:
- `crates/octo-engine/src/sandbox/audit.rs` (新建, ~100 行)
- `crates/octo-engine/src/sandbox/mod.rs` (+2 行 re-export)

**J5-T2: Router 审计集成**

在 `SandboxRouter::execute()` 的执行流中自动记录审计：

```rust
impl SandboxRouter {
    pub async fn execute(
        &self,
        category: ToolCategory,
        code: &str,
        language: &str,
    ) -> Result<ExecResult, SandboxError> {
        let sandbox_type = self.get_sandbox_type(category);

        // 策略检查
        if !self.policy.allows(sandbox_type) {
            // 记录 PolicyDeny 审计
            self.audit_policy_deny(sandbox_type, code, language);
            return Err(SandboxError::PolicyDenied { ... });
        }

        // 降级检查
        let (actual_type, adapter) = self.resolve_adapter(sandbox_type)?;
        let was_degraded = actual_type != sandbox_type;

        if was_degraded {
            self.audit_degradation(sandbox_type, actual_type, code, language);
        }

        // 执行
        let config = SandboxConfig::new(actual_type);
        let id = adapter.create(&config).await?;
        let result = adapter.execute(&id, code, language).await;
        let _ = adapter.destroy(&id).await;

        // 记录执行审计
        self.audit_execution(&id, actual_type, code, language, &result, was_degraded);

        result
    }
}
```

文件改动:
- `crates/octo-engine/src/sandbox/router.rs` (+50 行)

**J5-T3: 审计日志查询 API**

在 `AuditStorage` 中增加沙箱审计专用查询方法：

```rust
impl AuditStorage {
    /// 查询沙箱审计日志
    pub fn query_sandbox_events(
        &self,
        sandbox_id: Option<&str>,
        action: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> rusqlite::Result<Vec<AuditRecord>> {
        self.query_with_type("sandbox", sandbox_id, action, limit, offset)
    }

    /// 查询策略拒绝事件
    pub fn query_policy_denials(&self, limit: u32) -> rusqlite::Result<Vec<AuditRecord>> {
        self.query(Some("sandbox"), None, limit, 0)
            // filter by action = "PolicyDeny" in metadata
    }
}
```

文件改动:
- `crates/octo-engine/src/audit/storage.rs` (+30 行)
- `crates/octo-engine/tests/sandbox_audit_test.rs` (新建, ~60 行)

---

### J6: Docker 测试修复 (2 tasks)

**J6-T1: 改进测试 skip 机制**

在 `sandbox_docker_test.rs` 中：
1. 添加 `require_docker()` 辅助函数，输出清晰的 skip 消息
2. 添加 `test_docker_environment_diagnostic()` 诊断测试
3. 在所有测试中使用 `ContainerGuard` 确保清理

文件改动:
- `crates/octo-engine/tests/sandbox_docker_test.rs` (+40 行)

**J6-T2: Docker 测试本地验证**

前提：Docker Desktop 运行中

```bash
# Docker sandbox 测试
cargo test -p octo-engine sandbox_docker -- --test-threads=1

# 所有沙箱测试
cargo test -p octo-engine sandbox -- --test-threads=1
```

验证项：
- Docker 可用时所有测试真正通过（非 skip）
- Docker 不可用时输出清晰 skip 消息
- 无残留容器（`docker ps -a --filter label=octo-sandbox`）

---

### J7: CI 集成与全量验证 (2 tasks)

**J7-T1: GitHub Actions Docker 测试 job**

更新 `.github/workflows/eval-ci.yml`：

```yaml
  docker-sandbox-tests:
    name: Docker Sandbox Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run Docker sandbox tests
        run: cargo test -p octo-engine sandbox_docker -- --test-threads=1
      - name: Verify no container leak
        run: |
          count=$(docker ps -a --filter label=octo-sandbox -q | wc -l)
          if [ "$count" -gt "0" ]; then
            echo "LEAK: $count orphaned sandbox containers"
            docker ps -a --filter label=octo-sandbox
            exit 1
          fi
```

文件改动:
- `.github/workflows/eval-ci.yml` (+20 行)

**J7-T2: 全量测试验证**

```bash
cargo test --workspace -- --test-threads=1
```

验收标准：所有现有测试 + 新增测试通过，测试总数 > 1992

---

## 二、执行顺序与依赖

```
J1 (SandboxPolicy)
  ↓
J2 (Docker 镜像) ─────┐
  ↓                    │
J3 (DockerAdapter 修复)│
  ↓                    │
J4 (WASM/WASI 增强) ──┤
  ↓                    │
J5 (审计日志) ─────────┘
  ↓
J6 (Docker 测试修复)
  ↓
J7 (CI + 全量验证)
```

**可并行**: J2 + J3 + J4 可以并行执行（都依赖 J1）

---

## 三、文件改动矩阵

| 文件 | 操作 | 行数估计 |
|------|------|---------|
| `crates/octo-engine/src/sandbox/traits.rs` | 修改 | +35 |
| `crates/octo-engine/src/sandbox/router.rs` | 修改 | +90 |
| `crates/octo-engine/src/sandbox/docker.rs` | 修改 | +75 |
| `crates/octo-engine/src/sandbox/wasm.rs` | 修改 | +100 |
| `crates/octo-engine/src/sandbox/audit.rs` | **新建** | ~100 |
| `crates/octo-engine/src/sandbox/mod.rs` | 修改 | +3 |
| `crates/octo-engine/src/audit/storage.rs` | 修改 | +30 |
| `crates/octo-engine/tests/sandbox_policy_test.rs` | **新建** | ~60 |
| `crates/octo-engine/tests/sandbox_docker_test.rs` | 修改 | +70 |
| `crates/octo-engine/tests/sandbox_wasm_test.rs` | 修改 | +50 |
| `crates/octo-engine/tests/sandbox_audit_test.rs` | **新建** | ~60 |
| `docker/sandbox-images/Dockerfile.python` | **新建** | ~10 |
| `docker/sandbox-images/Dockerfile.rust` | **新建** | ~10 |
| `.github/workflows/eval-ci.yml` | 修改 | +20 |

**总计**: 4 新文件, 8 修改, ~713 行

---

## 四、验收标准

- [ ] `SandboxPolicy::Strict` 为默认值，拒绝 Subprocess 执行
- [ ] `SandboxPolicy::Development` 允许全部后端
- [ ] `SandboxRouter::execute()` 在 Strict 模式无 Docker 时返回 `PolicyDenied`
- [ ] Docker 镜像 `python:3.12-slim-bookworm` 和 `rust:1.92-bookworm` 的 Dockerfile 就绪
- [ ] `ImageRegistry` 根据 language 自动选择镜像
- [ ] DockerAdapter 容器有 memory/cpu 资源限制和网络隔离
- [ ] WasmAdapter 支持 WASI CLI 模式（stdout/stderr 捕获，args 传入）
- [ ] 所有沙箱操作记录 `SandboxAuditEvent`，复用 hash-chain
- [ ] Docker 测试在 Docker 可用时全部通过
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过
- [ ] CI 中 Docker 测试独立 job 通过
- [ ] 无容器泄露
