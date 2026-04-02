# Rust 编译速度优化方案

## 硬件环境

- **机器**: MacBook Pro M3 Max
- **CPU**: 16 核 (12P + 4E)
- **内存**: 128GB 统一内存
- **存储**: NVMe SSD
- **OS**: macOS (Darwin 25.3.0, aarch64)

## 优化措施总览

| # | 优化项 | 影响范围 | 预期提升 | 状态 |
|---|--------|----------|----------|------|
| 1 | sccache 编译缓存 | 增量/重复构建 | **50-90%** | ✅ 已启用 |
| 2 | split-debuginfo = "unpacked" | linking 阶段 | **~70%** | ✅ 已启用 |
| 3 | codegen-units = 256 | 代码生成并行化 | **20-30%** | ✅ 已启用 |
| 4 | 14 并行 jobs | 全阶段编译 | **10-20%** | ✅ 已启用 |
| 5 | 依赖 opt-level=1 | 第三方库构建 | 运行时加速 | ✅ 已启用 |
| 6 | incremental = true | 增量编译 | **40-60%** | ✅ 已启用 |
| 7 | Gatekeeper 排除 | macOS 安全扫描 | **视情况** | ⚠️ 需手动 |
| 8 | Cranelift 后端 | codegen 阶段 | **~50%** | ❌ 待评估 |

## 已实施的优化详解

### 1. sccache 编译缓存

**原理**: sccache 是 Mozilla 开发的编译缓存工具，拦截 rustc 调用并缓存编译产物。当输入文件未变化时，直接返回缓存结果而不调用编译器。

**安装**: `brew install sccache`

**配置** (`.cargo/config.toml`):
```toml
[build]
rustc-wrapper = "/opt/homebrew/bin/sccache"
```

**效果**:
- 首次构建: 无加速 (需要填充缓存)
- 后续构建: 未修改的 crate 直接命中缓存，**跳过编译**
- 跨项目共享: 不同项目使用相同依赖版本时，缓存可复用
- `cargo clean` 后重建也能命中缓存

**监控**: `sccache --show-stats` 查看缓存命中率

### 2. split-debuginfo = "unpacked"

**原理**: macOS 默认将 DWARF 调试信息嵌入到最终二进制中，linker 需要处理大量调试数据。`unpacked` 模式将调试信息拆分为独立的 `.dSYM` 目录下的小文件，大幅减少链接器工作量。

**配置** (`Cargo.toml`):
```toml
[profile.dev]
split-debuginfo = "unpacked"
```

**效果**: Rust 1.51+ 支持，macOS 上 linking 阶段可加速 **~70%**。这是 macOS 上最重要的单项优化。

### 3. codegen-units = 256

**原理**: codegen-units 控制 LLVM 代码生成的并行度。值越大，编译器可以将一个 crate 拆分为更多并行处理单元。

**配置** (`Cargo.toml`):
```toml
[profile.dev]
codegen-units = 256

[profile.dev.package."*"]
codegen-units = 256
```

**权衡**:
- codegen-units 越多，编译越快，但生成的代码优化程度越低
- dev 构建不关心运行性能，所以 256 是最佳选择
- release 构建应保持 `codegen-units = 1` 以获得最佳优化

### 4. 14 并行 jobs

**原理**: M3 Max 有 16 核，设置 14 个并行编译任务可以充分利用 CPU，同时保留 2 核给系统和其他进程。

**配置** (`.cargo/config.toml`):
```toml
[build]
jobs = 14
```

### 5. 依赖 opt-level=1

**原理**: 第三方依赖只在版本变化时重新编译。用 `opt-level=1` 编译依赖，编译时间增加很小，但运行时性能明显提升（特别是 serde、tokio 等热路径库）。

**配置** (`Cargo.toml`):
```toml
[profile.dev.package."*"]
opt-level = 1
```

### 6. incremental = true

**原理**: 增量编译让 rustc 只重新编译修改过的部分，而不是整个 crate。这是 Rust 默认行为，但显式声明确保不被意外覆盖。

**配置** (`Cargo.toml`):
```toml
[profile.dev]
incremental = true
```

## 需要手动操作的优化

### 7. macOS Gatekeeper 排除

macOS 的 Gatekeeper 会在每次执行新编译的二进制时进行安全扫描。对于频繁的编译-运行循环，这会显著增加延迟。

**操作步骤**:
1. 打开 **系统设置 → 隐私与安全性 → 开发者工具**
2. 添加你的终端应用 (Terminal / iTerm2 / WezTerm)
3. 重启终端

或使用命令行:
```bash
# 对 cargo target 目录禁用 Gatekeeper
sudo spctl --add /Users/sujiangwen/sandbox/LLM/speechless.ai/Autonomous-Agents/octo-sandbox/target
```

## 编译瓶颈分析 (cargo --timings, 2026-04-02)

Clean build octo-cli = **112s**，各 crate 耗时：

| 耗时 | Crate | 分类 | 优化手段 |
|------|-------|------|----------|
| 40.5s | pdf-extract | PDF 处理 | Feature gate → `pdf` |
| 31.0s | cranelift-codegen | WASM 编译器 | Feature gate → `sandbox-wasm` |
| 20.2s | octo-engine | 核心代码 | 增量编译（无法 gate） |
| 19.6s | rmcp | MCP SDK | 无（必需） |
| 14.2s | wasmtime | WASM 运行时 | Feature gate → `sandbox-wasm` |
| 13.9s | wasmparser | WASM | Feature gate |
| 13.6s | wasmtime-wasi | WASM | Feature gate |
| 10.5s | pulley-interpreter | WASM | Feature gate |
| 10.5s | wasmtime-environ | WASM | Feature gate |
| 8.8s | reqwest | HTTP | 无（必需） |
| 8.3s | bollard-stubs | Docker | Feature gate → `sandbox-docker` |
| 8.0s | bollard | Docker | Feature gate |
| 8.2s | sqlx-sqlite | SQLite | 无（必需） |

**WASM 全家桶 ≈ 94s（50%）、PDF ≈ 40s（22%）、Docker ≈ 16s（9%）**

### Feature Gate 方案（最大收益）

将 WASM、Docker、PDF 改为 opt-in feature，日常开发不编译：

| Feature | Crate | 默认 | 说明 |
|---------|-------|------|------|
| `sandbox-wasm` | octo-sandbox | 关 | Wasmtime WASM 沙箱 |
| `sandbox-docker` | octo-sandbox | 关 | Docker 容器沙箱 |
| `pdf` | octo-engine | 关 | PDF 文件解析工具 |
| `full` | octo-cli | 关 | 启用上述全部 |

日常开发 `cargo build` 不带 feature → 编译时间 **112s → ~40s**（-65%）。
完整构建 `cargo build --features full` → 包含全部功能。

### codegen-units 调整

原值 256 对 16 核 M3 Max 过高（增加 linker 合并开销），调至 16：

```toml
[profile.dev]
codegen-units = 16

[profile.dev.package."*"]
codegen-units = 16
```

### lld 链接器

```bash
brew install llvm
```

`~/.cargo/config.toml`:
```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-fuse-ld=/opt/homebrew/opt/llvm/bin/ld64.lld"]
```

注意：需实测是否稳定，部分 macOS 框架可能与 lld 不兼容。

### 优化效果预估（叠加）

| 优化 | 首次 clean build | 缓存后 clean build | 增量编译 |
|------|-----------------|-------------------|---------|
| 基线 | 112s | 112s | ~15s |
| +sccache（已启用） | 112s | ~25s | ~15s |
| +codegen-units=16 | -5s | -5s | -1s |
| +feature gate | **-70s** | - | **-5s** |
| **全部叠加** | **~40s** | **~15s** | **~8s** |

## 待评估的优化

### 8. Cranelift 后端 (实验性)

**原理**: Cranelift 是一个替代 LLVM 的代码生成后端，编译速度显著更快（~50%），但生成的代码运行速度较慢。适合 dev 构建。

**状态**: 需要 nightly 工具链，且不支持所有特性（如内联汇编）。部分项目可能无法使用。

**评估条件**:
- 项目不依赖内联汇编
- 可以接受 nightly 工具链
- dev 构建不关心运行时性能

**如果需要启用**:
```bash
rustup install nightly
rustup component add rustc-codegen-cranelift-preview --toolchain nightly
```
```toml
# .cargo/config.toml
[unstable]
codegen-backend = true

[profile.dev]
codegen-backend = "cranelift"
```

## 配置文件位置

| 文件 | 用途 |
|------|------|
| `.cargo/config.toml` | sccache、jobs、linker flags |
| `Cargo.toml` | profile 设置 (split-debuginfo, codegen-units, opt-level) |

## 构建命令参考

```bash
# 检查编译 (不生成二进制，最快)
cargo check --workspace

# 开发构建
cargo build

# 查看编译时间分布 (生成 HTML 报告)
cargo build --timings

# 查看 sccache 缓存统计
sccache --show-stats

# 清理缓存后重建 (测试冷启动)
cargo clean && time cargo build
```

## 为什么不用 mold/lld?

- **mold**: 不支持 macOS (仅 Linux)。macOS 版本 sold 是商业软件
- **lld**: 在 macOS aarch64 上支持不完善，Apple 原生 linker 已经做了 ARM64 优化
- **zld**: 已废弃 (deprecated)
- **结论**: macOS aarch64 上 Apple 原生 linker + `split-debuginfo=unpacked` 是当前最佳方案

## 补充: 系统级优化建议

1. **减少系统负载**: 编译时关闭不必要的重型程序 (LLM 推理、Docker 等)
2. **内存管理**: 128GB 内存充足，但如果同时运行 LLM 服务可能被占用殆尽，导致 swap，严重拖慢编译
3. **磁盘空间**: 确保 NVMe SSD 有足够的剩余空间 (建议 >50GB)
4. **Spotlight 排除**: 在系统设置中将项目目录和 `target/` 目录添加到 Spotlight 排除列表
