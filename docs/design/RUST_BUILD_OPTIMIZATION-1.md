# Rust 编译速度优化方案

## 硬件环境

- **机器**: MacBook Pro M3 Max
- **CPU**: 16 核 (12P + 4E)
- **内存**: 128GB 统一内存
- **存储**: NVMe SSD
- **OS**: macOS (Darwin 25.3.0, aarch64)
- **Rust 版本**: 1.92.0

## 优化措施总览

| # | 优化项 | 影响范围 | 预期提升 | 状态 |
|---|--------|----------|----------|------|
| 1 | sccache 编译缓存 | 增量/重复构建 | **50-90%** | ⏳ 待实施 |
| 2 | split-debuginfo = "unpacked" | linking 阶段 | **~70%** | ⏳ 待实施 |
| 3 | codegen-units = 256 | 代码生成并行化 | **20-30%** | ⏳ 待实施 |
| 4 | 14-16 并行 jobs | 全阶段编译 | **10-20%** | ⏳ 待实施 |
| 5 | 依赖 opt-level=1 | 第三方库构建 | 运行时加速 | ⏳ 待实施 |
| 6 | incremental = true | 增量编译 | **40-60%** | ⏳ 待实施 |
| 7 | Gatekeeper 排除 | macOS 安全扫描 | **视情况** | ⚠️ 需手动 |
| 8 | cargo check 替代 build | 开发工作流 | **60-80%** | ⭐ 新增 |
| 9 | cargo watch 自动重编译 | 开发工作流 | ⭐ 新增 | ⭐ 新增 |
| 10 | Rust 1.92+ 新特性 | 全阶段 | **10-20%** | ⭐ 新增 |

## 2025-2026 最新优化发现

### 1. cargo check 替代 cargo build

**原理**: `cargo check` 只进行类型检查，不生成二进制文件，是开发过程中最常用的命令。

**效果**:
- `cargo check` 比 `cargo build` 快 **60-80%**
- 适合开发过程中的快速反馈循环
- 大部分构建时间花在代码生成阶段，而 `cargo check` 跳过这一步

**使用建议**:
```bash
# 开发时首选 cargo check
cargo check --workspace

# 只需验证代码正确性时使用
cargo check

# 需要运行代码时才用 cargo build
cargo build
```

### 2. cargo watch 自动重编译

**原理**: 监视文件变化并自动重新编译，实现类似 Node.js 的快速反馈循环。

**安装**:
```bash
cargo install cargo-watch
```

**使用**:
```bash
# 监视并自动运行
cargo watch -x run

# 监视并自动检查
cargo watch -x check

# 监视并自动测试
cargo watch -x test

# 组合多个命令
cargo watch -x check -x test
```

### 3. Rust 1.92+ 新特性 (2025-2026)

根据 Rust Blog 2025-2026 年的更新:

- **Rust 1.91**: 引入了 `build-dir` 分离，改进增量编译
- **Rust 1.92**: cargo-wizard 工具，可以自动优化构建配置
- **Rust 1.93**: 改进了 `cargo check` / `cargo clippy` / `cargo test` 的并行锁定

**新工具 - cargo-wizard**:
```bash
cargo install cargo-wizard

# 自动优化构建时间
cargo wizard optimize --build-time

# 自动优化运行时性能
cargo wizard optimize --runtime

# 自动优化二进制大小
cargo wizard optimize --binary-size
```

### 4. sccache 编译缓存

**原理**: sccache 是 Mozilla 开发的编译缓存工具，拦截 rustc 调用并缓存编译产物。

**安装**:
```bash
brew install sccache
```

**配置** (创建 `.cargo/config.toml`):
```toml
[build]
rustc-wrapper = "/opt/homebrew/bin/sccache"

[build]
jobs = 14
```

**效果**:
- 首次构建: 无加速 (需要填充缓存)
- 后续构建: 未修改的 crate 直接命中缓存，**跳过编译**
- 跨项目共享: 不同项目使用相同依赖版本时，缓存可复用
- `cargo clean` 后重建也能命中缓存

**监控**:
```bash
sccache --show-stats
```

### 5. split-debuginfo = "unpacked"

**原理**: macOS 默认将 DWARF 调试信息嵌入到最终二进制中，linker 需要处理大量调试数据。

**配置** (在 `Cargo.toml` 中添加):
```toml
[profile.dev]
split-debuginfo = "unpacked"
```

**效果**: macOS 上 linking 阶段可加速 **~70%**。

### 6. codegen-units = 256

**原理**: 值越大，编译器可以将一个 crate 拆分为更多并行处理单元。

**配置**:
```toml
[profile.dev]
codegen-units = 256

[profile.dev.package."*"]
codegen-units = 256
```

**权衡**:
- codegen-units 越多，编译越快，但生成的代码优化程度越低
- dev 构建不关心运行性能，所以 256 是最佳选择
- release 构建应保持 `codegen-units = 1`

### 7. 14-16 并行 jobs

**原理**: M3 Max 有 16 核，设置 14-16 个并行编译任务可以充分利用 CPU。

**配置** (`.cargo/config.toml`):
```toml
[build]
jobs = 16
```

### 8. 依赖 opt-level=1

**原理**: 第三方依赖只在版本变化时重新编译。用 `opt-level=1` 编译依赖，运行时性能明显提升。

**配置**:
```toml
[profile.dev.package."*"]
opt-level = 1
```

### 9. incremental = true

**原理**: 增量编译让 rustc 只重新编译修改过的部分。

**配置**:
```toml
[profile.dev]
incremental = true
```

## 需要手动操作的优化

### macOS Gatekeeper 排除

macOS 的 Gatekeeper 会在每次执行新编译的二进制时进行安全扫描。

**操作步骤**:
1. 打开 **系统设置 → 隐私与安全性 → 开发者工具**
2. 添加你的终端应用 (Terminal / iTerm2 / WezTerm)
3. 重启终端

## 配置文件位置

| 文件 | 用途 |
|------|------|
| `.cargo/config.toml` | sccache、jobs、linker flags |
| `Cargo.toml` | profile 设置 |

## 构建命令参考

```bash
# 开发首选 - 类型检查 (最快)
cargo check --workspace

# 完整构建
cargo build

# 开发时使用 watch 模式
cargo watch -x check

# 查看编译时间分布 (生成 HTML 报告)
cargo build --timings

# 查看 sccache 缓存统计
sccache --show-stats

# 清理缓存后重建 (测试冷启动)
cargo clean && time cargo build

# 使用 cargo-wizard 优化
cargo wizard optimize --build-time
```

## 推荐的开发工作流

1. **日常开发**: 使用 `cargo check` 快速验证代码
2. **保存时自动检查**: `cargo watch -x check`
3. **运行前完整构建**: `cargo build`
4. **提交前全面检查**: `cargo check && cargo clippy && cargo test`
5. **发布前性能优化**: 确保 release 构建使用 `codegen-units = 1`

## 为什么不用 mold/lld?

- **mold**: 不支持 macOS (仅 Linux)
- **lld**: 在 macOS aarch64 上支持不完善，Apple 原生 linker 已经做了 ARM64 优化
- **zld**: 已废弃 (deprecated)
- **结论**: macOS aarch64 上 Apple 原生 linker + `split-debuginfo=unpacked` 是当前最佳方案

## 补充: 系统级优化建议

1. **减少系统负载**: 编译时关闭不必要的重型程序 (LLM 推理、Docker 等)
2. **内存管理**: 128GB 内存充足，但如果同时运行 LLM 服务可能被占用殆尽，导致 swap
3. **磁盘空间**: 确保 NVMe SSD 有足够的剩余空间 (建议 >50GB)
4. **Spotlight 排除**: 在系统设置中将项目目录和 `target/` 目录添加到 Spotlight 排除列表
5. **排除 Time Machine**: 对 target 目录禁用 Time Machine 备份
