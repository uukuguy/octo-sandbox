# Phase J — Docker 测试修复

**日期**: 2026-03-14
**前置**: Phase I COMPLETE（SWE-bench 适配）
**目标**: 修复 Docker sandbox 的 7 个测试，使 Docker 环境下全部通过；为 SWE-bench 验证器提供可靠的 Docker 基础设施

---

## 背景

当前 Docker 测试状态：
- `crates/octo-engine/tests/sandbox_docker_test.rs` — 7 个测试，全部 `#[cfg(feature = "sandbox-docker")]`
- 测试在有 Docker daemon 时内部 skip（`if !adapter.is_available() { return; }`）
- 测试在无 Docker daemon 时被 feature gate 编译排除
- 第 8 个测试 `test_docker_adapter_without_feature` 在 `#[cfg(not(feature = "sandbox-docker"))]` 下
- `sandbox-docker` 是 `octo-engine` 的 **default feature**，所以正常编译会包含这些测试

### 问题根因

Docker 测试"失败"的原因不是代码 bug，而是**环境问题**：
1. Docker daemon 未运行 → `adapter.is_available()` 返回 false → 测试 skip
2. 在 CI（GitHub Actions ubuntu-latest）上 Docker 可用，但本地开发机可能没有

### 本 Phase 的目标

1. 确保 Docker daemon 运行时，7 个测试全部 **真正通过**（不是 skip）
2. 验证 `DockerAdapter` 的完整生命周期（create → execute → destroy）
3. 为 Phase I 的 `SweVerifier` 提供可靠的 Docker 执行层
4. 改善测试的可观测性（Docker 不可用时明确报告而非静默 skip）

---

## 一、当前代码分析

### DockerAdapter 架构 (`crates/octo-engine/src/sandbox/docker.rs`)

```
DockerAdapter
├── new(image) — 创建适配器，尝试连接 Docker daemon
├── is_available() — 检查 Docker daemon 是否可达
├── is_ready() — 异步检查 Docker daemon 状态
├── create(config) — 创建容器 (docker create + docker start)
├── execute(id, cmd, lang) — 在容器内执行命令 (docker exec)
├── destroy(id) — 停止并删除容器 (docker stop + docker rm)
├── sandbox_type() — 返回 SandboxType::Docker
└── image() — 返回默认镜像名
```

依赖: `bollard` crate（Rust Docker API client）

### 现有测试 (`sandbox_docker_test.rs`)

| 测试 | 验证内容 | 当前状态 |
|------|---------|---------|
| `test_docker_adapter_create` | 创建+销毁容器 | skip if no Docker |
| `test_docker_adapter_create_and_execute` | 创建+执行 echo+销毁 | skip if no Docker |
| `test_docker_adapter_execute_stderr` | stderr 输出捕获 | skip if no Docker |
| `test_docker_adapter_failed_command` | 失败命令 exit code | skip if no Docker |
| `test_docker_adapter_not_found` | 不存在容器的执行 | skip if no Docker |
| `test_docker_adapter_destroy_not_found` | 不存在容器的销毁 | skip if no Docker |
| `test_docker_adapter_with_env_vars` | 环境变量注入 | skip if no Docker |
| `test_docker_adapter_is_ready` | 异步就绪检查 | skip if no Docker |
| `test_docker_adapter_default_image` | 默认镜像名 | ✅ 始终通过 |

---

## 二、任务分组

### J1: Docker 环境检测改进

**J1-T1: 改进测试的 skip 机制**

当前的 skip 方式（`eprintln!` + `return`）不够明确。改为使用清晰的测试输出。

文件改动: `crates/octo-engine/tests/sandbox_docker_test.rs` (~20 行)

```rust
/// Helper: skip test if Docker is not available, with clear message
fn require_docker(adapter: &DockerAdapter) -> bool {
    if !adapter.is_available() {
        eprintln!("⚠️  SKIPPED: Docker daemon not available. Start Docker to run this test.");
        eprintln!("   On macOS: open Docker Desktop");
        eprintln!("   On Linux: sudo systemctl start docker");
        return false;
    }
    true
}
```

**J1-T2: 添加 Docker 环境诊断测试**

新增 1 个诊断测试，报告 Docker 环境状态：

```rust
#[tokio::test]
#[cfg(feature = "sandbox-docker")]
async fn test_docker_environment_diagnostic() {
    let adapter = DockerAdapter::new("alpine:latest");

    println!("Docker diagnostic:");
    println!("  Feature enabled: true");
    println!("  Daemon available: {}", adapter.is_available());
    println!("  Daemon ready: {}", adapter.is_ready().await);
    println!("  Default image: {}", adapter.image());

    if adapter.is_available() {
        // 验证 alpine 镜像可拉取
        let config = SandboxConfig::new(SandboxType::Docker);
        match adapter.create(&config).await {
            Ok(id) => {
                println!("  Container created: {}", id);
                let _ = adapter.destroy(&id).await;
                println!("  Container destroyed: OK");
            }
            Err(e) => {
                println!("  Container create failed: {}", e);
            }
        }
    }
}
```

### J2: DockerAdapter 代码修复

**J2-T1: 审查并修复 DockerAdapter 生命周期**

读取完整的 `docker.rs` 实现，检查以下潜在问题：

1. **容器清理**: `destroy()` 是否正确处理已停止的容器？
2. **镜像拉取**: 如果 `alpine:latest` 不在本地，`create()` 是否自动拉取？
3. **超时处理**: `execute()` 是否有超时？长时间运行的命令是否会卡住？
4. **并发安全**: `instances` RwLock 是否正确使用？
5. **资源泄露**: 测试失败时容器是否被清理？

文件改动: `crates/octo-engine/src/sandbox/docker.rs` (预计 ~30 行修复)

可能的修复：
- 添加镜像自动拉取逻辑（如果 create 失败且错误是 image not found）
- 为 execute 添加超时包装
- 确保 destroy 是幂等的（多次调用不报错）

**J2-T2: 添加容器自动清理机制**

确保测试中的容器即使 panic 也能被清理：

```rust
/// RAII guard for Docker containers — auto-cleanup on drop
struct ContainerGuard<'a> {
    adapter: &'a DockerAdapter,
    id: SandboxId,
}

impl<'a> Drop for ContainerGuard<'a> {
    fn drop(&mut self) {
        // 使用 tokio::task::block_in_place 确保异步 destroy 执行
        let adapter = self.adapter;
        let id = self.id.clone();
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            let _ = rt.block_on(adapter.destroy(&id));
        });
    }
}
```

### J3: SWE-bench Docker 镜像集成

**J3-T1: 创建 SWE-bench 专用 Docker 镜像构建**

文件改动: `crates/octo-eval/Dockerfile.swe-bench` (在 Phase I 已创建)

验证镜像可以：
1. Clone 一个小型 Python 仓库
2. 安装依赖 (`pip install -e .`)
3. 运行 pytest
4. 应用 git patch

**J3-T2: SweVerifier 集成测试**

新文件: `crates/octo-eval/tests/swe_verifier_test.rs` (~50 行)

```rust
#[tokio::test]
#[cfg(feature = "sandbox-docker")]
async fn test_swe_verifier_gold_patch() {
    // 需要 Docker
    // 1. 加载第一个 swe_bench_lite 任务
    // 2. 用 gold patch 调用 verify_with_gold()
    // 3. 验证 passed=true
}

#[tokio::test]
#[cfg(feature = "sandbox-docker")]
async fn test_swe_verifier_bad_patch() {
    // 1. 加载第一个任务
    // 2. 用空 patch 调用 verify()
    // 3. 验证 passed=false
}
```

### J4: CI Docker 支持

**J4-T1: 更新 GitHub Actions 工作流**

文件改动: `.github/workflows/eval-ci.yml` (~15 行)

```yaml
  docker-tests:
    name: Docker Sandbox Tests
    runs-on: ubuntu-latest
    services:
      docker:
        image: docker:dind
        options: --privileged
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Run Docker sandbox tests
        run: cargo test -p octo-engine sandbox_docker -- --test-threads=1
      - name: Pull SWE-bench base image
        run: docker pull python:3.11-slim
      - name: Run SWE-bench mock verification
        run: cargo run -p octo-eval -- run --suite swe_bench --output eval_output/swe_bench
        env:
          DOCKER_AVAILABLE: "true"
```

### J5: 验证

**J5-T1: 本地 Docker 测试**

前提: Docker Desktop 运行中

```bash
# Docker sandbox 测试
cargo test -p octo-engine sandbox_docker -- --test-threads=1

# SWE-bench mock 验证
cargo run -p octo-eval -- run --suite swe_bench
```

**J5-T2: 全量测试**

```bash
cargo test --workspace -- --test-threads=1
```

---

## 三、文件改动矩阵

| 文件 | 操作 | 行数估计 |
|------|------|---------|
| `crates/octo-engine/tests/sandbox_docker_test.rs` | 修改 | +30 |
| `crates/octo-engine/src/sandbox/docker.rs` | 修改 | +30 |
| `crates/octo-eval/tests/swe_verifier_test.rs` | **新建** | ~50 |
| `.github/workflows/eval-ci.yml` | 修改 | +15 |

**总计**: 1 新文件, 3 修改, ~125 行

---

## 四、验收标准

- [ ] Docker Desktop 运行时，7 个 Docker sandbox 测试全部**真正通过**（非 skip）
- [ ] Docker 不可用时，测试输出清晰的 skip 消息
- [ ] `SweVerifier::verify_with_gold()` 至少对 1 个 SWE-bench 任务通过
- [ ] CI 中 Docker 测试独立 job 通过
- [ ] 容器泄露检查: 测试后 `docker ps -a` 无残留容器
- [ ] `cargo test --workspace -- --test-threads=1` 全部通过（Docker 不可用时 skip Docker 测试）
