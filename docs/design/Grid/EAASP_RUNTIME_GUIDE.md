# EAASP L1 运行时操作指南

本文档说明如何构建、启动和验证 Grid 平台的两个 L1 运行时。

## 架构概览

```
                    ┌──────────────┐
                    │  L3 治理层    │  (未来：拉起容器、下发策略)
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │                         │
    ┌─────────▼──────────┐   ┌─────────▼──────────────┐
    │  grid-runtime      │   │  claude-code-runtime    │
    │  (Rust, :50051)    │   │  (Python, :50052)       │
    │  T1 Harness        │   │  T1 Harness             │
    │  ← grid-engine →   │   │  ← claude-agent-sdk →   │
    └────────────────────┘   └─────────────────────────┘
              │                         │
              └────────────┬────────────┘
                           │
                 ┌─────────▼─────────┐
                 │  eaasp-certifier   │
                 │  (16 方法合规验证)  │
                 └───────────────────┘
```

两个运行时都实现相同的 gRPC `RuntimeService`（16 方法），可互换使用。

---

## 前置条件

| 依赖 | 用途 | 安装 |
|------|------|------|
| Rust 1.75+ | 编译 grid-runtime / certifier | `rustup update` |
| Python 3.12+ | claude-code-runtime | 系统自带或 `brew install python@3.12` |
| uv | Python 包管理 | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| Node.js 18+ | claude-agent-sdk 需要 | `brew install node` |
| Docker | 容器化部署（可选） | Docker Desktop |
| protoc | proto 编译（Rust 自带） | `brew install protobuf` |

**环境变量**（在 `.env` 中配置）：

```bash
ANTHROPIC_API_KEY=sk-ant-xxxxx        # 必须
ANTHROPIC_BASE_URL=https://...        # 可选，自定义 API 端点
ANTHROPIC_MODEL_NAME=claude-sonnet-4-20250514  # 可选，默认 sonnet
```

---

## 一、首次设置

```bash
# 1. Rust 依赖（自动通过 Cargo）
cargo check --workspace

# 2. Python 依赖
make claude-runtime-setup

# 3. 编译 Python proto stubs（首次 + proto 变更后）
make claude-runtime-proto
```

---

## 二、开发验证（不需要 Docker）

### 跑单元测试（不需要 API Key）

```bash
# Rust 测试（grid-runtime + hook-bridge + certifier）
cargo test -p grid-runtime -p grid-hook-bridge -p eaasp-certifier -- --test-threads=1

# Python 测试（39 tests）
make claude-runtime-test
```

### 一键集成验证（需要 API Key）

```bash
make verify-dual-runtime
```

这个命令做了以下事情：
1. `cargo build --release` 编译 grid-runtime 和 certifier
2. 启动 grid-runtime（:50051）
3. 启动 claude-code-runtime（:50052）
4. 用 certifier 验证两个 runtime 的 16 方法合规性
5. 自动关闭所有进程

如果只想验证其中一个：

```bash
./scripts/verify-dual-runtime.sh --grid-only
./scripts/verify-dual-runtime.sh --claude-only
./scripts/verify-dual-runtime.sh --skip-build   # 已编译过，跳过 build
```

### 手动分步启动（用于调试）

```bash
# 终端 1：grid-runtime
cargo run -p grid-runtime --release

# 终端 2：claude-code-runtime
make claude-runtime-start

# 终端 3：验证
cargo run -p eaasp-certifier --release -- verify --endpoint http://localhost:50051
cargo run -p eaasp-certifier --release -- verify --endpoint http://localhost:50052
```

---

## 三、容器化部署

### 构建镜像

```bash
# grid-runtime（Rust）
docker build -f crates/grid-runtime/Dockerfile -t grid-runtime .

# claude-code-runtime（Python）
make claude-runtime-build
```

### 运行容器

```bash
# grid-runtime
docker run --rm -p 50051:50051 \
  -e ANTHROPIC_API_KEY \
  -e ANTHROPIC_BASE_URL \
  grid-runtime

# claude-code-runtime
make claude-runtime-run
# 等价于：
# docker run --rm -p 50052:50052 \
#   -e ANTHROPIC_API_KEY -e ANTHROPIC_BASE_URL -e ANTHROPIC_MODEL_NAME \
#   claude-code-runtime
```

### 验证容器

```bash
# certifier 可以本地运行，验证容器中的 runtime
cargo run -p eaasp-certifier --release -- verify --endpoint http://localhost:50051
cargo run -p eaasp-certifier --release -- verify --endpoint http://localhost:50052

# 或 JSON 格式输出
cargo run -p eaasp-certifier --release -- verify --endpoint http://localhost:50051 --format json
```

---

## 四、命令速查

| 命令 | 说明 |
|------|------|
| `make claude-runtime-setup` | 安装 Python 依赖 |
| `make claude-runtime-proto` | 编译 proto → Python stubs |
| `make claude-runtime-test` | 跑 Python 单元测试 |
| `make claude-runtime-start` | 本地启动 Python runtime :50052 |
| `make claude-runtime-build` | 构建 Docker 镜像 |
| `make claude-runtime-run` | Docker 运行（需 ANTHROPIC_API_KEY） |
| `make verify-dual-runtime` | 一键编译+启动+验证两个 runtime |

---

## 五、排查问题

### grid-runtime 启动失败

```
Failed to build AgentRuntime: ...
```

→ 检查 `.env` 中 `ANTHROPIC_API_KEY` 是否正确

### claude-code-runtime 启动后 Send 失败

```
SDK error: Claude Code not found
```

→ 需要安装 Claude Code CLI：`npm install -g @anthropic-ai/claude-code`

### certifier 报 "connection refused"

→ 确认 runtime 已在对应端口启动：`nc -z localhost 50051`

### proto 编译报错

```bash
# 重新编译
make claude-runtime-proto

# 如果 proto 文件有改动，Rust 侧也需要重新编译
cargo build -p grid-runtime
```

---

## 六、端口约定

| 端口 | 服务 | 说明 |
|------|------|------|
| 50051 | grid-runtime | Rust T1 Harness |
| 50052 | claude-code-runtime | Python T1 Harness |
| 3001 | grid-server | Workbench API |
| 5180 | web (Vite) | 前端开发服务器 |
